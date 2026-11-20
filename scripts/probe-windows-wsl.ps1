param(
    [Parameter(Mandatory)]
    [string]$Installer
)

$ErrorActionPreference = 'Stop'
$stage = Join-Path $env:RUNNER_TEMP "AgentBro WSL $([Guid]::NewGuid().ToString('N'))"
$filesystem = Join-Path $stage 'rootfs.tar.gz'
$distribution = "AgentBroSmoke-$([Guid]::NewGuid().ToString('N').Substring(0, 8))"
New-Item -ItemType Directory -Path $stage | Out-Null

$wsl = Join-Path $env:SystemRoot 'System32\wsl.exe'
& $wsl --version
& curl.exe --fail --location --silent --show-error --retry 2 --max-time 90 --output $filesystem `
    'https://cdimage.ubuntu.com/ubuntu-base/releases/24.04/release/ubuntu-base-24.04.5-base-amd64.tar.gz'
if ($LASTEXITCODE -ne 0) { throw 'Ubuntu base download failed' }
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $filesystem).Hash -ne 'e77b6f10c2590cef872b33ee9f635a0e3fd1f57fb074c0e52b5c7f56147a0c86') {
    throw 'The Ubuntu base archive did not match the official SHA-256'
}

$app = $null
$registered = $false
try {
    & $wsl --import $distribution (Join-Path $stage 'distro') $filesystem --version 2
    if ($LASTEXITCODE -ne 0) { throw "WSL 2 distribution import failed with exit code $LASTEXITCODE" }
    $registered = $true
    & $wsl --list --verbose
    & $wsl --distribution $distribution --exec /bin/sh -c 'uname -r; printf "Linux home %s\n" "$HOME"'
    if ($LASTEXITCODE -ne 0) { throw 'The imported WSL 2 distribution could not start' }

    $listen = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    $listen.Start()
    $port = $listen.LocalEndpoint.Port
    $listen.Stop()
    $env:AGENTBRO_HOOK_PORT = "$port"
    $env:WSLENV = (@($env:WSLENV, 'AGENTBRO_HOOK_PORT') | Where-Object { $_ }) -join ':'
    $installDir = Join-Path $stage 'Installed AgentBro'
    $setup = Start-Process -FilePath $Installer -ArgumentList "/S /D=$installDir" -PassThru
    if (-not $setup.WaitForExit(90000) -or $setup.ExitCode -ne 0) { throw 'AgentBro installer failed' }

    $app = Start-Process -FilePath (Join-Path $installDir 'agentbro.exe') -PassThru
    $deadline = (Get-Date).AddSeconds(60)
    do {
        $ready = Get-NetTCPConnection -LocalPort $port -State Listen -OwningProcess $app.Id -ErrorAction SilentlyContinue
        if ($ready) { break }
        if ($app.HasExited) { throw 'AgentBro exited during startup' }
        Start-Sleep -Milliseconds 500
    } until ((Get-Date) -ge $deadline)
    if (-not $ready) { throw 'The installed AgentBro loopback listener never started' }

    $bridge = Get-ChildItem -LiteralPath $installDir -Filter 'agentbro-bridge.exe' -Recurse -File |
        Select-Object -First 1 -ExpandProperty FullName
    if (-not $bridge) { throw 'The AgentBro installer did not contain a bridge' }
    $linuxBridge = (& $wsl --distribution $distribution --exec wslpath -u $bridge | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or -not $linuxBridge.StartsWith('/')) { throw 'wslpath could not translate the installed Windows bridge' }
    Write-Output "WSL translated the packaged AgentBro bridge to $linuxBridge"

    $session = 'wsl-native-smoke'
    $project = 'AgentBroWslSmoke'
    foreach ($event in @(
        @{ hook_event_name = 'SessionStart'; session_id = $session; cwd = "/root/$project" },
        @{ hook_event_name = 'PreToolUse'; session_id = $session; cwd = "/root/$project"; tool_name = 'Read'; tool_input = @{ file_path = "/root/$project/smoke.txt" } }
    )) {
        ($event | ConvertTo-Json -Depth 6 -Compress) | & $wsl --distribution $distribution --exec $linuxBridge --source claude-code
        if ($LASTEXITCODE -ne 0) { throw "The Windows bridge failed from WSL on $($event.hook_event_name)" }
    }

    $windowsPowerShell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    & $windowsPowerShell -NoProfile -NonInteractive -Mta -File "$PSScriptRoot/test-windows-island.ps1" `
        -AppProcessId $app.Id -ProjectName $project
    if ($LASTEXITCODE -ne 0) { throw 'The WSL fixture did not reach the native Island' }
    Write-Output 'The WSL 2 distribution executed the installed Windows bridge and the native Island received its Hook events.'
} finally {
    if ($app -and -not $app.HasExited) { Stop-Process -Id $app.Id -ErrorAction SilentlyContinue }
    if ($registered) { & $wsl --unregister $distribution }
}
