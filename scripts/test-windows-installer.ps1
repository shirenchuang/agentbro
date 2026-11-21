param(
    [Parameter(Mandatory)]
    [string]$Installer,
    [switch]$VerifyIsland,
    [switch]$VerifyQwenWork,
    [switch]$VerifyPackManifest,
    [switch]$TracePermissionUi,
    [string]$SymbolsDirectory,
    [switch]$VerifyDrag,
    [switch]$VerifyStartupStackSampler
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
if ($TracePermissionUi -and ($env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or -not ($VerifyIsland -or $VerifyPackManifest))) {
    throw 'Permission tracing requires an Island check on an isolated GitHub-hosted runner'
}

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

$packFixtures = @()
$packImportPath = $null
$packExportPath = $null
if ($VerifyPackManifest) {
    $manifestDirectory = Join-Path $stage '共享 技能'
    $packImportPath = Join-Path $manifestDirectory '导入 技能包.agentbro-pack.json'
    $packExportPath = Join-Path $manifestDirectory '导出 技能包.agentbro-pack.json'
    $null = New-Item -ItemType Directory -Path $manifestDirectory
    $profileDirectory = if ($env:HOME) { $env:HOME } else { $env:USERPROFILE }
    $skillRoot = Join-Path $profileDirectory '.agentbro\skills'
    $suffix = [Guid]::NewGuid().ToString('N').Substring(0, 8)
    $skillIds = @("windows-smoke-zulu-$suffix", "windows-smoke-alpha-$suffix")
    foreach ($skillId in $skillIds) {
        $skillDirectory = Join-Path $skillRoot $skillId
        $null = New-Item -ItemType Directory -Path $skillDirectory -Force
        $packFixtures += $skillDirectory
        @"
---
name: $skillId
description: Windows native manifest fixture
---
Windows native manifest fixture.
"@ | Set-Content -LiteralPath (Join-Path $skillDirectory 'SKILL.md') -Encoding utf8
    }
    @{
        format = 'agentbro-skill-pack'
        version = 1
        name = "Native Windows pack $suffix"
        description = 'Windows native manifest round trip'
        tags = @('native-smoke', '回归')
        skillIds = $skillIds
    } | ConvertTo-Json | Set-Content -LiteralPath $packImportPath -Encoding utf8
}

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

function Write-LocalStartupStacks {
    $pdb = Get-ChildItem -LiteralPath $SymbolsDirectory -Filter agentbro.pdb -Recurse -File |
        Select-Object -First 1
    if (-not $pdb) { throw 'The Windows artifact does not contain the AgentBro PDB' }

    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) {
        throw 'The Visual Studio toolset locator is unavailable'
    }
    $visualStudio = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $visualStudio) { throw 'The Visual C++ x64 toolset is unavailable' }
    & (Join-Path $visualStudio 'Common7\Tools\Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation |
        Out-Host

    $source = Join-Path $PSScriptRoot 'windows-startup-stacks.cpp'
    Push-Location $stage
    try {
        & cl.exe /nologo /std:c++17 /EHsc /W4 /WX $source /link dbghelp.lib | Out-Host
        if ($LASTEXITCODE -ne 0) { throw 'Could not compile the Windows startup sampler' }
    } finally {
        Pop-Location
    }

    $sampler = Join-Path $stage 'windows-startup-stacks.exe'
    $start = [System.Diagnostics.ProcessStartInfo]::new($sampler)
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.ArgumentList.Add([string]$app.Id)
    $start.ArgumentList.Add($pdb.DirectoryName)
    $capture = [System.Diagnostics.Process]::Start($start)
    try {
        $output = $capture.StandardOutput.ReadToEndAsync()
        $errors = $capture.StandardError.ReadToEndAsync()
        if (-not $capture.WaitForExit(15000)) {
            $capture.Kill()
            throw 'Local startup stack capture timed out'
        }
        $stacks = $output.GetAwaiter().GetResult()
        Write-Output $stacks
        if ($capture.ExitCode -ne 0) {
            throw "Local startup stack capture failed: $($errors.GetAwaiter().GetResult())"
        }
        if ($stacks -notmatch '(?m)^  agentbro![^+]+\+0x') {
            throw 'Local startup stacks do not contain a resolved AgentBro frame'
        }
    } finally {
        $capture.Dispose()
    }
}

function Write-StartupStacks {
    if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq 'X64') {
        try {
            Write-LocalStartupStacks
            return
        } catch {
            if ($VerifyStartupStackSampler) { throw }
            Write-Output "Local startup stack capture unavailable: $_"
        }
    } elseif ($VerifyStartupStackSampler) {
        throw 'The local startup stack sampler requires x64 Windows'
    }

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

function Test-InstalledIsland([string]$ProjectName, [string]$SessionId, [string]$Workspace) {
    $windowsPowerShell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $start = [System.Diagnostics.ProcessStartInfo]::new($windowsPowerShell)
    $scriptPath = Join-Path $PSScriptRoot 'test-windows-island.ps1'
    foreach ($argument in @('-NoProfile', '-NonInteractive', '-Mta', '-File', $scriptPath,
            '-AppProcessId', [string]$app.Id, '-ProjectName', $ProjectName,
            '-SessionId', $SessionId, '-Workspace', $Workspace)) {
        $start.ArgumentList.Add($argument)
    }
    if ($VerifyPackManifest) {
        foreach ($argument in @('-PackImportPath', $packImportPath, '-PackExportPath', $packExportPath)) {
            $start.ArgumentList.Add($argument)
        }
    }
    if ($VerifyQwenWork) { $start.ArgumentList.Add('-VerifyQwenWork') }
    if ($TracePermissionUi) { $start.ArgumentList.Add('-TracePermissionUi') }
    if ($VerifyDrag) { $start.ArgumentList.Add('-VerifyDrag') }
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $tester = [System.Diagnostics.Process]::Start($start)
    try {
        $output = $tester.StandardOutput.ReadToEndAsync()
        $errors = $tester.StandardError.ReadToEndAsync()
        $desktopTimeout = if ($VerifyPackManifest) { 180000 } else { 120000 }
        if (-not $tester.WaitForExit($desktopTimeout)) {
            throw 'The installed desktop interaction check timed out'
        }
        Write-Output ($output.GetAwaiter().GetResult())
        if ($tester.ExitCode -ne 0) {
            throw "The installed app did not pass the desktop interaction check: $($errors.GetAwaiter().GetResult())"
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
    if ($VerifyStartupStackSampler) {
        Write-StartupStacks
        Write-Output 'Local stack sampler resolved AgentBro frames before the installed desktop check.'
    }
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
    if ($VerifyIsland -or $VerifyPackManifest -or $VerifyDrag -or $VerifyQwenWork) {
        # The Git Bash Hook can report a short-lived intermediary as its parent PID.
        # The test PowerShell process stays alive as the parent of this UI fixture.
        $event.session_id = $restoredSession
        $event.cwd = $restoredWorkspace
        $event.tool_input.file_path = Join-Path $restoredWorkspace 'smoke.txt'
        Send-ClaudeToolEvent $bridgePath @('--source', 'claude-code') $event
        Test-InstalledIsland $restoredProjectName $restoredSession $restoredWorkspace
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
    foreach ($fixture in $packFixtures) {
        Remove-Item -LiteralPath $fixture -Recurse -ErrorAction SilentlyContinue
    }
}
