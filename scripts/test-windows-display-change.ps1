function Test-InstalledIslandDisplayChange([IntPtr]$WindowHandle) {
    if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
        throw 'The display-mode check requires an isolated GitHub-hosted runner'
    }

    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class AgentBroDisplayModes {
    // DEVMODEW includes fixed UTF-16 buffers and overlapping printer/display fields.
    [StructLayout(LayoutKind.Explicit, Size = 220)]
    public struct Mode {
        [FieldOffset(68)] public ushort Size;
        [FieldOffset(168)] public uint BitsPerPixel;
        [FieldOffset(172)] public uint Width;
        [FieldOffset(176)] public uint Height;
        [FieldOffset(184)] public uint Frequency;
    }

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern bool EnumDisplaySettings(string device, int index, ref Mode mode);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int ChangeDisplaySettingsEx(string device, ref Mode mode, IntPtr window, uint flags, IntPtr reserved);

    private static bool Read(int index, out Mode mode) {
        mode = new Mode { Size = (ushort)Marshal.SizeOf<Mode>() };
        return EnumDisplaySettings(null, index, ref mode);
    }

    public static Mode Current() {
        Mode current;
        if (!Read(-1, out current)) throw new InvalidOperationException("Cannot read the current display mode");
        return current;
    }

    public static Mode Alternative(Mode current) {
        Mode candidate;
        for (int index = 0; Read(index, out candidate); index++) {
            if (candidate.BitsPerPixel == current.BitsPerPixel &&
                candidate.Frequency == current.Frequency &&
                candidate.Width >= 800 && candidate.Height >= 600 &&
                Math.Abs((long)candidate.Width - current.Width) >= 128 &&
                ChangeDisplaySettingsEx(null, ref candidate, IntPtr.Zero, 2, IntPtr.Zero) == 0) {
                return candidate;
            }
        }
        throw new InvalidOperationException("The hosted display offers no suitable alternate mode");
    }

    public static void Apply(Mode mode) {
        int result = ChangeDisplaySettingsEx(null, ref mode, IntPtr.Zero, 0, IntPtr.Zero);
        if (result != 0) throw new InvalidOperationException("Display mode change failed: " + result);
    }
}
'@

    $original = [AgentBroDisplayModes]::Current()
    $alternate = [AgentBroDisplayModes]::Alternative($original)
    if (-not [AgentBroSmokeInput]::SetCursorPos(32, 200)) {
        throw 'Could not move the pointer away from the Island before changing display mode'
    }

    $oldCenter = $original.Width / 2.0
    $newCenter = $alternate.Width / 2.0
    $null = Wait-IslandValue "the Island centered at x=$oldCenter before the display mode change" {
        [Math]::Abs((Get-IslandCenterX $WindowHandle) - $oldCenter) -le 2
    }
    Write-Output "Changing the native display from $($original.Width)x$($original.Height) to $($alternate.Width)x$($alternate.Height)."
    try {
        [AgentBroDisplayModes]::Apply($alternate)
        $null = Wait-IslandValue 'Windows to apply the alternate display mode' {
            [AgentBroDisplayModes]::Current().Width -eq $alternate.Width
        }
        try {
            $null = Wait-IslandValue "the Island to recenter at x=$newCenter after the display mode change" {
                [Math]::Abs((Get-IslandCenterX $WindowHandle) - $newCenter) -le 2
            }
        } catch {
            Write-Output "Native Island center after the display mode change is x=$(Get-IslandCenterX $WindowHandle)."
            throw
        }
    } finally {
        [AgentBroDisplayModes]::Apply($original)
    }

    $null = Wait-IslandValue "the Island to recenter at x=$oldCenter after restoring the original mode" {
        [AgentBroDisplayModes]::Current().Width -eq $original.Width -and
            [Math]::Abs((Get-IslandCenterX $WindowHandle) - $oldCenter) -le 2
    }
    Write-Output "Windows Island recentered at x=$newCenter and x=$oldCenter after changing and restoring the display mode."
}
