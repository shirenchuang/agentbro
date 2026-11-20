param(
    [Parameter(Mandatory)]
    [string]$Installer
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

$app = Start-Process -FilePath $appPath -WorkingDirectory $installDir -PassThru
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
        (Test-Path -LiteralPath $bridgePath -PathType Leaf) -and (Test-Path -LiteralPath $settingsPath -PathType Leaf)
    }
    $deployedHash = (Get-FileHash -LiteralPath $bridgePath).Hash
    $bundledHashes = Get-FileHash -LiteralPath $bundledBridges.FullName
    if ($deployedHash -notin $bundledHashes.Hash) {
        throw 'The deployed bridge does not match any executable bundled by the installer'
    }

    $sessionId = "windows-installer-smoke-$PID"
    $event = @{
        hook_event_name = 'PreToolUse'
        session_id = $sessionId
        cwd = $stage
        tool_name = 'Read'
        tool_input = @{ file_path = (Join-Path $stage 'smoke.txt') }
    } | ConvertTo-Json -Compress

    $start = [System.Diagnostics.ProcessStartInfo]::new($bridgePath, '--source claude-code')
    $start.UseShellExecute = $false
    $start.RedirectStandardInput = $true
    $bridge = [System.Diagnostics.Process]::Start($start)
    $bridge.StandardInput.WriteLine($event)
    $bridge.StandardInput.Close()
    if (-not $bridge.WaitForExit(10000) -or $bridge.ExitCode -ne 0) {
        throw 'The installed bridge failed to send the test event'
    }

    Wait-AppCondition 'AgentBro to receive the bridge tool event' {
        (Test-Path -LiteralPath $logPath -PathType Leaf) -and
            (Select-String -LiteralPath $logPath -SimpleMatch "[ToolUse] session=$sessionId tool=Read" -Quiet)
    }
    Write-Output 'Windows installer, app startup, Hook deployment, and bridge delivery passed.'
} catch {
    if (Test-Path -LiteralPath $logPath -PathType Leaf) {
        Get-Content -LiteralPath $logPath -Tail 30
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
