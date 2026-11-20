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
$null = New-Item -ItemType Directory -Path $package
@{ name = $pluginName; version = '1.0.0'; type = 'module'; exports = './index.js'; agentbroHookPort = 17894;
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
