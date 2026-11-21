param(
    [Parameter(Mandatory)]
    [string]$Installer,
    [switch]$VerifyIsland,
    [switch]$VerifyProjectLock,
    [string]$SymbolsDirectory
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

$lockProject = Join-Path $stage 'Native project lock fixture'
if ($VerifyProjectLock) {
    $originalSkill = Join-Path $lockProject '.agents/skills/native-existing'
    $null = New-Item -ItemType Directory -Path $originalSkill -Force
    Set-Content -LiteralPath (Join-Path $originalSkill 'SKILL.md') -Value 'User fixture' -Encoding utf8
    $lockPath = Join-Path $lockProject 'skills-lock.json'
    @'
{
  "version": 2,
  "skills": {
    "native-restored": {
      "source": "vercel-labs/skills",
      "sourceType": "github",
      "commit": "57217cf86ba38148a57af9e90d14a4c049558330",
      "skillName": "find-skills",
      "skillPath": "skills/find-skills/SKILL.md",
      "computedHash": "7bc223e2aafbdbf6e50ea243ba6ef67fe432bb069ab439fea0390795dac0bf7f"
    },
    "native-mismatch": {
      "source": "vercel-labs/skills",
      "sourceType": "github",
      "commit": "57217cf86ba38148a57af9e90d14a4c049558330",
      "skillName": "find-skills",
      "skillPath": "skills/find-skills/SKILL.md",
      "computedHash": "0000000000000000000000000000000000000000000000000000000000000000"
    },
    "native-existing": {
      "source": "vercel-labs/skills",
      "sourceType": "github",
      "commit": "57217cf86ba38148a57af9e90d14a4c049558330",
      "skillName": "find-skills",
      "skillPath": "skills/find-skills/SKILL.md",
      "computedHash": "7bc223e2aafbdbf6e50ea243ba6ef67fe432bb069ab439fea0390795dac0bf7f"
    }
  }
}
'@ | Set-Content -LiteralPath $lockPath -Encoding utf8
    $originalLockHash = (Get-FileHash -LiteralPath $lockPath).Hash
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

$restoredSession = '恢复会话'
$restoredProjectName = 'Windows restored project'
$restoredWorkspace = Join-Path $stage $restoredProjectName
$restoredProjectsDir = Join-Path $env:CLAUDE_CONFIG_DIR 'projects\restored-project'
$null = New-Item -ItemType Directory -Path $restoredWorkspace, $restoredProjectsDir
@{
    sessionId = $restoredSession
    cwd = $restoredWorkspace
    type = 'user'
    message = @{ role = 'user'; content = $restoredProjectName }
} | ConvertTo-Json -Depth 5 -Compress |
    Set-Content -LiteralPath (Join-Path $restoredProjectsDir "$restoredSession.jsonl") -Encoding utf8

$webViewPath = Join-Path ${env:ProgramFiles(x86)} 'Microsoft\EdgeWebView\Application'
Write-Output 'Installed WebView2 Runtime versions:'
Get-ChildItem -LiteralPath $webViewPath -Directory -ErrorAction SilentlyContinue |
    Select-Object -ExpandProperty Name

$stdoutPath = Join-Path $stage 'agentbro.stdout.log'
$stderrPath = Join-Path $stage 'agentbro.stderr.log'
$app = Start-Process -FilePath $appPath -WorkingDirectory $installDir -PassThru `
    -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
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

function Write-StartupStacks {
    $debugger = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\Debuggers\x64\cdb.exe'
    if (-not (Test-Path -LiteralPath $debugger -PathType Leaf)) {
        Write-Output 'Windows Debugging Tools are unavailable on this runner.'
        return
    }

    $start = [System.Diagnostics.ProcessStartInfo]::new($debugger)
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $stackLog = Join-Path $stage 'agentbro-startup-stack.log'
    $symbolCache = Join-Path $stage 'debug-symbol-cache'
    $symbolPath = "$SymbolsDirectory;srv*$symbolCache*https://msdl.microsoft.com/download/symbols"
    foreach ($argument in @('-pv', '-p', "$($app.Id)", '-y', $symbolPath, '-logo', $stackLog, '-c', '~* k 24; q')) {
        $start.ArgumentList.Add($argument)
    }
    $capture = [System.Diagnostics.Process]::Start($start)
    try {
        if (-not $capture.WaitForExit(20000)) {
            $capture.Kill()
            Write-Output 'Startup stack capture timed out.'
        }
        if (Test-Path -LiteralPath $stackLog -PathType Leaf) {
            Get-Content -LiteralPath $stackLog -TotalCount 250
        }
    } finally {
        $capture.Dispose()
    }
}

function Send-ClaudeToolEvent([string]$Executable, [string[]]$Arguments, [hashtable]$Event) {
    $start = [System.Diagnostics.ProcessStartInfo]::new($Executable)
    foreach ($argument in $Arguments) {
        $start.ArgumentList.Add($argument)
    }
    $start.UseShellExecute = $false
    $start.RedirectStandardInput = $true
    $bridge = [System.Diagnostics.Process]::Start($start)
    try {
        $bridge.StandardInput.WriteLine(($Event | ConvertTo-Json -Compress))
        $bridge.StandardInput.Close()
        if (-not $bridge.WaitForExit(10000) -or $bridge.ExitCode -ne 0) {
            throw "The Hook command failed while sending fixture $($Event.session_id)"
        }
    } finally {
        if (-not $bridge.HasExited) {
            Stop-Process -Id $bridge.Id -ErrorAction SilentlyContinue
        }
        $bridge.Dispose()
    }
    Wait-AppCondition 'AgentBro to receive the bridge tool event' {
        Select-String -LiteralPath $stdoutPath -SimpleMatch "[ToolUse] session=$($Event.session_id) tool=Read" -Quiet
    }
}

function Test-InstalledIsland([string]$ProjectName) {
    $windowsPowerShell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $start = [System.Diagnostics.ProcessStartInfo]::new($windowsPowerShell)
    $scriptPath = Join-Path $PSScriptRoot 'test-windows-island.ps1'
    foreach ($argument in @('-NoProfile', '-NonInteractive', '-Sta', '-File', $scriptPath,
            '-AppProcessId', [string]$app.Id, '-ProjectName', $ProjectName)) {
        $start.ArgumentList.Add($argument)
    }
    if ($VerifyProjectLock) {
        $start.ArgumentList.Add('-LockProjectPath')
        $start.ArgumentList.Add($lockProject)
    }
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $tester = [System.Diagnostics.Process]::Start($start)
    try {
        $output = $tester.StandardOutput.ReadToEndAsync()
        $errors = $tester.StandardError.ReadToEndAsync()
        if (-not $tester.WaitForExit(120000)) {
            throw 'The installed Island desktop interaction check timed out'
        }
        Write-Output ($output.GetAwaiter().GetResult())
        if ($tester.ExitCode -ne 0) {
            throw "The installed Island did not pass the desktop interaction check: $($errors.GetAwaiter().GetResult())"
        }
    } finally {
        if (-not $tester.HasExited) { Stop-Process -Id $tester.Id -ErrorAction SilentlyContinue }
        $tester.Dispose()
    }
}

$hookReady = $false
try {
    Wait-AppCondition 'the installed app to listen on its Hook port' {
        Get-NetTCPConnection -LocalPort $port -State Listen -OwningProcess $app.Id -ErrorAction SilentlyContinue
    }
    $hookReady = $true
    Wait-AppCondition 'AgentBro to restore the Unicode session' {
        Select-String -LiteralPath $stdoutPath -SimpleMatch "session ${restoredSession}: project=$restoredProjectName" -Quiet
    }
    Write-Output 'The existing Unicode session was restored at startup.'
    Wait-AppCondition 'the bridge and Claude Hook configuration to be deployed' {
        (Test-Path -LiteralPath $bridgePath -PathType Leaf) -and
            (Select-String -LiteralPath $stdoutPath -SimpleMatch 'Claude Code hooks installed successfully' -Quiet)
    }
    Select-String -LiteralPath $stdoutPath -SimpleMatch 'Startup:', 'Listening on TCP:' |
        Select-Object -ExpandProperty Line
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
    }

    Send-ClaudeToolEvent $bashPath @('-c', $bridgeHooks[0].command) $event
    Write-Output 'Windows installation, Claude Hook repair, and event delivery through Git Bash passed.'
    if ($VerifyProjectLock) {
        Test-InstalledIsland 'Native project lock fixture'
        $restored = Join-Path $lockProject '.agents/skills/native-restored/SKILL.md'
        $normalized = [IO.File]::ReadAllText($restored).Replace("`r`n", "`n")
        $actualHash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($normalized)))
        if ($actualHash -ne 'c00eeea0e13e74fe4a9d84ba0a8542205a1b736d65f13134fe1a6647eb14976f') { throw 'Restored Skill bytes differ from the pinned GitHub file' }
        if (Test-Path -LiteralPath (Join-Path $lockProject '.agents/skills/native-mismatch')) {
            throw 'The mismatched Skill was written to the project'
        }
        if ((Get-Content -LiteralPath (Join-Path $lockProject '.agents/skills/native-existing/SKILL.md') -Raw).Trim() -ne 'User fixture') {
            throw 'The existing project Skill was overwritten'
        }
        if ((Get-FileHash -LiteralPath $lockPath).Hash -ne $originalLockHash) { throw 'The project lock file changed' }
        Write-Output 'Verified the pinned GitHub Skill bytes, hash mismatch rejection, and preservation of the existing Skill and lock file.'
    } elseif ($VerifyIsland) {
        # The Git Bash Hook can report a short-lived intermediary as its parent PID.
        # The test PowerShell process stays alive as the parent of this UI fixture.
        $event.session_id = $restoredSession
        $event.cwd = $restoredWorkspace
        $event.tool_input.file_path = Join-Path $restoredWorkspace 'smoke.txt'
        Send-ClaudeToolEvent $bridgePath @('--source', 'claude-code') $event
        Test-InstalledIsland $restoredProjectName
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
    foreach ($diagnosticLog in @($logPath, $stdoutPath, $stderrPath)) {
        if (Test-Path -LiteralPath $diagnosticLog -PathType Leaf) {
            Write-Output "Last lines of $diagnosticLog"
            Get-Content -LiteralPath $diagnosticLog -Tail 30
        }
    }
    if ($SymbolsDirectory -and -not $hookReady -and -not $app.HasExited) {
        try {
            Write-StartupStacks
        } catch {
            Write-Output "Could not capture AgentBro startup stacks: $_"
        }
    }
    throw
} finally {
    if (-not $app.HasExited) {
        Stop-Process -Id $app.Id -ErrorAction SilentlyContinue
    }
}
