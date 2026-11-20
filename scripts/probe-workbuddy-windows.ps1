param(
    [Parameter(Mandatory)]
    [ValidateSet('cn', 'global')]
    [string]$Edition
)

$ErrorActionPreference = 'Stop'
$packages = @{
    cn = @{
        Url = 'https://download.codebuddy.cn/workbuddy/saas/win32-x64-user/WorkBuddy-win32-x64-user-5.8.3.41334564-db0e9b51.exe'
        Sha256 = '824DE10F3196C501004F540EB84EE881292FEF2F5DF9FB3FC48AF9EA5366C03C'
    }
    global = @{
        Url = 'https://download.codebuddy.ai/workbuddy/saas/win32-x64-user/WorkBuddy-win32-x64-user-5.9.4.42593613-fa0a0901.exe'
        Sha256 = '6FF4BE433D664AD14C88210B5B82EBB3674D7047AAB8AE52459006C587BE2EE0'
    }
}

$package = $packages[$Edition]
$installer = Join-Path $env:RUNNER_TEMP "WorkBuddy-$Edition-setup.exe"
& curl.exe --fail --location --silent --show-error --retry 2 --max-time 240 --output $installer $package.Url
if ($LASTEXITCODE -ne 0) { throw "WorkBuddy '$Edition' installer download failed" }
if ((Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash -ne $package.Sha256) {
    throw "WorkBuddy '$Edition' installer SHA-256 differs from the WinGet manifest"
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
    Write-Output "WorkBuddy profile $($profile.Name) (settings.json=$settings, mcp.json=$mcp)"
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
