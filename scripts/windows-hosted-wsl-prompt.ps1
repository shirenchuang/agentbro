$ErrorActionPreference = 'Stop'
if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or $env:RUNNER_ARCH -ne 'ARM64') {
    throw 'Desktop prompt cleanup requires an isolated GitHub-hosted Windows ARM64 runner'
}

Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

public static class AgentBroWslPrompt {
    private const int Minimize = 6;

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string className, string title);

    [DllImport("user32.dll")]
    private static extern bool IsWindowVisible(IntPtr window);

    [DllImport("user32.dll")]
    private static extern bool IsIconic(IntPtr window);

    [DllImport("user32.dll")]
    private static extern bool ShowWindowAsync(IntPtr window, int command);

    [DllImport("user32.dll")]
    private static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);

    public static IEnumerable<IntPtr> VisibleWindows(string className, string title) {
        for (var window = FindWindowEx(IntPtr.Zero, IntPtr.Zero, className, title);
             window != IntPtr.Zero;
             window = FindWindowEx(IntPtr.Zero, window, className, title)) {
            if (IsWindowVisible(window) && !IsIconic(window)) yield return window;
        }
    }

    public static string[] MinimizeWindows(string className, string title) {
        var requests = new List<string>();
        foreach (var window in VisibleWindows(className, title)) {
            uint processId;
            GetWindowThreadProcessId(window, out processId);
            var now = DateTime.UtcNow;
            var foreground = GetForegroundWindow();
            if (ShowWindowAsync(window, Minimize)) {
                requests.Add(string.Format(
                    "Requested minimization for hosted WSL prompt HWND {0} ({1}, PID {2}) at {3:O}. Foreground HWND {4}.",
                    window, className, processId, now, foreground));
            }
        }
        return requests.ToArray();
    }
}
'@

function Acknowledge-PagingFileNotice {
    foreach ($handle in [AgentBroWslPrompt]::VisibleWindows('#32770', 'System Properties')) {
        try {
            $window = [Windows.Automation.AutomationElement]::FromHandle($handle)
            $owner = Get-Process -Id $window.Current.ProcessId -ErrorAction SilentlyContinue
            if ($owner.ProcessName -ne 'SystemPropertiesPerformance') { continue }
            $notice = $window.FindAll([Windows.Automation.TreeScope]::Descendants, $textControl) |
                Where-Object { $_.Current.Name -like 'Windows created a temporary paging file on your computer*' } |
                Select-Object -First 1
            if (-not $notice) { continue }
            $ok = $window.FindFirst([Windows.Automation.TreeScope]::Descendants, $okButton)
            $invoke = $null
            if ($ok -and $ok.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$invoke)) {
                $invoke.Invoke()
                Write-Output "Acknowledged the hosted paging-file notice HWND $handle (PID $($owner.Id)) at $([DateTime]::UtcNow.ToString('O'))."
            }
        } catch {
            if ($_.Exception -isnot [Windows.Automation.ElementNotAvailableException] -and
                $_.Exception.InnerException -isnot [Windows.Automation.ElementNotAvailableException]) { throw }
        }
    }
}

$textControl = [Windows.Automation.PropertyCondition]::new(
    [Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::Text)
$okButton = [Windows.Automation.AndCondition]::new(
    [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::Button),
    [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty, 'OK'))
$title = Join-Path $env:SystemRoot 'System32\wsl.exe'
Write-Output 'Watching for the hosted Windows ARM64 WSL update prompt and paging-file notice.'
while ($true) {
    Acknowledge-PagingFileNotice
    foreach ($windowClass in 'CASCADIA_HOSTING_WINDOW_CLASS', 'ConsoleWindowClass') {
        [AgentBroWslPrompt]::MinimizeWindows($windowClass, $title)
    }
    Start-Sleep -Milliseconds 100
}
