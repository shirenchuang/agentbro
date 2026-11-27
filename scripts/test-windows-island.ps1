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
    [string]$PackExportPath,
    [string]$PackArchivePath,
    [string]$PinnedPackImportPath,
    [string]$PinnedPackExportPath,
    [string]$PackManifestLink,
    [string]$DiagnosticsExportPath,
    [switch]$TracePermissionUi,
    [switch]$VerifyQuestionReconnect,
    [switch]$VerifyQwenWork,
    [switch]$VerifyDrag,
    [switch]$VerifyDisplayChange,
    [switch]$CloseStartupSettings,
    [switch]$AutomationFocusPermission,
    [switch]$VerifyGlobalShortcuts,
    [switch]$VerifyAgentPackageConsole,
    [switch]$VerifyRegisteredAgentApp
)

$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
if ($TracePermissionUi) {
    if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or -not $env:RUNNER_TEMP) {
        throw 'Permission screenshots require an isolated GitHub-hosted runner'
    }
    $permissionTraceDirectory = Join-Path $env:RUNNER_TEMP 'agentbro-permission-trace'
    $null = New-Item -ItemType Directory -Path $permissionTraceDirectory -Force
}

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class AgentBroSmokeInput {
    private const uint InputMouse = 0;
    private const uint InputKeyboard = 1;
    private const uint LeftDown = 0x0002;
    private const uint LeftUp = 0x0004;
    private const uint KeyUp = 0x0002;
    private const uint KeyUnicode = 0x0004;

    [StructLayout(LayoutKind.Sequential)]
    public struct Point { public int X; public int Y; }

    [StructLayout(LayoutKind.Sequential)]
    public struct Rect { public int Left, Top, Right, Bottom; }

    [StructLayout(LayoutKind.Sequential)]
    private struct GuiThreadInfo {
        public uint Size, Flags;
        public IntPtr Active, Focus, Capture, MenuOwner, MoveSize, Caret;
        public Rect CaretRect;
    }

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
    private static extern short GetAsyncKeyState(int key);

    public static bool IsLeftButtonDown() { return GetAsyncKeyState(0x01) < 0; }

    [DllImport("user32.dll")]
    public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);

    [DllImport("user32.dll")]
    public static extern uint GetDpiForWindow(IntPtr window);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr window, out Rect rect);

    [DllImport("user32.dll")]
    private static extern IntPtr WindowFromPoint(Point point);

    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr window);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool GetGUIThreadInfo(uint thread, ref GuiThreadInfo info);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetClassName(IntPtr window, StringBuilder name, int capacity);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetWindowText(IntPtr window, StringBuilder title, int capacity);

    [DllImport("user32.dll")]
    private static extern IntPtr GetAncestor(IntPtr window, uint flags);

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);

    public static IntPtr WindowAtCursor() {
        Point point;
        if (!GetCursorPos(out point)) return IntPtr.Zero;
        return GetAncestor(WindowFromPoint(point), 2);
    }

    public static uint ProcessAtCursor() {
        uint processId;
        GetWindowThreadProcessId(WindowAtCursor(), out processId);
        return processId;
    }

    private static string DescribeWindow(IntPtr window) {
        if (window == IntPtr.Zero) return "none";
        var name = new StringBuilder(128);
        GetClassName(window, name, name.Capacity);
        return string.Format("{0} ({1})", window, name);
    }

    public static string DescribeForegroundFocus() {
        return DescribeForegroundFocus(GetForegroundWindow());
    }

    private static string DescribeForegroundFocus(IntPtr foreground) {
        if (foreground == IntPtr.Zero) return "no foreground window";
        uint processId;
        uint thread = GetWindowThreadProcessId(foreground, out processId);
        var title = new StringBuilder(256);
        GetWindowText(foreground, title, title.Capacity);
        string processName = "unknown";
        try {
            using (var process = System.Diagnostics.Process.GetProcessById((int)processId)) {
                processName = process.ProcessName;
            }
        } catch (ArgumentException) { }
          catch (InvalidOperationException) { }
          catch (System.ComponentModel.Win32Exception) { }
        string owner = string.Format("foreground {0}, title '{1}', process {2} ({3})",
            DescribeWindow(foreground), title, processName, processId);
        var info = new GuiThreadInfo { Size = (uint)Marshal.SizeOf<GuiThreadInfo>() };
        if (thread != 0 && GetGUIThreadInfo(thread, ref info)) {
            owner += string.Format(", active {0}, focus {1}, capture {2}",
                DescribeWindow(info.Active), DescribeWindow(info.Focus), DescribeWindow(info.Capture));
        }
        return owner;
    }

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint SendInput(uint count, Input[] inputs, int size);

    public static void Click() {
        SendMouseButton(LeftDown);
        System.Threading.Thread.Sleep(60);
        SendMouseButton(LeftUp);
    }

    public static void PressLeft() { SendMouseButton(LeftDown); }
    public static void ReleaseLeft() { SendMouseButton(LeftUp); }

    public static void SetGlobalShortcutKey(ushort key, bool pressed) {
        const ushort Control = 0x11;
        const ushort Alt = 0x12;
        InjectInputs(pressed
            ? new[] { KeyboardEvent(Control, 0, 0), KeyboardEvent(Alt, 0, 0), KeyboardEvent(key, 0, 0) }
            : new[] { KeyboardEvent(key, 0, KeyUp), KeyboardEvent(Alt, 0, KeyUp), KeyboardEvent(Control, 0, KeyUp) });
    }

    public static void PressKeyInWindow(IntPtr window, ushort key) {
        var foreground = GetForegroundWindow();
        if (window == IntPtr.Zero || foreground != window) {
            throw new InvalidOperationException(string.Format(
                "Refused native key 0x{0:X2} for HWND {1} at {2:O}. Observed {3}.",
                key, window, DateTime.UtcNow, DescribeForegroundFocus(foreground)));
        }
        InjectInputs(new[] { KeyboardEvent(key, 0, 0), KeyboardEvent(key, 0, KeyUp) });
    }

    public static bool PasteInWindow(IntPtr window) {
        if (window == IntPtr.Zero || GetForegroundWindow() != window) return false;
        const ushort Control = 0x11;
        const ushort V = 0x56;
        InjectInputs(new[] {
            KeyboardEvent(Control, 0, 0), KeyboardEvent(V, 0, 0),
            KeyboardEvent(V, 0, KeyUp), KeyboardEvent(Control, 0, KeyUp)
        });
        return true;
    }

    public static void AppendCharacter(char unit) {
        const ushort End = 0x23;
        InjectInputs(new[] {
            KeyboardEvent(End, 0, 0),
            KeyboardEvent(End, 0, KeyUp),
            KeyboardEvent(0, unit, KeyUnicode),
            KeyboardEvent(0, unit, KeyUnicode | KeyUp)
        });
    }

    public static void ScrollWheel(int delta) {
        const uint MouseWheel = 0x0800;
        InjectInputs(new[] {
            new Input {
                Type = InputMouse,
                Data = new InputData { Mouse = new MouseInput { Flags = MouseWheel, Data = unchecked((uint)delta) } }
            }
        });
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

function Wait-IslandValue([string]$Description, [scriptblock]$ReadValue, [int]$TimeoutSeconds = 15) {
    $timer = [Diagnostics.Stopwatch]::StartNew()
    do {
        $value = & $ReadValue
        if ($value) { return $value }
        Start-Sleep -Milliseconds 250
    } while ($timer.Elapsed.TotalSeconds -lt $TimeoutSeconds)
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
                return [PSCustomObject]@{
                    Element = $region
                    Handle = [IntPtr]$window.Current.NativeWindowHandle
                }
            }
        }
    }
}

function Move-ToIslandHoverPoint([IntPtr]$WindowHandle, [Windows.Automation.AutomationElement]$Region) {
    $native = [AgentBroSmokeInput+Rect]::new()
    $dpi = [AgentBroSmokeInput]::GetDpiForWindow($WindowHandle)
    if ($dpi -eq 0 -or -not [AgentBroSmokeInput]::GetWindowRect($WindowHandle, [ref]$native)) {
        throw 'Could not read the native Island window for cursor entry'
    }
    $x = [int](($native.Left + $native.Right) / 2)
    $y = [int]($native.Top + 12 * ($dpi / 96.0))
    if ($TracePermissionUi) {
        $uia = $Region.Current.BoundingRectangle
        Write-Host "Island cursor entry: ($x, $y). Accessibility region center Y: $($uia.Top + $uia.Height / 2)."
    }
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

function Find-IslandSessionGroup(
    [Windows.Automation.AutomationElement]$Root,
    [scriptblock]$MatchesTitle,
    [string]$VisibleText
) {
    $groups = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::Group)
    foreach ($group in $Root.FindAll([Windows.Automation.TreeScope]::Descendants, $groups)) {
        $state = $group.Current
        if ($state.IsOffscreen -or ($state.ClassName -split '\s+') -notcontains 'hover-list__card' -or
            -not (& $MatchesTitle ([string]$state.Name))) { continue }
        if ($VisibleText) {
            $textName = [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::NameProperty, $VisibleText)
            $visible = $group.FindAll([Windows.Automation.TreeScope]::Descendants, $textName) |
                Where-Object { -not $_.Current.IsOffscreen } | Select-Object -First 1
            if (-not $visible) { continue }
        }
        return $group
    }
}

function Find-IslandSessionOpener([Windows.Automation.AutomationElement]$Group) {
    if (-not $Group) { return }
    $title = $Group.Current.Name
    Find-IslandButton $Group { param($Name) $Name -eq $title }
}

function Close-StartupSettings {
    $settings = Wait-IslandValue 'the first-run Settings window' {
        foreach ($window in Get-IslandWindows) {
            if (Find-IslandButton $window { param($Name) $Name -eq 'Start AgentBro' }) {
                return $window
            }
        }
    }
    $handle = $settings.Current.NativeWindowHandle
    $close = $settings.GetCurrentPattern([Windows.Automation.WindowPattern]::Pattern)
    $close.Close()
    $null = Wait-IslandValue 'the Settings window to close' {
        -not (Get-IslandWindows | Where-Object { $_.Current.NativeWindowHandle -eq $handle })
    }
    Write-Output 'Closed the first-run Settings window before exercising the Island.'
}

function Write-IslandAccessibilityBounds {
    param([IntPtr]$WindowHandle, [string]$FixtureProject = '')

    $native = [AgentBroSmokeInput+Rect]::new()
    if ([AgentBroSmokeInput]::GetWindowRect($WindowHandle, [ref]$native)) {
        $dpi = [AgentBroSmokeInput]::GetDpiForWindow($WindowHandle)
        Write-Host "Native window $WindowHandle bounds: $($native.Left),$($native.Top),$($native.Right),$($native.Bottom) at $dpi DPI."
    }
    $window = [Windows.Automation.AutomationElement]::FromHandle($WindowHandle)
    if ($window) {
        $window.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition) |
            Select-Object -ExpandProperty Current |
            Where-Object {
                $_.ClassName -in @('WRY_WEBVIEW', 'Chrome_WidgetWin_1', 'BrowserRootView', 'notch-panel', 'hover-list__card') -or
                $_.ControlType -eq [Windows.Automation.ControlType]::Document -or
                $_.Name -in @('Back to sessions', 'Deny', 'Allow Once') -or
                ($FixtureProject -and $_.Name -and $_.Name.Contains($FixtureProject))
            } |
            Select-Object Name, ClassName, IsOffscreen, @{
                Name = 'Bounds'
                Expression = { $_.BoundingRectangle.ToString([Globalization.CultureInfo]::InvariantCulture) }
            } |
            ConvertTo-Json -Depth 3 -Compress |
            Write-Host
    }
}

function Click-IslandElement(
    [Windows.Automation.AutomationElement]$Element,
    [string]$Description,
    [switch]$CenterAfterLayout,
    [scriptblock]$FindElement
) {
    $clickLabel = $Description
    $null = Wait-IslandValue "a verified native click on $clickLabel" {
        try {
            $current = if ($FindElement) { & $FindElement } else { $Element }
            if (-not $current -or -not $current.Current.IsEnabled) { return }
            $point = [Windows.Point]::new(0, 0)
            if ($CenterAfterLayout) {
                $bounds = $current.Current.BoundingRectangle
                if ($bounds.IsEmpty) { return }
                $point = [Windows.Point]::new($bounds.Left + $bounds.Width / 2, $bounds.Top + $bounds.Height / 2)
            } elseif (-not $current.TryGetClickablePoint([ref]$point)) {
                return
            }

            $point = [Windows.Point]::new([int]$point.X, [int]$point.Y)
            if (-not [AgentBroSmokeInput]::SetCursorPos([int]$point.X, [int]$point.Y)) {
                throw "Could not move the Windows cursor onto $clickLabel"
            }
            Start-Sleep -Milliseconds 250
            $target = if ($FindElement) { & $FindElement } else { $Element }
            if (-not $target -or -not $target.Current.IsEnabled -or
                ($CenterAfterLayout -and -not $bounds.Equals($target.Current.BoundingRectangle))) { return }
            $cursor = [AgentBroSmokeInput+Point]::new()
            if (-not [AgentBroSmokeInput]::GetCursorPos([ref]$cursor) -or
                $cursor.X -ne $point.X -or $cursor.Y -ne $point.Y -or
                $AppProcessId -ne [AgentBroSmokeInput]::ProcessAtCursor()) { return }

            $hit = [Windows.Automation.AutomationElement]::FromPoint($point)
            $hitName = $hit.Current.Name
            $walker = [Windows.Automation.TreeWalker]::RawViewWalker
            for ($ancestor = $hit; $ancestor; $ancestor = $walker.GetParent($ancestor)) {
                if (-not $target.Equals($ancestor)) { continue }
                [AgentBroSmokeInput]::Click()
                Write-Host "$clickLabel at $($point.X), $($point.Y). Hit: $hitName. Native owner: AgentBro ($AppProcessId)."
                return $true
            }
            Write-Host "Waiting for $clickLabel at $($point.X), $($point.Y). Current hit: $hitName."
        } catch {
            if ($_.Exception -isnot [Windows.Automation.ElementNotAvailableException] -and
                $_.Exception.InnerException -isnot [Windows.Automation.ElementNotAvailableException]) { throw }
        }
    }
}

function Test-KeyboardPermissionDenial(
    [Windows.Automation.AutomationElement]$Island,
    [IntPtr]$WindowHandle,
    [int]$Attempt
) {
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
            tool_use_id = "windows-smoke-keyboard-denial-$Attempt"
            tool_input = @{ command = 'echo AgentBro Windows permission fixture' }
        }
        $line = ($event | ConvertTo-Json -Compress) + "`n"
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($line)
        $stream.Write($bytes, 0, $bytes.Length)
        $reader = [System.IO.StreamReader]::new($stream)
        $reply = $reader.ReadLineAsync()

        $showSessions = Wait-IslandValue 'the permission card and its session list button' {
            Find-IslandButton $Island {
                param($Name)
                $Name -eq 'Make agents easier to use' -or $Name -match '^Show all \d+ sessions$'
            }
        }
        if ($TracePermissionUi -and $Attempt -eq 1) {
            $cardFrame = [AgentBroSmokeInput+Rect]::new()
            if ([AgentBroSmokeInput]::GetWindowRect($WindowHandle, [ref]$cardFrame)) {
                Write-Output "Permission card HWND size: $($cardFrame.Right - $cardFrame.Left)x$($cardFrame.Bottom - $cardFrame.Top)."
            }
        }
        Click-IslandElement $showSessions 'Permission card session list button' -CenterAfterLayout
        $session = Wait-IslandValue 'the waiting session opener and independent actions' {
            $group = Find-IslandSessionGroup $Island { param($Name) $Name.Contains($ProjectName) }
            $open = Find-IslandSessionOpener $group
            if ($open -and
                (Find-IslandButton $group { param($Name) $Name.StartsWith('Mute notifications') }) -and
                (Find-IslandButton $group { param($Name) $Name -eq 'Deny' })) {
                $open
            }
        }
        if (Find-IslandButton $session { $true }) {
            throw 'The waiting session opener still contains another button in Windows UI Automation'
        }
        Write-Output 'Windows UI Automation exposed the session opener, mute, and inline Deny as independent buttons.'
        $listFrame = [AgentBroSmokeInput+Rect]::new()
        $hasListFrame = $TracePermissionUi -and $Attempt -eq 1 -and
            [AgentBroSmokeInput]::GetWindowRect($WindowHandle, [ref]$listFrame)
        Click-IslandElement $session 'Island session opener with a pending permission' -CenterAfterLayout

        $detailClock = [Diagnostics.Stopwatch]::StartNew()
        try {
            $deny = Wait-IslandValue 'Deny in the session detail' {
                if (Find-IslandButton $Island { param($Name) $Name -eq 'Back to sessions' }) {
                    Find-IslandButton $Island { param($Name) $Name -eq 'Deny' }
                }
            }
        } catch {
            if ($TracePermissionUi) {
                try {
                    Write-IslandAccessibilityBounds $WindowHandle
                    Save-PermissionFrame $detailClock $Attempt 'before-space-timeout'
                } catch {
                    Write-Host "Could not capture permission accessibility bounds: $_"
                }
            }
            throw
        }
        if ($hasListFrame) {
            Write-Output "Waiting session list HWND size: $($listFrame.Right - $listFrame.Left)x$($listFrame.Bottom - $listFrame.Top)."
            Write-IslandAccessibilityBounds $WindowHandle
        }
        if ($TracePermissionUi) {
            Write-Host "Before requesting Deny focus at $([DateTimeOffset]::UtcNow.ToString('O')): $([AgentBroSmokeInput]::DescribeForegroundFocus())."
        }
        $denyHasFocus = {
            if ([AgentBroSmokeInput]::GetForegroundWindow() -ne $WindowHandle) { return $false }
            $focused = [Windows.Automation.AutomationElement]::FocusedElement
            $focused -and $deny.Current.HasKeyboardFocus -and
                [Windows.Automation.Automation]::Compare($focused.GetRuntimeId(), $deny.GetRuntimeId())
        }
        $clock = [Diagnostics.Stopwatch]::new()
        try {
            if ($AutomationFocusPermission) {
                $deny.SetFocus()
            } else {
                $tabs = 0
                while ($tabs -lt 24 -and -not (& $denyHasFocus)) {
                    [AgentBroSmokeInput]::PressKeyInWindow($WindowHandle, 0x09)
                    $tabs += 1
                    Start-Sleep -Milliseconds 125
                }
                Write-Output "Native Tab presses to reach Deny: $tabs."
            }
            $null = Wait-IslandValue 'keyboard focus on Deny in the foreground Island window' $denyHasFocus
        } catch {
            Write-Host "Island window: $WindowHandle. Foreground window: $([AgentBroSmokeInput]::GetForegroundWindow())."
            if ($TracePermissionUi) {
                try {
                    $focused = [Windows.Automation.AutomationElement]::FocusedElement
                    Write-Host ("Focused UIA control: '{0}' ({1}). Deny focusable: {2}. Deny focused: {3}." -f
                        $focused.Current.Name, $focused.Current.ClassName,
                        $deny.Current.IsKeyboardFocusable, $deny.Current.HasKeyboardFocus)
                    Write-IslandAccessibilityBounds $WindowHandle
                    Save-PermissionFrame $clock $Attempt 'focus-timeout'
                } catch {
                    Write-Host "Could not inspect keyboard focus: $_"
                }
            }
            throw
        }
        if ($TracePermissionUi) {
            Save-PermissionFrame $clock $Attempt 'before-space'
            Write-Host "Before sending Space: $([AgentBroSmokeInput]::DescribeForegroundFocus())."
        }
        if ($reply.IsCompleted) { throw 'The permission request ended before the Space key was sent' }
        $clock.Start()
        [AgentBroSmokeInput]::PressKeyInWindow($WindowHandle, 0x20)
        Write-Output 'Sent native Space to the focused Deny button in the session detail.'
        if (-not $reply.Wait(8000)) {
            if ($TracePermissionUi) {
                Write-Host "After the Hook timed out: $([AgentBroSmokeInput]::DescribeForegroundFocus())."
                Save-PermissionFrame $clock $Attempt 'hook-timeout'
            }
            throw 'The Hook did not receive a reply after keyboard denial in the detail view'
        }
        $decision = $reply.GetAwaiter().GetResult() | ConvertFrom-Json
        if ($decision.decision -ne 'deny') { throw "Unexpected Hook permission decision: $($decision.decision)" }
        Write-Output "Original Hook socket confirmed deny at $($clock.ElapsedMilliseconds) ms."
        if ($TracePermissionUi) { Save-PermissionFrame $clock $Attempt 'hook-confirmed' }
        $sampleTimes = @(250, 1000, 3000, 7000, 12000)
        $observation = @{ last = ''; sampleIndex = 0 }
        $null = Wait-IslandValue 'the resolved permission in the detail' {
            if ($TracePermissionUi -and $observation.sampleIndex -lt $sampleTimes.Count -and
                $clock.ElapsedMilliseconds -ge $sampleTimes[$observation.sampleIndex]) {
                Save-PermissionFrame $clock $Attempt 'while-polling'
                do { $observation.sampleIndex += 1 } while ($observation.sampleIndex -lt $sampleTimes.Count -and
                    $clock.ElapsedMilliseconds -ge $sampleTimes[$observation.sampleIndex])
                if ($observation.sampleIndex -eq $sampleTimes.Count) {
                    Write-IslandAccessibilityBounds $WindowHandle
                }
            }
            $hasBack = [bool](Find-IslandButton $Island { param($Name) $Name -eq 'Back to sessions' })
            $hasDeny = [bool](Find-IslandButton $Island { param($Name) $Name -eq 'Deny' })
            if ($TracePermissionUi) {
                $state = "detail=$hasBack deny=$hasDeny"
                if ($state -ne $observation.last) {
                    Write-Host "Windows accessibility at $($clock.ElapsedMilliseconds) ms: $state"
                    $observation.last = $state
                }
            }
            $hasBack -and -not $hasDeny
        }
        if ($TracePermissionUi) {
            $focused = [Windows.Automation.AutomationElement]::FocusedElement
            Write-Output "After approval, native keyboard focus: '$($focused.Current.Name)'."
            if ($Attempt -eq 1) { Write-IslandAccessibilityBounds $WindowHandle }
            Save-PermissionFrame $clock $Attempt 'uia-resolved'
        }
        Write-Output "Windows Island accepted Space on the detail Deny button and the waiting Hook received deny. Detail verified at $($clock.ElapsedMilliseconds) ms."
    } finally {
        $connection.Dispose()
    }
}

function Open-HookFixture([hashtable]$Event) {
    $connection = [System.Net.Sockets.TcpClient]::new()
    try {
        $connection.Connect([System.Net.IPAddress]::Loopback, [int]$env:AGENTBRO_HOOK_PORT)
        $stream = $connection.GetStream()
        $bytes = [System.Text.Encoding]::UTF8.GetBytes(($Event | ConvertTo-Json -Depth 8 -Compress) + "`n")
        $stream.Write($bytes, 0, $bytes.Length)
        $reader = [System.IO.StreamReader]::new($stream)
        [PSCustomObject]@{ Connection = $connection; Reply = $reader.ReadLineAsync() }
    } catch {
        $connection.Dispose()
        throw
    }
}

function Open-GlobalPermissionFixture([string]$Label, [string]$TargetSession) {
    $command = "echo AgentBro global shortcut $Label"
    $fixture = Open-HookFixture @{
        agent = 'claude-code'
        event = 'PermissionRequest'
        session_id = $TargetSession
        cwd = $Workspace
        tool_name = 'Bash'
        tool_use_id = "global-denial-$Label"
        tool_input = @{ command = $command }
    }
    $fixture | Add-Member -NotePropertyName Command -NotePropertyValue $command -PassThru
}

function Test-InstalledGlobalShortcuts([IntPtr]$WindowHandle) {
    if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
        throw 'Global key input requires an isolated GitHub-hosted runner'
    }
    $waitForPrompt = {
        param($Fixture)
        $null = Wait-IslandValue "the visible global permission fixture $($Fixture.Command)" {
            $island = Find-VisibleIsland
            $island -and (Test-PackVisibleText $island.Element $Fixture.Command)
        }
    }
    $readDenial = {
        param($Fixture)
        if (-not $Fixture.Reply.Wait(8000)) { throw "Global keydown did not answer $($Fixture.Command)" }
        $response = $Fixture.Reply.GetAwaiter().GetResult() | ConvertFrom-Json
        if ($response.decision -ne 'deny') { throw "Unexpected global shortcut response: $($response.decision)" }
    }
    $first = Open-GlobalPermissionFixture 'first' "$SessionId-global-first"
    $second = $null
    try {
        & $waitForPrompt $first
        $second = Open-GlobalPermissionFixture 'second' $SessionId
        $showSessions = Wait-IslandValue 'the global permission queue session list button' {
            $island = Find-VisibleIsland
            if ($island) {
                Find-IslandButton $island.Element {
                    param($Name)
                    $Name -eq 'Make agents easier to use' -or $Name -match '^Show all \d+ sessions$'
                }
            }
        }
        Click-IslandElement $showSessions 'Global permission queue session list button' -CenterAfterLayout
        & $waitForPrompt $second
        if ($first.Reply.IsCompleted -or $second.Reply.IsCompleted) {
            throw 'Both permission sockets must be waiting before the shortcut is pressed'
        }
        Write-Output 'Both permission Hooks are waiting. The older session published its request second.'
        try {
            [AgentBroSmokeInput]::SetGlobalShortcutKey(0x78, $true)
            $replied = [System.Threading.Tasks.Task]::WaitAny(
                [System.Threading.Tasks.Task[]]@($first.Reply, $second.Reply), 8000)
            if ($replied -eq 1) { throw 'Global keydown answered the older session instead of the first pending request' }
            if ($replied -lt 0) { throw 'Global keydown did not answer either waiting permission' }
            & $readDenial $first
            if ($second.Reply.Wait(300)) { throw 'Holding the first shortcut replied to the second Hook' }
            Write-Output 'Global Ctrl+Alt+F9 denied the earliest pending Hook on keydown. The older session is still waiting.'
        } finally {
            [AgentBroSmokeInput]::SetGlobalShortcutKey(0x78, $false)
        }
        if ($second.Reply.Wait(700)) {
            throw "Global Ctrl+Alt+F9 key release replied to the second Hook: $($second.Reply.GetAwaiter().GetResult())"
        }
        try {
            [AgentBroSmokeInput]::SetGlobalShortcutKey(0x78, $true)
            & $readDenial $second
        } finally {
            [AgentBroSmokeInput]::SetGlobalShortcutKey(0x78, $false)
        }
        Write-Output 'The second Hook received deny only after a fresh Ctrl+Alt+F9 keydown.'
    } finally {
        if ($second) { $second.Connection.Dispose() }
        $first.Connection.Dispose()
    }

    foreach ($visible in @($false, $true)) {
        try {
            [AgentBroSmokeInput]::SetGlobalShortcutKey(0x77, $true)
            $null = Wait-IslandValue "the native Island visibility changing to $visible on keydown" {
                [AgentBroSmokeInput]::IsWindowVisible($WindowHandle) -eq $visible
            } 5
        } finally {
            [AgentBroSmokeInput]::SetGlobalShortcutKey(0x77, $false)
        }
        Start-Sleep -Milliseconds 350
        if ([AgentBroSmokeInput]::IsWindowVisible($WindowHandle) -ne $visible) {
            throw 'Global Ctrl+Alt+F8 toggled the Island again on key release'
        }
    }
    Write-Output 'Ctrl+Alt+F8 hid and showed the native Island on separate presses. Both releases preserved visibility.'
}

function Test-InstalledGlobalQuestionShortcut([IntPtr]$WindowHandle) {
    $event = @{
        agent = 'claude-code'
        event = 'AskQuestion'
        keep_write_open = $true
        session_id = $SessionId
        cwd = $Workspace
        question = 'Pick a single answer for the global shortcut'
        options = @('Suggested answer', 'Alternative answer')
    }
    $simple = Open-HookFixture $event
    try {
        $null = Wait-IslandValue 'the single-choice question' {
            $island = Find-VisibleIsland
            $island -and (Test-PackVisibleText $island.Element $event.question)
        }
        try { [AgentBroSmokeInput]::SetGlobalShortcutKey(0x79, $true) }
        finally { [AgentBroSmokeInput]::SetGlobalShortcutKey(0x79, $false) }
        if (-not $simple.Reply.Wait(8000)) { throw 'The global shortcut did not answer the single-choice Hook' }
        $response = $simple.Reply.GetAwaiter().GetResult() | ConvertFrom-Json
        if ($response.answer -ne 'Suggested answer') { throw 'The global shortcut chose the wrong single-choice answer' }
        Write-Output 'Ctrl+Alt+F10 returned the first option to the original single-choice Hook.'
    } finally {
        $simple.Connection.Dispose()
    }

    $event.question = 'Pick a region?'
    $event.options = @('Preview region', 'Production region')
    $event.questions = @(
        @{ question = 'Pick a region?'; options = @(@{ label = 'Preview region' }, @{ label = 'Production region' }) },
        @{ question = 'Pick an output?'; options = @(@{ label = 'Summary output' }, @{ label = 'Detailed output' }) }
    )
    $multipart = Open-HookFixture $event
    try {
        $null = Wait-IslandValue 'both parts of the multipart question' {
            $island = Find-VisibleIsland
            $island -and (Test-PackVisibleText $island.Element 'Pick an output?')
        }
        try { [AgentBroSmokeInput]::SetGlobalShortcutKey(0x77, $true) }
        finally { [AgentBroSmokeInput]::SetGlobalShortcutKey(0x77, $false) }
        $null = Wait-IslandValue 'the native Island hidden before the global question shortcut' {
            -not [AgentBroSmokeInput]::IsWindowVisible($WindowHandle)
        }
        $desktop = [Windows.Automation.AutomationElement]::RootElement.Current.BoundingRectangle
        if (-not [AgentBroSmokeInput]::SetCursorPos([int]$desktop.Left + 24, [int]$desktop.Bottom - 48)) {
            throw 'Could not move the pointer away before the global question shortcut'
        }
        try { [AgentBroSmokeInput]::SetGlobalShortcutKey(0x79, $true) }
        finally { [AgentBroSmokeInput]::SetGlobalShortcutKey(0x79, $false) }
        $null = Wait-IslandValue 'the Island reopened with the unresolved multipart question' {
            $island = Find-VisibleIsland
            $island -and (Test-PackVisibleText $island.Element 'Pick an output?')
        }
        Write-Output 'Ctrl+Alt+F10 reopened the multipart question while the pointer was away.'
        if ($multipart.Reply.Wait(1600)) {
            throw "The global shortcut completed the multipart Hook: $($multipart.Reply.GetAwaiter().GetResult())"
        }
        $island = Find-VisibleIsland
        if (-not ($island -and (Test-PackVisibleText $island.Element 'Pick an output?'))) {
            throw 'The focused multipart question collapsed while the pointer was away'
        }
        Write-Output 'The keyboard-opened question remained visible beyond the automatic collapse delay.'

        $null = Wait-IslandValue 'the Island foreground window after the global question shortcut' {
            [AgentBroSmokeInput]::GetForegroundWindow() -eq $WindowHandle
        } 5
        foreach ($target in @(
            @{ Label = 'Production region'; Key = 0x20; KeyName = 'Space' },
            @{ Label = 'Detailed output'; Key = 0x0D; KeyName = 'Enter' },
            @{ Label = 'Submit All'; Key = 0x20; KeyName = 'Space' }
        )) {
            $button = Wait-IslandValue "$($target.Label) button in the Island" {
                $island = Find-VisibleIsland
                if ($island -and $island.Handle -eq $WindowHandle) {
                    Find-IslandButton $island.Element { param($Name) $Name.EndsWith($target.Label) }
                }
            }
            $hasFocus = {
                $focused = [Windows.Automation.AutomationElement]::FocusedElement
                $focused -and $button.Current.HasKeyboardFocus -and
                    [Windows.Automation.Automation]::Compare($focused.GetRuntimeId(), $button.GetRuntimeId())
            }
            for ($tabs = 0; $tabs -lt 24 -and -not (& $hasFocus); $tabs++) {
                [AgentBroSmokeInput]::PressKeyInWindow($WindowHandle, 0x09)
                Start-Sleep -Milliseconds 125
            }
            try {
                $null = Wait-IslandValue "native Tab focus on $($target.Label)" $hasFocus 3
            } catch {
                $focused = [Windows.Automation.AutomationElement]::FocusedElement
                if ($focused) {
                    Write-Host "UI Automation focus: $($focused.Current.ControlType.ProgrammaticName) '$($focused.Current.Name)', PID $($focused.Current.ProcessId)."
                }
                Write-Host "Native focus: $([AgentBroSmokeInput]::DescribeForegroundFocus())."
                throw
            }
            [AgentBroSmokeInput]::PressKeyInWindow($WindowHandle, $target.Key)
            Write-Output "Sent native $($target.KeyName) to the focused $($target.Label) control."
        }
        if (-not $multipart.Reply.Wait(8000)) { throw 'The multipart Hook did not receive the keyboard answer' }
        $response = $multipart.Reply.GetAwaiter().GetResult() | ConvertFrom-Json
        $answers = $response.answer | ConvertFrom-Json
        if ($answers.'Pick a region?' -ne 'Production region' -or $answers.'Pick an output?' -ne 'Detailed output') {
            throw 'The manual reply did not include both selected answers'
        }
        Write-Output 'The original multipart Hook received both answers selected and submitted with the native keyboard.'
    } finally {
        $multipart.Connection.Dispose()
    }
}

function Send-BackgroundToolFixture([string]$ToolUseId) {
    foreach ($event in @('PreToolUse', 'PostToolUse')) {
        $fixture = Open-HookFixture @{
            agent = 'claude-code'
            event = $event
            session_id = $SessionId
            cwd = $Workspace
            tool_use_id = $ToolUseId
            tool_name = 'Read'
            tool_input = @{ file_path = 'smoke.txt' }
        }
        try {
            if (-not $fixture.Reply.Wait(5000)) { throw "The $event Hook did not finish" }
            $null = $fixture.Reply.GetAwaiter().GetResult()
        } finally {
            $fixture.Connection.Dispose()
        }
    }
}

function Test-InstalledQuestionReply(
    [Windows.Automation.AutomationElement]$Island,
    [switch]$Disconnect
) {
    $connection = [System.Net.Sockets.TcpClient]::new()
    $reply = $null
    try {
        $connection.Connect([System.Net.IPAddress]::Loopback, [int]$env:AGENTBRO_HOOK_PORT)
        $stream = $connection.GetStream()
        $answer = 'Reply to Windows test'
        $event = @{
            agent = 'claude-code'
            event = 'AskQuestion'
            keep_write_open = $true
            session_id = $SessionId
            cwd = $Workspace
            question = 'Can the Windows Island return this answer?'
            options = @($answer, 'Skip Windows test')
        }
        $line = ($event | ConvertTo-Json -Compress) + "`n"
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($line)
        $stream.Write($bytes, 0, $bytes.Length)
        $reader = [System.IO.StreamReader]::new($stream)
        $reply = $reader.ReadLineAsync()

        $findOption = {
            $group = Find-IslandSessionGroup $Island { param($Name) $Name.Contains($ProjectName) }
            if ($group) { Find-IslandButton $group { param($Name) $Name.EndsWith($answer) } }
        }
        $view = Wait-IslandValue 'the question card or inline answer' {
            $option = & $findOption
            if ($option) { return @{ Option = $option } }
            $showSessions = Find-IslandButton $Island {
                param($Name)
                $Name -eq 'Make agents easier to use' -or $Name -match '^Show all \d+ sessions$'
            }
            if ($showSessions) { return @{ ShowSessions = $showSessions } }
        }
        if ($view.ShowSessions) {
            Click-IslandElement $view.ShowSessions 'Question card session list button' -CenterAfterLayout
        }
        $null = Wait-IslandValue 'the inline answer in the waiting session' $findOption
        if ($Disconnect) {
            $connection.Dispose()
        } else {
            Send-BackgroundToolFixture 'question-background-read'
            Click-IslandElement -Description 'Inline question answer after background tool activity' -CenterAfterLayout -FindElement $findOption
            if (-not $reply.Wait(8000)) { throw 'The original Hook did not receive an answer from the Windows Island' }
            $response = $reply.GetAwaiter().GetResult() | ConvertFrom-Json
            if ($response.answer -ne $answer) { throw "Unexpected Hook question answer: $($response.answer)" }
        }
        $null = Wait-IslandValue 'the closed question to disappear from the session list' {
            $group = Find-IslandSessionGroup $Island { param($Name) $Name.Contains($ProjectName) }
            $group -and -not (Find-IslandButton $group { param($Name) $Name.EndsWith($answer) })
        }
        if ($Disconnect) {
            Write-Output 'Windows Island removed the AskQuestion card after its Hook disconnected.'
        } else {
            Write-Output 'Windows Island sent the inline question answer to the original Hook socket.'
        }
    } catch {
        Write-Host "Question state before cleanup: disconnect=$Disconnect, pendingRead=$($reply -and -not $reply.IsCompleted)."
        try {
            Save-FailedRunnerDesktop
            $current = Find-VisibleIsland
            if ($current) { Write-IslandAccessibilityBounds $current.Handle $ProjectName }
        } catch {
            Write-Host "Could not capture the pending question desktop: $_"
        }
        if ($env:AGENTBRO_QUESTION_TRACE_PORT) {
            & node (Join-Path $PSScriptRoot 'probe-windows-question-ui.mjs') $env:AGENTBRO_QUESTION_TRACE_PORT $SessionId $answer
        }
        throw
    } finally {
        $connection.Dispose()
    }
}

function Test-InstalledPlanReview(
    [Windows.Automation.AutomationElement]$Island,
    [switch]$Disconnect
) {
    $connection = [System.Net.Sockets.TcpClient]::new()
    try {
        $connection.Connect([System.Net.IPAddress]::Loopback, [int]$env:AGENTBRO_HOOK_PORT)
        $stream = $connection.GetStream()
        $event = @{
            agent = 'claude-code'
            event = 'PlanApproval'
            keep_write_open = $true
            session_id = $SessionId
            cwd = $Workspace
            plan_title = 'Windows Island plan review'
            plan_content = 'Confirm that the Island returns the manual review decision.'
        }
        $line = ($event | ConvertTo-Json -Compress) + "`n"
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($line)
        $stream.Write($bytes, 0, $bytes.Length)
        $reader = [System.IO.StreamReader]::new($stream)
        $reply = $reader.ReadLineAsync()

        $findManualReview = {
            Find-IslandButton $Island { param($Name) $Name -eq 'Manual Review' }
        }
        $null = Wait-IslandValue 'the plan manual review button' $findManualReview
        if ($Disconnect) {
            $connection.Dispose()
        } else {
            Send-BackgroundToolFixture 'plan-background-read'
            Click-IslandElement -Description 'Plan manual review after background tool activity' -CenterAfterLayout -FindElement $findManualReview
            if (-not $reply.Wait(8000)) { throw 'The original Hook did not receive the review decision from the Windows Island' }
            $response = $reply.GetAwaiter().GetResult() | ConvertFrom-Json
            if ($response.mode -ne 'manual') { throw "Unexpected Hook plan decision: $($response.mode)" }
        }
        $null = Wait-IslandValue 'the closed plan to disappear' {
            -not (Find-IslandButton $Island { param($Name) $Name -eq 'Manual Review' })
        }
        if ($Disconnect) {
            Write-Output 'Windows Island removed the PlanApproval card after its Hook disconnected.'
        } else {
            Write-Output 'Windows Island sent the manual review decision to the original Hook socket.'
        }
    } finally {
        $connection.Dispose()
    }
}

function Get-IslandCenterX([IntPtr]$WindowHandle) {
    $bounds = [AgentBroSmokeInput+Rect]::new()
    if (-not [AgentBroSmokeInput]::GetWindowRect($WindowHandle, [ref]$bounds)) {
        throw 'Could not read the native Island bounds'
    }
    ($bounds.Left + $bounds.Right) / 2.0
}

function Test-InstalledIslandDrag(
    [Windows.Automation.AutomationElement]$Island,
    [IntPtr]$WindowHandle,
    [scriptblock]$FindSession
) {
    $dpi = [AgentBroSmokeInput]::GetDpiForWindow($WindowHandle)
    if ($dpi -eq 0) { throw 'Could not read the native Island DPI' }
    $scale = $dpi / 96.0
    foreach ($direction in @(1, -1)) {
        $region = $Island.Current.BoundingRectangle
        $startX = [int]($region.Left + $region.Width / 2)
        $startY = [int]($region.Top + 4 * $scale)
        if (-not [AgentBroSmokeInput]::SetCursorPos($startX, $startY)) {
            throw 'Could not move the cursor onto the Island drag handle'
        }
        Start-Sleep -Milliseconds 150
        if ([AgentBroSmokeInput]::WindowAtCursor() -ne $WindowHandle) {
            throw 'The native Island does not own the drag start point'
        }

        $before = Get-IslandCenterX $WindowHandle
        $releaseClock = [Diagnostics.Stopwatch]::new()
        [AgentBroSmokeInput]::PressLeft()
        try {
            Start-Sleep -Milliseconds 40
            $targetX = [int]($startX + $direction * 14 * $scale)
            if (-not [AgentBroSmokeInput]::SetCursorPos($targetX, $startY)) {
                throw 'Could not move the held pointer across the Island'
            }
            if ($direction -gt 0) {
                $probe = @{ Offset = 42 }
                $null = Wait-IslandValue 'native Island movement with the pointer held' {
                    if ([Math]::Abs((Get-IslandCenterX $WindowHandle) - $before) -ge (12 * $scale)) {
                        return $true
                    }
                    if (-not [AgentBroSmokeInput]::SetCursorPos([int]($startX + $probe.Offset * $scale), $startY)) {
                        throw 'Could not move the cursor while the native drag starts'
                    }
                    $probe.Offset = if ($probe.Offset -eq 42) { 68 } else { 42 }
                    $false
                }
                Write-Output "Native Island moved from $before to $(Get-IslandCenterX $WindowHandle) at $dpi DPI."
            } else {
                Start-Sleep -Milliseconds 20
            }
            $heldCenter = Get-IslandCenterX $WindowHandle
            $releaseClock.Start()
        } finally {
            [AgentBroSmokeInput]::ReleaseLeft()
        }

        $null = Wait-IslandValue 'Windows to report the physical mouse button released' {
            -not [AgentBroSmokeInput]::IsLeftButtonDown()
        }
        Write-Output "Windows confirmed the physical mouse button is released at $([DateTimeOffset]::UtcNow.ToString('O'))."
        $releasePositions = [Collections.Generic.List[string]]::new()
        $null = Wait-IslandValue 'the released Island window to settle' {
            $firstCenter = Get-IslandCenterX $WindowHandle
            $releasePositions.Add("$($releaseClock.ElapsedMilliseconds) ms: $firstCenter")
            Start-Sleep -Milliseconds 200
            $secondCenter = Get-IslandCenterX $WindowHandle
            $releasePositions.Add("$($releaseClock.ElapsedMilliseconds) ms: $secondCenter")
            [Math]::Abs($secondCenter - $firstCenter) -le 1
        }
        $released = Get-IslandCenterX $WindowHandle
        if ($direction -gt 0 -and $released - $before -lt (12 * $scale)) {
            Write-Host "Native Island center immediately before releasing: $heldCenter. After release: $($releasePositions -join ', ')."
            throw "Island returned toward its starting point on release: $before -> $released"
        }
        $cursor = [AgentBroSmokeInput+Point]::new()
        if (-not [AgentBroSmokeInput]::GetCursorPos([ref]$cursor)) {
            throw 'Could not read the cursor position after releasing the Island'
        }
        $probeX = [int]($cursor.X + $direction * 30 * $scale)
        if (-not [AgentBroSmokeInput]::SetCursorPos($probeX, $startY)) {
            throw 'Could not move the cursor after releasing the Island'
        }
        Write-Output "Moved the released pointer to ($probeX, $startY) at $([DateTimeOffset]::UtcNow.ToString('O'))."
        foreach ($sample in 1..4) {
            Start-Sleep -Milliseconds 150
            $current = Get-IslandCenterX $WindowHandle
            if ([Math]::Abs($current - $released) -gt 2) {
                Write-Host "Position changed at $([DateTimeOffset]::UtcNow.ToString('O')) after $($releaseClock.ElapsedMilliseconds) ms. After release: $($releasePositions -join ', ')."
                throw "Island moved after release: $released -> $current (started at $before)"
            }
        }
        Write-Output "Native Island remained at $released after release and further cursor movement."
    }

    $desktop = [Windows.Automation.AutomationElement]::RootElement.Current.BoundingRectangle
    if (-not [AgentBroSmokeInput]::SetCursorPos([int]$desktop.Left + 24, [int]$desktop.Bottom - 48)) {
        throw 'Could not move the cursor away from the Island after dragging'
    }
    $null = Wait-IslandValue 'the Island session list to collapse after dragging' {
        -not (& $FindSession)
    }
    if ([Math]::Abs((Get-IslandCenterX $WindowHandle) - $released) -gt 2) {
        throw "The Island lost its released center $released when the session list collapsed"
    }

    Move-ToIslandHoverPoint $WindowHandle $Island
    $null = Wait-IslandValue 'the Island session list to reopen at the released position' $FindSession
    if ([Math]::Abs((Get-IslandCenterX $WindowHandle) - $released) -gt 2) {
        throw "The Island lost its released center $released when the session list reopened"
    }
    Write-Output "Native Island remained at $released through collapse and reopening of the session list."
}

function Test-InstalledQwenWork([Windows.Automation.AutomationElement]$Island) {
    $root = Join-Path ([Environment]::GetFolderPath('UserProfile')) '.qwenworkcn'
    if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or (Test-Path -LiteralPath $root)) {
        throw 'The QwenWork fixture requires an isolated hosted runner with no existing QwenWork profile'
    }
    $null = New-Item -Path $root -ItemType Directory
    $fixture = Join-Path $PSScriptRoot 'windows-qwenwork-fixture.py'
    try {
        $fixtureOutput = & python $fixture $root seed
        if ($LASTEXITCODE -ne 0) { throw 'Could not create the QwenWork fixture databases' }
        $labels = $fixtureOutput | ConvertFrom-Json
        $null = Wait-IslandValue 'the waiting QwenWork desktop task among completed history' {
            Find-IslandSessionGroup $Island { param($Name) $Name -eq $labels.agent } $labels.waiting
        }
        $null = Wait-IslandValue 'the running QwenWork Flash task with recent messages and stale session metadata' {
            Find-IslandSessionGroup $Island { param($Name) $Name.Contains($labels.flashTitle) } $labels.running
        }
        Write-Output 'The installed Island kept the waiting desktop task and the recently active Flash task visible amid completed history.'

        & python $fixture $root resume
        if ($LASTEXITCODE -ne 0) { throw 'Could not resume the waiting QwenWork fixture' }
        $null = Wait-IslandValue 'the waiting QwenWork task to resume in the installed Island' {
            Find-IslandSessionGroup $Island { param($Name) $Name -eq $labels.agent } $labels.running
        }
        Write-Output 'The installed Island changed the desktop task from waiting to running after the database update.'
    } catch {
        try {
            Save-FailedRunnerDesktop
            Write-Output 'Island controls before removing the QwenWork databases:'
            $Island.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition) |
                Select-Object -First 35 -ExpandProperty Current |
                Select-Object Name, ClassName, IsOffscreen |
                ConvertTo-Json -Depth 3 -Compress
        } catch {
            Write-Output "Could not inspect the QwenWork fixture on the Island: $_"
        }
        throw
    } finally {
        $null = Wait-IslandValue 'the QwenWork fixture files to be released and removed' {
            if (Test-Path -LiteralPath $root) {
                Remove-Item -LiteralPath $root -Recurse -ErrorAction SilentlyContinue
            }
            -not (Test-Path -LiteralPath $root)
        }
    }
    $null = Wait-IslandValue 'the removed QwenWork fixture to leave the Island' {
        -not (Find-IslandSessionGroup $Island { param($Name) $Name.Contains($labels.agent) })
    }
}

function Save-RunnerDesktop([string]$Path) {
    Add-Type -AssemblyName System.Windows.Forms, System.Drawing
    $screen = [Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bitmap = [Drawing.Bitmap]::new($screen.Width, $screen.Height)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($screen.Location, [Drawing.Point]::Empty, $screen.Size)
        $bitmap.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

function Save-PermissionFrame([Diagnostics.Stopwatch]$Clock, [int]$Attempt, [string]$Stage) {
    $name = '{0:D2}-{1:D5}-{2}.png' -f $Attempt, $Clock.ElapsedMilliseconds, $Stage
    Save-RunnerDesktop (Join-Path $permissionTraceDirectory $name)
    Write-Host "Permission desktop frame $name"
}

function Save-FailedRunnerDesktop {
    if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or -not $env:RUNNER_TEMP) { return }
    $path = Join-Path $env:RUNNER_TEMP 'agentbro-island-failure.png'
    if (-not (Test-Path -LiteralPath $path)) { Save-RunnerDesktop $path }
}

$previousDpi = [AgentBroSmokeInput]::SetThreadDpiAwarenessContext([IntPtr](-4))
$originalCursor = [AgentBroSmokeInput+Point]::new()
$cursorSaved = [AgentBroSmokeInput]::GetCursorPos([ref]$originalCursor)
$island = $null
$islandWindow = $null
try {
    if ($CloseStartupSettings) { Close-StartupSettings }
    $islandWindow = Wait-IslandValue 'the visible native Island region' { Find-VisibleIsland }
    if ($VerifyDisplayChange) {
        . "$PSScriptRoot/test-windows-display-change.ps1"
        Test-InstalledIslandDisplayChange $islandWindow.Handle
        $islandWindow = Wait-IslandValue 'the visible native Island region after the display mode change' { Find-VisibleIsland }
    }
    $island = $islandWindow.Element
    Move-ToIslandHoverPoint $islandWindow.Handle $island
    if ($VerifyQwenWork) {
        Test-InstalledQwenWork $island
        $desktop = [Windows.Automation.AutomationElement]::RootElement.Current.BoundingRectangle
        if (-not [AgentBroSmokeInput]::SetCursorPos([int]$desktop.Left + 24, [int]$desktop.Bottom - 48)) {
            throw 'Could not move the cursor away from the Island after the QwenWork fixture'
        }
        Start-Sleep -Milliseconds 250
        $islandWindow = Wait-IslandValue 'the visible Island after the QwenWork fixture' { Find-VisibleIsland }
        $island = $islandWindow.Element
        Move-ToIslandHoverPoint $islandWindow.Handle $island
    }

    $findSession = {
        $group = Find-IslandSessionGroup $island { param($Name) $Name.Contains($ProjectName) } 'smoke.txt'
        Find-IslandSessionOpener $group
    }
    if ($VerifyDrag) {
        try {
            $null = Wait-IslandValue 'the fixture session and its Read target in the hover list' $findSession
        } catch {
            if ($TracePermissionUi) {
                try {
                    Write-IslandAccessibilityBounds $islandWindow.Handle -FixtureProject $ProjectName
                    $freshRoot = Find-VisibleIsland
                    if ($freshRoot -and $freshRoot.Handle -eq $islandWindow.Handle) {
                        $freshGroup = Find-IslandSessionGroup $freshRoot.Element { param($Name) $Name.Contains($ProjectName) } 'smoke.txt'
                        $freshRow = Find-IslandSessionOpener $freshGroup
                        Write-Host "Fresh Island region reports the fixture row: $([bool]$freshRow)."
                    }
                } catch {
                    Write-Host "Could not inspect the fixture hover list accessibility: $_"
                }
            }
            throw
        }
        Test-InstalledIslandDrag $island $islandWindow.Handle $findSession
    }
    Click-IslandElement -Description 'Island session' -FindElement $findSession

    $titleName = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::NameProperty, $ProjectName)
    $null = Wait-IslandValue 'the clicked session in the detail view' {
        $title = $island.FindFirst([Windows.Automation.TreeScope]::Descendants, $titleName)
        $title -and -not $title.Current.IsOffscreen -and
            (Find-IslandButton $island { param($Name) $Name -eq 'Back to sessions' })
    }
    Write-Output 'Windows Island rendered the Claude Code activity and opened its session with native mouse input.'
    $attemptCount = if ($TracePermissionUi) { 6 } else { 1 }
    foreach ($attempt in 1..$attemptCount) {
        Write-Output "Windows approval attempt $attempt of $attemptCount."
        Test-KeyboardPermissionDenial $island $islandWindow.Handle $attempt
    }
    if ($env:AGENTBRO_QUESTION_TRACE_PORT) {
        & node (Join-Path $PSScriptRoot 'probe-windows-question-ui.mjs') $env:AGENTBRO_QUESTION_TRACE_PORT $SessionId 'Reply to Windows test' start
        if ($LASTEXITCODE -ne 0) { throw 'Could not attach the question renderer trace' }
    }
    $questionAttempts = if ($VerifyQuestionReconnect) { 8 } else { 1 }
    for ($attempt = 1; $attempt -le $questionAttempts; $attempt++) {
        Write-Output "Windows question reconnect attempt $attempt of $questionAttempts."
        Test-InstalledQuestionReply $island -Disconnect
        Test-InstalledQuestionReply $island
    }
    Test-InstalledPlanReview $island -Disconnect
    Test-InstalledPlanReview $island
    if ($PackImportPath -or $PinnedPackImportPath -or $PackManifestLink -or $DiagnosticsExportPath -or $VerifyAgentPackageConsole -or $VerifyRegisteredAgentApp -or $VerifyGlobalShortcuts) {
        . (Join-Path $PSScriptRoot 'test-windows-pack-manifest.ps1')
    }
    if ($PackExportPath) {
        Test-InstalledPackManifest $island $PackImportPath $PackExportPath
    }
    if ($PackArchivePath) {
        Test-InstalledPackArchive $island $PackImportPath $PackArchivePath
    }
    if ($PinnedPackImportPath) {
        Test-InstalledPinnedPackManifest $island $PinnedPackImportPath $PinnedPackExportPath
    }
    if ($PackManifestLink) {
        Test-InstalledPackManifestLink $island $PackManifestLink
    }
    if ($DiagnosticsExportPath) {
        . (Join-Path $PSScriptRoot 'test-windows-diagnostics-export.ps1')
        Test-InstalledDiagnosticsExport $island $DiagnosticsExportPath
    }
    if ($VerifyAgentPackageConsole -or $VerifyRegisteredAgentApp) {
        . (Join-Path $PSScriptRoot 'test-windows-agent-package.ps1')
    }
    if ($VerifyAgentPackageConsole) {
        Test-InstalledAgentPackageConsole $island
    }
    if ($VerifyRegisteredAgentApp) {
        Test-InstalledRegisteredAgentApp $island
        Test-InstalledRegisteredAgentApp $island -AgentName 'Qoder Desktop' -ExeName 'Qoder.exe'
        Test-InstalledAgentAfterPathChange $island
    }
    if ($VerifyGlobalShortcuts) {
        Test-InstalledGlobalShortcuts $islandWindow.Handle
        Test-InstalledGlobalQuestionShortcut $islandWindow.Handle
    }
} catch {
    try {
        Save-FailedRunnerDesktop
    } catch {
        Write-Output "Could not capture the runner desktop: $_"
    }
    try {
        if ($TracePermissionUi -and $islandWindow) {
            Write-IslandAccessibilityBounds $islandWindow.Handle
            Write-Host "Focus after failure at $([DateTimeOffset]::UtcNow.ToString('O')): $([AgentBroSmokeInput]::DescribeForegroundFocus())."
            Get-CimInstance Win32_Process -Filter "Name = 'wsl.exe' OR Name = 'wslhost.exe'" |
                ForEach-Object {
                    $parent = Get-Process -Id $_.ParentProcessId -ErrorAction SilentlyContinue
                    Write-Host "$($_.Name) ($($_.ProcessId)) parent: $($parent.ProcessName) ($($_.ParentProcessId))."
                }
        }
        if ($PackImportPath -or $PinnedPackImportPath -or $PackManifestLink -or $DiagnosticsExportPath -or $VerifyAgentPackageConsole -or $VerifyRegisteredAgentApp) {
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
