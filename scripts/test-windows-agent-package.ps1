function Open-ManagedAgent([Windows.Automation.AutomationElement]$Island, [string]$AgentName) {
    $settings = Open-InstalledAgentSkills $Island
    $agents = Wait-IslandValue 'the Agent Management navigation item' { Find-PackControl $settings 'Agent 管理' }
    Invoke-PackControl $agents | Out-Host
    $search = Wait-IslandValue 'the Agent search' {
        Find-PackControl $settings 'Search Agents' ([Windows.Automation.ControlType]::Edit)
    }
    Click-IslandElement $search 'Agent search' | Out-Host
    $handle = [IntPtr]$settings.Current.NativeWindowHandle
    $searchValue = $search.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)
    if ($searchValue.Current.Value.Length -gt 0) {
        $keys = @(0x23) + @(0x08) * $searchValue.Current.Value.Length
        foreach ($key in $keys) {
            if (-not [AgentBroSmokeInput]::PressKeyInWindow($handle, $key)) {
                throw 'Agent Management lost focus while clearing the Agent search'
            }
        }
    }
    foreach ($character in $AgentName.ToCharArray()) {
        if ([AgentBroSmokeInput]::GetForegroundWindow() -ne $handle) {
            throw 'Agent Management lost focus before entering the Agent search'
        }
        [AgentBroSmokeInput]::AppendCharacter($character)
    }
    Click-IslandElement $null "$AgentName in Agent Management" -FindElement {
        Find-IslandButton $settings { param($Name) $Name -eq $AgentName }
    } | Out-Host
    $settings
}

function Test-InstalledAgentPackageConsole([Windows.Automation.AutomationElement]$Island) {
    $settings = Open-ManagedAgent $Island 'Claude Code'
    $settingsBounds = $settings.Current.BoundingRectangle
    $scale = [AgentBroSmokeInput]::GetDpiForWindow([IntPtr]$settings.Current.NativeWindowHandle) / 96.0
    if ($scale -le 0) { throw 'Could not read the Settings window DPI' }
    if ($settingsBounds.Width / $scale -gt 1050) {
        $transform = $settings.GetCurrentPattern([Windows.Automation.TransformPattern]::Pattern)
        $transform.Resize(1024 * $scale, $settingsBounds.Height)
    }
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
    $labelHeights = foreach ($label in @('当前版本 2.0.0', '可更新到 999.0.0')) {
        $rectangles = @(Wait-IslandValue "the visible bounds of Agent version label $label" {
            $range = Find-PackDocumentText $settings $label
            if ($range) { $range.GetBoundingRectangles() }
        })
        $bounds = [Windows.Rect]::Empty
        foreach ($rectangle in $rectangles) {
            $bounds = [Windows.Rect]::Union($bounds, $rectangle)
        }
        $height = $bounds.Height / $scale
        if ($height -le 0 -or $height -gt 24) {
            throw "The Agent version label '$label' occupies $([int]$height)px instead of one line"
        }
        [int]$height
    }
    Write-Output "Agent version labels stayed on one line in compact Settings ($($labelHeights -join ', ')px high)."
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

function Test-InstalledRegisteredAgentApp([Windows.Automation.AutomationElement]$Island) {
    if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
        throw 'The App Paths fixture requires an isolated GitHub-hosted runner'
    }
    $registration = 'Software\Microsoft\Windows\CurrentVersion\App Paths\Cursor.exe'
    $registry = [Microsoft.Win32.Registry]::CurrentUser
    $existing = $registry.OpenSubKey($registration)
    if ($existing) {
        $existing.Dispose()
        throw 'The runner already has a Cursor App Paths registration'
    }
    $folderName = "AgentBro 应用 $([Guid]::NewGuid().ToString('N'))"
    $directory = Join-Path $env:TEMP $folderName
    $executable = Join-Path $directory 'Cursor.exe'
    try {
        $null = New-Item -ItemType Directory -Path $directory
        Copy-Item -LiteralPath (Join-Path $env:SystemRoot 'System32\where.exe') -Destination $executable
        $key = $registry.CreateSubKey($registration)
        try {
            $key.SetValue('', ('"%TEMP%\' + $folderName + '\Cursor.exe"'), [Microsoft.Win32.RegistryValueKind]::ExpandString)
        } finally {
            $key.Dispose()
        }

        $settings = Open-ManagedAgent $Island 'Cursor'
        $refresh = Wait-IslandValue 'the enabled Agent version refresh' {
            $button = Find-PackControl $settings '获取版本'
            if ($button -and $button.Current.IsEnabled) { $button }
        }
        Invoke-PackControl $refresh
        $paths = Wait-IslandValue 'the Paths & settings tab' { Find-PackControl $settings 'Paths & settings' }
        Invoke-PackControl $paths
        $null = Wait-IslandValue 'the installed Agent and its registered application path' {
            (Test-PackVisibleText $settings $executable) -and (Find-PackControl $settings '无需更新')
        }
        if (Find-PackControl $settings '卸载 Agent') {
            throw 'The registered Windows desktop Agent offered automatic uninstall'
        }
        Write-Output 'Agent Management detected the registered Cursor fixture and displayed its custom executable path.'

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
