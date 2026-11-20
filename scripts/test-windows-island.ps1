param(
    [Parameter(Mandatory)]
    [int]$AppProcessId,
    [Parameter(Mandatory)]
    [string]$ProjectName,
    [string]$WorkBuddyMode,
    [string]$WorkBuddyEditionName,
    [string]$WorkBuddySettingsPath,
    [string]$WorkBuddyRoot
)

$ErrorActionPreference = 'Stop'
if ($WorkBuddyMode) { [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false) }

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

    [DllImport("user32.dll")]
    private static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    private static extern bool ShowWindowAsync(IntPtr window, int command);

    public static uint ForegroundProcess() {
        uint processId;
        GetWindowThreadProcessId(GetForegroundWindow(), out processId);
        return processId;
    }

    public static void Minimize(IntPtr window) {
        ShowWindowAsync(window, 6);
    }

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

function Find-NamedControl([Windows.Automation.AutomationElement]$Root, [string]$Name) {
    $condition = [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty, $Name)
    $Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
}

function Invoke-NamedControl([Windows.Automation.AutomationElement]$Element) {
    $Element.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke()
}

function Find-VisibleWorkBuddyCard {
    foreach ($card in $island.FindAll([Windows.Automation.TreeScope]::Descendants, $buttonCondition)) {
        $state = $card.Current
        if (-not $state.IsOffscreen -and ([string]$state.Name).Contains($ProjectName) -and
            (Find-NamedControl $card 'Jump to terminal')) {
            return $card
        }
    }
}

function Get-WorkBuddyWindow {
    Get-Process -ErrorAction SilentlyContinue | Where-Object {
        $_.MainWindowHandle -ne 0 -and $_.Path -and $_.Path.StartsWith("$WorkBuddyRoot\", [StringComparison]::OrdinalIgnoreCase)
    } | Select-Object -First 1
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

$previousDpi = [AgentBroSmokeMouse]::SetThreadDpiAwarenessContext([IntPtr](-4))
$originalCursor = [AgentBroSmokeMouse+Point]::new()
$cursorSaved = [AgentBroSmokeMouse]::GetCursorPos([ref]$originalCursor)
$island = $null
$settingsWindow = $null
try {
    $island = Wait-IslandValue 'the visible native Island region' { Find-VisibleIsland }
    Move-ToElement $island

    $buttonCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::Button)

    if ($WorkBuddyMode -eq 'Detect') {
        $open = Wait-IslandValue 'the Island Settings button' { Find-NamedControl $island 'Settings' }
        Invoke-NamedControl $open
        $settingsWindow = Wait-IslandValue 'the AgentBro settings window' {
            foreach ($window in Get-IslandWindows) {
                if ((Find-NamedControl $window 'Agent Skills') -or (Find-NamedControl $window 'Back to Settings') -or
                    (Find-NamedControl $window 'Start AgentBro')) { return $window }
            }
        }
        $start = Find-NamedControl $settingsWindow 'Start AgentBro'
        if ($start) { Invoke-NamedControl $start }
        $back = Find-NamedControl $settingsWindow 'Back to Settings'
        if ($back) { Invoke-NamedControl $back }
        $skills = Wait-IslandValue 'the Agent Skills section' { Find-NamedControl $settingsWindow 'Agent Skills' }
        Invoke-NamedControl $skills
        $manage = Wait-IslandValue 'the Agent Management tab' {
            Find-NamedControl $settingsWindow ('Agent ' + [char]0x7ba1 + [char]0x7406)
        }
        Invoke-NamedControl $manage
        $null = Wait-IslandValue 'WorkBuddy listed among installed Agents' {
            foreach ($button in $settingsWindow.FindAll([Windows.Automation.TreeScope]::Descendants, $buttonCondition)) {
                if ($button.Current.Name -eq $WorkBuddyEditionName) { return $button }
            }
        }
        $otherEdition = if ($WorkBuddyEditionName -eq 'WorkBuddy AI') { 'WorkBuddy' } else { 'WorkBuddy AI' }
        foreach ($button in $settingsWindow.FindAll([Windows.Automation.TreeScope]::Descendants, $buttonCondition)) {
            if ($button.Current.Name -eq $otherEdition -and -not $button.Current.IsOffscreen) {
                throw "Agent Management also listed the uninstalled $otherEdition edition"
            }
        }
        Write-Output "Agent Management recognized the official $WorkBuddyEditionName installation before configuring its hooks."
        Invoke-NamedControl (Find-NamedControl $settingsWindow 'Close')
        return
    }

    if ($WorkBuddyMode -in @('Install', 'Uninstall')) {
        $open = Wait-IslandValue 'the Settings button' { Find-NamedControl $island 'Settings' }
        Invoke-NamedControl $open
        $settingsWindow = Wait-IslandValue 'the AgentBro settings window' {
            foreach ($window in Get-IslandWindows) {
                if ((Find-NamedControl $window 'Island') -or (Find-NamedControl $window 'Integration') -or
                    (Find-NamedControl $window 'Start AgentBro')) { return $window }
            }
        }
        $start = Find-NamedControl $settingsWindow 'Start AgentBro'
        if ($start) { Invoke-NamedControl $start }
        $back = Find-NamedControl $settingsWindow 'Back to Settings'
        if ($back) { Invoke-NamedControl $back }
        if (-not (Find-NamedControl $settingsWindow 'Integration')) {
            $nav = Wait-IslandValue 'the Island settings section' { Find-NamedControl $settingsWindow 'Island' }
            Invoke-NamedControl $nav
        }
        $tab = Wait-IslandValue 'the Integration tab' { Find-NamedControl $settingsWindow 'Integration' }
        Invoke-NamedControl $tab
        $profileName = Split-Path -Leaf (Split-Path -Parent $WorkBuddySettingsPath)
        $actionButton = Wait-IslandValue "the $WorkBuddyEditionName '$WorkBuddyMode' button" {
            $nodes = $settingsWindow.FindAll([Windows.Automation.TreeScope]::Descendants,
                [Windows.Automation.Condition]::TrueCondition)
            for ($i = 0; $i -lt $nodes.Count - 2; $i++) {
                if ($nodes[$i].Current.Name -ne $WorkBuddyEditionName -or
                    $nodes[$i + 1].Current.Name -notlike "*$profileName*settings.json") { continue }
                for ($j = $i + 2; $j -lt [Math]::Min($i + 8, $nodes.Count); $j++) {
                    $target = $nodes[$j]
                    if ($target.Current.Name -eq $WorkBuddyMode -and
                        $target.Current.ControlType -eq [Windows.Automation.ControlType]::Button) { return $target }
                }
            }
        }
        Invoke-NamedControl $actionButton
        $null = Wait-IslandValue "the $WorkBuddyMode action to update $profileName/settings.json" {
            try {
                $settings = Get-Content -LiteralPath $WorkBuddySettingsPath -Raw | ConvertFrom-Json
                $managed = $settings.hooks | ConvertTo-Json -Depth 20 -Compress
                ($managed -match 'agentbro-bridge') -eq ($WorkBuddyMode -eq 'Install')
            } catch {
                return
            }
        }
        Write-Output "AgentBro $WorkBuddyMode updated only the expected WorkBuddy Hook profile."
        Invoke-NamedControl (Find-NamedControl $settingsWindow 'Close')
        return
    }
    if ($WorkBuddyMode) {
        $null = Wait-IslandValue "the visible WorkBuddy phase '$ProjectName'" {
            if ($WorkBuddyMode -eq 'Completed') {
                $completion = Find-NamedControl $island $ProjectName
                if ($completion -and -not $completion.Current.IsOffscreen) { return $completion }
            }
            Find-VisibleWorkBuddyCard
        }
        $phaseLabel = if ($WorkBuddyMode -eq 'Jump') { 'Waiting for confirmation' } else { $WorkBuddyMode }
        Write-Output "Windows Island showed WorkBuddy task phase: $phaseLabel"
        if ($WorkBuddyMode -notin @('Jump', 'OpenFromSession')) { return }
        if ((Find-NamedControl $island 'Deny') -or (Find-NamedControl $island 'Allow Once')) {
            throw 'WorkBuddy showed an inline approval action'
        }
        $qwen = Wait-IslandValue 'the running WorkBuddy window before the jump' { Get-WorkBuddyWindow }
        [AgentBroSmokeMouse]::Minimize($qwen.MainWindowHandle)
        $null = Wait-IslandValue 'WorkBuddy to lose foreground focus' {
            [AgentBroSmokeMouse]::ForegroundProcess() -ne $qwen.Id
        }
        $point = Wait-IslandValue 'a native click target for Jump to terminal' {
            $card = Find-VisibleWorkBuddyCard
            if (-not $card) { return }
            $jump = Find-NamedControl $card 'Jump to terminal'
            $candidate = [Windows.Point]::new(0, 0)
            if (-not $jump -or $jump.Current.IsOffscreen -or -not $jump.TryGetClickablePoint([ref]$candidate)) { return }
            if (-not [AgentBroSmokeMouse]::SetCursorPos([int]$candidate.X, [int]$candidate.Y)) { return }
            Start-Sleep -Milliseconds 250
            $hit = [Windows.Automation.AutomationElement]::FromPoint($candidate)
            if ($hit.Current.Name -eq 'Jump to terminal' -and [AgentBroSmokeMouse]::ProcessAtCursor() -eq $AppProcessId) { return $candidate }
        }
        Write-Output "Native cursor hit the Island Jump button at $($point.X), $($point.Y)."
        [AgentBroSmokeMouse]::Click()
        $null = Wait-IslandValue 'the official WorkBuddy window to regain foreground focus' {
            $foreground = Get-Process -Id ([AgentBroSmokeMouse]::ForegroundProcess()) -ErrorAction SilentlyContinue
            $foreground.Path -and $foreground.Path.StartsWith("$WorkBuddyRoot\", [StringComparison]::OrdinalIgnoreCase)
        }
        Write-Output 'The native Island action brought the official WorkBuddy desktop window to the foreground.'
        return
    }
    $session = Wait-IslandValue 'the fixture session and its Read target in the hover list' {
        foreach ($button in $island.FindAll([Windows.Automation.TreeScope]::Descendants, $buttonCondition)) {
            $state = $button.Current
            $name = [string]$state.Name
            if (-not $state.IsOffscreen -and $name.Contains($ProjectName) -and $name.Contains('smoke.txt')) {
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
    $owner = Get-Process -Id $nativeOwner -ErrorAction SilentlyContinue
    Write-Output "Island session at $($point.X), $($point.Y). Hit: $($underCursor.Current.Name). Native owner: $($owner.ProcessName) ($nativeOwner)."
    if ($nativeOwner -ne $AppProcessId) {
        throw "A different window owns the click point: $($owner.ProcessName) / $($owner.MainWindowTitle)"
    }
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
        Save-FailedRunnerDesktop
    } catch {
        Write-Output "Could not capture the runner desktop: $_"
    }
    try {
        if ($settingsWindow) {
            $settingsWindow.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition) |
                Select-Object -First 125 -ExpandProperty Current |
                Select-Object LocalizedControlType, Name, IsOffscreen | ConvertTo-Json -Depth 3 -Compress
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
        [void][AgentBroSmokeMouse]::SetCursorPos($originalCursor.X, $originalCursor.Y)
    }
    if ($previousDpi -ne [IntPtr]::Zero) {
        [void][AgentBroSmokeMouse]::SetThreadDpiAwarenessContext($previousDpi)
    }
}
