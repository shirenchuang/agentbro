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
    [switch]$TracePermissionUi,
    [int]$RendererTracePort,
    [switch]$VerifyQwenWork,
    [switch]$VerifyDrag,
    [switch]$CloseStartupSettings,
    [switch]$AutomationFocusPermission,
    [switch]$VerifyAgentPackageConsole
)

$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
if ($TracePermissionUi) {
    if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or -not $env:RUNNER_TEMP) {
        throw 'Permission screenshots require an isolated GitHub-hosted runner'
    }
    $permissionTraceDirectory = Join-Path $env:RUNNER_TEMP 'agentbro-permission-trace'
    $null = New-Item -ItemType Directory -Path $permissionTraceDirectory -Force
    if ($RendererTracePort) { . (Join-Path $PSScriptRoot 'windows-island-renderer.ps1') }
}

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
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
    private static extern bool GetClientRect(IntPtr window, out Rect rect);

    [DllImport("user32.dll")]
    private static extern bool ClientToScreen(IntPtr window, ref Point point);

    public static bool GetClientScreenRect(IntPtr window, out Rect rect) {
        var origin = new Point();
        if (!GetClientRect(window, out rect) || !ClientToScreen(window, ref origin)) return false;
        rect.Left += origin.X;
        rect.Right += origin.X;
        rect.Top += origin.Y;
        rect.Bottom += origin.Y;
        return true;
    }

    [DllImport("user32.dll")]
    private static extern IntPtr WindowFromPoint(Point point);

    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool GetGUIThreadInfo(uint thread, ref GuiThreadInfo info);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetClassName(IntPtr window, StringBuilder name, int capacity);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetWindowText(IntPtr window, StringBuilder title, int capacity);

    private delegate bool EnumChildProc(IntPtr window, IntPtr data);

    [DllImport("user32.dll")]
    private static extern bool EnumChildWindows(IntPtr window, EnumChildProc callback, IntPtr data);

    public static IntPtr[] WebContentWindows(IntPtr window) {
        var handles = new List<IntPtr>();
        EnumChildWindows(window, (child, data) => {
            var name = new StringBuilder(128);
            GetClassName(child, name, name.Capacity);
            string className = name.ToString();
            if (className == "Chrome_RenderWidgetHostHWND" || className == "Chrome_WidgetWin_1") {
                handles.Add(child);
            }
            return true;
        }, IntPtr.Zero);
        return handles.ToArray();
    }

    [DllImport("user32.dll")]
    private static extern IntPtr GetAncestor(IntPtr window, uint flags);

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);

    public static IntPtr WindowAtCursor() {
        Point point;
        if (!GetCursorPos(out point)) return IntPtr.Zero;
        return WindowAtScreenPoint(point.X, point.Y);
    }

    public static IntPtr WindowAtScreenPoint(int x, int y) {
        return GetAncestor(WindowFromPoint(new Point { X = x, Y = y }), 2);
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
        var info = new GuiThreadInfo { Size = (uint)Marshal.SizeOf<GuiThreadInfo>() };
        if (!GetGUIThreadInfo(0, ref info))
            return string.Format("GetGUIThreadInfo failed: {0}", Marshal.GetLastWin32Error());
        var foreground = GetForegroundWindow();
        var title = new StringBuilder(256);
        GetWindowText(foreground, title, title.Capacity);
        uint processId;
        GetWindowThreadProcessId(foreground, out processId);
        string processName = "unknown";
        try {
            using (var process = System.Diagnostics.Process.GetProcessById((int)processId)) {
                processName = process.ProcessName;
            }
        } catch (ArgumentException) { }
          catch (InvalidOperationException) { }
          catch (System.ComponentModel.Win32Exception) { }
        return string.Format("foreground {0}, title '{1}', process {2} ({3}), active {4}, focus {5}, capture {6}",
            DescribeWindow(foreground), title, processName, processId, DescribeWindow(info.Active),
            DescribeWindow(info.Focus), DescribeWindow(info.Capture));
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

    public static bool PressKeyInWindow(IntPtr window, ushort key) {
        if (window == IntPtr.Zero || GetForegroundWindow() != window) return false;
        InjectInputs(new[] { KeyboardEvent(key, 0, 0), KeyboardEvent(key, 0, KeyUp) });
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

function Get-IslandAccessibilitySnapshot([long]$WindowHandle, [string]$FixtureProject) {
    $ErrorActionPreference = 'Stop'
    Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
    $started = [DateTimeOffset]::UtcNow.ToString('O')
    $window = [Windows.Automation.AutomationElement]::FromHandle([IntPtr]$WindowHandle)
    $elements = @($window.FindAll(
        [Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition
    ) | Select-Object -ExpandProperty Current |
        Where-Object {
            $_.ClassName -in @('WRY_WEBVIEW', 'Chrome_WidgetWin_1', 'BrowserRootView', 'notch-panel', 'hover-list__card') -or
            $_.ControlType -eq [Windows.Automation.ControlType]::Document -or
            $_.Name -in @('Back to sessions', 'Deny', 'Allow Once') -or
            $_.Name -match '^Show all \d+ sessions$' -or
            ($FixtureProject -and $_.Name -and $_.Name.Contains($FixtureProject))
        } | Select-Object Name, ClassName, IsOffscreen, HasKeyboardFocus, @{
            Name = 'Bounds'
            Expression = { $_.BoundingRectangle.ToString([Globalization.CultureInfo]::InvariantCulture) }
        })
    [PSCustomObject]@{
        ClientPid = $PID
        WindowHandle = $WindowHandle
        RootClass = $window.Current.ClassName
        StartedAt = $started
        FinishedAt = [DateTimeOffset]::UtcNow.ToString('O')
        Elements = $elements
    }
}

function Write-IslandAccessibilityBounds([IntPtr]$WindowHandle) {
    $native = [AgentBroSmokeInput+Rect]::new()
    if ([AgentBroSmokeInput]::GetWindowRect($WindowHandle, [ref]$native)) {
        $dpi = [AgentBroSmokeInput]::GetDpiForWindow($WindowHandle)
        Write-Host "Native window $WindowHandle bounds: $($native.Left),$($native.Top),$($native.Right),$($native.Bottom) at $dpi DPI."
    }
    Get-IslandAccessibilitySnapshot $WindowHandle.ToInt64() $ProjectName | ConvertTo-Json -Depth 4 -Compress | Write-Host
}

function Write-IslandAccessibilityComparison([IntPtr]$WindowHandle, [string]$Stage) {
    $job = $null
    try {
        $job = Start-Job -ScriptBlock ${function:Get-IslandAccessibilitySnapshot} -ArgumentList $WindowHandle.ToInt64(), $ProjectName
        Write-Host "Comparing accessibility clients for $Stage at $([DateTimeOffset]::UtcNow.ToString('O'))."
        Save-RunnerDesktop (Join-Path $permissionTraceDirectory "$Stage-before-clients.png")
        try {
            Write-IslandAccessibilityBounds $WindowHandle
        } catch {
            Write-Host "The smoke test's accessibility client failed: $_"
        }
        if (-not (Wait-Job -Job $job -Timeout 8)) {
            throw 'The independent accessibility client timed out'
        }
        $snapshot = Receive-Job -Job $job
        if (-not $snapshot -or $snapshot.ClientPid -eq $PID) {
            throw 'The independent accessibility client did not return a distinct process snapshot'
        }
        Write-Host "Independent accessibility client: $($snapshot | ConvertTo-Json -Depth 4 -Compress)"
        Write-Host "Foreground during child accessibility queries: $([AgentBroSmokeInput]::DescribeForegroundFocus())."
        $children = [AgentBroSmokeInput]::WebContentWindows($WindowHandle)
        if (-not $children) { Write-Host "No Chromium child window was found under Island HWND $WindowHandle." }
        foreach ($child in $children) {
            try {
                $direct = Get-IslandAccessibilitySnapshot $child.ToInt64() $ProjectName
                Write-Host "Direct child accessibility client: $($direct | ConvertTo-Json -Depth 4 -Compress)"
            } catch {
                Write-Host "Could not inspect Chromium child HWND ${child}: $_"
            }
        }
        Save-RunnerDesktop (Join-Path $permissionTraceDirectory "$Stage-after-clients.png")
        Write-Host "Finished comparing accessibility clients at $([DateTimeOffset]::UtcNow.ToString('O'))."
        if ($RendererTracePort) {
            $endpoint = if ($rendererDiscovery -and $rendererDiscovery.State -eq 'Completed') {
                Receive-Job -Job $rendererDiscovery -Keep
            }
            Write-IslandRendererState $RendererTracePort $WindowHandle $endpoint
        }
    } catch {
        Write-Host "Could not compare accessibility clients: $_"
    } finally {
        if ($job) { Remove-Job -Job $job -Force }
    }
}

function Click-IslandElement(
    [Windows.Automation.AutomationElement]$Element,
    [string]$Description,
    [switch]$CenterAfterLayout,
    [scriptblock]$FindElement
) {
    $point = if ($CenterAfterLayout) {
        $rect = Wait-IslandValue "stable bounds for $Description" {
            $before = $Element.Current.BoundingRectangle
            Start-Sleep -Milliseconds 200
            $after = $Element.Current.BoundingRectangle
            if (-not $after.IsEmpty -and $before.Equals($after)) { $after }
        }
        Write-Host "$Description bounds: $($rect.ToString([Globalization.CultureInfo]::InvariantCulture))."
        [Windows.Point]::new($rect.Left + $rect.Width / 2, $rect.Top + $rect.Height / 2)
    } else {
        Wait-IslandValue "a clickable point on $Description" {
            try {
                $current = if ($FindElement) { & $FindElement } else { $Element }
                $candidate = [Windows.Point]::new(0, 0)
                if ($current -and $current.TryGetClickablePoint([ref]$candidate)) { $candidate }
            } catch {
                if (-not $FindElement -or ($_.Exception -isnot [Windows.Automation.ElementNotAvailableException] -and
                    $_.Exception.InnerException -isnot [Windows.Automation.ElementNotAvailableException])) { throw }
            }
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
    if ($TracePermissionUi) {
        Write-Host "Native click for $Description at $([DateTimeOffset]::UtcNow.ToString('O'))."
    }
    [AgentBroSmokeInput]::Click()
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
                    if (-not [AgentBroSmokeInput]::PressKeyInWindow($WindowHandle, 0x09)) {
                        throw 'The Island lost foreground focus during native Tab navigation'
                    }
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
            Write-Host "Before sending Space at $([DateTimeOffset]::UtcNow.ToString('O')): $([AgentBroSmokeInput]::DescribeForegroundFocus())."
        }
        if ($reply.IsCompleted) { throw 'The permission request ended before the Space key was sent' }
        $clock.Start()
        if (-not [AgentBroSmokeInput]::PressKeyInWindow($WindowHandle, 0x20)) {
            throw "Foreground changed before Space could be sent to the Island window $WindowHandle."
        }
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
    } catch {
        if ($TracePermissionUi) {
            Write-IslandAccessibilityComparison $WindowHandle "approval-$Attempt-failure"
        }
        throw
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
        $null = Wait-IslandValue 'both live QwenWork tasks among completed history' {
            $waiting = Find-IslandSessionGroup $Island { param($Name) $Name -eq $labels.agent } $labels.waiting
            $flash = Find-IslandSessionGroup $Island { param($Name) $Name.Contains($labels.flashTitle) } $labels.running
            $waiting -and $flash
        }
        Write-Output 'The installed Island rendered the waiting desktop task and running Flash task amid 70 newer completed tasks in each database.'

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
$rendererDiscovery = $null
$testingPermissions = $false
try {
    if ($CloseStartupSettings) { Close-StartupSettings }
    $islandWindow = Wait-IslandValue 'the visible native Island region' { Find-VisibleIsland }
    $island = $islandWindow.Element
    if ($RendererTracePort) {
        try {
            # Select by Tauri's window label without enabling Chromium accessibility ahead of the test.
            $rendererDiscovery = Start-Job -ArgumentList (Join-Path $PSScriptRoot 'windows-island-renderer.ps1'), $RendererTracePort -ScriptBlock {
                param($ProbeScript, $Port)
                $ErrorActionPreference = 'Stop'
                . $ProbeScript
                Find-IslandRendererEndpoint $Port
            }
        } catch {
            Write-Host "Could not start renderer discovery: $_"
        }
    }
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
                    Write-IslandAccessibilityBounds $islandWindow.Handle
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
    $testingPermissions = $true
    foreach ($attempt in 1..$attemptCount) {
        Write-Output "Windows approval attempt $attempt of $attemptCount."
        Test-KeyboardPermissionDenial $island $islandWindow.Handle $attempt
    }
    $testingPermissions = $false
    if ($TracePermissionUi) {
        Write-IslandAccessibilityComparison $islandWindow.Handle 'after-denials'
    }
    if ($PackImportPath -or $VerifyAgentPackageConsole) {
        . (Join-Path $PSScriptRoot 'test-windows-pack-manifest.ps1')
    }
    if ($PackImportPath) {
        Test-InstalledPackManifest $island $PackImportPath $PackExportPath
    }
    if ($VerifyAgentPackageConsole) {
        . (Join-Path $PSScriptRoot 'test-windows-agent-package.ps1')
        Test-InstalledAgentPackageConsole $island
    }
} catch {
    try {
        Save-FailedRunnerDesktop
    } catch {
        Write-Output "Could not capture the runner desktop: $_"
    }
    try {
        if ($TracePermissionUi -and $islandWindow) {
            Write-Host "Focus after failure at $([DateTimeOffset]::UtcNow.ToString('O')): $([AgentBroSmokeInput]::DescribeForegroundFocus())."
            if (-not $testingPermissions) {
                Write-IslandAccessibilityComparison $islandWindow.Handle 'island-failure'
            }
            Get-CimInstance Win32_Process -Filter "Name = 'wsl.exe' OR Name = 'wslhost.exe'" |
                ForEach-Object {
                    $parent = Get-Process -Id $_.ParentProcessId -ErrorAction SilentlyContinue
                    Write-Host "$($_.Name) ($($_.ProcessId)) parent: $($parent.ProcessName) ($($_.ParentProcessId))."
                }
        }
        if ($PackImportPath -or $VerifyAgentPackageConsole) {
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
    if ($rendererDiscovery) { Remove-Job -Job $rendererDiscovery -Force }
    if ($cursorSaved) {
        [void][AgentBroSmokeInput]::SetCursorPos($originalCursor.X, $originalCursor.Y)
    }
    if ($previousDpi -ne [IntPtr]::Zero) {
        [void][AgentBroSmokeInput]::SetThreadDpiAwarenessContext($previousDpi)
    }
}
