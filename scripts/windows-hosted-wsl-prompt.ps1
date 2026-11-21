$ErrorActionPreference = 'Stop'
if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or $env:RUNNER_ARCH -ne 'ARM64') {
    throw 'WSL prompt cleanup requires an isolated GitHub-hosted Windows ARM64 runner'
}

Add-Type -TypeDefinition @'
using System;
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

    public static int MinimizeWindows(string className, string title) {
        var count = 0;
        for (var window = FindWindowEx(IntPtr.Zero, IntPtr.Zero, className, title);
             window != IntPtr.Zero;
             window = FindWindowEx(IntPtr.Zero, window, className, title)) {
            if (IsWindowVisible(window) && !IsIconic(window) && ShowWindowAsync(window, Minimize)) count++;
        }
        return count;
    }
}
'@

$title = Join-Path $env:SystemRoot 'System32\wsl.exe'
Write-Output 'Watching for the hosted Windows ARM64 WSL update prompt.'
while ($true) {
    foreach ($windowClass in 'CASCADIA_HOSTING_WINDOW_CLASS', 'ConsoleWindowClass') {
        $requestCount = [AgentBroWslPrompt]::MinimizeWindows($windowClass, $title)
        if ($requestCount) {
            Write-Output "Requested minimization for $requestCount hosted WSL prompt window(s) at $([DateTimeOffset]::UtcNow.ToString('O'))."
        }
    }
    Start-Sleep -Milliseconds 100
}
