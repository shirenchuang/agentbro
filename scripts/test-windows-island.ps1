param(
    [Parameter(Mandatory)]
    [int]$AppProcessId,
    [Parameter(Mandatory)]
    [string]$ProjectName
)

$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class AgentBroSmokeMouse {
    private const uint InputMouse = 0;
    private const uint LeftDown = 0x0002;
    private const uint LeftUp = 0x0004;

    [StructLayout(LayoutKind.Sequential)]
    public struct Point { public int X; public int Y; }

    [StructLayout(LayoutKind.Sequential)]
    private struct MouseInput {
        public int X, Y;
        public uint Data, Flags, Time;
        public UIntPtr ExtraInfo;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct Input { public uint Type; public MouseInput Mouse; }

    [DllImport("user32.dll")]
    public static extern bool GetCursorPos(out Point point);

    [DllImport("user32.dll")]
    public static extern bool SetCursorPos(int x, int y);

    [DllImport("user32.dll")]
    public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);

    [DllImport("user32.dll")]
    private static extern IntPtr WindowFromPoint(Point point);

    [DllImport("user32.dll")]
    private static extern IntPtr GetAncestor(IntPtr window, uint flags);

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);

    public static uint ProcessAtCursor() {
        Point point;
        if (!GetCursorPos(out point)) return 0;
        uint processId;
        GetWindowThreadProcessId(GetAncestor(WindowFromPoint(point), 2), out processId);
        return processId;
    }

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint SendInput(uint count, Input[] inputs, int size);

    public static void Click() {
        SendMouseButton(LeftDown);
        System.Threading.Thread.Sleep(60);
        SendMouseButton(LeftUp);
    }

    private static void SendMouseButton(uint flags) {
        var inputs = new[] { new Input { Type = InputMouse, Mouse = new MouseInput { Flags = flags } } };
        if (SendInput(1, inputs, Marshal.SizeOf<Input>()) != 1)
            throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
    }
}
'@

function Wait-IslandValue([string]$Description, [scriptblock]$ReadValue) {
    $timer = [Diagnostics.Stopwatch]::StartNew()
    do {
        $value = & $ReadValue
        if ($value) { return $value }
        Start-Sleep -Milliseconds 250
    } while ($timer.Elapsed.TotalSeconds -lt 15)
    throw "Timed out waiting for $Description"
}

function Get-IslandWindows {
    $processCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ProcessIdProperty, $AppProcessId)
    [Windows.Automation.AutomationElement]::RootElement.FindAll(
        [Windows.Automation.TreeScope]::Children, $processCondition)
}

function Find-VisibleIsland {
    $regionName = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::NameProperty, 'AgentBro')
    foreach ($window in Get-IslandWindows) {
        foreach ($region in $window.FindAll([Windows.Automation.TreeScope]::Descendants, $regionName)) {
            $state = $region.Current
            if ($state.LocalizedControlType -eq 'region' -and -not $state.IsOffscreen -and -not $state.BoundingRectangle.IsEmpty) {
                return $region
            }
        }
    }
}

function Move-ToElement([Windows.Automation.AutomationElement]$Element) {
    $rect = $Element.Current.BoundingRectangle
    $x = [int]($rect.X + $rect.Width / 2)
    $y = [int]($rect.Y + $rect.Height / 2)
    if (-not [AgentBroSmokeMouse]::SetCursorPos($x, $y)) {
        throw "Could not move the Windows cursor to $x, $y"
    }
}

$previousDpi = [AgentBroSmokeMouse]::SetThreadDpiAwarenessContext([IntPtr](-4))
$originalCursor = [AgentBroSmokeMouse+Point]::new()
$cursorSaved = [AgentBroSmokeMouse]::GetCursorPos([ref]$originalCursor)
$island = $null
try {
    $island = Wait-IslandValue 'the visible native Island region' { Find-VisibleIsland }
    Move-ToElement $island

    $buttonCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::Button)
    $session = Wait-IslandValue 'the fixture session and its Read target in the hover list' {
        foreach ($button in $island.FindAll([Windows.Automation.TreeScope]::Descendants, $buttonCondition)) {
            $state = $button.Current
            if (-not $state.IsOffscreen -and $state.Name.Contains($ProjectName) -and $state.Name.Contains('smoke.txt')) {
                return $button
            }
        }
    }
    $point = Wait-IslandValue 'a clickable point on the fixture session' {
        $candidate = [Windows.Point]::new(0, 0)
        if ($session.TryGetClickablePoint([ref]$candidate)) { $candidate }
    }
    if (-not [AgentBroSmokeMouse]::SetCursorPos([int]$point.X, [int]$point.Y)) {
        throw 'Could not move the Windows cursor onto the session'
    }
    Start-Sleep -Milliseconds 250
    $underCursor = [Windows.Automation.AutomationElement]::FromPoint($point)
    $nativeOwner = [AgentBroSmokeMouse]::ProcessAtCursor()
    $ownerName = (Get-Process -Id $nativeOwner -ErrorAction SilentlyContinue).ProcessName
    Write-Output "Clicking Island session at $($point.X), $($point.Y). Hit: $($underCursor.Current.Name). Native owner: $ownerName ($nativeOwner). AgentBro: $AppProcessId."
    [AgentBroSmokeMouse]::Click()

    $titleName = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::NameProperty, $ProjectName)
    $backName = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::NameProperty, 'Back to sessions')
    $backButtonCondition = [Windows.Automation.AndCondition]::new($backName, $buttonCondition)
    $null = Wait-IslandValue 'the clicked session in the detail view' {
        $title = $island.FindFirst([Windows.Automation.TreeScope]::Descendants, $titleName)
        $back = $island.FindFirst([Windows.Automation.TreeScope]::Descendants, $backButtonCondition)
        $title -and $back -and -not $title.Current.IsOffscreen -and -not $back.Current.IsOffscreen
    }
    Write-Output 'Windows Island rendered the Claude Code activity and opened its session with native mouse input.'
} catch {
    try {
        if ($island) {
            $island.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition) |
                Select-Object -First 35 -ExpandProperty Current |
                Select-Object LocalizedControlType, Name, IsOffscreen, @{Name='Bounds'; Expression={$_.BoundingRectangle.ToString()}} |
                ConvertTo-Json -Depth 3 -Compress
        } else {
            Get-IslandWindows | Select-Object -ExpandProperty Current |
                Select-Object Name, ClassName, IsOffscreen, BoundingRectangle |
                ConvertTo-Json -Depth 3 -Compress
        }
    } catch {
        Write-Output "Could not read Island accessibility state: $_"
    }
    throw
} finally {
    if ($cursorSaved) {
        [void][AgentBroSmokeMouse]::SetCursorPos($originalCursor.X, $originalCursor.Y)
    }
    if ($previousDpi -ne [IntPtr]::Zero) {
        [void][AgentBroSmokeMouse]::SetThreadDpiAwarenessContext($previousDpi)
    }
}
