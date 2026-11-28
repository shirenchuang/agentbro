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
        Click-IslandElement -Description 'Island Settings button' -CenterAfterLayout -FindElement {
            Find-PackControl $Island 'Settings' ([Windows.Automation.ControlType]::Button)
        }
        $settings = Wait-IslandValue 'the native Settings window' $findSettings
    }
    $welcome = Find-PackControl $settings 'Start AgentBro'
    if ($welcome) {
        Click-IslandElement $null 'Start AgentBro' -FindElement {
            Find-PackControl $settings 'Start AgentBro' ([Windows.Automation.ControlType]::Button)
        } | Out-Host
        $null = Wait-IslandValue 'the completed first-run dialog to close' {
            -not (Find-PackControl $settings 'Set up your island')
        }
        $configPath = Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'agentbro\config.json'
        $persisted = Get-Content -LiteralPath $configPath -Encoding UTF8 -Raw | ConvertFrom-Json
        if (-not $persisted.analyticsConsentPromptCompleted -or -not $persisted.analyticsEnabled -or
            $persisted.islandSurfaceMode -cne 'island') {
            throw 'First-run setup closed before the selected surface and completion were saved.'
        }
        Write-Host 'First-run setup saved the selected surface and completion before opening Agent Skills.'
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

function Test-InstalledPackArchive(
    [Windows.Automation.AutomationElement]$Island,
    [string]$ManifestPath,
    [string]$ArchivePath
) {
    $manifest = Get-Content -LiteralPath $ManifestPath -Encoding UTF8 -Raw | ConvertFrom-Json
    $profileDirectory = if ($env:HOME) { $env:HOME } else { $env:USERPROFILE }
    $skillRoot = Join-Path $profileDirectory '.agentbro\skills'
    $conflictingId = $manifest.skillIds[0]
    $localId = $conflictingId.ToUpperInvariant()
    $copyId = "$conflictingId-restored"
    $missingIds = @($manifest.skillIds | Select-Object -Skip 1)
    $attachment = '附件 示例.bin'
    $attachmentPath = Join-Path $skillRoot "$($missingIds[0])\$attachment"
    [IO.File]::WriteAllBytes($attachmentPath, [byte[]]@(0, 13, 10, 31, 255, 128))
    $attachmentBytes = [Convert]::ToBase64String([IO.File]::ReadAllBytes($attachmentPath))
    $sourceBytes = @{}
    foreach ($id in $manifest.skillIds) {
        $sourceBytes[$id] = [Convert]::ToBase64String([IO.File]::ReadAllBytes((Join-Path $skillRoot "$id\SKILL.md")))
    }

    $settings = Open-InstalledAgentSkills $Island
    $packs = Wait-IslandValue 'the Skill Packs navigation item' { Find-PackControl $settings '技能包' }
    Invoke-PackControl $packs
    Import-PackFixture $settings $ManifestPath $manifest.name
    $menu = Wait-IslandValue 'the Pack actions menu for ZIP export' { Find-PackControl $settings '更多操作' }
    Invoke-PackControl $menu
    $export = Wait-IslandValue 'the Export with Skill files (ZIP) menu item' {
        Find-PackControl $settings 'Export with Skill files (ZIP)'
    }
    Invoke-PackControl $export
    Set-PackDialogFile 'Save' $ArchivePath
    $null = Wait-IslandValue 'the ZIP file at the requested Unicode Save path' {
        (Find-PackControl $settings 'Skill Pack and files exported.') -and
            (Test-Path -LiteralPath $ArchivePath -PathType Leaf)
    }

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($ArchivePath)
    try {
        $definition = $zip.GetEntry('agentbro-skill-pack.json')
        if (-not $definition -or -not $zip.GetEntry("skills/$($missingIds[0])/$attachment")) {
            throw 'The exported archive did not include the definition and the Unicode binary attachment.'
        }
        $reader = [IO.StreamReader]::new($definition.Open(), [Text.Encoding]::UTF8)
        try {
            $archived = $reader.ReadToEnd() | ConvertFrom-Json
        } finally {
            $reader.Dispose()
        }
        if ($archived.name -cne $manifest.name -or ($archived.skillIds -join ',') -cne ($manifest.skillIds -join ',')) {
            throw 'The ZIP did not preserve the intended Skill Pack and member order.'
        }
    } finally {
        $zip.Dispose()
    }
    Write-Output 'The installed app saved an ordered Skill Pack ZIP with a Unicode attachment.'

    foreach ($id in $manifest.skillIds) {
        Move-Item -LiteralPath (Join-Path $skillRoot $id) -Destination (Join-Path (Split-Path $ArchivePath) "sender-$id")
    }
    $localPath = Join-Path $skillRoot "$localId\SKILL.md"
    $null = [IO.Directory]::CreateDirectory((Split-Path $localPath))
    $localContent = "---`nname: $localId`n---`nKeep the local edit.`n"
    [IO.File]::WriteAllText($localPath, $localContent, [Text.UTF8Encoding]::new($false))

    $open = Wait-IslandValue 'the enabled Import Skill Pack action for the ZIP' {
        $button = Find-PackControl $settings 'Import Skill Pack'
        if ($button -and $button.Current.IsEnabled) { $button }
    }
    Invoke-PackControl $open
    Set-PackDialogFile 'Open' $ArchivePath
    $batchAction = "Restore $($missingIds.Count) missing Skills"
    $restore = Wait-IslandValue 'the batch restore action in the ZIP review' {
        $button = Find-PackControl $settings $batchAction
        if ($button -and $button.Current.IsEnabled -and (Test-PackVisibleText $settings "ID is occupied by $localId")) { $button }
    }
    $create = Find-PackControl $settings 'Create Skill Pack'
    if (-not $create -or $create.Current.IsEnabled -or (Test-Path -LiteralPath $attachmentPath)) {
        throw 'The ZIP preview must wait for confirmation before writing Skills or creating the Pack.'
    }
    Invoke-PackControl $restore
    $copy = Wait-IslandValue 'the ZIP copy action after restoring the missing members' {
        if (Find-PackControl $settings $batchAction) { return }
        $button = Find-PackControl $settings "Restore a copy of $conflictingId"
        if ($button -and $button.Current.IsEnabled) { $button }
    }
    Invoke-PackControl $copy
    $confirmCopy = Wait-IslandValue 'the enabled Restore copy action for the archived Skill' {
        $button = Find-PackControl $settings 'Restore copy'
        if ($button -and $button.Current.IsEnabled) { $button }
    }
    Invoke-PackControl $confirmCopy
    $create = Wait-IslandValue 'the enabled Create Skill Pack action after ZIP restore' {
        $button = Find-PackControl $settings 'Create Skill Pack'
        if ($button -and $button.Current.IsEnabled -and (Test-PackVisibleText $settings "Manifest ID $conflictingId")) { $button }
    }
    if ([IO.File]::ReadAllText($localPath) -cne $localContent -or
        [Convert]::ToBase64String([IO.File]::ReadAllBytes($attachmentPath)) -cne $attachmentBytes) {
        throw 'Restoring the ZIP changed the local conflict or the binary attachment.'
    }
    foreach ($id in $manifest.skillIds) {
        $restoredId = if ($id -ceq $conflictingId) { $copyId } else { $id }
        $bytes = [Convert]::ToBase64String([IO.File]::ReadAllBytes((Join-Path $skillRoot "$restoredId\SKILL.md")))
        if ($bytes -cne $sourceBytes[$id]) { throw "The restored files for $restoredId changed." }
    }
    Invoke-PackControl $create
    $null = Wait-IslandValue 'the imported ZIP Skill Pack confirmation' {
        Find-PackControl $settings "Imported $($manifest.name) as a new Skill Pack."
    }
    Write-Output 'The installed app opened the ZIP, restored the missing Skills and the case-colliding ID as a copy, and created the reviewed Pack.'
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
    $agentSkillRoot = Join-Path $env:CLAUDE_CONFIG_DIR 'skills'
    $defaultAgentSkillRoot = Join-Path $profileDirectory '.claude\skills'
    $createAction = 'Create and choose Agents'
    $batchAction = "Restore $($manifest.skillIds.Count - 1) missing Skills"
    $conflictingId = $manifest.skillIds[0]
    $copyId = "${conflictingId}-restored"
    $localPath = Join-Path $skillRoot "$conflictingId\SKILL.md"
    $localContent = "---`nname: $conflictingId`n---`nKeep the locally edited Skill.`n"
    New-Item -ItemType Directory -Path (Split-Path $localPath) -Force | Out-Null
    [IO.File]::WriteAllText($localPath, $localContent, [Text.UTF8Encoding]::new($false))
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
        $button = Find-PackControl $settings $batchAction
        if ($button -and $button.Current.IsEnabled) { $button }
    }
    $confirm = Find-PackControl $settings $createAction
    if (-not $confirm -or $confirm.Current.IsEnabled) {
        throw 'Import must wait until the missing Skills are explicitly restored.'
    }
    foreach ($skillId in $manifest.skillIds) {
        if ($skillId -cne $conflictingId -and (Test-Path -LiteralPath (Join-Path $skillRoot $skillId))) {
            throw "The review unexpectedly installed $skillId."
        }
    }
    Invoke-PackControl $restore
    $copy = Wait-IslandValue 'the copy action for the conflicting pinned Skill' {
        foreach ($id in ($manifest.skillIds | Select-Object -Skip 1)) {
            if (-not (Test-Path -LiteralPath (Join-Path $skillRoot "$id\SKILL.md") -PathType Leaf)) { return }
        }
        if (Find-PackControl $settings $batchAction) { return }
        $button = Find-PackControl $settings "Restore a copy of $conflictingId"
        if ($button -and $button.Current.IsEnabled) { $button }
    }
    Invoke-PackControl $copy
    $confirmCopy = Wait-IslandValue 'the enabled Restore copy action' {
        $button = Find-PackControl $settings 'Restore copy'
        if ($button -and $button.Current.IsEnabled) { $button }
    }
    $createBeforeCopy = Find-PackControl $settings $createAction
    if (-not $createBeforeCopy -or $createBeforeCopy.Current.IsEnabled) {
        throw 'The native review offered to create the Pack before resolving the collision.'
    }
    Invoke-PackControl $confirmCopy
    $confirm = Wait-IslandValue 'the enabled Create and choose Agents action after verifying restored Skills' {
        $button = Find-PackControl $settings $createAction
        if ($button -and $button.Current.IsEnabled -and (Test-PackVisibleText $settings "Manifest ID $conflictingId")) { $button }
    }
    if ([IO.File]::ReadAllText($localPath) -cne $localContent) {
        throw 'The restored copy changed the pre-existing Skill.'
    }
    $importedIds = @($copyId) + @($manifest.skillIds | Select-Object -Skip 1)
    foreach ($skillId in $manifest.skillIds) {
        $source = $manifest.skillSources.PSObject.Properties[$skillId].Value
        $destinationId = if ($skillId -ceq $conflictingId) { $copyId } else { $skillId }
        $content = Get-Content -LiteralPath (Join-Path $skillRoot "$destinationId\SKILL.md") -Encoding UTF8 -Raw
        if (-not $content.Contains("Pinned fixture commit at $($source.skillPath).") -or $content -cmatch 'Later branch content') {
            throw "The restored Skill $skillId did not come from the recorded commit and path."
        }
    }
    if (-not (Test-PackVisibleText $settings 'The original Skill Pack was applied to Claude Code, unavailable-fixture.')) {
        throw 'The import preview did not show the Agent recommendations.'
    }
    Invoke-PackControl $confirm
    $null = Wait-IslandValue 'the application preview enabled by the suggested Claude Code target' {
        $button = Find-PackControl $settings '预览影响'
        if ($button -and $button.Current.IsEnabled -and
            (Test-PackVisibleText $settings 'This environment cannot preselect unavailable-fixture.')) { $button }
    }
    $mode = Wait-IslandValue 'the distribution mode selector' {
        Find-PackControl $settings '分发方式' ([Windows.Automation.ControlType]::ComboBox)
    }
    $mode.SetFocus()
    $null = Wait-IslandValue 'focus in the distribution mode selector' { $mode.Current.HasKeyboardFocus }
    [AgentBroSmokeInput]::PressKeyInWindow([IntPtr]$settings.Current.NativeWindowHandle, 0x23)
    $preview = Wait-IslandValue 'the enabled application preview' {
        $button = Find-PackControl $settings '预览影响'
        if ($button -and $button.Current.IsEnabled) { $button }
    }
    Invoke-PackControl $preview
    $apply = Wait-IslandValue 'the enabled confirmation of the Claude Code distribution' {
        $button = Find-PackControl $settings '执行应用'
        if ($button -and $button.Current.IsEnabled -and (Test-PackVisibleText $settings 'Claude Code')) { $button }
    }
    foreach ($id in $importedIds) {
        if (-not (Test-PackVisibleText $settings (Join-Path $agentSkillRoot $id))) {
            throw "The review did not display $id under the configured Claude profile."
        }
        if (Test-Path -LiteralPath (Join-Path $agentSkillRoot $id)) {
            throw "The Skill $id was distributed before confirming the preview."
        }
    }
    Invoke-PackControl $apply
    $null = Wait-IslandValue 'the confirmed Skills applied to Claude Code' {
        $paths = @($importedIds | ForEach-Object { Join-Path $agentSkillRoot "$_\SKILL.md" })
        @($paths | Where-Object { -not (Test-Path -LiteralPath $_ -PathType Leaf) }).Count -eq 0 -and
            -not (Find-PackControl $settings '执行应用')
    }
    $null = Wait-IslandValue 'the refreshed Pack with one applied Agent' {
        Find-PackControl $settings '已应用 Agent 1' ([Windows.Automation.ControlType]::TabItem)
    }
    foreach ($id in $importedIds) {
        $target = Join-Path $agentSkillRoot $id
        if (Test-Path -LiteralPath (Join-Path $defaultAgentSkillRoot $id)) {
            throw "A copy of $id was written to the default Claude profile."
        }
        if ((Get-Item -LiteralPath $target).Attributes -band [IO.FileAttributes]::ReparsePoint) {
            throw "The confirmed copy of $id is a link."
        }
        if ([IO.File]::ReadAllText((Join-Path $target 'SKILL.md')) -cne
            [IO.File]::ReadAllText((Join-Path $skillRoot "$id\SKILL.md"))) {
            throw "Claude Code did not receive the reviewed contents of $id."
        }
    }
    $editedId = $importedIds[1]
    $editedPath = Join-Path $skillRoot "$editedId\SKILL.md"
    $editedContent = (Get-Content -LiteralPath $editedPath -Encoding UTF8 -Raw) -replace '\r\n', "`n"
    $editedContent += "`nEdited on this machine.`n"
    [IO.File]::WriteAllText($editedPath, $editedContent, [Text.UTF8Encoding]::new($false))
    $editedBytes = [Text.Encoding]::UTF8.GetBytes("SKILL.md`0$([Text.Encoding]::UTF8.GetByteCount($editedContent))`0$editedContent")
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        $editedHash = [BitConverter]::ToString($sha256.ComputeHash($editedBytes)).Replace('-', '').ToLowerInvariant()
    } finally {
        $sha256.Dispose()
    }
    $menu = Wait-IslandValue 'the new pinned pack actions menu' { Find-PackControl $settings '更多操作' }
    Invoke-PackControl $menu
    $export = Wait-IslandValue 'the Export for migration action' {
        Find-PackControl $settings 'Export for migration'
    }
    Invoke-PackControl $export
    Set-PackDialogFile 'Save' $ExportPath
    $null = Wait-IslandValue 'the exported pinned Skill Pack file' { Test-Path -LiteralPath $ExportPath -PathType Leaf }
    $saved = Get-Content -LiteralPath $ExportPath -Encoding UTF8 -Raw | ConvertFrom-Json
    if ($saved.version -ne 4 -or ($saved.skillIds -join ',') -cne ($importedIds -join ',') -or
        ($saved.suggestedAgentIds -join ',') -cne 'claude-code') {
        throw 'The migration export did not preserve the copied Skill order and the applied built-in Agent.'
    }
    foreach ($skillId in $manifest.skillIds) {
        $destinationId = if ($skillId -ceq $conflictingId) { $copyId } else { $skillId }
        $pinProperty = $saved.skillSources.PSObject.Properties[$destinationId]
        $hash = $saved.skillFingerprints.PSObject.Properties[$destinationId].Value
        if ($skillId -ceq $editedId) {
            if ($pinProperty -or $hash -cne $editedHash) {
                throw "The exported Pack did not preserve the edited fingerprint separately from the old source for $skillId."
            }
            continue
        }
        $expected = $manifest.skillSources.PSObject.Properties[$skillId].Value
        $pin = $pinProperty.Value
        if ($pin.repository -cne $expected.repository -or $pin.commit -cne $expected.commit -or
            $pin.skillPath -cne $expected.skillPath -or
            $hash -cne $manifest.skillFingerprints.PSObject.Properties[$skillId].Value) {
            throw "The exported Pack did not retain the verified GitHub source and fingerprint for $skillId."
        }
    }
    Write-Output 'The installed app preselected Claude Code, previewed and wrote the copies under CLAUDE_CONFIG_DIR only after confirmation, and re-exported the applied Agent and verified sources.'
}
