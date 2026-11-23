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
    private const uint GetTextMessage = 0x000D;
    private const uint CommandMessage = 0x0111;
    private const uint AbortIfHung = 0x0002;
    private const uint ErrorOnExit = 0x0020;
    private static readonly HashSet<IntPtr> inspectedNotices = new HashSet<IntPtr>();
    private delegate bool EnumWindowCallback(IntPtr window, IntPtr state);

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
    private static extern int GetDlgCtrlID(IntPtr window);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetClassName(IntPtr window, StringBuilder className, int capacity);

    [DllImport("user32.dll")]
    private static extern bool EnumChildWindows(IntPtr parent, EnumWindowCallback callback, IntPtr state);

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
        foreach (var window in VisibleWindows(null, "System Properties")) {
            uint processId;
            GetWindowThreadProcessId(window, out processId);
            if (processId == 0) continue;
            try {
                using (var owner = Process.GetProcessById((int)processId)) {
                    if (!owner.ProcessName.Equals("SystemPropertiesPerformance", StringComparison.OrdinalIgnoreCase))
                        continue;
                }
            } catch (ArgumentException) {
                continue;
            }
            var className = new StringBuilder(128);
            GetClassName(window, className, className.Capacity);
            if (className.ToString() != "#32770") continue;
            bool hasPagingText = false;
            IntPtr ok = IntPtr.Zero;
            var controls = new List<string>();
            EnumChildWindows(window, (control, _) => {
                if (!IsWindowVisible(control)) return true;
                className.Clear();
                GetClassName(control, className, className.Capacity);
                var text = new StringBuilder(512);
                IntPtr result;
                if (SendMessageTimeout(control, GetTextMessage, (IntPtr)text.Capacity, text,
                    AbortIfHung | ErrorOnExit, 100, out result) != IntPtr.Zero) {
                    var label = text.ToString();
                    if (label.StartsWith("Windows created a temporary paging file on your computer", StringComparison.Ordinal))
                        hasPagingText = true;
                    if (className.ToString() == "Button" && label == "OK") ok = control;
                }
                controls.Add(className.ToString());
                return true;
            }, IntPtr.Zero);
            if (hasPagingText && ok != IntPtr.Zero && PostMessage(window, CommandMessage, (IntPtr)GetDlgCtrlID(ok), ok)) {
                requests.Add(string.Format(
                    "Requested acknowledgement of the hosted paging-file notice HWND {0} (PID {1}) at {2:O}.",
                    window, processId, DateTime.UtcNow));
            } else if (inspectedNotices.Add(window)) {
                requests.Add(string.Format(
                    "Inspected System Properties HWND {0} (PID {1}). Paging text: {2}, OK button: {3}, controls: {4}.",
                    window, processId, hasPagingText, ok != IntPtr.Zero, string.Join(", ", controls)));
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
