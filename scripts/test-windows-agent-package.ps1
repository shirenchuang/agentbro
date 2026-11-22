function Test-InstalledAgentPackageConsole([Windows.Automation.AutomationElement]$Island) {
    $settings = Open-InstalledAgentSkills $Island
    $agents = Wait-IslandValue 'the Agent Management navigation item' { Find-PackControl $settings 'Agent 管理' }
    Invoke-PackControl $agents
    $search = Wait-IslandValue 'the Agent search' { Find-PackControl $settings 'Search Agents' }
    Click-IslandElement $search 'Agent search'
    $handle = [IntPtr]$settings.Current.NativeWindowHandle
    foreach ($character in 'Claude Code'.ToCharArray()) {
        if ([AgentBroSmokeInput]::GetForegroundWindow() -ne $handle) {
            throw 'Agent Management lost focus before entering the Agent search'
        }
        [AgentBroSmokeInput]::AppendCharacter($character)
    }
    $claude = Wait-IslandValue 'Claude Code in Agent Management' {
        Find-IslandButton $settings { param($Name) $Name -eq 'Claude Code' }
    }
    Invoke-PackControl $claude
    $null = Wait-IslandValue 'the fixture Claude Code version' {
        Find-PackControl $settings '可更新到 999.0.0'
    }
    $update = Wait-IslandValue 'the Claude Code update action' {
        Find-IslandButton $settings { param($Name) $Name -eq '更新此 Agent' }
    }
    Invoke-PackControl $update
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

    $uninstall = Wait-IslandValue 'the uninstall button for the installed Agent' {
        $button = Find-PackControl $settings '卸载 Agent'
        if ($button -and $button.Current.IsEnabled) { $button }
    }
    Invoke-PackControl $uninstall
    $confirm = Wait-IslandValue 'the uninstall confirmation' { Find-PackControl $settings '确认卸载' }
    Invoke-PackControl $confirm
    $null = Wait-IslandValue 'the Agent still detected after the first uninstall' {
        Test-PackVisibleText $settings 'The uninstall command completed. AgentBro still detects Claude Code.'
    }
    $fixtureLauncher = Join-Path (Split-Path $env:AGENTBRO_PACKAGE_CONSOLE_RESULT) 'bin\claude.cmd'
    if (-not (Test-Path -LiteralPath $fixtureLauncher -PathType Leaf)) {
        throw 'The first package uninstall unexpectedly removed the fixture launcher'
    }
    Write-Output 'The first uninstall exited successfully. Agent Management kept the detected Agent visible.'

    $retry = Wait-IslandValue 'the enabled uninstall after detecting the remaining Agent' {
        $button = Find-PackControl $settings '卸载 Agent'
        if ($button -and $button.Current.IsEnabled) { $button }
    }
    Invoke-PackControl $retry
    $confirm = Wait-IslandValue 'the second uninstall confirmation' { Find-PackControl $settings '确认卸载' }
    Invoke-PackControl $confirm
    $null = Wait-IslandValue 'the confirmed uninstall of the fixture launcher' {
        -not (Test-Path -LiteralPath $fixtureLauncher -PathType Leaf) -and
            (Test-PackVisibleText $settings 'Agent「Claude Code」已卸载')
    }
    Write-Output 'Agent Management confirmed the uninstall after the fixture launcher was removed.'
}
