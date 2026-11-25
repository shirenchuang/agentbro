function Test-InstalledDiagnosticsExport(
    [Windows.Automation.AutomationElement]$Island,
    [string]$ArchivePath
) {
    if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or -not $env:RUNNER_TEMP) {
        throw 'The diagnostics fixture requires an isolated GitHub-hosted runner.'
    }
    $logs = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'com.agentbro.desktop\logs'
    $activeLog = Join-Path $logs 'agentbro.log'
    if (-not (Test-Path -LiteralPath $activeLog -PathType Leaf)) { throw 'The app did not write its current log.' }
    $rotatedName = 'agentbro_2001-04-27_01-02-03.log'
    $rotatedLog = Join-Path $logs $rotatedName
    if (Test-Path -LiteralPath $rotatedLog) { throw 'The diagnostics fixture log already exists.' }
    $fixture = 'AGENTBRO_PRIVATE_DIAGNOSTICS_FIXTURE'
    @(
        '[2001-04-27][01:02:03][agentbro_lib::platform::windows_startup][ERROR] Startup: could not locate the WebView2 runtime: ' + $fixture
        '[2001-04-27][01:02:04][agentbro_lib::commands][INFO] Send message: session=fixture, msg=' + $fixture
    ) | Set-Content -LiteralPath $rotatedLog -Encoding Ascii

    try {
        $settings = Open-InstalledAgentSkills $Island
        $button = [Windows.Automation.ControlType]::Button
        Invoke-PackControl (Wait-IslandValue 'the back to Settings button' {
            Find-PackControl $settings 'Back to Settings' $button
        })
        Invoke-PackControl (Wait-IslandValue 'the About settings page' {
            $about = Find-PackControl $settings 'About' $button
            if ($about) { $about } else { Find-PackControl $settings '关于' $button }
        })
        $buttonCondition = [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty, $button)
        $export = Wait-IslandValue 'the accessible Export Diagnostics button' {
            $settings.FindAll([Windows.Automation.TreeScope]::Descendants, $buttonCondition) |
                Where-Object { $_.Current.Name -in 'Export', '导出' -and $_.Current.IsEnabled } |
                Select-Object -First 1
        }
        Invoke-PackControl $export
        Set-PackDialogFile 'Save' $ArchivePath

        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $archive = Wait-IslandValue 'the finalized diagnostics ZIP' {
            if (Test-Path -LiteralPath $ArchivePath -PathType Leaf) {
                try { [IO.Compression.ZipFile]::OpenRead($ArchivePath) } catch [IO.InvalidDataException], [IO.IOException] {}
            }
        }
        try {
            $current = $archive.GetEntry('logs/agentbro.log')
            $rotated = $archive.GetEntry("logs/$rotatedName")
            if (-not $current -or -not $rotated) { throw 'The diagnostics ZIP omitted the current or rotated application log.' }
            $reader = [IO.StreamReader]::new($current.Open())
            try { $startup = $reader.ReadToEnd() } finally { $reader.Dispose() }
            $reader = [IO.StreamReader]::new($rotated.Open())
            try { $previous = $reader.ReadToEnd() } finally { $reader.Dispose() }
            if ($startup -notmatch 'Startup: WebView2 runtime [0-9]+(?:\.[0-9]+){3}' -or
                $startup -notmatch 'Startup: scheduling Hook server' -or
                $startup -notmatch 'Listening on TCP: 127\.0\.0\.1:\d+') {
                throw 'The diagnostics ZIP is missing the observed WebView2 or Hook startup records.'
            }
            if ($previous -notmatch 'could not locate the WebView2 runtime' -or
                $previous.Contains($fixture) -or $previous.Contains('Send message:')) {
                throw 'The diagnostics ZIP lost the known failure category or included private fixture content.'
            }
            Write-Output 'The installed app exported live WebView2 and Hook startup records and a sanitized rotated failure record.'
        } finally {
            $archive.Dispose()
        }
    } finally {
        Remove-Item -LiteralPath $rotatedLog
    }
}
