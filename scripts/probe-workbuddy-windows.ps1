param(
    [Parameter(Mandatory)]
    [ValidateSet('cn', 'global')]
    [string]$Edition,
    [string]$AgentBroInstaller
)

$ErrorActionPreference = 'Stop'
$packages = @{
    cn = @{
        Url = 'https://download.codebuddy.cn/workbuddy/saas/win32-x64-user/WorkBuddy-win32-x64-user-5.11.3.43806934-ecace0f7.exe'
        Sha256 = '2fd40059eea2f9a2d641c675a66f2476d71b49d9eb32f19db8c318768133b56b'
    }
    global = @{
        Url = 'https://download.codebuddy.ai/workbuddy/saas/win32-x64-user/WorkBuddy-win32-x64-user-5.11.4.44121853-0026dff0.exe'
        Sha256 = '38841a16a766e488eeba9adf322b92239e69cbb7a309dac7a35fee495496441c'
    }
}

$package = $packages[$Edition]
$installer = Join-Path $env:RUNNER_TEMP "WorkBuddy-$Edition-setup.exe"
& curl.exe --fail --location --silent --show-error --retry 2 --max-time 240 --output $installer $package.Url
if ($LASTEXITCODE -ne 0) { throw "WorkBuddy '$Edition' installer download failed" }
if ((Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash -ne $package.Sha256) {
    throw "WorkBuddy '$Edition' installer SHA-256 differs from the official update metadata"
}
$signature = Get-AuthenticodeSignature -LiteralPath $installer
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'Tencent') {
    throw "Unexpected WorkBuddy installer signature: $($signature.Status), $($signature.SignerCertificate.Subject)"
}
Write-Output "Verified WorkBuddy $Edition installer hash and signature ($($signature.SignerCertificate.Subject))."

$setup = Start-Process -FilePath $installer -ArgumentList '/S /currentuser' -PassThru
if (-not $setup.WaitForExit(360000)) {
    throw "WorkBuddy '$Edition' setup did not finish in six minutes"
}
if ($setup.ExitCode -ne 0) { throw "WorkBuddy '$Edition' setup exited with $($setup.ExitCode)" }

$registry = @(
    'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
)
$entries = Get-ItemProperty -Path $registry -ErrorAction SilentlyContinue |
    Where-Object { $_.DisplayName -match 'WorkBuddy' }
$entries | Select-Object DisplayName, DisplayVersion, InstallLocation, DisplayIcon, UninstallString | Format-List
foreach ($protocol in @('workbuddy', 'workbuddy-ai')) {
    $protocolKey = Get-Item "Registry::HKEY_CLASSES_ROOT\$protocol\shell\open\command" -ErrorAction SilentlyContinue
    if ($protocolKey) { Write-Output "Protocol $protocol is registered as $($protocolKey.GetValue(''))" }
}

$programParents = @(
    (Join-Path $env:LOCALAPPDATA 'Programs'),
    $env:LOCALAPPDATA,
    $env:ProgramFiles,
    ${env:ProgramFiles(x86)}
) | Where-Object { $_ } | Select-Object -Unique
$installRoots = @($entries | ForEach-Object { $_.InstallLocation } | Where-Object { $_ })
foreach ($parent in $programParents) {
    $installRoots += Get-ChildItem -LiteralPath $parent -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match 'work.?buddy' } | Select-Object -ExpandProperty FullName
}
foreach ($entry in $entries) {
    if ($entry.DisplayIcon -match '^"?([^"]+?\.exe)(?:"|,|$)') {
        $installRoots += Split-Path $Matches[1] -Parent
    }
}
$installRoots = $installRoots | Select-Object -Unique | Where-Object { Test-Path -LiteralPath $_ -PathType Container }
$executables = $installRoots | ForEach-Object {
    Get-ChildItem -LiteralPath $_ -File -Filter '*.exe' -Recurse -Depth 1 -ErrorAction SilentlyContinue
}
$executables | Select-Object -ExpandProperty FullName -Unique
$launcher = $executables | Where-Object { $_.Name -match '^WorkBuddy( ?AI)?\.exe$' } | Select-Object -First 1
if (-not $launcher) { throw "The installed WorkBuddy '$Edition' launcher could not be located" }
Write-Output "WorkBuddy desktop executable: $($launcher.FullName)"
Start-Process -FilePath $launcher.FullName
$deadline = (Get-Date).AddSeconds(60)
do {
    $window = Get-Process -Name $launcher.BaseName -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -eq $launcher.FullName -and $_.MainWindowHandle -ne 0 } |
        Select-Object -First 1
    if ($window) { break }
    Start-Sleep -Seconds 1
} until ((Get-Date) -ge $deadline)

foreach ($profile in (Get-ChildItem -LiteralPath $env:USERPROFILE -Force -Directory -Filter '.workbuddy*')) {
    $settings = Test-Path -LiteralPath (Join-Path $profile.FullName 'settings.json') -PathType Leaf
    $mcp = Test-Path -LiteralPath (Join-Path $profile.FullName 'mcp.json') -PathType Leaf
    $skills = Test-Path -LiteralPath (Join-Path $profile.FullName 'skills') -PathType Container
    Write-Output "WorkBuddy profile $($profile.Name) (settings.json=$settings, mcp.json=$mcp, skills=$skills)"
}
foreach ($relative in @('resources\app\product.json', 'resources\app.asar', 'resources\app.asar.unpacked\cli\bin', 'resources\app\cli\bin')) {
    $candidate = Join-Path $launcher.DirectoryName $relative
    if (Test-Path -LiteralPath $candidate) {
        Write-Output "Bundled resource: $candidate"
        if ($relative -like '*\bin') {
            Get-ChildItem -LiteralPath $candidate -File | Select-Object -ExpandProperty Name
        }
        if ($relative -like '*product.json') {
            Get-Content -LiteralPath $candidate -Raw | ConvertFrom-Json |
                Select-Object nameShort, applicationName, dataFolderName, urlProtocol | Format-List
        }
    }
}
if (-not $window) { throw "WorkBuddy '$Edition' never opened a visible desktop window" }
Write-Output "WorkBuddy opened its desktop window: $($window.MainWindowTitle)"

if (-not $AgentBroInstaller) { return }

$source = if ($Edition -eq 'global') { 'workbuddy-ai' } else { 'workbuddy' }
$otherSource = if ($Edition -eq 'global') { 'workbuddy' } else { 'workbuddy-ai' }
$editionName = if ($Edition -eq 'global') { 'WorkBuddy AI' } else { 'WorkBuddy' }
$profileRoot = Join-Path $env:USERPROFILE ".$source"
$settingsPath = Join-Path $profileRoot 'settings.json'
$otherSettingsPath = Join-Path $env:USERPROFILE ".$otherSource/settings.json"
$agentBro = $null

function Invoke-IslandCheck([string]$Mode, [string]$Title) {
    $windowsPowerShell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $start = [System.Diagnostics.ProcessStartInfo]::new($windowsPowerShell)
    foreach ($argument in @('-NoProfile', '-NonInteractive', '-Mta', '-File', "$PSScriptRoot/test-windows-island.ps1",
            '-AppProcessId', [string]$agentBro.Id, '-ProjectName', $Title, '-WorkBuddyMode', $Mode,
            '-WorkBuddyEditionName', $editionName, '-WorkBuddySettingsPath', $settingsPath, '-WorkBuddyRoot', $launcher.DirectoryName)) {
        $start.ArgumentList.Add($argument)
    }
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.StandardOutputEncoding = [System.Text.Encoding]::UTF8
    $start.StandardErrorEncoding = [System.Text.Encoding]::UTF8
    $tester = [System.Diagnostics.Process]::Start($start)
    try {
        $stdout = $tester.StandardOutput.ReadToEndAsync()
        $stderr = $tester.StandardError.ReadToEndAsync()
        if (-not $tester.WaitForExit(120000)) { throw "WorkBuddy Island test '$Mode' timed out" }
        Write-Output ($stdout.GetAwaiter().GetResult())
        if ($tester.ExitCode -ne 0) { throw "WorkBuddy Island test '$Mode' failed: $($stderr.GetAwaiter().GetResult())" }
    } finally {
        if (-not $tester.HasExited) { Stop-Process -Id $tester.Id -ErrorAction SilentlyContinue }
        $tester.Dispose()
    }
}

function Read-InertSettings([string]$Path) {
    if (Test-Path -LiteralPath $Path -PathType Leaf) {
        return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json -AsHashtable
    }
    return @{}
}

function Send-WorkBuddyHook([string]$Event, [hashtable]$Extra) {
    $settings = Read-InertSettings $settingsPath
    $command = $settings.hooks[$Event] | ForEach-Object { $_.hooks } |
        Where-Object { $_.command -match "agentbro-bridge.+--source $source(?: |$)" } |
        Select-Object -First 1 -ExpandProperty command
    if (-not $command) { throw "Missing the configured AgentBro '$Event' bridge command for $source" }
    $payload = @{ hook_event_name = $Event; session_id = 'workbuddy-windows-fixture'; cwd = $fixtureRoot }
    foreach ($entry in $Extra.GetEnumerator()) { $payload[$entry.Key] = $entry.Value }
    ($payload | ConvertTo-Json -Compress) | & $gitBash --noprofile --norc -c $command
    if ($LASTEXITCODE -ne 0) { throw "The configured '$Event' command did not run under Git Bash" }
}

try {
    $gitBash = Join-Path $env:ProgramFiles 'Git\bin\bash.exe'
    if (-not (Test-Path -LiteralPath $gitBash)) { throw "Git Bash is missing: $gitBash" }
    $listen = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    $listen.Start()
    $port = $listen.LocalEndpoint.Port
    $listen.Stop()
    $env:AGENTBRO_HOOK_PORT = "$port"
    $agentBroRoot = Join-Path $env:RUNNER_TEMP "AgentBro WorkBuddy $Edition smoke"
    $agentBroSetup = Start-Process -FilePath $AgentBroInstaller -ArgumentList "/S /D=$agentBroRoot" -PassThru
    if (-not $agentBroSetup.WaitForExit(90000) -or $agentBroSetup.ExitCode -ne 0) { throw 'AgentBro install failed' }
    $agentBro = Start-Process -FilePath (Join-Path $agentBroRoot 'agentbro.exe') -PassThru
    $deadline = (Get-Date).AddSeconds(60)
    do {
        $ready = Get-NetTCPConnection -LocalPort $port -State Listen -OwningProcess $agentBro.Id -ErrorAction SilentlyContinue
        if ($ready) { break }
        if ($agentBro.HasExited) { throw 'AgentBro exited before starting the hook listener' }
        Start-Sleep -Milliseconds 500
    } until ((Get-Date) -ge $deadline)
    if (-not $ready) { throw 'AgentBro hook listener did not start' }

    Invoke-IslandCheck Detect $editionName
    New-Item -ItemType Directory -Path $profileRoot -Force | Out-Null
    New-Item -ItemType Directory -Path (Split-Path -Parent $otherSettingsPath) -Force | Out-Null
    $settings = Read-InertSettings $settingsPath
    $settings.hooks ??= @{}
    $thirdPartyCommand = 'echo third-party-workbuddy-fixture'
    $settings.hooks.SessionStart = @($settings.hooks.SessionStart | Where-Object { $_ }) +
        @(@{ hooks = @(@{ type = 'command'; command = $thirdPartyCommand }) })
    $settings | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $settingsPath -Encoding utf8
    $otherSettings = Read-InertSettings $otherSettingsPath
    $otherSettings.AgentBroProbeSentinel = 'Leave the other edition profile alone'
    $otherSettings | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $otherSettingsPath -Encoding utf8
    $otherHash = (Get-FileHash -LiteralPath $otherSettingsPath).Hash

    Invoke-IslandCheck Install $editionName
    $fixtureRoot = Join-Path $env:RUNNER_TEMP 'WorkBuddyFixture'
    New-Item -ItemType Directory -Path $fixtureRoot -Force | Out-Null
    Send-WorkBuddyHook SessionStart @{}
    Send-WorkBuddyHook UserPromptSubmit @{ prompt = 'WorkBuddy is processing the test task' }
    Invoke-IslandCheck Processing 'WorkBuddy is processing the test task'
    Start-Sleep -Seconds 8
    if ($Edition -eq 'global') {
        Send-WorkBuddyHook Notification @{ notification_type = 'permission_prompt'; message = 'Confirmation needed in the desktop' }
        Invoke-IslandCheck Jump 'Confirm this action in WorkBuddy AI'
    } else {
        Invoke-IslandCheck OpenFromSession 'WorkBuddy is processing the test task'
    }
    Send-WorkBuddyHook Stop @{ last_assistant_message = 'WorkBuddy finished the test task' }
    Invoke-IslandCheck Completed 'WorkBuddy finished the test task'
    Invoke-IslandCheck Uninstall $editionName

    $settings = Read-InertSettings $settingsPath
    if (($settings.hooks.SessionStart | ForEach-Object { $_.hooks.command }) -notcontains $thirdPartyCommand) {
        throw 'AgentBro removed a third-party WorkBuddy hook'
    }
    if ((Get-FileHash -LiteralPath $otherSettingsPath).Hash -ne $otherHash) {
        throw 'AgentBro modified the other edition profile'
    }
    Write-Output "Verified installed $editionName, lifecycle through configured Git Bash hooks, native Island jump, and clean uninstall."
} finally {
    if ($agentBro -and -not $agentBro.HasExited) { Stop-Process -Id $agentBro.Id -ErrorAction SilentlyContinue }
}
