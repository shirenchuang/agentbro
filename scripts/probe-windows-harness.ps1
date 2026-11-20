param([Parameter(Mandatory)][string]$AgentBroInstaller, [switch]$UseAgentBroHookInstaller)

$ErrorActionPreference = 'Stop'
$stage = Join-Path $env:RUNNER_TEMP "AgentBro Harness probe $([Guid]::NewGuid().ToString('N'))"
$null = New-Item -ItemType Directory -Path $stage
$env:DSH_HOME = Join-Path $stage 'Harness profile'
$installer = Join-Path $stage 'harness-setup.exe'
& curl.exe --fail --location --silent --show-error --retry 2 --max-time 180 --output $installer `
    https://download.deepseek.com/desktop/dsh-latest-windows-x64.exe
if ($LASTEXITCODE -ne 0) { throw 'Official DeepSeek Harness download failed' }
$signature = Get-AuthenticodeSignature -LiteralPath $installer
if ($signature.Status -ne 'Valid') { throw "DeepSeek Harness signature status: $($signature.Status)" }
Write-Output "Installer signed by $($signature.SignerCertificate.Subject)"

$setup = Start-Process -FilePath $installer -ArgumentList '/S' -PassThru
if (-not $setup.WaitForExit(120000) -or $setup.ExitCode -ne 0) {
    throw 'Official DeepSeek Harness silent installation failed'
}
$desktop = @($env:LOCALAPPDATA, (Join-Path $env:LOCALAPPDATA 'Programs'), $env:ProgramFiles) |
    ForEach-Object { Join-Path $_ 'DeepSeek Harness\DeepSeek Harness.exe' } |
    Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
    Select-Object -First 1
if (-not $desktop) { throw 'The DeepSeek Harness desktop application was not found' }
$cli = Join-Path (Split-Path -Parent $desktop) 'resources\runtime\cli\bin\dsh.cmd'
if (-not (Test-Path -LiteralPath $cli -PathType Leaf)) { throw "Bundled CLI missing: $cli" }
Write-Output "Bundled CLI: $cli"
& $cli --version
if ($LASTEXITCODE -ne 0) { throw 'The desktop CLI failed to run' }

$manifest = Join-Path $env:DSH_HOME 'profiles\desktop\package.json'
$app = Start-Process -FilePath $desktop -PassThru
try {
    $deadline = (Get-Date).AddSeconds(45)
    while (-not (Test-Path -LiteralPath $manifest -PathType Leaf) -and (Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 500
    }
    if (-not (Test-Path -LiteralPath $manifest -PathType Leaf)) { throw "Desktop profile not initialized: $manifest" }
    Write-Output "Desktop profile initialized at $manifest"
} finally {
    if (-not $app.HasExited) {
        $null = $app.CloseMainWindow()
        $null = $app.WaitForExit(10000)
    }
    Get-Process -Name 'DeepSeek Harness' -ErrorAction SilentlyContinue | Stop-Process
}

$pluginName = 'agentbro-bridge-deepseek-harness'
$package = Join-Path $stage $pluginName
$listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$listener.Start()
$port = $listener.LocalEndpoint.Port
$listener.Stop()
$env:AGENTBRO_HOOK_PORT = "$port"
$null = New-Item -ItemType Directory -Path $package
@{ name = $pluginName; version = '1.0.0'; type = 'module'; exports = './index.js'; agentbroHookPort = $port;
    dsh = @{ bundle = @{ patch = './bundle.yml' } } } |
    ConvertTo-Json -Depth 8 |
    Set-Content -LiteralPath (Join-Path $package 'package.json') -Encoding utf8
Set-Content -LiteralPath (Join-Path $package 'bundle.yml') -Value "- insert:`n    - id: $pluginName`n      name: $pluginName" -Encoding utf8
Copy-Item -LiteralPath "$PSScriptRoot/../src-tauri/src/agents/deepseek_harness_bridge.js" -Destination (Join-Path $package 'index.js')

& $cli plugin --profile desktop add "file:$package" --offline --ignore-scripts
if ($LASTEXITCODE -ne 0) { throw 'The desktop CLI could not install the AgentBro bridge' }
$profile = Split-Path -Parent $manifest
$installed = Join-Path $profile "node_modules\$pluginName\index.js"
$pkg = Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json
if ($pluginName -notin $pkg.dsh.profile.bundles -or -not (Test-Path -LiteralPath $installed -PathType Leaf)) {
    throw 'The desktop profile did not register the AgentBro bridge'
}
Write-Output 'Official DeepSeek Harness desktop CLI installed the AgentBro bridge.'

& $cli plugin --profile desktop remove $pluginName
if ($LASTEXITCODE -ne 0) { throw 'The desktop CLI could not remove the AgentBro bridge' }
$pkg = Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json
if ($pluginName -in $pkg.dsh.profile.bundles -or (Test-Path -LiteralPath $installed)) {
    throw 'The desktop profile did not remove the AgentBro bridge'
}
Write-Output 'Official DeepSeek Harness desktop CLI removed the AgentBro bridge.'

$fixtureName = 'agentbro-harness-ci-fixture'
$fixture = Join-Path $stage $fixtureName
$output = Join-Path $stage 'results'
$null = New-Item -ItemType Directory -Path $fixture, $output
@{ name = $fixtureName; version = '1.0.0'; type = 'module'; exports = './index.js';
    dsh = @{ bundle = @{ patch = './bundle.yml' } } } |
    ConvertTo-Json -Depth 8 |
    Set-Content -LiteralPath (Join-Path $fixture 'package.json') -Encoding utf8
Set-Content -LiteralPath (Join-Path $fixture 'bundle.yml') -Value "- insert:`n    - id: $fixtureName`n      name: $fixtureName" -Encoding utf8
Copy-Item -LiteralPath "$PSScriptRoot/harness-approval-fixture.mjs" -Destination (Join-Path $fixture 'index.js')
$pluginSpecs = @("file:$fixture")
if (-not $UseAgentBroHookInstaller) { $pluginSpecs += "file:$package" }
& $cli plugin --profile desktop add @pluginSpecs --offline --ignore-scripts
if ($LASTEXITCODE -ne 0) { throw 'The desktop CLI could not install the requested plugins' }

$agentBroDir = Join-Path $stage 'AgentBro app'
$agentBroSetup = Start-Process -FilePath $AgentBroInstaller -ArgumentList "/S /D=$agentBroDir" -PassThru
if (-not $agentBroSetup.WaitForExit(90000) -or $agentBroSetup.ExitCode -ne 0) { throw 'AgentBro installation failed' }
$agentBro = Start-Process -FilePath (Join-Path $agentBroDir 'agentbro.exe') -PassThru
try {
    $deadline = (Get-Date).AddSeconds(45)
    do {
        $ready = Get-NetTCPConnection -LocalPort $port -State Listen -OwningProcess $agentBro.Id -ErrorAction SilentlyContinue
        if ($ready) { break }
        if ($agentBro.HasExited) { throw 'AgentBro exited before Hook startup' }
        Start-Sleep -Milliseconds 500
    } until ((Get-Date) -ge $deadline)
    if (-not $ready) { throw 'AgentBro Hook port was unavailable' }
    $windowsPowerShell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    if ($UseAgentBroHookInstaller) {
        & $windowsPowerShell -NoProfile -NonInteractive -Mta -File "$PSScriptRoot/test-windows-island.ps1" `
            -AppProcessId $agentBro.Id -ProjectName 'DeepSeek Harness' -HookOperation Install -HarnessPluginPath $installed
        if ($LASTEXITCODE -ne 0) { throw 'AgentBro could not install the deepseek-harness Hook' }
        $bridgeManifest = Get-Content -LiteralPath (Join-Path (Split-Path -Parent $installed) 'package.json') -Raw | ConvertFrom-Json
        if ($bridgeManifest.agentbroHookPort -ne $port) { throw 'AgentBro installed an incorrect Hook port' }
        Write-Output "AgentBro discovered the desktop installation and installed its Harness plugin on Hook port $port."
    }
    $env:AGENTBRO_HARNESS_FIXTURE_OUTPUT = $output
    $harness = Start-Process -FilePath $desktop -PassThru
    & $windowsPowerShell -NoProfile -NonInteractive -Mta -File "$PSScriptRoot/test-windows-island.ps1" `
        -AppProcessId $agentBro.Id -ProjectName 'AgentBro Harness fixture' -HarnessOutput $output -LegacyHarnessDeny
    if ($LASTEXITCODE -ne 0) { throw 'The Harness Island interaction check did not pass' }
    if ($UseAgentBroHookInstaller) {
        Get-Process -Name 'DeepSeek Harness' -ErrorAction SilentlyContinue | Stop-Process
        & $windowsPowerShell -NoProfile -NonInteractive -Mta -File "$PSScriptRoot/test-windows-island.ps1" `
            -AppProcessId $agentBro.Id -ProjectName 'DeepSeek Harness' -HookOperation Uninstall -HarnessPluginPath $installed
        if ($LASTEXITCODE -ne 0) { throw 'AgentBro could not uninstall the deepseek-harness Hook' }
        Write-Output 'AgentBro removed its Hook from the official Harness desktop profile.'
    }
} finally {
    Get-ChildItem -LiteralPath $output -File | ForEach-Object {
        Write-Output "$($_.Name): $(Get-Content -LiteralPath $_.FullName -Raw)"
    }
    $agentBroLog = Join-Path $env:LOCALAPPDATA 'com.agentbro.desktop\logs\agentbro.log'
    if (Test-Path -LiteralPath $agentBroLog) {
        Select-String -LiteralPath $agentBroLog -Pattern 'deepseek-harness|Permission response' | Select-Object -Last 20 -ExpandProperty Line
    }
    if (-not $agentBro.HasExited) { Stop-Process -Id $agentBro.Id }
    Get-Process -Name 'DeepSeek Harness' -ErrorAction SilentlyContinue | Stop-Process
}
