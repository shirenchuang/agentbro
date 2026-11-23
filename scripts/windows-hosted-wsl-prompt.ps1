$ErrorActionPreference = 'Stop'
if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or $env:RUNNER_ARCH -ne 'ARM64') {
    throw 'WSL prompt cleanup requires an isolated GitHub-hosted Windows ARM64 runner'
}

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

    public static string[] MinimizeWindows(string className, string title) {
        var requests = new List<string>();
        for (var window = FindWindowEx(IntPtr.Zero, IntPtr.Zero, className, title);
             window != IntPtr.Zero;
             window = FindWindowEx(IntPtr.Zero, window, className, title)) {
            if (!IsWindowVisible(window) || IsIconic(window)) continue;
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

$title = Join-Path $env:SystemRoot 'System32\wsl.exe'
Write-Output 'Watching for the hosted Windows ARM64 WSL update prompt.'
while ($true) {
    foreach ($windowClass in 'CASCADIA_HOSTING_WINDOW_CLASS', 'ConsoleWindowClass') {
        [AgentBroWslPrompt]::MinimizeWindows($windowClass, $title)
    }
    Start-Sleep -Milliseconds 100
}
