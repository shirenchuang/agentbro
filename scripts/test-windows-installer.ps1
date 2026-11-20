param(
    [Parameter(Mandatory)]
    [string]$Installer,
    [switch]$VerifyIsland
)

$ErrorActionPreference = 'Stop'
$stage = Join-Path $env:RUNNER_TEMP "AgentBro smoke $([Guid]::NewGuid().ToString('N'))"
$installDir = Join-Path $stage 'installed app'
$fixtureBin = Join-Path $stage 'bin'
$env:CLAUDE_CONFIG_DIR = Join-Path $stage 'Claude config'
$env:Path = "$fixtureBin;$env:Path"
New-Item -ItemType Directory -Path $fixtureBin, $env:CLAUDE_CONFIG_DIR | Out-Null
Set-Content -LiteralPath (Join-Path $fixtureBin 'claude.cmd') -Value '@echo 2.0.0' -Encoding ascii

$listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
$listener.Start()
$port = $listener.LocalEndpoint.Port
if ($VerifyIsland) {
    $debugListener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    $debugListener.Start()
    $debugPort = $debugListener.LocalEndpoint.Port
    $debugListener.Stop()
}
$listener.Stop()
$env:AGENTBRO_HOOK_PORT = "$port"

$installerProcess = Start-Process -FilePath $Installer -ArgumentList "/S /D=$installDir" -PassThru
if (-not $installerProcess.WaitForExit(90000)) {
    $installerProcess.Kill()
    throw 'NSIS installation timed out'
}
if ($installerProcess.ExitCode -ne 0) {
    throw "NSIS installation failed with exit code $($installerProcess.ExitCode)"
}

$appPath = Join-Path $installDir 'agentbro.exe'
$settingsPath = Join-Path $env:CLAUDE_CONFIG_DIR 'settings.json'
$bridgePath = Join-Path $env:USERPROFILE '.agentbro\bin\agentbro-bridge.exe'
$logPath = Join-Path $env:LOCALAPPDATA 'com.agentbro.desktop\logs\agentbro.log'
if (-not (Test-Path -LiteralPath $appPath -PathType Leaf)) {
    throw "Installed executable is missing: $appPath"
}
$bundledBridges = @(Get-ChildItem -LiteralPath $installDir -Recurse -File | Where-Object {
    $_.Name -in 'agentbro-bridge', 'agentbro-bridge.exe'
})
if ($bundledBridges.Count -eq 0) {
    throw 'The installed package is missing the bridge resource'
}

$legacyCommand = "$bridgePath --source claude-code"
$userCommand = 'echo user-hook'
$fixtureSettings = @{
    hooks = @{
        PreToolUse = @(@{
            matcher = 'Read'
            hooks = @(
                @{ type = 'command'; command = $legacyCommand },
                @{ type = 'command'; command = $userCommand }
            )
        })
    }
}
$fixtureSettings | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $settingsPath -Encoding utf8

$stdoutPath = Join-Path $stage 'agentbro.stdout.log'
$stderrPath = Join-Path $stage 'agentbro.stderr.log'
$webViewArguments = $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
$webViewProfile = $env:WEBVIEW2_USER_DATA_FOLDER
try {
    if ($VerifyIsland) {
        $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$debugPort --remote-debugging-address=127.0.0.1"
        $env:WEBVIEW2_USER_DATA_FOLDER = Join-Path $stage 'webview'
    }
    $app = Start-Process -FilePath $appPath -WorkingDirectory $installDir -PassThru `
        -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
} finally {
    if ($VerifyIsland) {
        $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = $webViewArguments
        $env:WEBVIEW2_USER_DATA_FOLDER = $webViewProfile
    }
}
$bridge = $null
function Wait-AppCondition([string]$Description, [scriptblock]$Condition) {
    $deadline = (Get-Date).AddSeconds(45)
    do {
        if ($app.HasExited) {
            throw "AgentBro exited with code $($app.ExitCode) while waiting for $Description"
        }
        if (& $Condition) {
            return
        }
        Start-Sleep -Milliseconds 500
    } until ((Get-Date) -ge $deadline)
    throw "Timed out waiting for $Description"
}

try {
    Wait-AppCondition 'the installed app to listen on its Hook port' {
        Get-NetTCPConnection -LocalPort $port -State Listen -OwningProcess $app.Id -ErrorAction SilentlyContinue
    }
    Wait-AppCondition 'the bridge and Claude Hook configuration to be deployed' {
        (Test-Path -LiteralPath $bridgePath -PathType Leaf) -and
            (Test-Path -LiteralPath $logPath -PathType Leaf) -and
            (Select-String -LiteralPath $logPath -SimpleMatch 'Claude Code hooks installed successfully' -Quiet)
    }
    $deployedHash = (Get-FileHash -LiteralPath $bridgePath).Hash
    $bundledHashes = Get-FileHash -LiteralPath $bundledBridges.FullName
    if ($deployedHash -notin $bundledHashes.Hash) {
        throw 'The deployed bridge does not match any executable bundled by the installer'
    }

    $hookGroups = (Get-Content -LiteralPath $settingsPath -Raw | ConvertFrom-Json).hooks.PreToolUse
    $bridgeHooks = @($hookGroups | ForEach-Object { $_.hooks } | Where-Object {
        $_.type -eq 'command' -and $_.command -like '*agentbro-bridge.exe*'
    })
    if ($bridgeHooks.Count -ne 1 -or $bridgeHooks[0].command -eq $legacyCommand) {
        throw 'The installed app did not replace the legacy Claude Hook command'
    }
    $userHooks = @($hookGroups | Where-Object { $_.matcher -eq 'Read' } | ForEach-Object { $_.hooks } | Where-Object {
        $_.type -eq 'command' -and $_.command -eq $userCommand
    })
    if ($userHooks.Count -ne 1) {
        throw 'The installed app did not preserve the user Hook and its matcher'
    }

    $bashPath = $env:CLAUDE_CODE_GIT_BASH_PATH
    if (-not $bashPath) {
        $bashPath = Join-Path $env:ProgramFiles 'Git\bin\bash.exe'
    }
    if (-not (Test-Path -LiteralPath $bashPath -PathType Leaf)) {
        throw "Git Bash was not found at $bashPath"
    }

    $sessionId = "windows-installer-smoke-$PID"
    $event = @{
        hook_event_name = 'PreToolUse'
        session_id = $sessionId
        cwd = $stage
        tool_name = 'Read'
        tool_input = @{ file_path = (Join-Path $stage 'smoke.txt') }
    } | ConvertTo-Json -Compress
    $eventFile = Join-Path $stage 'hook-event.json'
    Set-Content -LiteralPath $eventFile -Value $event -Encoding utf8

    $start = [System.Diagnostics.ProcessStartInfo]::new($bashPath)
    $start.ArgumentList.Add('-c')
    # The bridge reports Bash as the agent PID. Keep Bash alive while inspecting the session.
    $start.ArgumentList.Add($bridgeHooks[0].command + ' < "$1" || exit "$?"; IFS= read -r _ || :')
    $start.ArgumentList.Add('agentbro-smoke')
    $start.ArgumentList.Add($eventFile.Replace('\', '/'))
    $start.UseShellExecute = $false
    $start.RedirectStandardInput = $true
    $bridge = [System.Diagnostics.Process]::Start($start)

    Wait-AppCondition 'AgentBro to receive the bridge tool event' {
        if ($bridge.HasExited) {
            throw "The configured Claude Hook exited with code $($bridge.ExitCode) before event delivery"
        }
        (Test-Path -LiteralPath $logPath -PathType Leaf) -and
            (Select-String -LiteralPath $logPath -SimpleMatch "[ToolUse] session=$sessionId tool=Read" -Quiet)
    }
    Write-Output 'Windows installation, Claude Hook repair, and event delivery through Git Bash passed.'
    if ($VerifyIsland) {
        & (Join-Path $PSScriptRoot 'test-windows-island.ps1') -DebuggerPort $debugPort -ProjectName (Split-Path $stage -Leaf)
    }
    $bridge.StandardInput.Close()
    if (-not $bridge.WaitForExit(10000) -or $bridge.ExitCode -ne 0) {
        throw 'The configured Claude Hook failed when run through Git Bash'
    }
} catch {
    Write-Output "AgentBro PID: $($app.Id), expected Hook port: $port"
    Get-Process -Id $app.Id -ErrorAction SilentlyContinue |
        Select-Object Id, SessionId, CPU, Responding, MainWindowTitle |
        Format-List
    Get-CimInstance Win32_Process -Filter "ParentProcessId = $($app.Id)" -ErrorAction SilentlyContinue |
        Select-Object Name, ProcessId, SessionId |
        Format-Table
    Get-NetTCPConnection -OwningProcess $app.Id -State Listen -ErrorAction SilentlyContinue |
        Select-Object LocalAddress, LocalPort |
        Format-Table
    $webViewPath = Join-Path ${env:ProgramFiles(x86)} 'Microsoft\EdgeWebView\Application'
    Write-Output 'WebView2 Runtime versions:'
    Get-ChildItem -LiteralPath $webViewPath -Directory -ErrorAction SilentlyContinue |
        Select-Object -ExpandProperty Name
    foreach ($diagnosticLog in @($logPath, $stdoutPath, $stderrPath)) {
        if (Test-Path -LiteralPath $diagnosticLog -PathType Leaf) {
            Write-Output "Last lines of $diagnosticLog"
            Get-Content -LiteralPath $diagnosticLog -Tail 30
        }
    }
    throw
} finally {
    if ($bridge -and -not $bridge.HasExited) {
        Stop-Process -Id $bridge.Id -ErrorAction SilentlyContinue
    }
    if (-not $app.HasExited) {
        Stop-Process -Id $app.Id -ErrorAction SilentlyContinue
    }
}
