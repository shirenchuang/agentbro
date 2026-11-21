param(
    [Parameter(Mandatory)]
    [int]$AppProcessId,
    [Parameter(Mandatory)]
    [string]$ProjectName,
    [string]$LockProjectPath
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
    private const uint Wheel = 0x0800;

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

    public static void ScrollDown() {
        SendMouseButton(Wheel, unchecked((uint)-240));
    }

    private static void SendMouseButton(uint flags, uint data = 0) {
        var inputs = new[] { new Input { Type = InputMouse, Mouse = new MouseInput { Flags = flags, Data = data } } };
        if (SendInput(1, inputs, Marshal.SizeOf<Input>()) != 1)
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

function Save-FailedRunnerDesktop([string]$FileName = 'agentbro-island-failure.png') {
    if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or -not $env:RUNNER_TEMP) { return }
    Add-Type -AssemblyName System.Windows.Forms, System.Drawing
    $screen = [Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bitmap = [Drawing.Bitmap]::new($screen.Width, $screen.Height)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($screen.Location, [Drawing.Point]::Empty, $screen.Size)
        $bitmap.Save((Join-Path $env:RUNNER_TEMP $FileName), [Drawing.Imaging.ImageFormat]::Png)
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

$previousDpi = [AgentBroSmokeMouse]::SetThreadDpiAwarenessContext([IntPtr](-4))
$originalCursor = [AgentBroSmokeMouse+Point]::new()
$cursorSaved = [AgentBroSmokeMouse]::GetCursorPos([ref]$originalCursor)
$island = $null
try {
    $island = Wait-IslandValue 'the visible native Island region' { Find-VisibleIsland }
    Move-ToElement $island

    if ($LockProjectPath) {
        $settings = $null
        $open = Wait-IslandValue 'the Island Settings button' { Find-NamedControl $island 'Settings' }
        Invoke-NamedControl $open
        $settings = Wait-IslandValue 'the native Settings window' {
            foreach ($window in Get-IslandWindows) {
                if ((Find-NamedControl $window 'Agent Skills') -or (Find-NamedControl $window 'Start AgentBro')) {
                    return $window
                }
            }
        }
        $welcome = Find-NamedControl $settings 'Start AgentBro'
        if ($welcome) { Invoke-NamedControl $welcome }
        $agentSkills = Wait-IslandValue 'the Agent Skills section' { Find-NamedControl $settings 'Agent Skills' }
        Invoke-NamedControl $agentSkills
        $projects = Wait-IslandValue 'the Projects navigation item' { Find-NamedControl $settings 'Projects' }
        Invoke-NamedControl $projects
        $null = [AgentBroSmokeMouse]::SetCursorPos(30, 400)
        Write-Output 'Opened Projects from the native Settings sidebar.'

        $label = [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::Edit)
        $pathBox = Wait-IslandValue 'the project folder field' {
            foreach ($element in $settings.FindAll([Windows.Automation.TreeScope]::Descendants, $label)) {
                $state = $element.Current
                if ($state.AutomationId -eq 'skill-project-root' -or
                    $state.Name -like '*导入项目*' -or $state.Name -eq '/Users/me/work/repo') { return $element }
            }
        }
        $filled = $false
        try {
            $pathBox.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).SetValue($LockProjectPath)
            Start-Sleep -Milliseconds 300
            $add = Find-NamedControl $settings '添加'
            $filled = $add -and $add.Current.IsEnabled
        } catch {}
        if (-not $filled) {
            Add-Type -AssemblyName System.Windows.Forms
            Set-Clipboard -Value $LockProjectPath
            $pathBox.SetFocus()
            [Windows.Forms.SendKeys]::SendWait('^a^v')
        }
        $add = Wait-IslandValue 'the enabled Add Project button' {
            $button = Find-NamedControl $settings '添加'
            if ($button -and $button.Current.IsEnabled) { return $button }
        }
        Invoke-NamedControl $add
        $null = Wait-IslandValue 'the lock file preview' { Find-NamedControl $settings 'native-restored' }
        if (-not (Find-NamedControl $settings 'native-existing')) { throw 'Existing Skill is absent from the preview' }
        if (-not (Find-NamedControl $settings 'native-mismatch')) { throw 'Second missing Skill is absent from the preview' }
        $null = Wait-IslandValue 'the Windows local computer name' {
            Find-NamedControl $settings 'Current runtime environment: This PC'
        }
        $windowBounds = $settings.Current.BoundingRectangle
        foreach ($label in @('打开', '重新扫描', '移除')) {
            $button = Wait-IslandValue "the visible project action $label" {
                $element = Find-NamedControl $settings $label
                if ($element -and -not $element.Current.IsOffscreen) { return $element }
            }
            $bounds = $button.Current.BoundingRectangle
            if ($bounds.IsEmpty -or $bounds.Left -lt $windowBounds.Left -or $bounds.Right -gt $windowBounds.Right) {
                throw "Project action $label is clipped by the Settings window"
            }
        }
        Save-FailedRunnerDesktop 'agentbro-projects-overview.png'
        Write-Output "Project actions fit in the Windows Settings window ($([int]$windowBounds.Width)px); local environment displays This PC."
        $select = Wait-IslandValue 'the Select all restorable checkbox' {
            Find-NamedControl $settings 'Select all restorable'
        }
        $select.GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern).Toggle()
        $restore = Wait-IslandValue 'the restore selection button for two Skills' {
            $button = Find-NamedControl $settings 'Restore selected (2)'
            if ($button -and $button.Current.IsEnabled) { return $button }
        }
        Invoke-NamedControl $restore
        $finished = Wait-IslandValue 'the completed restore and single retryable Skill' {
            Find-NamedControl $settings 'Restore selected (1)'
        } 65
        $buttonBounds = $finished.Current.BoundingRectangle
        if (-not [AgentBroSmokeMouse]::SetCursorPos([int]($buttonBounds.X + $buttonBounds.Width / 2),
                [int]($buttonBounds.Y + $buttonBounds.Height / 2))) {
            throw 'Could not move the mouse to the project lock panel'
        }
        [AgentBroSmokeMouse]::ScrollDown()
        $lockPanel = Wait-IslandValue 'the lock panel after scrolling' { Find-NamedControl $settings 'skills-lock.json' }
        $items = [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::ListItem)
        $null = Wait-IslandValue 'the restored and checksum mismatch rows in the native panel' {
            $visibleNames = @($lockPanel.FindAll([Windows.Automation.TreeScope]::Descendants, $items) |
                Where-Object { -not $_.Current.IsOffscreen } |
                ForEach-Object { $_.Current.Name })
            if (($visibleNames -like 'native-restored*Restored') -and
                ($visibleNames -like 'native-mismatch*Downloaded Skill content does not match the lock file')) {
                return $true
            }
        }
        Save-FailedRunnerDesktop 'agentbro-projects-restoration.png'
        Write-Output 'The native Projects UI restored the selected Skill and reported a separate hash mismatch.'
        return
    }

    $buttonCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::Button)
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
        if ($settings) {
            $panel = Find-NamedControl $settings 'skills-lock.json'
            if ($panel) {
                $panel.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition) |
                    Select-Object -First 90 -ExpandProperty Current |
                    Select-Object LocalizedControlType, Name, IsOffscreen |
                    ConvertTo-Json -Depth 3 -Compress
            }
            $settings.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition) |
                Where-Object { $_.Current.LocalizedControlType -in @('edit', 'button', 'check box') } |
                Select-Object -First 65 -ExpandProperty Current |
                Select-Object LocalizedControlType, Name, AutomationId, IsEnabled, IsOffscreen |
                ConvertTo-Json -Depth 3 -Compress
        }
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
