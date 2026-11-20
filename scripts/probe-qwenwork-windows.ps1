$ErrorActionPreference = 'Stop'
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
$desktop = $executables | Where-Object { $_.BaseName -match '^(qwenwork(cn)?|千问办公)$' } | Select-Object -First 1
if (-not $desktop) { throw 'No QwenWork desktop executable was found after installation' }
Write-Output "Installed desktop version: $($desktop.VersionInfo.ProductVersion)"
$app = Start-Process -FilePath $desktop.FullName -PassThru
try {
    $deadline = (Get-Date).AddSeconds(45)
    do {
        $window = Get-Process -Name $desktop.BaseName -ErrorAction SilentlyContinue |
            Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
        if ($window) { break }
        Start-Sleep -Milliseconds 500
    } until ((Get-Date) -ge $deadline)
    if (-not $window) { throw 'QwenWork did not open a desktop window' }
    Write-Output "QwenWork window title: $($window.MainWindowTitle)"
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
} finally {
    Get-Process -Name $desktop.BaseName -ErrorAction SilentlyContinue | Stop-Process -ErrorAction SilentlyContinue
}
