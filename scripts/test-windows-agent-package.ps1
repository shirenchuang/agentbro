function Test-InstalledAgentPackageConsole([Windows.Automation.AutomationElement]$Island) {
    $settings = Open-InstalledAgentSkills $Island
    $agents = Wait-IslandValue 'the Agent Management navigation item' { Find-PackControl $settings 'Agent 管理' }
    Invoke-PackControl $agents
    if (-not (Find-IslandButton $settings { param($Name) $Name -eq 'Claude Code' })) {
        $uninstalled = Wait-IslandValue 'the collapsed Agent list' {
            Find-IslandButton $settings { param($Name) $Name -like '*未安装 Agent*' }
        }
        Invoke-PackControl $uninstalled
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
    $null = Wait-IslandValue 'the completed fixture package update' {
        (Test-Path -LiteralPath $env:AGENTBRO_PACKAGE_CONSOLE_RESULT) -and (Find-PackControl $settings 'Agent 更新完成')
    }
    $null = Wait-IslandValue 'stdout and stderr in Agent Management' {
        (Find-PackControl $settings '> agentbro-package-fixture-stdout') -and
            (Find-PackControl $settings '! agentbro-package-fixture-stderr')
    }
    $report = Get-Content -LiteralPath $env:AGENTBRO_PACKAGE_CONSOLE_RESULT -Raw -Encoding utf8 | ConvertFrom-Json
    Write-Output 'The installed Agent Management ran the fixture update and displayed both output streams.'
    Write-Output "Installed package console probe: $($report | ConvertTo-Json -Compress)"
}
