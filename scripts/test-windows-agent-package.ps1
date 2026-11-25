function Invoke-ManagedAgentButton([Windows.Automation.AutomationElement]$Settings, [string]$Name) {
    $null = Wait-IslandValue "invoking $Name in Agent Management" {
        try {
            $button = Find-PackControl $Settings $Name ([Windows.Automation.ControlType]::Button)
            $invoke = $null
            if ($button -and $button.Current.IsEnabled -and
                $button.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$invoke)) {
                $invoke.Invoke()
                return $true
            }
        } catch {
            if ($_.Exception -isnot [Windows.Automation.ElementNotAvailableException] -and
                $_.Exception.InnerException -isnot [Windows.Automation.ElementNotAvailableException]) { throw }
        }
    }
}

function Open-ManagedAgent(
    [Windows.Automation.AutomationElement]$Island,
    [string]$AgentName,
    [switch]$RefreshPrograms,
    [string]$SearchQuery = $AgentName
) {
    $settings = Open-InstalledAgentSkills $Island
    $agents = Wait-IslandValue 'the Agent Management navigation item' { Find-PackControl $settings 'Agent 管理' }
    Invoke-PackControl $agents | Out-Host
    if ($RefreshPrograms) {
        $refresh = Wait-IslandValue 'the enabled Agent version refresh' {
            $button = Find-PackControl $settings '获取版本'
            if ($button -and $button.Current.IsEnabled) { $button }
        }
        Invoke-PackControl $refresh
    }
    $search = Wait-IslandValue 'the Agent search' {
        Find-PackControl $settings 'Search Agents' ([Windows.Automation.ControlType]::Edit)
    }
    Click-IslandElement $search 'Agent search' | Out-Host
    $handle = [IntPtr]$settings.Current.NativeWindowHandle
    $searchValue = $search.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)
    if ($searchValue.Current.Value.Length -gt 0) {
        $keys = @(0x23) + @(0x08) * $searchValue.Current.Value.Length
        foreach ($key in $keys) {
            [AgentBroSmokeInput]::PressKeyInWindow($handle, $key)
        }
    }
    foreach ($character in $SearchQuery.ToCharArray()) {
        if ([AgentBroSmokeInput]::GetForegroundWindow() -ne $handle) {
            throw 'Agent Management lost focus before entering the Agent search'
        }
        [AgentBroSmokeInput]::AppendCharacter($character)
    }
    Invoke-ManagedAgentButton $settings $AgentName
    Write-Host "Selected $AgentName through Windows UI Automation."
    $settings
}

function Test-InstalledAgentPackageAcrossSettingsWindows(
    [Windows.Automation.AutomationElement]$Settings,
    [string]$FixtureLauncher
) {
    $fixtureStage = Split-Path $env:AGENTBRO_PACKAGE_CONSOLE_RESULT
    Copy-Item -LiteralPath (Join-Path $fixtureStage 'claude.cmd.fixture') -Destination $FixtureLauncher
    '{"name":"@anthropic-ai/claude-code","version":"2.0.0","bin":{"claude":"./cli.js"}}' |
        Set-Content -LiteralPath $env:AGENTBRO_PACKAGE_FIXTURE_MANIFEST -Encoding ascii
    Remove-Item -LiteralPath @(
        $env:AGENTBRO_PACKAGE_CONSOLE_RESULT,
        $env:AGENTBRO_PACKAGE_FIXTURE_UPDATED,
        $env:AGENTBRO_PACKAGE_FIXTURE_RESUME
    )
    Click-IslandElement $null 'refresh Agent versions before reopening Settings' -FindElement {
        $button = Find-PackControl $Settings '获取版本' ([Windows.Automation.ControlType]::Button)
        if ($button -and $button.Current.IsEnabled) { $button }
    }
    $restored = Wait-IslandValue 'the restored Claude Code fixture in Installed Agents' {
        $agent = Find-IslandButton $Settings { param($Name) $Name -eq 'Claude Code' }
        if ($agent) {
            $classes = $agent.Current.ClassName -split '\s+'
            if ($classes -contains 'sm2-sidebar__subitem' -and $classes -notcontains 'sm2-sidebar__subitem--muted') { $agent }
        }
    }
    Click-IslandElement $restored 'select the restored Claude Code fixture' -CenterAfterLayout
    $null = New-Item -ItemType File -Path $env:AGENTBRO_PACKAGE_FIXTURE_CHANGED_VERSION -Force
    Click-IslandElement $null 'start a package update before closing Settings' -FindElement {
        $button = Find-PackControl $Settings '更新此 Agent' ([Windows.Automation.ControlType]::Button)
        if ($button -and $button.Current.IsEnabled) { $button }
    }
    try {
        $null = Wait-IslandValue 'the package command holding across Settings close' {
            Test-Path -LiteralPath $env:AGENTBRO_PACKAGE_CONSOLE_RESULT -PathType Leaf
        }
        $handle = $Settings.Current.NativeWindowHandle
        $Settings.GetCurrentPattern([Windows.Automation.WindowPattern]::Pattern).Close()
        $null = Wait-IslandValue 'destruction of Settings during the package command' {
            -not (Get-IslandWindows | Where-Object { $_.Current.NativeWindowHandle -eq $handle })
        }

        $island = Wait-IslandValue 'the Island after closing Settings' { Find-VisibleIsland }
        Move-ToIslandHoverPoint $island.Handle $island.Element
        $null = Wait-IslandValue 'the expanded Island controls after closing Settings' {
            Find-IslandButton $island.Element { param($Name) $Name -eq 'Quit AgentBro' }
        }
        $reopened = Open-ManagedAgent $island.Element 'Claude Code'
        $null = Wait-IslandValue 'package output restored in the new Settings window' {
            Find-PackControl $reopened '* $ npm install -g @anthropic-ai/claude-code@latest'
        }
        $null = Wait-IslandValue 'the old version in the new Settings window before the update finishes' {
            Test-PackVisibleText $reopened '当前版本 2.0.0'
        }
        Write-Output 'Agent Management restored the package command output after recreating Settings.'
        Click-IslandElement $null 'try another update from the new Settings window' -FindElement {
            $button = Find-PackControl $reopened '更新此 Agent' ([Windows.Automation.ControlType]::Button)
            if ($button -and $button.Current.IsEnabled) { $button }
        }
        $null = Wait-IslandValue 'the busy result from the native package command' {
            if (Test-Path -LiteralPath $env:AGENTBRO_PACKAGE_FIXTURE_UPDATED) {
                throw 'The reopened Settings window started another package update while the first was still running'
            }
            Test-PackVisibleText $reopened 'Another Agent package operation is running.'
        }
        Write-Output 'The native app rejected an overlapping package update from the reopened Settings window.'
    } finally {
        $null = New-Item -ItemType File -Path $env:AGENTBRO_PACKAGE_FIXTURE_RESUME -Force
    }
    $null = Wait-IslandValue 'package stdout and stderr in the new Settings window' {
        (Find-PackControl $reopened '> agentbro-package-fixture-stdout') -and
            (Find-PackControl $reopened '! agentbro-package-fixture-stderr')
    }
    $null = Wait-IslandValue 'the new installed version in the recreated Settings window without a manual refresh' {
        (Test-Path -LiteralPath $env:AGENTBRO_PACKAGE_FIXTURE_UPDATED -PathType Leaf) -and
            (Test-PackVisibleText $reopened '当前版本 999.0.0')
    }
    Write-Output 'The recreated Settings window refreshed the installed version when the package command finished.'
    Test-InstalledCustomAgentForm $reopened
}

function Test-InstalledCustomAgentForm([Windows.Automation.AutomationElement]$Settings) {
    $title = 'Add Claude Code Instance...'
    $buttonType = [Windows.Automation.ControlType]::Button
    $editType = [Windows.Automation.ControlType]::Edit
    $handle = [IntPtr]$Settings.Current.NativeWindowHandle
    $root = Join-Path (Split-Path $env:AGENTBRO_PACKAGE_CONSOLE_RESULT) 'custom-claude'
    $null = New-Item -ItemType Directory -Path $root
    $nameFocused = {
        $focused = [Windows.Automation.AutomationElement]::FocusedElement
        $focused -and $focused.Current.Name -eq '显示名称' -and $focused.Current.HasKeyboardFocus
    }
    $enterRootAndSave = {
        Click-IslandElement $null 'enter the custom configuration root' -FindElement {
            Find-PackControl $Settings '配置根目录' $editType
        }
        foreach ($character in $root.ToCharArray()) {
            if ([AgentBroSmokeInput]::GetForegroundWindow() -ne $handle) {
                throw 'The custom Agent form lost focus while entering the configuration root'
            }
            [AgentBroSmokeInput]::AppendCharacter($character)
            Start-Sleep -Milliseconds 30
        }
        $null = Wait-IslandValue 'the full configuration root in the custom Agent form' {
            $configRoot = Find-PackControl $Settings '配置根目录' $editType
            $configRoot -and $configRoot.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).Current.Value -ceq $root
        }
        $save = Wait-IslandValue 'the custom Agent save action' { Find-PackControl $Settings '保存 Agent' $buttonType }
        Invoke-PackControl $save
    }
    Click-IslandElement $null 'open the custom Agent form' -FindElement {
        Find-PackControl $Settings $title $buttonType
    }
    $null = Wait-IslandValue 'initial focus in the custom Agent display name' $nameFocused
    [AgentBroSmokeInput]::AppendCharacter('!')
    & $enterRootAndSave
    $null = Wait-IslandValue 'the custom Agent rejection with the draft still in the form' {
        $name = Find-PackControl $Settings '显示名称' $editType
        $configRoot = Find-PackControl $Settings '配置根目录' $editType
        $name -and $configRoot -and (Test-PackVisibleText $Settings 'Agent ID cannot be empty') -and
            $name.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).Current.Value -ceq '!' -and
            $configRoot.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).Current.Value -ceq $root
    }
    [AgentBroSmokeInput]::PressKeyInWindow($handle, 0x1B)
    $null = Wait-IslandValue 'focus restored to Add Claude Code Instance after Escape' {
        $focused = [Windows.Automation.AutomationElement]::FocusedElement
        $focused -and $focused.Current.Name -eq $title -and $focused.Current.HasKeyboardFocus -and
            -not (Find-PackControl $Settings '保存 Agent' $buttonType)
    }
    Write-Output 'The installed custom Agent form displayed the save error, retained both fields and restored focus after Escape.'
    [AgentBroSmokeInput]::PressKeyInWindow($handle, 0x20)
    $null = Wait-IslandValue 'the reopened custom Agent display name' $nameFocused
    $displayName = '研发助手'
    foreach ($character in $displayName.ToCharArray()) {
        [AgentBroSmokeInput]::AppendCharacter($character)
    }
    & $enterRootAndSave
    $null = Wait-IslandValue 'the saved custom Agent with a Chinese-only display name' {
        -not (Find-PackControl $Settings '保存 Agent' $buttonType) -and
            (Test-PackVisibleText $Settings $displayName) -and
            (Test-Path -LiteralPath (Join-Path $root 'settings.json') -PathType Leaf)
    }
    Write-Output 'The installed app saved and displayed the Chinese-only custom Agent and created its Hook settings.'
}

function Test-InstalledAgentPackageConsole([Windows.Automation.AutomationElement]$Island) {
    $settings = Open-ManagedAgent $Island 'Claude Code'
    $fixtureStage = Split-Path $env:AGENTBRO_PACKAGE_CONSOLE_RESULT
    $fixtureLauncher = Join-Path $fixtureStage 'bin\claude.cmd'
    $fallbackLauncher = Join-Path $fixtureStage 'fallback bin\claude.exe'
    Click-IslandElement $null 'Paths & settings tab' -FindElement {
        Find-PackControl $settings 'Paths & settings' ([Windows.Automation.ControlType]::Button)
    }
    $heading = Wait-IslandValue 'the Paths & settings panel' { Find-PackControl $settings 'Local resources and configuration' }
    $bounds = $heading.Current.BoundingRectangle
    if (-not [AgentBroSmokeInput]::SetCursorPos([int]($bounds.Left + $bounds.Width / 2), [int]($bounds.Top + $bounds.Height / 2)) -or
        [AgentBroSmokeInput]::ProcessAtCursor() -ne $AppProcessId) {
        throw 'AgentBro does not own the Paths & settings scroll point'
    }
    [AgentBroSmokeInput]::ScrollWheel(-240)
    $detectedLauncher = Wait-IslandValue 'the Claude launcher in Paths & settings' {
        foreach ($candidate in @($fixtureLauncher, $fallbackLauncher)) {
            if (Test-PackVisibleText $settings $candidate) { return $candidate }
        }
    }
    if ($detectedLauncher -ne $fixtureLauncher) {
        throw "Agent Management selected the later executable: $detectedLauncher"
    }
    Write-Output 'Agent Management displayed the earlier Claude command shim while the later executable was present.'
    [AgentBroSmokeInput]::ScrollWheel(240)
    Remove-Item -LiteralPath $fallbackLauncher
    Click-IslandElement $null 'Agent overview tab' -FindElement {
        Find-PackControl $settings '概览' ([Windows.Automation.ControlType]::Button)
    }

    $null = Wait-IslandValue 'the installed fixture version' {
        Test-PackVisibleText $settings '当前版本 2.0.0'
    }
    Write-Output 'Agent Management reported the selected launcher version 2.0.0 with a separate npm default installation at 9.0.0.'
    if (Find-IslandButton $settings { param($Name) $Name -eq '更新此 Agent' }) {
        throw 'Agent Management offered an update while the registry tag was lower than the installed version'
    }
    $noUpdate = Find-IslandButton $settings { param($Name) $Name -eq '无需更新' }
    if (-not $noUpdate -or $noUpdate.Current.IsEnabled) {
        throw 'Agent Management did not disable updates for the lower registry tag'
    }
    Write-Output 'Agent Management kept 2.0.0 installed when the registry offered 1.0.0.'
    $null = New-Item -ItemType File -Path $env:AGENTBRO_PACKAGE_FIXTURE_NEW_TAG
    $fetchVersions = Wait-IslandValue 'the enabled version refresh after advancing the registry tag' {
        $button = Find-PackControl $settings '获取版本'
        if ($button -and $button.Current.IsEnabled) { $button }
    }
    Invoke-PackControl $fetchVersions
    $null = Wait-IslandValue 'the fixture Claude Code version' {
        Find-PackControl $settings '可更新到 999.0.0'
    }
    $null = Wait-IslandValue 'the detected CLI under Installed Agents' {
        $agent = Find-IslandButton $settings { param($Name) $Name -eq 'Claude Code' }
        if ($agent) {
            $classes = $agent.Current.ClassName -split '\s+'
            $classes -contains 'sm2-sidebar__subitem' -and $classes -notcontains 'sm2-sidebar__subitem--muted'
        }
    }
    Write-Output 'The sidebar grouped the detected Claude Code CLI under Installed Agents.'
    $update = Wait-IslandValue 'the Claude Code update action' {
        Find-IslandButton $settings { param($Name) $Name -eq '更新此 Agent' }
    }
    $npm = Join-Path $fixtureStage 'bin\npm.cmd'
    $savedNpm = Join-Path $fixtureStage 'npm.saved.cmd'
    $invalidNpm = Join-Path $fixtureStage 'bin\npm.exe'
    Move-Item -LiteralPath $npm -Destination $savedNpm
    try {
        Set-Content -LiteralPath $invalidNpm -Value 'Not a Windows executable' -Encoding ascii
        Invoke-PackControl $update
        $null = Wait-IslandValue 'the Windows package launch error' {
            (Test-PackVisibleText $settings '(os error 193)') -or
                (Test-PackVisibleText $settings '(os error 216)')
        }
        Write-Output 'Agent Management received the Windows package launch error.'
        $null = Wait-IslandValue 'the package launch error in the retained transcript' {
            Test-PackVisibleText $settings '! Could not start package command'
        }
        Write-Output 'The package transcript retained the terminal launch error.'
    } finally {
        Remove-Item -LiteralPath $invalidNpm -ErrorAction SilentlyContinue
        Move-Item -LiteralPath $savedNpm -Destination $npm
    }
    $refresh = Wait-IslandValue 'the enabled version refresh after restoring npm' {
        $button = Find-PackControl $settings '获取版本'
        if ($button -and $button.Current.IsEnabled) { $button }
    }
    Invoke-PackControl $refresh
    $update = Wait-IslandValue 'the enabled update after restoring npm' {
        $button = Find-IslandButton $settings { param($Name) $Name -eq '更新此 Agent' }
        if ($button -and $button.Current.IsEnabled) { $button }
    }
    Invoke-PackControl $update
    try {
        $null = Wait-IslandValue 'the running fixture update' {
            Test-Path -LiteralPath $env:AGENTBRO_PACKAGE_CONSOLE_RESULT -PathType Leaf
        }
        Click-IslandElement $null 'Projects during the Agent update' -FindElement {
            Find-PackControl $settings 'Projects' ([Windows.Automation.ControlType]::Button)
        }
        $null = Wait-IslandValue 'the Projects page during the Agent update' {
            Find-PackControl $settings '项目'
        }
        Click-IslandElement $null 'return to Agent Management during the update' -FindElement {
            Find-PackControl $settings 'Agent 管理' ([Windows.Automation.ControlType]::Button)
        }
        $running = Wait-IslandValue 'the disabled package action after returning to Agent Management' {
            Find-IslandButton $settings { param($Name) $Name -in @('正在更新', '更新此 Agent') }
        }
        if ($running.Current.Name -ne '正在更新' -or $running.Current.IsEnabled) {
            throw 'Agent Management offered another update while the existing package command was still running'
        }
        Write-Output 'Agent Management kept the running package update disabled after returning from Projects.'
    } finally {
        $null = New-Item -ItemType File -Path $env:AGENTBRO_PACKAGE_FIXTURE_RESUME -Force
    }
    $null = Wait-IslandValue 'the unchanged installed version after the first update' {
        (Test-Path -LiteralPath $env:AGENTBRO_PACKAGE_CONSOLE_RESULT) -and
            (Test-PackVisibleText $settings 'The update command completed. Claude Code is still at 2.0.0.')
    }
    if (Find-PackControl $settings 'Agent 更新完成') {
        throw 'The installed app reported an update without a detected version change'
    }
    Write-Output 'The first update exited successfully and Agent Management still reported version 2.0.0.'
    $retry = Wait-IslandValue 'the enabled retry for the package update' {
        $button = Find-IslandButton $settings { param($Name) $Name -eq '更新此 Agent' }
        if ($button -and $button.Current.IsEnabled) { $button }
    }
    Invoke-PackControl $retry
    $null = Wait-IslandValue 'the completed fixture package update' {
        (Test-Path -LiteralPath $env:AGENTBRO_PACKAGE_FIXTURE_UPDATED) -and
            (Find-PackControl $settings '999.0.0') -and (Find-PackControl $settings 'Agent 更新完成')
    }
    Write-Output 'Agent Management confirmed the retry after detecting version 999.0.0.'
    $defaultManifest = Join-Path $env:npm_config_prefix 'node_modules\@anthropic-ai\claude-code\package.json'
    if ((Get-Content -LiteralPath $defaultManifest -Raw | ConvertFrom-Json).version -ne '9.0.0') {
        throw 'The npm update modified the separate default installation'
    }
    $report = Get-Content -LiteralPath $env:AGENTBRO_PACKAGE_CONSOLE_RESULT -Raw -Encoding utf8 | ConvertFrom-Json
    Write-Output "Installed package console probe: $($report | ConvertTo-Json -Compress)"
    $null = Wait-IslandValue 'stdout and stderr in Agent Management' {
        (Find-PackControl $settings '> agentbro-package-fixture-stdout') -and
            (Find-PackControl $settings '! agentbro-package-fixture-stderr')
    }
    Write-Output 'The installed Agent Management ran the fixture update and displayed both output streams.'
    if ($report.shellConsoleVisible -or $report.toolConsoleVisible) {
        throw 'The package update opened a visible Windows console'
    }

    $fixtureSkills = Join-Path $env:CLAUDE_CONFIG_DIR 'skills\uninstall-fixture\SKILL.md'
    $fixtureSettings = Join-Path $env:CLAUDE_CONFIG_DIR 'settings.json'
    $reviewedSettings = Get-Content -LiteralPath $fixtureSettings -Raw
    if ($reviewedSettings -notmatch 'agentbro-bridge') {
        throw 'The uninstall fixture must have the installed AgentBro Hook'
    }
    $uninstall = Wait-IslandValue 'the uninstall button for the installed Agent' {
        $button = Find-PackControl $settings '卸载 Agent'
        if ($button -and $button.Current.IsEnabled) { $button }
    }
    Click-IslandElement $uninstall 'open the Agent uninstall confirmation'
    $handle = [IntPtr]$settings.Current.NativeWindowHandle
    $null = Wait-IslandValue 'keyboard focus on Cancel in the uninstall confirmation' {
        $focused = [Windows.Automation.AutomationElement]::FocusedElement
        $focused -and $focused.Current.Name -eq '取消' -and $focused.Current.HasKeyboardFocus
    }
    [AgentBroSmokeInput]::PressKeyInWindow($handle, 0x1B)
    $null = Wait-IslandValue 'focus restored to the uninstall button after Escape' {
        $focused = [Windows.Automation.AutomationElement]::FocusedElement
        $focused -and $focused.Current.Name -eq '卸载 Agent' -and $focused.Current.HasKeyboardFocus -and
            -not (Find-PackControl $settings '确认卸载')
    }
    Write-Output 'The uninstall confirmation focused Cancel and restored focus after native Escape.'
    [AgentBroSmokeInput]::PressKeyInWindow($handle, 0x20)
    foreach ($issue in @(
        'The uninstall command failed. uninstall failed for claude-code (exit code: 1)',
        'AgentBro still detects Claude Code.'
    )) {
        $confirm = Wait-IslandValue 'the uninstall confirmation reviewing unmanaged Skills and the Hook' {
            $button = Find-PackControl $settings '确认卸载'
            if ($button -and (Test-PackVisibleText $settings '未管理 Skills') -and
                (Test-PackVisibleText $settings '移除 AgentBro Hook')) { $button }
        }
        Invoke-PackControl $confirm
        $null = Wait-IslandValue "the preserved resources message after $issue" {
            (Test-PackVisibleText $settings $issue) -and
                (Test-PackVisibleText $settings 'AgentBro kept the Skills and Hooks.')
        }
        if (-not (Test-Path -LiteralPath $fixtureLauncher -PathType Leaf) -or
            -not (Test-Path -LiteralPath $fixtureSkills -PathType Leaf) -or
            (Get-Content -LiteralPath $fixtureSettings -Raw) -cne $reviewedSettings) {
            throw "The unsuccessful uninstall changed the launcher, unmanaged Skill, or Hook settings"
        }
        Write-Output "Verified that the launcher, unmanaged Skill, and Hook settings were preserved. $issue"
        $retry = Wait-IslandValue 'the enabled uninstall after preserving the Agent resources' {
            $button = Find-PackControl $settings '卸载 Agent'
            if ($button -and $button.Current.IsEnabled) { $button }
        }
        Invoke-PackControl $retry
    }

    $confirm = Wait-IslandValue 'the successful uninstall confirmation' { Find-PackControl $settings '确认卸载' }
    Invoke-PackControl $confirm
    $null = Wait-IslandValue 'the confirmed uninstall of the fixture launcher' {
        -not (Test-Path -LiteralPath $fixtureLauncher -PathType Leaf) -and
            (Test-PackVisibleText $settings 'Agent「Claude Code」已卸载')
    }
    $cleanedSettings = Get-Content -LiteralPath $fixtureSettings -Raw
    if (-not (Test-Path -LiteralPath (Join-Path $env:npm_config_prefix 'claude.cmd') -PathType Leaf) -or
        (Get-Content -LiteralPath $defaultManifest -Raw | ConvertFrom-Json).version -ne '9.0.0') {
        throw 'The npm uninstall modified the separate default installation'
    }
    if ((Test-Path -LiteralPath $fixtureSkills -PathType Leaf) -or
        $cleanedSettings -match 'agentbro-bridge' -or $cleanedSettings -notmatch 'echo user-hook') {
        throw 'The successful uninstall must remove the reviewed Skill and AgentBro Hook and keep the user Hook'
    }
    Write-Output 'Agent Management removed the selected npm launcher and its reviewed resources, and preserved the separate npm installation.'
    Test-InstalledAgentPackageAcrossSettingsWindows $settings $fixtureLauncher
}

function Test-InstalledQoderSharedSkillsCleanup([Windows.Automation.AutomationElement]$Settings) {
    $profileDir = Join-Path $env:USERPROFILE '.qoder'
    if (Test-Path -LiteralPath $profileDir) { throw 'The runner already has a Qoder profile' }
    $skillDir = Join-Path $profileDir 'skills\qoder-cleanup-fixture'
    $skill = Join-Path $skillDir 'SKILL.md'
    $config = Join-Path $profileDir 'settings.json'
    try {
        $null = New-Item -ItemType Directory -Path $skillDir
        @('---', 'name: qoder-cleanup-fixture', 'description: Shared Qoder Skill fixture', '---', 'Fixture content') |
            Set-Content -LiteralPath $skill -Encoding utf8
        $originalSkill = Get-Content -LiteralPath $skill -Raw
        Invoke-ManagedAgentButton $Settings 'Hooks'
        Invoke-ManagedAgentButton $Settings '安装 Hook'
        $null = Wait-IslandValue 'the installed Qoder Desktop Hook' {
            (Test-Path -LiteralPath $config -PathType Leaf) -and (Find-PackControl $Settings '卸载 Hook')
        }
        $installedConfig = Get-Content -LiteralPath $config -Raw
        if ($installedConfig -notmatch '--source qoder-desktop') { throw 'The fixture must have the Qoder Desktop Hook' }

        Invoke-PackControl (Wait-IslandValue 'the Qoder Desktop rescan button' {
            $button = Find-PackControl $Settings '重新扫描此 Agent'
            if ($button -and $button.Current.IsEnabled) { $button }
        })
        $null = Wait-IslandValue 'the shared Skill counted in the Qoder Desktop inventory' {
            Test-PackVisibleText $Settings 'Scan complete: 0 managed, 1 unmanaged'
        }
        Invoke-PackControl (Wait-IslandValue 'the Qoder Desktop cleanup button' {
            $button = Find-PackControl $Settings 'Clean up AgentBro resources' ([Windows.Automation.ControlType]::Button)
            if ($button -and $button.Current.IsEnabled) { $button }
        })
        Invoke-PackControl (Wait-IslandValue 'the reviewed shared Skills preservation' {
            if (Test-PackVisibleText $Settings 'Shared Skills and their pack associations will be kept.') {
                Find-PackControl $Settings 'Confirm cleanup' ([Windows.Automation.ControlType]::Button)
            }
        })
        $null = Wait-IslandValue 'cleanup of the Qoder Desktop Hook' {
            -not (Find-PackControl $Settings 'Confirm cleanup') -and
                (Find-PackControl $Settings '安装 Hook') -and
                (Get-Content -LiteralPath $config -Raw) -notmatch '--source qoder-desktop'
        }
        if (-not (Test-Path -LiteralPath $skill -PathType Leaf) -or
            (Get-Content -LiteralPath $skill -Raw) -cne $originalSkill) {
            throw 'Cleaning up Qoder Desktop modified its shared Skill'
        }
        Write-Output 'Qoder Desktop cleanup removed its Hook and preserved the shared Skill on disk.'
    } finally {
        Remove-Item -LiteralPath $profileDir -Recurse -ErrorAction SilentlyContinue
    }
}

function Test-InstalledRegisteredAgentApp(
    [Windows.Automation.AutomationElement]$Island,
    [string]$AgentName = 'Cursor',
    [string]$ExeName = 'Cursor.exe'
) {
    if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
        throw 'The App Paths fixture requires an isolated GitHub-hosted runner'
    }
    $registration = "Software\Microsoft\Windows\CurrentVersion\App Paths\$ExeName"
    $registry = [Microsoft.Win32.Registry]::CurrentUser
    $existing = $registry.OpenSubKey($registration)
    if ($existing) {
        $existing.Dispose()
        throw "The runner already has a $ExeName App Paths registration"
    }
    $folderName = "AgentBro 应用 $([Guid]::NewGuid().ToString('N'))"
    $directory = Join-Path $env:TEMP $folderName
    $executable = Join-Path $directory $ExeName
    try {
        $null = New-Item -ItemType Directory -Path $directory
        Copy-Item -LiteralPath (Join-Path $env:SystemRoot 'System32\where.exe') -Destination $executable
        if ($AgentName -eq 'Qoder Desktop') {
            $resources = Join-Path $directory 'resources'
            $null = New-Item -ItemType Directory -Path $resources
            $null = New-Item -ItemType File -Path (Join-Path $resources 'app.asar')
        }
        $key = $registry.CreateSubKey($registration)
        try {
            $key.SetValue('', ('"%TEMP%\' + $folderName + '\' + $ExeName + '"'), [Microsoft.Win32.RegistryValueKind]::ExpandString)
        } finally {
            $key.Dispose()
        }

        $searchQuery = if ($AgentName -eq 'Qoder Desktop') { 'Qoder' } else { $AgentName }
        $settings = Open-ManagedAgent $Island $AgentName -RefreshPrograms -SearchQuery $searchQuery
        Invoke-ManagedAgentButton $settings 'Paths & settings'
        $null = Wait-IslandValue 'the installed Agent and its registered application path' {
            (Test-PackVisibleText $settings $executable) -and (Find-PackControl $settings '无需更新')
        }
        if (Find-PackControl $settings '卸载 Agent') {
            throw 'The registered Windows desktop Agent offered automatic uninstall'
        }
        Write-Output "Agent Management detected the registered $AgentName fixture and displayed its custom executable path."
        if ($AgentName -eq 'Qoder Desktop') {
            foreach ($otherEdition in @('Qoder IDE', 'Qoder CLI')) {
                $null = Wait-IslandValue "$otherEdition listed as uninstalled" {
                    $agent = Find-PackControl $settings $otherEdition ([Windows.Automation.ControlType]::Button)
                    $agent -and ($agent.Current.ClassName -split '\s+') -contains 'sm2-sidebar__subitem--muted'
                }
            }
            Write-Output 'Agent Management kept Qoder IDE and CLI separate from the registered Qoder Desktop app.'
            Test-InstalledQoderSharedSkillsCleanup $settings
        }

        Remove-Item -LiteralPath $executable
        $refresh = Wait-IslandValue 'the Agent refresh after removing the registered executable' {
            $button = Find-PackControl $settings '获取版本'
            if ($button -and $button.Current.IsEnabled) { $button }
        }
        Invoke-PackControl $refresh
        $null = Wait-IslandValue 'the missing Agent after its registered executable was removed' {
            (Find-PackControl $settings '打开安装页') -and -not (Test-PackVisibleText $settings $executable)
        }
        Write-Output 'Agent Management reported the fixture missing after removal of the registered executable.'
    } finally {
        $registry.DeleteSubKeyTree($registration, $false)
        Remove-Item -LiteralPath $directory -Recurse -ErrorAction SilentlyContinue
    }
}

function Test-InstalledAgentAfterPathChange([Windows.Automation.AutomationElement]$Island) {
    if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
        throw 'The saved PATH fixture requires an isolated GitHub-hosted runner'
    }

    $settings = Open-ManagedAgent $Island 'Qoder CLI' -SearchQuery 'Qoder' -RefreshPrograms
    $refreshPrograms = {
        $refresh = Wait-IslandValue 'the enabled Agent version refresh' {
            $button = Find-PackControl $settings '获取版本' ([Windows.Automation.ControlType]::Button)
            if ($button -and $button.Current.IsEnabled) { $button }
        }
        Invoke-PackControl $refresh
    }
    $isUndetected = {
        $agent = Find-PackControl $settings 'Qoder CLI' ([Windows.Automation.ControlType]::Button)
        $agent -and ($agent.Current.ClassName -split '\s+') -contains 'sm2-sidebar__subitem--muted'
    }
    $null = Wait-IslandValue 'Qoder CLI missing before the saved PATH change' $isUndetected
    $processId = $settings.Current.ProcessId
    $folderName = "AgentBro 命令行 $([Guid]::NewGuid().ToString('N'))"
    $directory = Join-Path $env:USERPROFILE $folderName
    $executable = Join-Path $directory 'qoder.exe'
    $environment = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
    if (-not $environment) { throw 'The runner has no user Environment registry key' }
    try {
        $original = $environment.GetValue('Path', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        $null = New-Item -ItemType Directory -Path $directory
        Copy-Item -LiteralPath (Join-Path $env:SystemRoot 'System32\where.exe') -Destination $executable
        $originalKind = if ($null -ne $original) { $environment.GetValueKind('Path') }
        $newEntry = '%USERPROFILE%\' + $folderName
        $changedPath = if ($original) { "$original;$newEntry" } else { $newEntry }
        try {
            $environment.SetValue('Path', $changedPath, [Microsoft.Win32.RegistryValueKind]::ExpandString)
            Write-Output 'Added an expandable CLI directory to the saved user PATH after AgentBro started.'
            & $refreshPrograms
            Invoke-ManagedAgentButton $settings 'Paths & settings'
            $null = Wait-IslandValue 'Qoder CLI discovered through the changed saved PATH' {
                (Test-PackVisibleText $settings $executable) -and -not (& $isUndetected)
            }
            Write-Output 'The installed Agent Management discovered Qoder CLI in the newly saved directory.'
        } finally {
            if ($null -eq $original) { $environment.DeleteValue('Path', $false) }
            else { $environment.SetValue('Path', $original, $originalKind) }
        }

        & $refreshPrograms
        $null = Wait-IslandValue 'Qoder CLI missing after restoring the saved PATH' {
            (& $isUndetected) -and -not (Test-PackVisibleText $settings $executable)
        }
        if ($settings.Current.ProcessId -ne $processId -or -not (Test-Path -LiteralPath $executable -PathType Leaf)) {
            throw 'The PATH discovery fixture must use the same process and executable throughout'
        }
        Write-Output 'Restoring the saved PATH hid the same Qoder CLI executable without restarting AgentBro.'
    } finally {
        $environment.Dispose()
        Remove-Item -LiteralPath $directory -Recurse -ErrorAction SilentlyContinue
    }
}
