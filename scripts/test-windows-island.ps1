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
    [switch]$VerifyDrag
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

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint SendInput(uint count, Input[] inputs, int size);

    public static void Click() {
        SendMouseButton(LeftDown);
        System.Threading.Thread.Sleep(60);
        SendMouseButton(LeftUp);
    }

    public static void PressLeft() { SendMouseButton(LeftDown); }
    public static void ReleaseLeft() { SendMouseButton(LeftUp); }

    public static bool PressSpaceInWindow(IntPtr window) {
        if (window == IntPtr.Zero || GetForegroundWindow() != window) return false;
        const ushort Space = 0x20;
        InjectInputs(new[] { KeyboardEvent(Space, 0, 0), KeyboardEvent(Space, 0, KeyUp) });
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

function Write-PermissionAccessibilityBounds {
    param([IntPtr]$WindowHandle)

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
                $_.ClassName -in @('WRY_WEBVIEW', 'Chrome_WidgetWin_1', 'BrowserRootView', 'notch-panel') -or
                $_.ControlType -eq [Windows.Automation.ControlType]::Document -or
                $_.Name -in @('Back to sessions', 'Deny', 'Allow Once')
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
    [Windows.Automation.AutomationElement]$RowAnchor,
    [switch]$CenterAfterLayout
) {
    $point = if ($RowAnchor) {
        $row = $Element.Current.BoundingRectangle
        $anchor = $RowAnchor.Current.BoundingRectangle
        [Windows.Point]::new($row.Left + $row.Width / 2, $anchor.Top + $anchor.Height / 2)
    } elseif ($CenterAfterLayout) {
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
        Click-IslandElement $showSessions 'Permission card session list button' -CenterAfterLayout
        $session = Wait-IslandValue 'the waiting session in the Island list' {
            Find-IslandButton $Island { param($Name) $Name.Contains($ProjectName) }
        }
        $rowAnchor = Wait-IslandValue 'the waiting session header' {
            Find-IslandButton $session { param($Name) $Name.StartsWith('Mute notifications') }
        }
        $listFrame = [AgentBroSmokeInput+Rect]::new()
        $hasListFrame = $TracePermissionUi -and $Attempt -eq 1 -and
            [AgentBroSmokeInput]::GetWindowRect($WindowHandle, [ref]$listFrame)
        # The center of a waiting row belongs to the inline permission preview.
        Click-IslandElement $session 'Island session header with a pending permission' $rowAnchor

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
                    Write-PermissionAccessibilityBounds $WindowHandle
                    Save-PermissionFrame $detailClock $Attempt 'before-space-timeout'
                } catch {
                    Write-Host "Could not capture permission accessibility bounds: $_"
                }
            }
            throw
        }
        if ($hasListFrame) {
            Write-Output "Waiting session list HWND height: $($listFrame.Bottom - $listFrame.Top)."
            Write-PermissionAccessibilityBounds $WindowHandle
        }
        $deny.SetFocus()
        $clock = [Diagnostics.Stopwatch]::new()
        if ($TracePermissionUi) { Save-PermissionFrame $clock $Attempt 'before-space' }
        try {
            $null = Wait-IslandValue 'keyboard focus on Deny in the foreground Island window' {
                if ([AgentBroSmokeInput]::GetForegroundWindow() -ne $WindowHandle) { return $false }
                $focused = [Windows.Automation.AutomationElement]::FocusedElement
                $focused -and $deny.Current.HasKeyboardFocus -and
                    [Windows.Automation.Automation]::Compare($focused.GetRuntimeId(), $deny.GetRuntimeId())
            }
        } catch {
            Write-Host "Island window: $WindowHandle. Foreground window: $([AgentBroSmokeInput]::GetForegroundWindow())."
            if ($TracePermissionUi) {
                try {
                    $focused = [Windows.Automation.AutomationElement]::FocusedElement
                    Write-Host ("Focused UIA control: '{0}' ({1}). Deny focusable: {2}. Deny focused: {3}." -f
                        $focused.Current.Name, $focused.Current.ClassName,
                        $deny.Current.IsKeyboardFocusable, $deny.Current.HasKeyboardFocus)
                    Write-PermissionAccessibilityBounds $WindowHandle
                    Save-PermissionFrame $clock $Attempt 'focus-timeout'
                } catch {
                    Write-Host "Could not inspect keyboard focus: $_"
                }
            }
            throw
        }
        if ($reply.IsCompleted) { throw 'The permission request ended before the Space key was sent' }
        $clock.Start()
        if (-not [AgentBroSmokeInput]::PressSpaceInWindow($WindowHandle)) {
            throw "Foreground changed before Space could be sent to the Island window $WindowHandle."
        }
        Write-Output 'Sent native Space to the focused Deny button in the session detail.'
        if (-not $reply.Wait(8000)) {
            Write-Host "Island window: $WindowHandle. Foreground window: $([AgentBroSmokeInput]::GetForegroundWindow())."
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
                    Write-PermissionAccessibilityBounds $WindowHandle
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
            if ($Attempt -eq 1) { Write-PermissionAccessibilityBounds $WindowHandle }
            Save-PermissionFrame $clock $Attempt 'uia-resolved'
        }
        Write-Output "Windows Island accepted Space on the detail Deny button and the waiting Hook received deny. Detail verified at $($clock.ElapsedMilliseconds) ms."
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
    [IntPtr]$WindowHandle
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
        } finally {
            [AgentBroSmokeInput]::ReleaseLeft()
        }

        $null = Wait-IslandValue 'Windows to report the physical mouse button released' {
            -not [AgentBroSmokeInput]::IsLeftButtonDown()
        }
        Write-Output 'Windows confirmed the physical mouse button is released.'
        $null = Wait-IslandValue 'the released Island window to settle' {
            $firstCenter = Get-IslandCenterX $WindowHandle
            Start-Sleep -Milliseconds 200
            [Math]::Abs((Get-IslandCenterX $WindowHandle) - $firstCenter) -le 1
        }
        $released = Get-IslandCenterX $WindowHandle
        $cursor = [AgentBroSmokeInput+Point]::new()
        if (-not [AgentBroSmokeInput]::GetCursorPos([ref]$cursor)) {
            throw 'Could not read the cursor position after releasing the Island'
        }
        $probeX = [int]($cursor.X + $direction * 30 * $scale)
        if (-not [AgentBroSmokeInput]::SetCursorPos($probeX, $startY)) {
            throw 'Could not move the cursor after releasing the Island'
        }
        foreach ($sample in 1..4) {
            Start-Sleep -Milliseconds 150
            $current = Get-IslandCenterX $WindowHandle
            if ([Math]::Abs($current - $released) -gt 2) {
                throw "Island followed the cursor after release: $released -> $current"
            }
        }
        Write-Output "Native Island remained at $released after release and further cursor movement."
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
    Save-RunnerDesktop (Join-Path $env:RUNNER_TEMP 'agentbro-island-failure.png')
}

$previousDpi = [AgentBroSmokeInput]::SetThreadDpiAwarenessContext([IntPtr](-4))
$originalCursor = [AgentBroSmokeInput+Point]::new()
$cursorSaved = [AgentBroSmokeInput]::GetCursorPos([ref]$originalCursor)
$island = $null
try {
    $islandWindow = Wait-IslandValue 'the visible native Island region' { Find-VisibleIsland }
    $island = $islandWindow.Element
    Move-ToElement $island

    $session = Wait-IslandValue 'the fixture session and its Read target in the hover list' {
        Find-IslandButton $island { param($Name) $Name.Contains($ProjectName) -and $Name.Contains('smoke.txt') }
    }
    if ($VerifyDrag) {
        Test-InstalledIslandDrag $island $islandWindow.Handle
        $session = Wait-IslandValue 'the fixture session after dragging the Island' {
            Find-IslandButton $island { param($Name) $Name.Contains($ProjectName) -and $Name.Contains('smoke.txt') }
        }
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
    $attemptCount = if ($TracePermissionUi) { 6 } else { 1 }
    foreach ($attempt in 1..$attemptCount) {
        Write-Output "Windows approval attempt $attempt of $attemptCount."
        Test-KeyboardPermissionDenial $island $islandWindow.Handle $attempt
    }
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
