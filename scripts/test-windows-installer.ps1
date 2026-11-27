param(
    [Parameter(Mandatory)]
    [string]$Installer,
    [switch]$VerifyIsland,
    [switch]$VerifyQwenWork,
    [switch]$VerifyPackManifest,
    [switch]$VerifyPackArchive,
    [switch]$VerifyPinnedPackManifest,
    [string]$PackManifestLink,
    [switch]$TracePermissionUi,
    [switch]$TraceQuestionUi,
    [string]$SymbolsDirectory,
    [switch]$VerifyDrag,
    [switch]$VerifyDisplayChange,
    [switch]$CloseStartupSettings,
    [switch]$AutomationFocusPermission,
    [switch]$VerifyGlobalShortcuts,
    [switch]$VerifyStartupStackSampler,
    [switch]$VerifyStartupLock,
    [switch]$VerifyStartupFailure,
    [switch]$VerifyDiagnosticsExport,
    [switch]$VerifyAgentPackageConsole,
    [switch]$VerifyRegisteredAgentApp
)

$ErrorActionPreference = 'Stop'
$stage = Join-Path $env:RUNNER_TEMP "AgentBro smoke $([Guid]::NewGuid().ToString('N'))"
$installDir = Join-Path $stage 'installed app'
$fixtureBin = Join-Path $stage 'bin'
$env:CLAUDE_CONFIG_DIR = Join-Path $stage 'Claude config'
$fixtureSearchPath = @($fixtureBin)
New-Item -ItemType Directory -Path $fixtureBin, $env:CLAUDE_CONFIG_DIR | Out-Null
Set-Content -LiteralPath (Join-Path $fixtureBin 'claude.cmd') -Value '@echo 2.0.0' -Encoding ascii
if ($VerifyAgentPackageConsole) {
    if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or -not $env:RUNNER_TEMP) {
        throw 'The package manager fixture requires an isolated GitHub-hosted runner'
    }
    $env:AGENTBRO_PACKAGE_CONSOLE_RESULT = Join-Path $stage 'package-console.json'
    $env:AGENTBRO_PACKAGE_CONSOLE_HOST = (Get-Process -Id $PID).Path
    $env:AGENTBRO_PACKAGE_CONSOLE_PROBE = Join-Path $PSScriptRoot 'probe-windows-package-console.ps1'
    $env:AGENTBRO_PACKAGE_FIXTURE_RESUME = Join-Path $stage 'package-resume'
    $env:AGENTBRO_PACKAGE_FIXTURE_UPDATED = Join-Path $stage 'package-updated'
    $env:AGENTBRO_PACKAGE_FIXTURE_CHANGED_VERSION = Join-Path $stage 'package-changed-version'
    $env:AGENTBRO_PACKAGE_FIXTURE_NEW_TAG = Join-Path $stage 'package-new-tag'
    $env:AGENTBRO_PACKAGE_OUTPUT_HOLDER = Join-Path $stage 'hold-package-output.cjs'
    @'
const { spawn } = require('node:child_process')
const { existsSync, writeFileSync } = require('node:fs')

if (process.argv[2] === 'hold') {
  writeFileSync(__filename + '.pid', String(process.pid))
  const deadline = Date.now() + 60000
  const check = setInterval(() => {
    if (existsSync(__filename + '.release') || !existsSync(__dirname) || Date.now() >= deadline) {
      clearInterval(check)
    }
  }, 50)
} else {
  // On Windows, Node terminates non-detached children when the launcher exits.
  const holder = spawn(process.execPath, [__filename, 'hold'], {
    detached: true,
    stdio: ['ignore', 'inherit', 'inherit'],
    windowsHide: true,
  })
  holder.unref()
}
'@ | Set-Content -LiteralPath $env:AGENTBRO_PACKAGE_OUTPUT_HOLDER -Encoding ascii
    $env:npm_config_offline = 'true'
    $env:npm_config_cache = Join-Path $stage 'isolated npm cache'
    $env:npm_config_prefix = Join-Path $stage 'isolated npm prefix'
    $env:AGENTBRO_PACKAGE_FIXTURE_PREFIX = $fixtureBin
    $packageRelative = 'node_modules\@anthropic-ai\claude-code'
    $env:AGENTBRO_PACKAGE_FIXTURE_MANIFEST = Join-Path $fixtureBin "$packageRelative\package.json"
    $fixtureLauncherText = @'
@ECHO off
SET "dp0=%~dp0"
node "%dp0%\node_modules\@anthropic-ai\claude-code\cli.js" %*
'@
    $fixtureLauncherBackup = Join-Path $stage 'claude.cmd.fixture'
    Set-Content -LiteralPath $fixtureLauncherBackup -Value $fixtureLauncherText -Encoding ascii
    $alternatePrefix = Join-Path $stage 'alternate npm prefix'
    foreach ($prefix in @($fixtureBin, $env:npm_config_prefix, $alternatePrefix)) {
        $package = Join-Path $prefix $packageRelative
        $null = New-Item -ItemType Directory -Path $package -Force
        $version = if ($prefix -eq $env:npm_config_prefix) { '9.0.0' } else { '2.0.0' }
        "{`"name`":`"@anthropic-ai/claude-code`",`"version`":`"$version`",`"bin`":{`"claude`":`"./cli.js`"}}" |
            Set-Content -LiteralPath (Join-Path $package 'package.json') -Encoding ascii
        Set-Content -LiteralPath (Join-Path $package 'cli.js') -Value 'console.log(require("./package.json").version)' -Encoding ascii
        Copy-Item -LiteralPath $fixtureLauncherBackup -Destination (Join-Path $prefix 'claude.cmd') -Force
    }
    $fallbackBin = Join-Path $stage 'fallback bin'
    $null = New-Item -ItemType Directory -Path $fallbackBin
    Copy-Item -LiteralPath (Join-Path $env:SystemRoot 'System32\whoami.exe') -Destination (Join-Path $fallbackBin 'claude.exe')
    $fixtureAlias = Join-Path $stage 'active npm prefix'
    $null = New-Item -ItemType Junction -Path $fixtureAlias -Target $fixtureBin
    $fixtureSearchPath = @($fixtureAlias, $fixtureBin, $fallbackBin)
    $fixtureNpm = @'
@echo off
if /i "%~1"=="list" (
    if "%~3"=="@anthropic-ai/claude-code" (
        if not "%~6"=="--no-update-notifier" exit /b 1
        echo {"dependencies":{"@anthropic-ai/claude-code":{"version":"9.0.0"}}}
    )
    exit /b 0
)
if /i "%~1"=="view" (
    if "%~2"=="@anthropic-ai/claude-code@latest" (
        if not "%~5 %~6 %~7"=="--no-update-notifier --fetch-retries=0 --fetch-timeout=4000" exit /b 1
        if exist "%AGENTBRO_PACKAGE_FIXTURE_NEW_TAG%" (
            echo 999.0.0
        ) else (
            echo 1.0.0
        )
    )
    exit /b 0
)
if "%~1 %~2 %~3"=="install -g @anthropic-ai/claude-code@999.0.0" (
    >> "%AGENTBRO_PACKAGE_CONSOLE_RESULT%.mutation" echo install
    if /i not "%npm_config_prefix%"=="%AGENTBRO_PACKAGE_FIXTURE_PREFIX%" exit /b 2
    >> "%AGENTBRO_PACKAGE_CONSOLE_RESULT%.trace" echo npm fixture entered the update
    set "agentbro_retry="
    if exist "%AGENTBRO_PACKAGE_CONSOLE_RESULT%" set "agentbro_retry=1"
    "%AGENTBRO_PACKAGE_CONSOLE_HOST%" -NoProfile -NonInteractive -File "%AGENTBRO_PACKAGE_CONSOLE_PROBE%"
    if errorlevel 1 (
        >> "%AGENTBRO_PACKAGE_CONSOLE_RESULT%.trace" echo PowerShell probe exited with an error
        exit /b 1
    )
    if exist "%AGENTBRO_PACKAGE_FIXTURE_CHANGED_VERSION%" set "agentbro_retry=1"
    if defined agentbro_retry (
        >"%npm_config_prefix%\node_modules\@anthropic-ai\claude-code\package.json" echo {"name":"@anthropic-ai/claude-code","version":"999.0.0","bin":{"claude":"./cli.js"}}
        type nul > "%AGENTBRO_PACKAGE_FIXTURE_UPDATED%"
    )
    echo agentbro-package-fixture-stdout
    >&2 echo agentbro-package-fixture-stderr
    if exist "%AGENTBRO_PACKAGE_FIXTURE_CHANGED_VERSION%" (
        node "%AGENTBRO_PACKAGE_OUTPUT_HOLDER%"
        if errorlevel 1 exit /b 1
    )
    exit /b 0
)
if "%~1 %~2 %~3"=="uninstall -g @anthropic-ai/claude-code" (
    >> "%AGENTBRO_PACKAGE_CONSOLE_RESULT%.mutation" echo uninstall
    if /i not "%npm_config_prefix%"=="%AGENTBRO_PACKAGE_FIXTURE_PREFIX%" exit /b 2
    if not exist "%AGENTBRO_PACKAGE_CONSOLE_RESULT%.uninstall-failed" (
        type nul > "%AGENTBRO_PACKAGE_CONSOLE_RESULT%.uninstall-failed"
        >&2 echo fixture uninstall failed
        exit /b 1
    )
    if exist "%AGENTBRO_PACKAGE_CONSOLE_RESULT%.uninstall" (
        del /q "%npm_config_prefix%\claude.cmd"
        if exist "%npm_config_prefix%\claude.cmd" exit /b 1
    ) else (
        type nul > "%AGENTBRO_PACKAGE_CONSOLE_RESULT%.uninstall"
    )
    exit /b 0
)
exit /b 1
'@
    Set-Content -LiteralPath (Join-Path $fixtureBin 'npm.cmd') -Value ($fixtureNpm -replace '\r?\n', "`r`n") -Encoding ascii
    $unmanagedSkill = Join-Path $env:CLAUDE_CONFIG_DIR 'skills\uninstall-fixture'
    $null = New-Item -ItemType Directory -Path $unmanagedSkill -Force
    @'
---
name: Uninstall fixture
description: Check that a failed uninstall preserves a local skill.
---

This Skill belongs to the test Agent.
'@ | Set-Content -LiteralPath (Join-Path $unmanagedSkill 'SKILL.md') -Encoding ascii
}
$env:Path = ($fixtureSearchPath + @($env:Path)) -join ';'

$listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
$listener.Start()
$port = $listener.LocalEndpoint.Port
$listener.Stop()
$env:AGENTBRO_HOOK_PORT = "$port"
if ($TracePermissionUi -and ($env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or -not ($VerifyIsland -or $VerifyPackManifest))) {
    throw 'Permission tracing requires an Island check on an isolated GitHub-hosted runner'
}
if ($TraceQuestionUi -and ($env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or -not $VerifyIsland)) {
    throw 'Question tracing requires an Island check on an isolated GitHub-hosted runner'
}
if ($VerifyGlobalShortcuts -and ($env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or -not $VerifyIsland)) {
    throw 'Global shortcut fixtures require an Island check on an isolated GitHub-hosted runner'
}

function Write-WindowsTerminalStarts([string]$SourceIdentifier, [object[]]$Baseline, [int]$AgentBroPid) {
    $starts = @(Get-Event -SourceIdentifier $SourceIdentifier -ErrorAction SilentlyContinue | ForEach-Object {
        $process = $_.SourceEventArgs.NewEvent
        [PSCustomObject]@{
            Name = $process.ProcessName
            Id = [int]$process.ProcessId
            ParentId = [int]$process.ParentProcessId
            StartedAt = [DateTime]::FromFileTimeUtc([long]$process.TIME_CREATED)
        }
    })
    $agentBroStart = $starts | Where-Object { $_.Id -eq $AgentBroPid -and $_.Name -eq 'agentbro.exe' } | Select-Object -First 1
    Write-Output "Windows process trace recorded $($starts.Count) starts. AgentBro start observed: $([bool]$agentBroStart)."
    $processes = $Baseline + $starts
    $terminalNames = @('wsl.exe', 'wslhost.exe', 'WindowsTerminal.exe')
    foreach ($terminal in $starts | Where-Object { $_.Name -in $terminalNames } | Sort-Object StartedAt) {
        $chain = [Collections.Generic.List[string]]::new()
        $current = $terminal
        for ($depth = 0; $current -and $depth -lt 8; $depth += 1) {
            $chain.Add("$($current.Name) ($($current.Id))")
            if (-not $current.ParentId) { break }
            $parent = $processes | Where-Object {
                $_.Id -eq $current.ParentId -and $_.StartedAt -le $current.StartedAt
            } | Sort-Object StartedAt -Descending | Select-Object -First 1
            if (-not $parent) { $chain.Add("unknown PID $($current.ParentId)") }
            $current = $parent
        }
        Write-Output "Windows terminal start at $($terminal.StartedAt.ToString('O')): $($chain -join ' <- ')"
    }
    foreach ($terminal in $Baseline | Where-Object { $_.Name -in $terminalNames }) {
        Write-Output "Windows terminal was already running at trace start: $($terminal.Name) ($($terminal.Id)), parent PID $($terminal.ParentId), started $($terminal.StartedAt.ToString('O'))."
    }
}

$processTraceId = "AgentBroInstallerProcessStarts-$PID"
$processTrace = $false
$processBaseline = @()
if ($TracePermissionUi) {
    try {
        Register-CimIndicationEvent -ClassName Win32_ProcessStartTrace -SourceIdentifier $processTraceId -ErrorAction Stop | Out-Null
        $processTrace = [bool](Get-EventSubscriber -SourceIdentifier $processTraceId -ErrorAction Stop)
        Write-Output "Windows process start trace subscribed: $processTrace."
        $processBaseline = @(Get-CimInstance -ClassName Win32_Process -Property Name,ProcessId,ParentProcessId,CreationDate |
            Where-Object { $_.CreationDate } | ForEach-Object {
                [PSCustomObject]@{
                    Name = $_.Name
                    Id = [int]$_.ProcessId
                    ParentId = [int]$_.ParentProcessId
                    StartedAt = $_.CreationDate.ToUniversalTime()
                }
            })
    } catch {
        Write-Output "Could not initialize Windows process traces: $_"
    }
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
$packArchivePath = $null
$pinnedPackPath = $null
$pinnedPackExportPath = $null
if ($PackManifestLink) {
    if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
        throw 'The GitHub Skill Pack link fixture requires an isolated GitHub-hosted runner'
    }
    $fixturePath = Join-Path $PSScriptRoot 'fixtures\github-skill-pack.agentbro-pack.json'
    $manifest = Get-Content -LiteralPath $fixturePath -Raw -Encoding utf8 | ConvertFrom-Json
    $profileDirectory = if ($env:HOME) { $env:HOME } else { $env:USERPROFILE }
    foreach ($skillId in $manifest.skillIds) {
        $skillDirectory = Join-Path $profileDirectory ".agentbro\skills\$skillId"
        $null = New-Item -ItemType Directory -Path $skillDirectory
        $packFixtures += $skillDirectory
        @"
---
name: $skillId
description: Public GitHub Skill Pack link fixture
---
Windows native Skill Pack link fixture.
"@ | Set-Content -LiteralPath (Join-Path $skillDirectory 'SKILL.md') -Encoding utf8
    }
}
if ($VerifyPinnedPackManifest) {
    if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
        throw 'The pinned Git checkout fixture requires an isolated GitHub-hosted runner'
    }
    $repo = Join-Path $stage 'Pinned Pack Git fixture'
    $members = @('review', 'release', 'support')
    $recorded = @{}
    foreach ($member in $members) {
        $relativePath = "skills/$member/SKILL.md"
        $source = Join-Path $repo $relativePath
        New-Item -ItemType Directory -Path (Split-Path $source) -Force | Out-Null
        $recorded[$member] = "---`nname: Pinned $member`n---`nPinned fixture commit at $relativePath.`n"
        [IO.File]::WriteAllText($source, $recorded[$member], [Text.UTF8Encoding]::new($false))
    }
    function Invoke-FixtureGit([string[]]$Arguments) {
        & git -C $repo -c 'user.name=AgentBro Fixture' -c 'user.email=fixture@example.invalid' -c commit.gpgSign=false @Arguments
        if ($LASTEXITCODE -ne 0) { throw "Git failed in the pinned Pack fixture: $Arguments" }
    }
    Invoke-FixtureGit @('init', '-b', 'main') | Out-Host
    Invoke-FixtureGit @('add', '.') | Out-Host
    Invoke-FixtureGit @('commit', '-m', 'Pinned Skill') | Out-Host
    $commit = Invoke-FixtureGit @('rev-parse', 'HEAD')
    foreach ($member in $members) {
        $source = Join-Path $repo "skills/$member/SKILL.md"
        [IO.File]::WriteAllText($source, $recorded[$member].Replace('Pinned fixture commit', 'Later branch content'), [Text.UTF8Encoding]::new($false))
    }
    Invoke-FixtureGit @('add', '.') | Out-Host
    Invoke-FixtureGit @('commit', '-m', 'Advance fixture branch') | Out-Host
    $repoUri = ([uri]::new($repo)).AbsoluteUri
    $env:GIT_CONFIG_COUNT = '1'
    $env:GIT_CONFIG_KEY_0 = "url.$repoUri.insteadOf"
    $env:GIT_CONFIG_VALUE_0 = 'https://github.com/agentbro-fixture/windows-pack-smoke.git'
    $suffix = [guid]::NewGuid().ToString('N').Substring(0, 8)
    $profileDirectory = if ($env:HOME) { $env:HOME } else { $env:USERPROFILE }
    $fingerprints = @{}
    $sources = @{}
    $skillIds = foreach ($member in $members) {
        $id = "windows-pinned-$member-$suffix"
        $packFixtures += Join-Path $profileDirectory ".agentbro\skills\$id"
        if ($member -eq $members[0]) {
            $packFixtures += Join-Path $profileDirectory ".agentbro\skills\${id}-restored"
            $packFixtures += Join-Path $env:CLAUDE_CONFIG_DIR "skills\${id}-restored"
        } else {
            $packFixtures += Join-Path $env:CLAUDE_CONFIG_DIR "skills\$id"
        }
        $content = $recorded[$member]
        $bytes = [Text.Encoding]::UTF8.GetBytes("SKILL.md`0$([Text.Encoding]::UTF8.GetByteCount($content))`0$content")
        $fingerprints[$id] = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
        $sources[$id] = @{
            repository = 'agentbro-fixture/windows-pack-smoke'
            commit = $commit
            skillPath = "skills/$member/SKILL.md"
        }
        $id
    }
    $pinnedPackPath = Join-Path $stage 'Pinned Pack.agentbro-pack.json'
    $pinnedPackExportPath = Join-Path $stage 'Re-exported pinned Pack.agentbro-pack.json'
    @{
        format = 'agentbro-skill-pack'
        version = 4
        name = "Native pinned Pack $suffix"
        description = ''
        tags = @()
        skillIds = @($skillIds)
        skillFingerprints = $fingerprints
        skillSources = $sources
        suggestedAgentIds = @('claude-code', 'unavailable-fixture')
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $pinnedPackPath -Encoding utf8
}
if ($VerifyPackManifest -or $VerifyPackArchive) {
    $manifestDirectory = Join-Path $stage '共享 技能'
    $packImportPath = Join-Path $manifestDirectory '导入 技能包.agentbro-pack.json'
    if ($VerifyPackManifest) { $packExportPath = Join-Path $manifestDirectory '导出 技能包.agentbro-pack.json' }
    $null = New-Item -ItemType Directory -Path $manifestDirectory
    $profileDirectory = if ($env:HOME) { $env:HOME } else { $env:USERPROFILE }
    $skillRoot = Join-Path $profileDirectory '.agentbro\skills'
    $suffix = [Guid]::NewGuid().ToString('N').Substring(0, 8)
    $skillIds = @("windows-smoke-zulu-$suffix", "windows-smoke-alpha-$suffix")
    if ($VerifyPackArchive) {
        $packArchivePath = Join-Path $manifestDirectory '迁移 技能包.agentbro-pack.zip'
        $skillIds += "windows-smoke-extra-$suffix"
        $packFixtures += Join-Path $skillRoot "$($skillIds[0])-restored"
        $packFixtures += Join-Path $skillRoot ($skillIds[0].ToUpperInvariant())
    }
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
$shortcutConfigPath = $null
if ($VerifyGlobalShortcuts) {
    $configDirectory = Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'agentbro'
    $shortcutConfigPath = Join-Path $configDirectory 'config.json'
    if (Test-Path -LiteralPath $shortcutConfigPath) { throw 'The runner already has an AgentBro config' }
    $null = New-Item -ItemType Directory -Path $configDirectory -Force
    @{
        soundEnabled = $false
        soundVolume = 0.5
        autoHide = $false
        smartSuppression = $false
        completionTimeout = 5
        showTokenUsage = $true
        theme = 'system'
        analyticsEnabled = $false
        globalShortcut = 'Control+Alt+F8'
        shortcutDeny = 'Control+Alt+F9'
        shortcutDenyEnabled = $true
        shortcutSkip = 'Control+Alt+F10'
        shortcutSkipEnabled = $true
    } | ConvertTo-Json | Set-Content -LiteralPath $shortcutConfigPath -Encoding utf8
}
$stderrPath = Join-Path $stage 'agentbro.stderr.log'

function Test-DuplicateStartup([string]$State) {
    $launchLog = Join-Path $stage "duplicate-$State.stdout.log"
    $launchError = Join-Path $stage "duplicate-$State.stderr.log"
    $duplicate = Start-Process -FilePath $appPath -WorkingDirectory $installDir -PassThru `
        -RedirectStandardOutput $launchLog -RedirectStandardError $launchError
    try {
        if (-not $duplicate.WaitForExit(10000)) {
            throw "Duplicate AgentBro launch stayed alive during $State."
        }
        if ($duplicate.ExitCode -ne 0 -or
            (Select-String -LiteralPath $launchLog -SimpleMatch 'Startup: creating configured windows' -Quiet)) {
            throw "Duplicate AgentBro launch did not exit before window creation during $State."
        }
        Write-Output "Duplicate Windows launch exited before WebView2 during $State."
    } catch {
        Get-Content -LiteralPath $launchLog, $launchError -Tail 20 -ErrorAction SilentlyContinue
        throw
    } finally {
        if (-not $duplicate.HasExited) {
            Stop-Process -Id $duplicate.Id -ErrorAction SilentlyContinue
            $null = $duplicate.WaitForExit(3000)
        }
        $duplicate.Dispose()
    }
}

if ($VerifyStartupLock) {
    $lockDirectory = Join-Path $env:USERPROFILE '.agentbro'
    $null = New-Item -ItemType Directory -Path $lockDirectory -Force
    $startupLockPath = Join-Path $lockDirectory "windows-startup-$port.lock"
    $fixtureLock = [IO.File]::Open($startupLockPath, 'OpenOrCreate', 'Write', 'ReadWrite')
    try {
        $fixtureLock.Lock(0, 1)
        if (Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue) {
            throw 'The cold-start exclusion fixture requires an unoccupied Hook port.'
        }
        Test-DuplicateStartup 'cold-start'
    } finally {
        $fixtureLock.Dispose()
    }
}

if ($VerifyStartupFailure) {
    $windowsPowerShell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    & $windowsPowerShell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'test-windows-startup-error.ps1') -AppPath $appPath -Stage $stage
    if ($LASTEXITCODE -ne 0) {
        throw 'The installed app failed the Windows startup error check.'
    }
}

if ($VerifyDrag) { $env:AGENTBRO_TRACE_ISLAND_DRAG = '1' }
$appEnvironment = @{}
if ($TraceQuestionUi) {
    $debugListener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    $debugListener.Start()
    $questionTracePort = $debugListener.LocalEndpoint.Port
    $debugListener.Stop()
    $appEnvironment['WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS'] = "--remote-debugging-port=$questionTracePort --remote-debugging-address=127.0.0.1"
    $appEnvironment['WEBVIEW2_USER_DATA_FOLDER'] = Join-Path $stage 'question trace WebView2 profile'
}
$app = Start-Process -FilePath $appPath -WorkingDirectory $installDir -PassThru `
    -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath -Environment $appEnvironment
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
    if ($VerifyPackManifest -or $VerifyPackArchive) {
        foreach ($argument in @('-PackImportPath', $packImportPath)) {
            $start.ArgumentList.Add($argument)
        }
    }
    if ($VerifyPackManifest) {
        $start.ArgumentList.Add('-PackExportPath')
        $start.ArgumentList.Add($packExportPath)
    }
    if ($VerifyPackArchive) {
        $start.ArgumentList.Add('-PackArchivePath')
        $start.ArgumentList.Add($packArchivePath)
    }
    if ($VerifyPinnedPackManifest) {
        foreach ($argument in @('-PinnedPackImportPath', $pinnedPackPath, '-PinnedPackExportPath', $pinnedPackExportPath)) {
            $start.ArgumentList.Add($argument)
        }
    }
    if ($PackManifestLink) {
        $start.ArgumentList.Add('-PackManifestLink')
        $start.ArgumentList.Add($PackManifestLink)
        Set-Clipboard -Value $PackManifestLink
    }
    if ($VerifyQwenWork) { $start.ArgumentList.Add('-VerifyQwenWork') }
    if ($VerifyDiagnosticsExport) {
        $start.ArgumentList.Add('-DiagnosticsExportPath')
        $start.ArgumentList.Add((Join-Path $stage 'AgentBro startup diagnostics.zip'))
    }
    if ($VerifyAgentPackageConsole) { $start.ArgumentList.Add('-VerifyAgentPackageConsole') }
    if ($VerifyRegisteredAgentApp) { $start.ArgumentList.Add('-VerifyRegisteredAgentApp') }
    if ($TracePermissionUi) { $start.ArgumentList.Add('-TracePermissionUi') }
    if ($TraceQuestionUi) {
        $start.ArgumentList.Add('-QuestionTracePort')
        $start.ArgumentList.Add("$questionTracePort")
    }
    if ($VerifyDrag) { $start.ArgumentList.Add('-VerifyDrag') }
    if ($VerifyDisplayChange) { $start.ArgumentList.Add('-VerifyDisplayChange') }
    if ($CloseStartupSettings) { $start.ArgumentList.Add('-CloseStartupSettings') }
    if ($AutomationFocusPermission) { $start.ArgumentList.Add('-AutomationFocusPermission') }
    if ($VerifyGlobalShortcuts) { $start.ArgumentList.Add('-VerifyGlobalShortcuts') }
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $tester = [System.Diagnostics.Process]::Start($start)
    try {
        $output = $tester.StandardOutput.ReadToEndAsync()
        $errors = $tester.StandardError.ReadToEndAsync()
        $desktopTimeout = if ($VerifyPackManifest -or $VerifyPackArchive -or $VerifyPinnedPackManifest -or $PackManifestLink -or
            $VerifyAgentPackageConsole -or $VerifyRegisteredAgentApp -or $VerifyDiagnosticsExport) { 180000 } else { 120000 }
        if (-not $tester.WaitForExit($desktopTimeout)) {
            throw 'The installed desktop interaction check timed out'
        }
        Write-Output ($output.GetAwaiter().GetResult())
        if ($tester.ExitCode -ne 0) {
            if ($VerifyAgentPackageConsole) {
                $fixtureTrace = "$env:AGENTBRO_PACKAGE_CONSOLE_RESULT.trace"
                if (Test-Path -LiteralPath $fixtureTrace -PathType Leaf) {
                    Write-Output 'Agent package fixture progress:'
                    Get-Content -LiteralPath $fixtureTrace
                } else {
                    Write-Output 'The Agent package fixture did not enter the update.'
                }
            }
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
    if ($VerifyStartupLock) {
        Test-DuplicateStartup 'active-primary'
    }
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
    if ($VerifyIsland -or $VerifyPackManifest -or $VerifyPackArchive -or $VerifyPinnedPackManifest -or $PackManifestLink -or
        $VerifyDrag -or $VerifyDisplayChange -or $VerifyQwenWork -or $VerifyAgentPackageConsole -or $VerifyRegisteredAgentApp -or $VerifyDiagnosticsExport) {
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
    if ($processTrace) {
        try {
            Write-WindowsTerminalStarts $processTraceId $processBaseline $app.Id
        } catch {
            Write-Output "Could not read Windows process traces: $_"
        } finally {
            Unregister-Event -SourceIdentifier $processTraceId -ErrorAction SilentlyContinue
        }
    }
    if ($VerifyDrag -and (Test-Path -LiteralPath $stdoutPath -PathType Leaf)) {
        Select-String -LiteralPath $stdoutPath -SimpleMatch 'Island position trace ' |
            Select-Object -ExpandProperty Line
    }
    if (-not $app.HasExited) {
        Stop-Process -Id $app.Id -ErrorAction SilentlyContinue
    }
    if ($shortcutConfigPath) {
        Remove-Item -LiteralPath $shortcutConfigPath -ErrorAction SilentlyContinue
    }
    foreach ($fixture in $packFixtures) {
        Remove-Item -LiteralPath $fixture -Recurse -ErrorAction SilentlyContinue
    }
}
