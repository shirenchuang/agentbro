param([string]$AgentBroInstaller, [switch]$VerifyAgentBro)

$ErrorActionPreference = 'Stop'

function Invoke-QwenWorkIslandCheck([string]$Mode, [string]$Title) {
    $windowsPowerShell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $start = [System.Diagnostics.ProcessStartInfo]::new($windowsPowerShell)
    foreach ($argument in @('-NoProfile', '-NonInteractive', '-Mta', '-File', "$PSScriptRoot/test-windows-island.ps1",
            '-AppProcessId', [string]$agentBro.Id, '-ProjectName', $Title, '-QwenWorkMode', $Mode,
            '-QwenWorkRoot', $desktop.DirectoryName)) {
        $start.ArgumentList.Add($argument)
    }
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $tester = [System.Diagnostics.Process]::Start($start)
    try {
        $stdout = $tester.StandardOutput.ReadToEndAsync()
        $stderr = $tester.StandardError.ReadToEndAsync()
        if (-not $tester.WaitForExit(120000)) { throw "QwenWork test '$Mode' timed out" }
        Write-Output ($stdout.GetAwaiter().GetResult())
        if ($tester.ExitCode -ne 0) { throw "QwenWork test '$Mode' failed: $($stderr.GetAwaiter().GetResult())" }
    } finally {
        if (-not $tester.HasExited) { Stop-Process -Id $tester.Id -ErrorAction SilentlyContinue }
        $tester.Dispose()
    }
}

function Get-QwenWorkProcesses {
    Get-Process -ErrorAction SilentlyContinue | Where-Object {
        $_.Path -and $_.Path.StartsWith("$($desktop.DirectoryName)\", [StringComparison]::OrdinalIgnoreCase)
    }
}

function Set-QwenWorkFixturePhase([string]$Phase) {
    & python "$PSScriptRoot/qwenwork-task-fixture.py" $fixtureDb $Phase
    if ($LASTEXITCODE -ne 0) { throw "Could not set the QwenWork fixture phase to $Phase" }
}
$stage = Join-Path $env:RUNNER_TEMP "AgentBro QwenWork probe $([Guid]::NewGuid().ToString('N'))"
$null = New-Item -ItemType Directory -Path $stage
$installer = Join-Path $stage 'qwenworkcn-setup-x64.exe'
& curl.exe --fail --location --silent --show-error --retry 2 --max-time 240 --output $installer `
    https://assets.qwenwork.cn/release/latest/qwenworkcn-setup-x64.exe
if ($LASTEXITCODE -ne 0) { throw 'Official QwenWork download failed' }
$signature = Get-AuthenticodeSignature -LiteralPath $installer
if ($signature.Status -ne 'Valid') { throw "QwenWork installer signature status: $($signature.Status)" }
Write-Output "Installer signed by $($signature.SignerCertificate.Subject)"

$setup = Start-Process -FilePath $installer -ArgumentList '/S' -PassThru
if (-not $setup.WaitForExit(180000) -or $setup.ExitCode -ne 0) { throw 'Official QwenWork silent installation failed' }
$uninstallLocations = @(
    'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
)
$entries = Get-ItemProperty -Path $uninstallLocations -ErrorAction SilentlyContinue |
    Where-Object { $_.DisplayName -match 'QwenWork|千问办公' }
$entries | Select-Object DisplayName, DisplayVersion, InstallLocation, DisplayIcon | Format-List
$roots = @($entries.InstallLocation) + @($env:ProgramFiles, $env:LOCALAPPDATA, (Join-Path $env:LOCALAPPDATA 'Programs'))
$executables = foreach ($root in ($roots | Where-Object { $_ } | Select-Object -Unique)) {
    Get-ChildItem -LiteralPath $root -Filter '*qwen*.exe' -File -ErrorAction SilentlyContinue
    Get-ChildItem -LiteralPath $root -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match 'qwen|千问' } |
        ForEach-Object { Get-ChildItem -LiteralPath $_.FullName -Filter '*.exe' -File }
}
$executables | Select-Object -ExpandProperty FullName -Unique
$desktop = $executables | Where-Object { $_.FullName -like '*\QwenWorkCN\Launcher.exe' } | Select-Object -First 1
if (-not $desktop) { throw 'No QwenWork desktop executable was found after installation' }
Write-Output "Installed launcher: $($desktop.FullName)"
Write-Output "Launcher product version: $($desktop.VersionInfo.ProductVersion)"
$agentBro = $null
try {
    if ($VerifyAgentBro) {
        $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
        $listener.Start()
        $port = $listener.LocalEndpoint.Port
        $listener.Stop()
        $env:AGENTBRO_HOOK_PORT = "$port"
        $installDir = Join-Path $stage 'AgentBro app'
        $agentBroSetup = Start-Process -FilePath $AgentBroInstaller -ArgumentList "/S /D=$installDir" -PassThru
        if (-not $agentBroSetup.WaitForExit(90000) -or $agentBroSetup.ExitCode -ne 0) { throw 'AgentBro installation failed' }
        $agentBro = Start-Process -FilePath (Join-Path $installDir 'agentbro.exe') -PassThru
        $deadline = (Get-Date).AddSeconds(45)
        do {
            $ready = Get-NetTCPConnection -LocalPort $port -State Listen -OwningProcess $agentBro.Id -ErrorAction SilentlyContinue
            if ($ready) { break }
            if ($agentBro.HasExited) { throw 'AgentBro exited before Hook startup' }
            Start-Sleep -Milliseconds 500
        } until ((Get-Date) -ge $deadline)
        if (-not $ready) { throw 'AgentBro Hook port was unavailable' }
        Invoke-QwenWorkIslandCheck Detect QwenWork
    }

    Start-Process -FilePath $desktop.FullName
    $deadline = (Get-Date).AddSeconds(45)
    do {
        $window = Get-QwenWorkProcesses |
            Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
        if ($window) { break }
        Start-Sleep -Milliseconds 500
    } until ((Get-Date) -ge $deadline)
    if (-not $window) { throw 'QwenWork did not open a desktop window' }
    Write-Output "QwenWork window title: $($window.MainWindowTitle)"
    Write-Output "QwenWork window process: $($window.Path)"
    $profile = Join-Path $env:USERPROFILE '.qwenworkcn'
    if (Test-Path -LiteralPath $profile -PathType Container) {
        Write-Output "QwenWork profile directory: $profile"
        $dbPaths = Get-ChildItem -LiteralPath $profile -Recurse -File -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -in @('host-agent-responses.db', 'qwenwork.db') }
        foreach ($database in $dbPaths) {
            Write-Output "Task state database: $($database.FullName)"
            & python -c 'import pathlib, sqlite3, sys; conn = sqlite3.connect(pathlib.Path(sys.argv[1]).as_uri()+"?mode=ro", uri=True); print({name: [r[1] for r in conn.execute("pragma table_info("+name+")")] for name in ("desk_turns", "session", "message")})' $database.FullName
            if ($LASTEXITCODE -ne 0) { throw 'Could not query the task database schema' }
        }
    }
    if ($VerifyAgentBro) {
        $fixtureDb = Join-Path $profile 'identities\agentbro-windows-fixture\qwapp\host-agent-responses.db'
        Set-QwenWorkFixturePhase in_progress
        Invoke-QwenWorkIslandCheck Processing '任务进行中'
        Set-QwenWorkFixturePhase waiting_interaction
        Invoke-QwenWorkIslandCheck Jump '请在千问办公中处理待确认事项'
        Set-QwenWorkFixturePhase completed
        Invoke-QwenWorkIslandCheck Completed '任务已完成'
    } else {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class QwenWorkWindowProbe {
    [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr window, int command);
    [DllImport("user32.dll")] private static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
    public static uint ForegroundProcess() {
        uint processId;
        GetWindowThreadProcessId(GetForegroundWindow(), out processId);
        return processId;
    }
}
'@
        $null = [QwenWorkWindowProbe]::ShowWindowAsync($window.MainWindowHandle, 6)
        Start-Sleep -Milliseconds 300
        if ([QwenWorkWindowProbe]::ForegroundProcess() -eq $window.Id) { throw 'QwenWork did not minimize' }
        Start-Process -FilePath $desktop.FullName
        $deadline = (Get-Date).AddSeconds(15)
        do {
            $foreground = Get-Process -Id ([QwenWorkWindowProbe]::ForegroundProcess()) -ErrorAction SilentlyContinue
            $activated = $foreground.Path -and $foreground.Path.StartsWith("$($desktop.DirectoryName)\", [StringComparison]::OrdinalIgnoreCase)
            if ($activated) { break }
            Start-Sleep -Milliseconds 250
        } until ((Get-Date) -ge $deadline)
        if (-not $activated) { throw 'The official launcher did not restore the running QwenWork window' }
        Write-Output 'The official launcher restored the running QwenWork window to the foreground.'
    }
} finally {
    if ($agentBro -and -not $agentBro.HasExited) { Stop-Process -Id $agentBro.Id -ErrorAction SilentlyContinue }
    Get-QwenWorkProcesses | Stop-Process -ErrorAction SilentlyContinue
}
