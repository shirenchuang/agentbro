param(
    [Parameter(Mandatory)]
    [int]$AppProcessId,
    [Parameter(Mandatory)]
    [string]$ProjectName,
    [Parameter(Mandatory)]
    [string]$SessionId,
    [Parameter(Mandatory)]
    [string]$Workspace,
    [string]$PackImportPath,
    [string]$PackExportPath
)

$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

public static class AgentBroSmokeInput {
    private const uint InputMouse = 0;
    private const uint InputKeyboard = 1;
    private const uint LeftDown = 0x0002;
    private const uint LeftUp = 0x0004;
    private const uint KeyUp = 0x0002;
    private const uint KeyUnicode = 0x0004;
    private const ushort Control = 0x11;
    private const ushort LetterA = 0x41;

    [StructLayout(LayoutKind.Sequential)]
    public struct Point { public int X; public int Y; }

    [StructLayout(LayoutKind.Sequential)]
    private struct MouseInput {
        public int X, Y;
        public uint Data, Flags, Time;
        public UIntPtr ExtraInfo;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct KeyboardInput {
        public ushort VirtualKey, ScanCode;
        public uint Flags, Time;
        public UIntPtr ExtraInfo;
    }

    [StructLayout(LayoutKind.Explicit)]
    private struct InputData {
        [FieldOffset(0)] public MouseInput Mouse;
        [FieldOffset(0)] public KeyboardInput Keyboard;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct Input { public uint Type; public InputData Data; }

    [DllImport("user32.dll")]
    public static extern bool GetCursorPos(out Point point);

    [DllImport("user32.dll")]
    public static extern bool SetCursorPos(int x, int y);

    [DllImport("user32.dll")]
    public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);

    [DllImport("user32.dll")]
    private static extern IntPtr WindowFromPoint(Point point);

    [DllImport("user32.dll")]
    private static extern IntPtr GetForegroundWindow();

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

    public static uint ForegroundProcess() {
        uint processId;
        GetWindowThreadProcessId(GetForegroundWindow(), out processId);
        return processId;
    }

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint SendInput(uint count, Input[] inputs, int size);

    public static void Click() {
        SendMouseButton(LeftDown);
        System.Threading.Thread.Sleep(60);
        SendMouseButton(LeftUp);
    }

    public static void PressSpace() {
        const ushort Space = 0x20;
        InjectInputs(new[] { KeyboardEvent(Space, 0, 0), KeyboardEvent(Space, 0, KeyUp) });
    }

    public static void ReplaceText(string text) {
        var inputs = new List<Input>(4 + text.Length * 2);
        inputs.Add(KeyboardEvent(Control, 0, 0));
        inputs.Add(KeyboardEvent(LetterA, 0, 0));
        inputs.Add(KeyboardEvent(LetterA, 0, KeyUp));
        inputs.Add(KeyboardEvent(Control, 0, KeyUp));
        foreach (char unit in text) {
            inputs.Add(KeyboardEvent(0, unit, KeyUnicode));
            inputs.Add(KeyboardEvent(0, unit, KeyUnicode | KeyUp));
        }
        InjectInputs(inputs.ToArray());
    }

    private static Input KeyboardEvent(ushort virtualKey, ushort scanCode, uint flags) {
        return new Input {
            Type = InputKeyboard,
            Data = new InputData {
                Keyboard = new KeyboardInput { VirtualKey = virtualKey, ScanCode = scanCode, Flags = flags }
            }
        };
    }

    private static void SendMouseButton(uint flags) {
        InjectInputs(new[] {
            new Input { Type = InputMouse, Data = new InputData { Mouse = new MouseInput { Flags = flags } } }
        });
    }

    private static void InjectInputs(Input[] inputs) {
        uint count = (uint)inputs.Length;
        if (SendInput(count, inputs, Marshal.SizeOf<Input>()) != count)
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
    if (-not [AgentBroSmokeInput]::SetCursorPos($x, $y)) {
        throw "Could not move the Windows cursor to $x, $y"
    }
}

function Find-IslandButton([Windows.Automation.AutomationElement]$Root, [scriptblock]$MatchesName) {
    $buttons = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::Button)
    foreach ($button in $Root.FindAll([Windows.Automation.TreeScope]::Descendants, $buttons)) {
        $state = $button.Current
        if (-not $state.IsOffscreen -and (& $MatchesName ([string]$state.Name))) { return $button }
    }
}

function Click-IslandElement(
    [Windows.Automation.AutomationElement]$Element,
    [string]$Description,
    [Windows.Automation.AutomationElement]$RowAnchor
) {
    $point = if ($RowAnchor) {
        $row = $Element.Current.BoundingRectangle
        $anchor = $RowAnchor.Current.BoundingRectangle
        [Windows.Point]::new($row.Left + $row.Width / 2, $anchor.Top + $anchor.Height / 2)
    } else {
        Wait-IslandValue "a clickable point on $Description" {
            $candidate = [Windows.Point]::new(0, 0)
            if ($Element.TryGetClickablePoint([ref]$candidate)) { $candidate }
        }
    }
    if (-not [AgentBroSmokeInput]::SetCursorPos([int]$point.X, [int]$point.Y)) {
        throw "Could not move the Windows cursor onto $Description"
    }
    Start-Sleep -Milliseconds 250
    $underCursor = [Windows.Automation.AutomationElement]::FromPoint($point)
    $nativeOwner = [AgentBroSmokeInput]::ProcessAtCursor()
    $owner = Get-Process -Id $nativeOwner -ErrorAction SilentlyContinue
    Write-Output "$Description at $($point.X), $($point.Y). Hit: $($underCursor.Current.Name). Native owner: $($owner.ProcessName) ($nativeOwner)."
    if ($nativeOwner -ne $AppProcessId) {
        throw "A different window owns the click point: $($owner.ProcessName) / $($owner.MainWindowTitle)"
    }
    [AgentBroSmokeInput]::Click()
}

function Test-KeyboardPermissionDenial([Windows.Automation.AutomationElement]$Island) {
    $connection = [System.Net.Sockets.TcpClient]::new()
    try {
        $connection.Connect([System.Net.IPAddress]::Loopback, [int]$env:AGENTBRO_HOOK_PORT)
        $stream = $connection.GetStream()
        $event = @{
            agent = 'claude-code'
            event = 'PermissionRequest'
            session_id = $SessionId
            cwd = $Workspace
            tool_name = 'Bash'
            tool_use_id = 'windows-smoke-keyboard-denial'
            tool_input = @{ command = 'echo AgentBro Windows permission fixture' }
        }
        $line = ($event | ConvertTo-Json -Compress) + "`n"
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($line)
        $stream.Write($bytes, 0, $bytes.Length)
        $reader = [System.IO.StreamReader]::new($stream)
        $reply = $reader.ReadLineAsync()

        $showSessions = Wait-IslandValue 'the permission card and its session list button' {
            Find-IslandButton $Island { param($Name) $Name -eq 'Make agents easier to use' }
        }
        Click-IslandElement $showSessions 'Permission card session list button'
        $session = Wait-IslandValue 'the waiting session in the Island list' {
            Find-IslandButton $Island { param($Name) $Name.Contains($ProjectName) }
        }
        $rowAnchor = Wait-IslandValue 'the waiting session header' {
            Find-IslandButton $session { param($Name) $Name.StartsWith('Mute notifications') }
        }
        # The center of a waiting row belongs to the inline permission preview.
        Click-IslandElement $session 'Island session header with a pending permission' $rowAnchor

        $deny = Wait-IslandValue 'Deny in the session detail' {
            if (Find-IslandButton $Island { param($Name) $Name -eq 'Back to sessions' }) {
                Find-IslandButton $Island { param($Name) $Name -eq 'Deny' }
            }
        }
        $deny.SetFocus()
        $null = Wait-IslandValue 'native keyboard focus on detail Deny' {
            $deny.Current.HasKeyboardFocus -and [AgentBroSmokeInput]::ForegroundProcess() -eq $AppProcessId
        }
        if ($reply.IsCompleted) { throw 'The permission request ended before the Space key was sent' }
        [AgentBroSmokeInput]::PressSpace()
        Write-Output 'Sent native Space to the focused Deny button in the session detail.'
        if (-not $reply.Wait(8000)) { throw 'The Hook did not receive a reply after keyboard denial in the detail view' }
        $decision = $reply.GetAwaiter().GetResult() | ConvertFrom-Json
        if ($decision.decision -ne 'deny') { throw "Unexpected Hook permission decision: $($decision.decision)" }
        $null = Wait-IslandValue 'the resolved permission in the detail' {
            (Find-IslandButton $Island { param($Name) $Name -eq 'Back to sessions' }) -and
                -not (Find-IslandButton $Island { param($Name) $Name -eq 'Deny' })
        }
        Write-Output 'Windows Island accepted Space on the detail Deny button and the waiting Hook received deny.'
    } finally {
        $connection.Dispose()
    }
}

function Save-FailedRunnerDesktop {
    if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or -not $env:RUNNER_TEMP) { return }
    Add-Type -AssemblyName System.Windows.Forms, System.Drawing
    $screen = [Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bitmap = [Drawing.Bitmap]::new($screen.Width, $screen.Height)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($screen.Location, [Drawing.Point]::Empty, $screen.Size)
        $bitmap.Save((Join-Path $env:RUNNER_TEMP 'agentbro-island-failure.png'), [Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

$previousDpi = [AgentBroSmokeInput]::SetThreadDpiAwarenessContext([IntPtr](-4))
$originalCursor = [AgentBroSmokeInput+Point]::new()
$cursorSaved = [AgentBroSmokeInput]::GetCursorPos([ref]$originalCursor)
$island = $null
try {
    $island = Wait-IslandValue 'the visible native Island region' { Find-VisibleIsland }
    Move-ToElement $island

    $session = Wait-IslandValue 'the fixture session and its Read target in the hover list' {
        Find-IslandButton $island { param($Name) $Name.Contains($ProjectName) -and $Name.Contains('smoke.txt') }
    }
    Click-IslandElement $session 'Island session'

    $titleName = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::NameProperty, $ProjectName)
    $null = Wait-IslandValue 'the clicked session in the detail view' {
        $title = $island.FindFirst([Windows.Automation.TreeScope]::Descendants, $titleName)
        $title -and -not $title.Current.IsOffscreen -and
            (Find-IslandButton $island { param($Name) $Name -eq 'Back to sessions' })
    }
    Write-Output 'Windows Island rendered the Claude Code activity and opened its session with native mouse input.'
    Test-KeyboardPermissionDenial $island
    if ($PackImportPath) {
        . (Join-Path $PSScriptRoot 'test-windows-pack-manifest.ps1')
        Test-InstalledPackManifest $island $PackImportPath $PackExportPath
    }
} catch {
    try {
        Save-FailedRunnerDesktop
    } catch {
        Write-Output "Could not capture the runner desktop: $_"
    }
    try {
        if ($PackImportPath) {
            foreach ($window in Get-IslandWindows) {
                Write-Output "Window: $($window.Current.Name) ($($window.Current.ClassName))"
                $window.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition) |
                    Select-Object -First 85 -ExpandProperty Current |
                    Select-Object LocalizedControlType, Name, ClassName, AutomationId, IsEnabled, IsOffscreen |
                    ConvertTo-Json -Depth 3 -Compress
            }
        } elseif ($island) {
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
        [void][AgentBroSmokeInput]::SetCursorPos($originalCursor.X, $originalCursor.Y)
    }
    if ($previousDpi -ne [IntPtr]::Zero) {
        [void][AgentBroSmokeInput]::SetThreadDpiAwarenessContext($previousDpi)
    }
}
