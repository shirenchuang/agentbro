param(
    [Parameter(Mandatory)][string]$AppPath,
    [Parameter(Mandatory)][string]$Stage
)

$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;

public static class AgentBroStartupDialog {
    private const uint GetTextMessage = 0x000D;
    private const uint CommandMessage = 0x0111;
    private const uint AbortIfHung = 0x0002;
    private const uint ErrorOnExit = 0x0020;

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string className, string title);

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);

    [DllImport("user32.dll")]
    private static extern bool IsWindowVisible(IntPtr window);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern IntPtr SendMessageTimeout(IntPtr window, uint message, IntPtr length,
        StringBuilder text, uint flags, uint timeout, out IntPtr result);

    [DllImport("user32.dll")]
    private static extern int GetDlgCtrlID(IntPtr window);

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool PostMessage(IntPtr window, uint message, IntPtr command, IntPtr control);

    public static IntPtr Find(uint processId) {
        for (var dialog = FindWindowEx(IntPtr.Zero, IntPtr.Zero, "#32770", "AgentBro");
             dialog != IntPtr.Zero;
             dialog = FindWindowEx(IntPtr.Zero, dialog, "#32770", "AgentBro")) {
            uint owner;
            GetWindowThreadProcessId(dialog, out owner);
            if (owner == processId && IsWindowVisible(dialog)) return dialog;
        }
        return IntPtr.Zero;
    }

    public static string ReadText(IntPtr dialog) {
        var body = new StringBuilder();
        for (var control = FindWindowEx(dialog, IntPtr.Zero, "Static", null);
             control != IntPtr.Zero;
             control = FindWindowEx(dialog, control, "Static", null)) {
            var text = new StringBuilder(2048);
            IntPtr result;
            if (SendMessageTimeout(control, GetTextMessage, (IntPtr)text.Capacity, text,
                AbortIfHung | ErrorOnExit, 250, out result) != IntPtr.Zero && text.Length != 0) {
                body.AppendLine(text.ToString());
            }
        }
        return body.ToString();
    }

    public static void Acknowledge(IntPtr dialog) {
        var button = FindWindowEx(dialog, IntPtr.Zero, "Button", null);
        if (button == IntPtr.Zero)
            throw new InvalidOperationException("The native startup error has no button control.");
        var buttonId = GetDlgCtrlID(button);
        if (!PostMessage(dialog, CommandMessage, (IntPtr)buttonId, button))
            throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not activate the native startup error button.");
    }
}
'@

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
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    $text = ''
    do {
        if ($launch.HasExited) {
            throw "AgentBro exited with code $($launch.ExitCode) before showing its startup error."
        }
        $dialog = [AgentBroStartupDialog]::Find($launch.Id)
        if ($dialog -ne [IntPtr]::Zero) {
            $text = [AgentBroStartupDialog]::ReadText($dialog)
            if ($text -match 'Microsoft Edge WebView2 Runtime') { break }
        }
        Start-Sleep -Milliseconds 100
    } until ([DateTime]::UtcNow -ge $deadline)
    if ($dialog -eq [IntPtr]::Zero) { throw 'Timed out waiting for the AgentBro startup error dialog.' }
    if ($text -notmatch 'Microsoft Edge WebView2 Runtime') {
        throw "The startup error did not explain which runtime to check. $text"
    }
    Write-Output $text.Trim()
    [AgentBroStartupDialog]::Acknowledge($dialog)
    if (-not $launch.WaitForExit(5000) -or $launch.ExitCode -ne 1) {
        throw 'Acknowledging the native startup error did not close the failed launch with exit code 1.'
    }
    $log = $stdout.GetAwaiter().GetResult()
    $startupError = $log -split "\r?\n" | Where-Object {
        $_ -match 'Startup: could not locate the WebView2 runtime: .+'
    }
    if (-not $startupError -or $log -match 'Startup: scheduling Hook server') {
        throw 'Startup did not report the unavailable WebView2 runtime before stopping setup.'
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
