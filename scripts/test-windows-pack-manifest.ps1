function Find-PackControl([Windows.Automation.AutomationElement]$Root, [string]$Name) {
    $condition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::NameProperty, $Name)
    foreach ($element in $Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition)) {
        if (-not $element.Current.IsOffscreen) { return $element }
    }
}

function Invoke-PackControl([Windows.Automation.AutomationElement]$Element) {
    $pattern = $null
    if ($Element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
        $pattern.Invoke()
        return
    }
    if ($Element.TryGetCurrentPattern([Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$pattern)) {
        $pattern.Expand()
        return
    }
    Move-ToElement $Element
    if ([AgentBroSmokeInput]::ProcessAtCursor() -ne $AppProcessId) {
        throw "AgentBro does not own the click point for $($Element.Current.Name)"
    }
    [AgentBroSmokeInput]::Click()
}

function Set-PackDialogFile([string]$Action, [string]$Path) {
    $dialogClass = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ClassNameProperty, '#32770')
    $dialog = Wait-IslandValue "the native $Action file dialog" {
        foreach ($window in Get-IslandWindows) {
            $candidates = @($window) + @($window.FindAll([Windows.Automation.TreeScope]::Descendants, $dialogClass))
            foreach ($candidate in $candidates) {
                if ($candidate.Current.ClassName -eq '#32770' -and -not $candidate.Current.IsOffscreen -and
                    (Find-PackControl $candidate $Action)) {
                    return $candidate
                }
            }
        }
    }
    $editCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::Edit)
    $fileName = Wait-IslandValue "the file name field in $Action" {
        foreach ($element in $dialog.FindAll([Windows.Automation.TreeScope]::Descendants, $editCondition)) {
            $state = $element.Current
            if ($state.IsEnabled -and -not $state.IsOffscreen -and
                (([string]$state.Name).TrimEnd(':') -eq 'File name' -or $state.AutomationId -eq '1001')) {
                return $element
            }
        }
    }
    $fileNameValue = $fileName.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)
    if ($Action -eq 'Save') {
        # The native edit commits the name. Sending the full path as keystrokes can lose characters.
        $prefix = $Path.Substring(0, $Path.Length - 1)
        $fileName.SetFocus()
        $fileNameValue.SetValue($prefix)
        $null = Wait-IslandValue 'the focused Save field with the path prefix' {
            $fileName.Current.HasKeyboardFocus -and $fileNameValue.Current.Value -ceq $prefix
        }
        [AgentBroSmokeInput]::AppendCharacter($Path[$Path.Length - 1])
    } else {
        $fileNameValue.SetValue($Path)
    }
    try {
        $null = Wait-IslandValue "the entered $Action path" {
            $fileNameValue.Current.Value -ceq $Path
        }
    } catch {
        Write-Output "Expected $Action path: [$Path]"
        Write-Output "File name control value: [$($fileNameValue.Current.Value)]"
        throw
    }
    $submit = Wait-IslandValue "the enabled $Action button" {
        $button = Find-PackControl $dialog $Action
        if ($button -and $button.Current.IsEnabled) { $button }
    }
    Invoke-PackControl $submit
}

function Import-PackFixture([Windows.Automation.AutomationElement]$Settings, [string]$Path, [string]$Name) {
    $open = Wait-IslandValue 'the enabled Import Skill Pack action' {
        $button = Find-PackControl $Settings 'Import Skill Pack'
        if ($button -and $button.Current.IsEnabled) { $button }
    }
    Invoke-PackControl $open
    Set-PackDialogFile 'Open' $Path
    $confirm = Wait-IslandValue 'the enabled Skill Pack import preview' {
        $button = Find-PackControl $Settings 'Create Skill Pack'
        if ($button -and $button.Current.IsEnabled -and (Find-PackControl $Settings $Name)) { $button }
    }
    Invoke-PackControl $confirm
    $null = Wait-IslandValue 'the imported Skill Pack confirmation' {
        Find-PackControl $Settings "Imported $Name as a new Skill Pack."
    }
    Write-Output 'The installed app opened the selected manifest and created a Skill Pack.'
}

function Assert-CompactPackHeader([Windows.Automation.AutomationElement]$Settings, [string]$PackName) {
    $bounds = $Settings.Current.BoundingRectangle
    $scale = [AgentBroSmokeInput]::GetDpiForWindow([IntPtr]$Settings.Current.NativeWindowHandle) / 96.0
    if ($scale -le 0) { throw 'Could not read the Settings window DPI' }
    if ($bounds.Width / $scale -gt 1050) {
        $transform = $Settings.GetCurrentPattern([Windows.Automation.TransformPattern]::Pattern)
        $transform.Resize(1024 * $scale, $bounds.Height)
    }
    $heroClass = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ClassNameProperty, 'sm2__pack-detail-hero')
    $hero = Wait-IslandValue 'the compact Skill Pack header' {
        $header = $Settings.FindFirst([Windows.Automation.TreeScope]::Descendants, $heroClass)
        if ($header -and $header.Current.BoundingRectangle.Width / $scale -le 440) { $header }
    }
    $title = Wait-IslandValue 'the selected pack title' { Find-PackControl $hero $PackName }
    $titleBounds = $title.Current.BoundingRectangle
    $heroBounds = $hero.Current.BoundingRectangle
    if ($titleBounds.Width -lt ($heroBounds.Width * 0.55)) {
        throw "The pack title is squeezed into $([int]($titleBounds.Width / $scale))px"
    }
    $labels = @('应用到 Agent', '编辑', '更多操作')
    foreach ($label in $labels) {
        $control = Wait-IslandValue "the visible $label pack action" { Find-PackControl $hero $label }
        $actionBounds = $control.Current.BoundingRectangle
        if ($actionBounds.IsEmpty -or $actionBounds.Top -lt $titleBounds.Bottom -or
            $actionBounds.Left -lt $heroBounds.Left -or $actionBounds.Right -gt $heroBounds.Right) {
            throw "The $label action does not fit below the pack title ($([int]($heroBounds.Width / $scale))px header)"
        }
    }
    Write-Output "Skill Pack actions fit below the title in a $([int]($heroBounds.Width / $scale))px Windows header."
}

function Test-InstalledPackManifest(
    [Windows.Automation.AutomationElement]$Island,
    [string]$ImportPath,
    [string]$ExportPath
) {
    $expected = Get-Content -LiteralPath $ImportPath -Encoding UTF8 -Raw | ConvertFrom-Json
    $open = Wait-IslandValue 'the Island Settings button' { Find-PackControl $Island 'Settings' }
    Invoke-PackControl $open
    $settings = Wait-IslandValue 'the native Settings window' {
        foreach ($window in Get-IslandWindows) {
            if ((Find-PackControl $window 'Agent Skills') -or (Find-PackControl $window 'Start AgentBro')) {
                return $window
            }
        }
    }
    $welcome = Find-PackControl $settings 'Start AgentBro'
    if ($welcome) { Invoke-PackControl $welcome }
    $agentSkills = Wait-IslandValue 'the Agent Skills section' { Find-PackControl $settings 'Agent Skills' }
    Invoke-PackControl $agentSkills
    $packs = Wait-IslandValue 'the Skill Packs navigation item' { Find-PackControl $settings '技能包' }
    Invoke-PackControl $packs
    Write-Output 'Opened Skill Packs from the installed app.'

    Import-PackFixture $settings $ImportPath $expected.name
    Assert-CompactPackHeader $settings $expected.name
    $menu = Wait-IslandValue 'the new pack actions menu' { Find-PackControl $settings '更多操作' }
    Invoke-PackControl $menu
    $export = Wait-IslandValue 'the Export Skill Pack action' { Find-PackControl $settings 'Export Skill Pack' }
    Invoke-PackControl $export
    Set-PackDialogFile 'Save' $ExportPath
    $null = Wait-IslandValue 'the export completion notice' {
        Find-PackControl $settings 'Skill Pack exported.'
    }
    if (-not (Test-Path -LiteralPath $ExportPath -PathType Leaf)) {
        $fixtureRoot = Split-Path (Split-Path $ExportPath)
        $savedCandidates = Get-ChildItem -LiteralPath $fixtureRoot -Filter '*.agentbro-pack.json*' -File -Recurse
        Write-Output "Files in the fixture: $($savedCandidates.FullName -join ', ')"
        throw 'The export reported success but the file was absent from the requested path'
    }
    $saved = Get-Content -LiteralPath $ExportPath -Encoding UTF8 -Raw | ConvertFrom-Json
    $keys = @($saved.PSObject.Properties.Name | Sort-Object)
    if (($keys -join ',') -cne 'description,format,name,skillIds,tags,version' -or
        $saved.format -cne $expected.format -or $saved.version -ne $expected.version -or
        $saved.name -cne $expected.name -or $saved.description -cne $expected.description -or
        ($saved.tags -join ',') -cne ($expected.tags -join ',') -or
        ($saved.skillIds -join ',') -cne ($expected.skillIds -join ',')) {
        throw 'The saved file did not preserve the portable Skill Pack definition and member order'
    }
    Write-Output 'The native Save dialog wrote the expected Skill Pack definition and ordered members.'
    Import-PackFixture $settings $ExportPath $expected.name
    Write-Output 'The installed app completed the Skill Pack round trip through native Windows file dialogs and Unicode paths.'
}
