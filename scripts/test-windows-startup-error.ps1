param(
    [Parameter(Mandatory)][string]$AppPath,
    [Parameter(Mandatory)][string]$Stage
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes

$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = $AppPath
$start.WorkingDirectory = Split-Path $AppPath
$start.UseShellExecute = $false
$start.RedirectStandardOutput = $true
$start.RedirectStandardError = $true
$start.EnvironmentVariables['WEBVIEW2_BROWSER_EXECUTABLE_FOLDER'] = Join-Path $Stage 'missing-webview2-runtime'

$launch = [Diagnostics.Process]::Start($start)
try {
    $stdout = $launch.StandardOutput.ReadToEndAsync()
    $stderr = $launch.StandardError.ReadToEndAsync()
    $condition = [Windows.Automation.AndCondition]::new(
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ProcessIdProperty, $launch.Id),
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ClassNameProperty, '#32770'),
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty, 'AgentBro')
    )
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        if ($launch.HasExited) {
            throw "AgentBro exited with code $($launch.ExitCode) before showing its startup error."
        }
        $dialog = [Windows.Automation.AutomationElement]::RootElement.FindFirst(
            [Windows.Automation.TreeScope]::Children, $condition)
        if ($dialog) { break }
        Start-Sleep -Milliseconds 100
    } until ([DateTime]::UtcNow -ge $deadline)
    if (-not $dialog) { throw 'Timed out waiting for the AgentBro startup error dialog.' }

    $textCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::Text)
    $text = $dialog.FindAll([Windows.Automation.TreeScope]::Descendants, $textCondition) |
        ForEach-Object { $_.Current.Name }
    if (($text -join ' ') -notmatch 'Microsoft Edge WebView2 Runtime') {
        throw "The startup error did not explain which runtime to check. $text"
    }
    $buttonCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::Button)
    $acknowledge = $dialog.FindFirst([Windows.Automation.TreeScope]::Descendants, $buttonCondition)
    $acknowledge.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke()
    if (-not $launch.WaitForExit(5000) -or $launch.ExitCode -ne 1) {
        throw 'Acknowledging the native startup error did not close the failed launch with exit code 1.'
    }
    $log = $stdout.GetAwaiter().GetResult()
    $startupError = $log -split "\r?\n" | Where-Object {
        $_ -match "Startup: could not create Windows window 'notch': .+"
    }
    if (-not $startupError -or $log -match 'Startup: scheduling Hook server') {
        throw 'Startup did not log the window creation failure before stopping setup.'
    }
    Write-Output $startupError
    Write-Output 'Acknowledged the installed native WebView2 startup error before normal AgentBro setup.'
} catch {
    if (-not $launch.HasExited) {
        $launch.Kill()
        $null = $launch.WaitForExit(3000)
    }
    foreach ($output in @($stdout, $stderr)) {
        if ($output.IsCompleted) { Write-Output ($output.GetAwaiter().GetResult()) }
    }
    throw
} finally {
    if (-not $launch.HasExited) {
        $launch.Kill()
        $null = $launch.WaitForExit(3000)
    }
    $launch.Dispose()
}
