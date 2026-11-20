param(
    [Parameter(Mandatory)]
    [int]$AppProcessId,
    [Parameter(Mandatory)]
    [string]$ProjectName,
    [string]$WslDistribution,
    [string]$WslAction,
    [switch]$WslJump
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

function Find-NamedControl([Windows.Automation.AutomationElement]$Root, [string]$Name) {
    $condition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::NameProperty, $Name)
    $Root.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
}

function Invoke-NamedControl([Windows.Automation.AutomationElement]$Element) {
    $Element.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke()
}

function Find-WslHookAction([Windows.Automation.AutomationElement]$Window, [string]$Agent, [string]$Action) {
    $controls = $Window.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)
    $inGroup = $false
    for ($i = 0; $i -lt $controls.Count; $i++) {
        $control = $controls[$i]
        if ($control.Current.Name -eq 'WSL Hooks') { $inGroup = $true }
        if (-not $inGroup -or $control.Current.Name -ne $Agent) { continue }
        for ($j = $i + 1; $j -lt [Math]::Min($controls.Count, $i + 10); $j++) {
            $button = $controls[$j]
            if ($button.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and $button.Current.Name -eq $Action -and $button.Current.IsEnabled) {
                return $button
            }
        }
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
    if ($WslAction) {
        $open = Wait-IslandValue 'the Island Settings button' { Find-NamedControl $island 'Settings' }
        Invoke-NamedControl $open
        $settingsWindow = Wait-IslandValue 'the AgentBro settings window' {
            foreach ($window in Get-IslandWindows) {
                if ((Find-NamedControl $window 'Island') -or (Find-NamedControl $window 'Integration') -or
                    (Find-NamedControl $window 'Start AgentBro')) { return $window }
            }
        }
        $start = Find-NamedControl $settingsWindow 'Start AgentBro'
        if ($start) { Invoke-NamedControl $start }
        if (-not (Find-NamedControl $settingsWindow 'Integration')) {
            $nav = Wait-IslandValue 'the Island settings section' { Find-NamedControl $settingsWindow 'Island' }
            Invoke-NamedControl $nav
        }
        $tab = Wait-IslandValue 'the Integration tab' { Find-NamedControl $settingsWindow 'Integration' }
        Invoke-NamedControl $tab
        $distribution = Wait-IslandValue 'the available WSL Distribution selector' {
            $control = Find-NamedControl $settingsWindow 'Distribution'
            if ($control -and $control.Current.ControlType -eq [Windows.Automation.ControlType]::ComboBox -and $control.Current.IsEnabled) {
                return $control
            }
        }
        $distribution.GetCurrentPattern([Windows.Automation.ExpandCollapsePattern]::Pattern).Expand()
        $option = Wait-IslandValue "the $WslDistribution choice" {
            foreach ($root in @($distribution, $settingsWindow, [Windows.Automation.AutomationElement]::RootElement)) {
                $option = Find-NamedControl $root $WslDistribution
                if ($option -and $option.Current.ControlType -eq [Windows.Automation.ControlType]::ListItem) { return $option }
            }
        }
        $option.GetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern).Select()
        $opposite = if ($WslAction -eq 'Install') { 'Uninstall' } else { 'Install' }
        foreach ($agent in @('Claude Code', 'Codex CLI')) {
            $button = Wait-IslandValue "$WslAction in the WSL $agent row" { Find-WslHookAction $settingsWindow $agent $WslAction }
            Invoke-NamedControl $button
            $null = Wait-IslandValue "the WSL $agent configuration to show $opposite" {
                Find-WslHookAction $settingsWindow $agent $opposite
            }
            Write-Output "$WslAction worked for $agent in $WslDistribution."
        }
        Invoke-NamedControl (Find-NamedControl $settingsWindow 'Close')
        return
    }
    if ($WslJump) {
        foreach ($agent in @('Claude', 'Codex')) {
            $null = Wait-IslandValue "the $agent session marked WSL in the native Island" {
                foreach ($button in $island.FindAll([Windows.Automation.TreeScope]::Descendants, $buttonCondition)) {
                    $name = [string]$button.Current.Name
                    if ($name.Contains($ProjectName) -and $name.Contains($agent) -and $name.Contains($WslDistribution)) {
                        return $button
                    }
                }
            }
        }
        $point = Wait-IslandValue 'the clickable WSL session jump button' {
            foreach ($card in $island.FindAll([Windows.Automation.TreeScope]::Descendants, $buttonCondition)) {
                $name = [string]$card.Current.Name
                if (-not $name.Contains($ProjectName) -or -not $name.Contains($WslDistribution)) { continue }
                $jump = Find-NamedControl $card 'Jump to terminal'
                $candidate = [Windows.Point]::new(0, 0)
                if (-not $jump -or -not $jump.TryGetClickablePoint([ref]$candidate)) { continue }
                if (-not [AgentBroSmokeMouse]::SetCursorPos([int]$candidate.X, [int]$candidate.Y)) { continue }
                Start-Sleep -Milliseconds 200
                $hit = [Windows.Automation.AutomationElement]::FromPoint($candidate)
                if ($hit.Current.Name -eq 'Jump to terminal' -and [AgentBroSmokeMouse]::ProcessAtCursor() -eq $AppProcessId) { return $candidate }
            }
        }
        [AgentBroSmokeMouse]::Click()
        Write-Output "Clicked the WSL session jump button at $($point.X), $($point.Y)."
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
