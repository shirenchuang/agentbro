param(
    [Parameter(Mandatory)]
    [string]$Installer,
    [switch]$VerifyIntegration
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

    if ($VerifyIntegration) {
        & $wsl --distribution $distribution --user root --exec /usr/sbin/useradd --create-home --shell /bin/bash agentbro
        if ($LASTEXITCODE -ne 0) { throw 'Could not create the WSL fixture user' }
        "[user]`ndefault=agentbro" | & $wsl --distribution $distribution --user root --exec /bin/sh -c 'cat >/etc/wsl.conf'
        if ($LASTEXITCODE -ne 0) { throw 'Could not configure the default WSL fixture user' }
        & $wsl --terminate $distribution
        if ($LASTEXITCODE -ne 0) { throw 'Could not restart the WSL fixture distribution' }
        $defaultUser = (& $wsl --distribution $distribution --exec id -un | Out-String).Trim()
        if ($defaultUser -ne 'agentbro') { throw "The default WSL fixture user is $defaultUser" }
        @{ hooks = @{ SessionStart = @(@{ hooks = @(@{ type = 'command'; command = 'echo existing-user-hook' }) }) } } |
            ConvertTo-Json -Depth 10 -Compress |
            & $wsl --distribution $distribution --exec /bin/sh -c 'umask 077; mkdir -p "$HOME/.claude" "$HOME/.codex" "$HOME/AgentBroWslSmoke"; cat > "$HOME/.claude/settings.json"; cp "$HOME/.claude/settings.json" "$HOME/.codex/hooks.json"'
        if ($LASTEXITCODE -ne 0) { throw 'Could not create the WSL Hook fixture settings' }
        'model = "fixture-model"' | & $wsl --distribution $distribution --exec /bin/sh -c 'cat > "$HOME/.codex/config.toml"'
        'printf "%s" "$PWD" > "$HOME/.agentbro-smoke-cwd"' |
            & $wsl --distribution $distribution --exec /bin/sh -c 'cat >> "$HOME/.bashrc"'
        if ($LASTEXITCODE -ne 0) { throw 'Could not add the WSL workspace fixture' }
    }

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

    if ($VerifyIntegration) {
        $env:WSLENV = (($env:WSLENV -split ':') | Where-Object { $_ -and $_ -notlike 'AGENTBRO_HOOK_PORT*' }) -join ':'
        $windowsPowerShell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
        function Invoke-IslandCheck([string[]]$Options) {
            & $windowsPowerShell -NoProfile -NonInteractive -Mta -File "$PSScriptRoot/test-windows-island.ps1" `
                -AppProcessId $app.Id -ProjectName 'AgentBroWslSmoke' -WslDistribution $distribution @Options
            if ($LASTEXITCODE -ne 0) { throw "The native Island WSL check failed: $Options" }
        }
        function Read-WslFixtureConfig([string]$Path) {
            $text = & $wsl --distribution $distribution --exec /bin/cat $Path | Out-String
            if ($LASTEXITCODE -ne 0) { throw "Could not read the WSL fixture configuration at $Path" }
            return $text
        }

        Invoke-IslandCheck @('-WslAction', 'Install')
        $projectPath = '/home/agentbro/AgentBroWslSmoke'
        foreach ($target in @(
            @{ Id = 'claude-code'; Path = '/home/agentbro/.claude/settings.json' },
            @{ Id = 'codex'; Path = '/home/agentbro/.codex/hooks.json' }
        )) {
            $config = Read-WslFixtureConfig $target.Path | ConvertFrom-Json -AsHashtable
            $mode = (& $wsl --distribution $distribution --exec stat -c '%a' $target.Path | Out-String).Trim()
            if ($mode -ne '600') { throw "Hook setup did not preserve permissions for $($target.Id)" }
            foreach ($event in @(
                @{ hook_event_name = 'SessionStart'; session_id = 'same-id'; cwd = $projectPath },
                @{ hook_event_name = 'PreToolUse'; session_id = 'same-id'; cwd = $projectPath; tool_name = 'Read'; tool_input = @{ file_path = "$projectPath/smoke.txt" } }
            )) {
                $managed = $config.hooks[$event.hook_event_name] | ForEach-Object { $_.hooks } |
                    Where-Object { $_.command -like "*--wsl-distro*--source $($target.Id)* # AgentBro WSL" } |
                    Select-Object -First 1 -ExpandProperty command
                if (-not $managed -or $managed -notlike "*--hook-port $port*") {
                    throw "Missing the configured hook for $($target.Id) $($event.hook_event_name)"
                }
                ($event | ConvertTo-Json -Depth 8 -Compress) | & $wsl --distribution $distribution --exec /bin/sh -c $managed
                if ($LASTEXITCODE -ne 0) { throw "The installed bridge did not run for $($target.Id)" }
            }
        }
        $codexConfig = Read-WslFixtureConfig '/home/agentbro/.codex/config.toml'
        if ($codexConfig -notmatch 'model = "fixture-model"' -or $codexConfig -notmatch 'hooks = true') {
            throw 'Codex hook activation did not preserve the fixture model setting'
        }
        Start-Sleep -Seconds 5
        Invoke-IslandCheck @('-WslJump')
        $jumpDeadline = (Get-Date).AddSeconds(20)
        do {
            $opened = & $wsl --distribution $distribution --exec /bin/cat '/home/agentbro/.agentbro-smoke-cwd' 2>$null
            if ($opened -eq $projectPath) { break }
            Start-Sleep -Milliseconds 500
        } until ((Get-Date) -ge $jumpDeadline)
        if ($opened -ne $projectPath) { throw "The WSL jump did not open $projectPath" }
        Write-Output 'Windows Island opened the fixture project in the WSL terminal.'
        $claude = Read-WslFixtureConfig '/home/agentbro/.claude/settings.json' | ConvertFrom-Json -AsHashtable
        $permission = $claude.hooks.PermissionRequest | ForEach-Object { $_.hooks } |
            Where-Object { $_.command -like '*--wsl-distro*--source claude-code* # AgentBro WSL' } |
            Select-Object -First 1 -ExpandProperty command
        if (-not $permission) { throw 'The WSL Claude permission Hook was not installed' }
        $start = [System.Diagnostics.ProcessStartInfo]::new($wsl)
        foreach ($argument in @('--distribution', $distribution, '--exec', '/bin/sh', '-c', $permission)) {
            $start.ArgumentList.Add($argument)
        }
        $start.UseShellExecute = $false
        $start.CreateNoWindow = $true
        $start.RedirectStandardInput = $true
        $start.RedirectStandardOutput = $true
        $request = [System.Diagnostics.Process]::Start($start)
        try {
            $reply = $request.StandardOutput.ReadToEndAsync()
            $request.StandardInput.WriteLine((@{
                hook_event_name = 'PermissionRequest'
                session_id = 'same-id'
                cwd = $projectPath
                tool_name = 'Read'
                tool_input = @{ file_path = "$projectPath/smoke.txt" }
            } | ConvertTo-Json -Depth 8 -Compress))
            $request.StandardInput.Close()
            Invoke-IslandCheck @('-WslDeny')
            if (-not $request.WaitForExit(15000)) { throw 'WSL did not receive the approval response' }
            $decision = $reply.GetAwaiter().GetResult() | ConvertFrom-Json
            if ($decision.hookSpecificOutput.decision.behavior -ne 'deny') { throw 'WSL did not receive Deny' }
            Write-Output 'The configured WSL Claude Hook received the native Island denial response.'
        } finally {
            if (-not $request.HasExited) { Stop-Process -Id $request.Id -ErrorAction SilentlyContinue }
            $request.Dispose()
        }
        Invoke-IslandCheck @('-WslAction', 'Uninstall')
        foreach ($name in @('.claude/settings.json', '.codex/hooks.json')) {
            $config = Read-WslFixtureConfig "/home/agentbro/$name"
            if ($config -notmatch 'echo existing-user-hook' -or $config -match '# AgentBro WSL') {
                throw "The uninstall did not preserve the existing hook in $name"
            }
        }
        Write-Output 'WSL Hook install, Linux-to-Windows event delivery, session separation, workspace jump, and uninstall passed.'
        return
    }

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
