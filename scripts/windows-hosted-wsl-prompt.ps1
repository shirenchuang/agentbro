$ErrorActionPreference = 'Stop'
if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or $env:RUNNER_ARCH -ne 'ARM64') {
    throw 'Desktop prompt cleanup requires an isolated GitHub-hosted Windows ARM64 runner'
}

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;

public static class AgentBroWslPrompt {
    private const int Minimize = 6;
    private const int OkControlId = 1;
    private const uint GetTextMessage = 0x000D;
    private const uint CommandMessage = 0x0111;
    private const uint AbortIfHung = 0x0002;
    private const uint ErrorOnExit = 0x0020;

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

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern IntPtr SendMessageTimeout(IntPtr window, uint message, IntPtr length,
        StringBuilder text, uint flags, uint timeout, out IntPtr result);

    [DllImport("user32.dll")]
    private static extern IntPtr GetDlgItem(IntPtr window, int controlId);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern bool PostMessage(IntPtr window, uint message, IntPtr command, IntPtr control);

    private static IEnumerable<IntPtr> VisibleWindows(string className, string title) {
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

    public static string[] AcknowledgePagingFileNotices() {
        var requests = new List<string>();
        foreach (var window in VisibleWindows("#32770", "System Properties")) {
            uint processId;
            GetWindowThreadProcessId(window, out processId);
            if (processId == 0) continue;
            try {
                using (var owner = Process.GetProcessById((int)processId)) {
                    if (owner.ProcessName != "SystemPropertiesPerformance") continue;
                }
            } catch (ArgumentException) {
                continue;
            }
            var ok = GetDlgItem(window, OkControlId);
            if (!IsWindowVisible(ok)) continue;
            for (var label = FindWindowEx(window, IntPtr.Zero, "Static", null);
                 label != IntPtr.Zero;
                 label = FindWindowEx(window, label, "Static", null)) {
                var text = new StringBuilder(512);
                IntPtr result;
                if (SendMessageTimeout(label, GetTextMessage, (IntPtr)text.Capacity, text,
                    AbortIfHung | ErrorOnExit, 100, out result) == IntPtr.Zero ||
                    !text.ToString().StartsWith("Windows created a temporary paging file on your computer", StringComparison.Ordinal))
                    continue;
                if (PostMessage(window, CommandMessage, (IntPtr)OkControlId, ok)) {
                    requests.Add(string.Format(
                        "Acknowledged the hosted paging-file notice HWND {0} (PID {1}) at {2:O}.",
                        window, processId, DateTime.UtcNow));
                }
                break;
            }
        }
        return requests.ToArray();
    }
}
'@

$title = Join-Path $env:SystemRoot 'System32\wsl.exe'
Write-Output 'Watching for the hosted Windows ARM64 WSL update prompt and paging-file notice.'
while ($true) {
    [AgentBroWslPrompt]::AcknowledgePagingFileNotices()
    foreach ($windowClass in 'CASCADIA_HOSTING_WINDOW_CLASS', 'ConsoleWindowClass') {
        [AgentBroWslPrompt]::MinimizeWindows($windowClass, $title)
    }
    Start-Sleep -Milliseconds 100
}
