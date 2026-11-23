function Find-PackControl(
    [Windows.Automation.AutomationElement]$Root,
    [string]$Name,
    [Windows.Automation.ControlType]$ControlType
) {
    $condition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::NameProperty, $Name)
    foreach ($element in $Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition)) {
        if (-not $element.Current.IsOffscreen -and (-not $ControlType -or $element.Current.ControlType -eq $ControlType)) {
            return $element
        }
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
    Click-IslandElement $Element $Element.Current.Name -CenterAfterLayout | Out-Host
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

function Test-PackVisibleText([Windows.Automation.AutomationElement]$Settings, [string]$Expected) {
    if (Find-PackControl $Settings $Expected) { return $true }
    $document = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::Document)
    foreach ($element in $Settings.FindAll([Windows.Automation.TreeScope]::Descendants, $document)) {
        $text = $null
        if ($element.TryGetCurrentPattern([Windows.Automation.TextPattern]::Pattern, [ref]$text) -and
            $text.DocumentRange.FindText($Expected, $false, $false)) {
            return $true
        }
    }
    return $false
}

function Import-PackFixture(
    [Windows.Automation.AutomationElement]$Settings,
    [string]$Path,
    [string]$Name,
    [switch]$VerifyContents
) {
    $open = Wait-IslandValue 'the enabled Import Skill Pack action' {
        $button = Find-PackControl $Settings 'Import Skill Pack'
        if ($button -and $button.Current.IsEnabled) { $button }
    }
    Invoke-PackControl $open
    Set-PackDialogFile 'Open' $Path
    try {
        $confirm = Wait-IslandValue 'the enabled Create Skill Pack button in the import preview' {
            $button = Find-PackControl $Settings 'Create Skill Pack'
            if ($button -and $button.Current.IsEnabled -and (Find-PackControl $Settings $Name)) { $button }
        }
        if ($VerifyContents) {
            $null = Wait-IslandValue 'the version 2 preview with a matching content label' {
                (Find-PackControl $Settings 'Recheck Skills') -and (Test-PackVisibleText $Settings 'Content matches')
            }
        }
    } catch {
        foreach ($label in @($Name, 'Import Skill Pack', 'Create Skill Pack', 'Recheck Skills', 'Content matches')) {
            $element = Find-PackControl $Settings $label
            $found = if ($element) { "enabled=$($element.Current.IsEnabled)" } else { 'absent' }
            Write-Host "Skill Pack preview UI Automation [$label] $found"
        }
        throw
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

function Open-InstalledAgentSkills([Windows.Automation.AutomationElement]$Island) {
    foreach ($window in Get-IslandWindows) {
        if (Find-PackControl $window '技能包') { return $window }
    }
    $findSettings = {
        foreach ($window in Get-IslandWindows) {
            if ((Find-PackControl $window 'Agent Skills') -or (Find-PackControl $window 'Start AgentBro')) {
                return $window
            }
        }
    }
    $settings = & $findSettings
    if (-not $settings) {
        $islandWindow = Wait-IslandValue 'the visible native Island window before reopening Settings' { Find-VisibleIsland }
        Move-ToIslandHoverPoint $islandWindow.Handle $Island
        $null = Wait-IslandValue 'a native click on the current Island Settings button' {
            $button = Find-PackControl $Island 'Settings' ([Windows.Automation.ControlType]::Button)
            if (-not $button) { return }
            $rect = $button.Current.BoundingRectangle
            if ($rect.IsEmpty) { return }
            $point = [Windows.Point]::new([int]($rect.Left + $rect.Width / 2), [int]($rect.Top + $rect.Height / 2))
            if (-not [AgentBroSmokeInput]::SetCursorPos([int]$point.X, [int]$point.Y)) {
                throw 'Could not move the Windows cursor onto Island Settings'
            }
            Start-Sleep -Milliseconds 250
            if ([AgentBroSmokeInput]::ProcessAtCursor() -ne $AppProcessId) { return }
            $walker = [Windows.Automation.TreeWalker]::ControlViewWalker
            for ($hit = [Windows.Automation.AutomationElement]::FromPoint($point); $hit; $hit = $walker.GetParent($hit)) {
                if ([Windows.Automation.Automation]::Compare($button, $hit)) {
                    [AgentBroSmokeInput]::Click()
                    Write-Host "Clicked the current Island Settings button at ($($point.X), $($point.Y))."
                    return $true
                }
            }
        }
        $settings = Wait-IslandValue 'the native Settings window' $findSettings
    }
    $welcome = Find-PackControl $settings 'Start AgentBro'
    if ($welcome) {
        Click-IslandElement $null 'Start AgentBro' -FindElement {
            Find-PackControl $settings 'Start AgentBro' ([Windows.Automation.ControlType]::Button)
        } | Out-Host
    }
    Click-IslandElement $null 'Agent Skills section' -FindElement {
        if (-not (Find-PackControl $settings 'Start AgentBro')) {
            Find-PackControl $settings 'Agent Skills' ([Windows.Automation.ControlType]::Button)
        }
    } | Out-Host
    $settings
}

function Test-InstalledPackManifest(
    [Windows.Automation.AutomationElement]$Island,
    [string]$ImportPath,
    [string]$ExportPath
) {
    $expected = Get-Content -LiteralPath $ImportPath -Encoding UTF8 -Raw | ConvertFrom-Json
    $settings = Open-InstalledAgentSkills $Island
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
    if (($keys -join ',') -cne 'description,format,name,skillFingerprints,skillIds,tags,version' -or
        $saved.format -cne $expected.format -or $saved.version -ne 2 -or
        $saved.name -cne $expected.name -or $saved.description -cne $expected.description -or
        ($saved.tags -join ',') -cne ($expected.tags -join ',') -or
        ($saved.skillIds -join ',') -cne ($expected.skillIds -join ',')) {
        throw 'The saved file did not preserve the portable Skill Pack definition and member order'
    }
    foreach ($skillId in $expected.skillIds) {
        $fingerprint = $saved.skillFingerprints.PSObject.Properties[$skillId].Value
        if ($fingerprint -cnotmatch '^[a-f0-9]{64}$') {
            throw "The export omitted the portable content fingerprint for $skillId"
        }
    }
    Write-Output 'The native Save dialog wrote the ordered Skill Pack and both content fingerprints.'
    Import-PackFixture $settings $ExportPath $expected.name -VerifyContents
    Write-Output 'The installed app completed the Skill Pack round trip through native Windows file dialogs and Unicode paths.'
}

function Open-LinkedPackFixture(
    [Windows.Automation.AutomationElement]$Settings,
    [string]$Link,
    [string]$Name
) {
    $open = Wait-IslandValue 'the Import from link button' {
        $button = Find-PackControl $Settings 'Import from link'
        if ($button -and $button.Current.IsEnabled) { $button }
    }
    Invoke-PackControl $open | Out-Host
    $field = Wait-IslandValue 'the GitHub link field' {
        Find-PackControl $Settings 'GitHub link' ([Windows.Automation.ControlType]::Edit)
    }
    Click-IslandElement $field 'GitHub link field' | Out-Host
    if (-not [AgentBroSmokeInput]::PasteInWindow([IntPtr]$Settings.Current.NativeWindowHandle)) {
        throw 'AgentBro Settings lost focus before pasting the GitHub link'
    }
    $review = Wait-IslandValue 'the pasted URL and enabled Review Skill Pack action' {
        $inputValue = $field.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)
        $button = Find-PackControl $Settings 'Review Skill Pack'
        if ($inputValue.Current.Value -ceq $Link -and $button -and $button.Current.IsEnabled) { $button }
    }
    Invoke-PackControl $review | Out-Host
    Wait-IslandValue 'the downloaded Skill Pack preview' {
        $button = Find-PackControl $Settings 'Create Skill Pack'
        if ($button -and $button.Current.IsEnabled -and (Test-PackVisibleText $Settings $Name)) { $button }
    } -TimeoutSeconds 25
}

function Test-InstalledPackManifestLink(
    [Windows.Automation.AutomationElement]$Island,
    [string]$Link
) {
    $fixturePath = Join-Path $PSScriptRoot 'fixtures\github-skill-pack.agentbro-pack.json'
    $manifest = Get-Content -LiteralPath $fixturePath -Raw -Encoding utf8 | ConvertFrom-Json
    $source = $Link -replace '^https://github\.com/([^/]+/[^/]+)/blob/', 'https://raw.githubusercontent.com/$1/'
    $settings = Open-InstalledAgentSkills $Island
    $packs = Wait-IslandValue 'the Skill Packs navigation item' { Find-PackControl $settings '技能包' }
    Invoke-PackControl $packs

    $null = Open-LinkedPackFixture $settings $Link $manifest.name
    $null = Wait-IslandValue 'the downloaded source URL' { Test-PackVisibleText $settings $source }
    foreach ($skillId in $manifest.skillIds) {
        $null = Wait-IslandValue "the downloaded Skill Pack member $skillId" { Test-PackVisibleText $settings $skillId }
    }
    $cancel = Wait-IslandValue 'the Cancel action on the downloaded preview' { Find-PackControl $settings 'Cancel' }
    Invoke-PackControl $cancel
    $null = Wait-IslandValue 'the preview closing without creating the pack' {
        -not (Find-PackControl $settings 'Create Skill Pack') -and -not (Test-PackVisibleText $settings $manifest.name)
    }
    Write-Output 'The installed app showed the downloaded source and member. Cancel left the pack uncreated.'

    $confirm = Open-LinkedPackFixture $settings $Link $manifest.name
    Invoke-PackControl $confirm
    $null = Wait-IslandValue 'the imported GitHub Skill Pack notice' {
        Find-PackControl $settings "Imported $($manifest.name) as a new Skill Pack."
    }
    Write-Output 'The installed app imported the public GitHub Skill Pack after explicit confirmation.'
}

function Test-InstalledPinnedPackManifest(
    [Windows.Automation.AutomationElement]$Island,
    [string]$ImportPath,
    [string]$ExportPath
) {
    $manifest = Get-Content -LiteralPath $ImportPath -Encoding UTF8 -Raw | ConvertFrom-Json
    $profileDirectory = if ($env:HOME) { $env:HOME } else { $env:USERPROFILE }
    $skillRoot = Join-Path $profileDirectory '.agentbro\skills'
    $settings = Open-InstalledAgentSkills $Island
    $packs = Wait-IslandValue 'the Skill Packs navigation item' { Find-PackControl $settings '技能包' }
    Invoke-PackControl $packs
    $open = Wait-IslandValue 'the enabled Import Skill Pack action' {
        $button = Find-PackControl $settings 'Import Skill Pack'
        if ($button -and $button.Current.IsEnabled) { $button }
    }
    Invoke-PackControl $open
    Set-PackDialogFile 'Open' $ImportPath
    $restore = Wait-IslandValue 'the batch Restore action for the missing pinned Skills' {
        $button = Find-PackControl $settings "Restore $($manifest.skillIds.Count) missing Skills"
        if ($button -and $button.Current.IsEnabled) { $button }
    }
    $confirm = Find-PackControl $settings 'Create Skill Pack'
    if (-not $confirm -or $confirm.Current.IsEnabled) {
        throw 'Import must wait until the missing Skills are explicitly restored.'
    }
    foreach ($skillId in $manifest.skillIds) {
        if (Test-Path -LiteralPath (Join-Path $skillRoot $skillId)) {
            throw "The review unexpectedly installed $skillId."
        }
    }
    Invoke-PackControl $restore
    $confirm = Wait-IslandValue 'the enabled Create Skill Pack action after verifying the restored Skills' {
        $button = Find-PackControl $settings 'Create Skill Pack'
        if ($button -and $button.Current.IsEnabled -and (Test-PackVisibleText $settings 'Content matches')) { $button }
    }
    foreach ($skillId in $manifest.skillIds) {
        $source = $manifest.skillSources.PSObject.Properties[$skillId].Value
        $content = Get-Content -LiteralPath (Join-Path $skillRoot "$skillId\SKILL.md") -Encoding UTF8 -Raw
        if (-not $content.Contains("Pinned fixture commit at $($source.skillPath).") -or $content -cmatch 'Later branch content') {
            throw "The restored Skill $skillId did not come from the recorded commit and path."
        }
    }
    Invoke-PackControl $confirm
    $null = Wait-IslandValue 'the imported pinned Skill Pack notice' {
        Find-PackControl $settings "Imported $($manifest.name) as a new Skill Pack."
    }
    $menu = Wait-IslandValue 'the new pinned pack actions menu' { Find-PackControl $settings '更多操作' }
    Invoke-PackControl $menu
    $export = Wait-IslandValue 'the Export with GitHub sources action' {
        Find-PackControl $settings 'Export with GitHub sources'
    }
    Invoke-PackControl $export
    Set-PackDialogFile 'Save' $ExportPath
    $null = Wait-IslandValue 'the exported pinned Skill Pack file' { Test-Path -LiteralPath $ExportPath -PathType Leaf }
    $saved = Get-Content -LiteralPath $ExportPath -Encoding UTF8 -Raw | ConvertFrom-Json
    if ($saved.version -ne 3 -or ($saved.skillIds -join ',') -cne ($manifest.skillIds -join ',')) {
        throw 'The exported Pack did not retain the version and member order.'
    }
    foreach ($skillId in $manifest.skillIds) {
        $expected = $manifest.skillSources.PSObject.Properties[$skillId].Value
        $pin = $saved.skillSources.PSObject.Properties[$skillId].Value
        if ($pin.repository -cne $expected.repository -or $pin.commit -cne $expected.commit -or
            $pin.skillPath -cne $expected.skillPath -or
            $saved.skillFingerprints.PSObject.Properties[$skillId].Value -cne $manifest.skillFingerprints.PSObject.Properties[$skillId].Value) {
            throw "The exported Pack did not retain the verified GitHub source and fingerprint for $skillId."
        }
    }
    Write-Output 'The installed app batch restored both Skills from the recorded Git commit, matched their fingerprints, and exported both pinned sources through native dialogs.'
}
