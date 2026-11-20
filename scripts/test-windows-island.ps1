param(
    [Parameter(Mandatory)]
    [int]$DebuggerPort,
    [Parameter(Mandatory)]
    [string]$ProjectName
)

$ErrorActionPreference = 'Stop'

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

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint SendInput(uint count, Input[] inputs, int size);

    public static void Click() {
        var inputs = new[] {
            new Input { Type = InputMouse, Mouse = new MouseInput { Flags = LeftDown } },
            new Input { Type = InputMouse, Mouse = new MouseInput { Flags = LeftUp } }
        };
        if (SendInput(2, inputs, Marshal.SizeOf<Input>()) != 2)
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

function Read-IslandWebView([string]$Expression) {
    $request = @{
        id = 1
        method = 'Runtime.evaluate'
        params = @{ expression = $Expression; awaitPromise = $true; returnByValue = $true }
    } | ConvertTo-Json -Depth 4 -Compress
    $payload = [Text.Encoding]::UTF8.GetBytes($request)
    $timeout = [Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(5))
    $buffer = [byte[]]::new(4096)
    $stream = [IO.MemoryStream]::new()
    try {
        [void]$socket.SendAsync([ArraySegment[byte]]::new($payload), 'Text', $true, $timeout.Token).GetAwaiter().GetResult()
        do {
            $stream.SetLength(0)
            do {
                $frame = $socket.ReceiveAsync([ArraySegment[byte]]::new($buffer), $timeout.Token).GetAwaiter().GetResult()
                if ($frame.MessageType -eq 'Close') { throw 'The Island WebView closed its debugger connection' }
                $stream.Write($buffer, 0, $frame.Count)
            } until ($frame.EndOfMessage)
            $response = [Text.Encoding]::UTF8.GetString($stream.ToArray()) | ConvertFrom-Json
        } until ($response.id -eq 1)
        if ($response.error -or $response.result.exceptionDetails) {
            throw "Island WebView evaluation failed: $($response | ConvertTo-Json -Depth 6 -Compress)"
        }
        return $response.result.result.value
    } finally {
        $stream.Dispose()
        $timeout.Dispose()
    }
}

$target = Wait-IslandValue 'the installed Island WebView target' {
    try {
        $pages = Invoke-RestMethod -Uri "http://127.0.0.1:$DebuggerPort/json/list" -TimeoutSec 2
        $pages | Where-Object {
            $_.type -eq 'page' -and $_.url -match '^https?://tauri\.localhost/(index\.html)?$'
        } | Select-Object -First 1
    } catch {
        return
    }
}
$debugSocket = [Uri]$target.webSocketDebuggerUrl
if (-not $debugSocket.IsLoopback -or $debugSocket.Port -ne $DebuggerPort -or $debugSocket.Scheme -ne 'ws') {
    throw 'The Island debugger WebSocket is outside the test port'
}

$originalCursor = [AgentBroSmokeMouse+Point]::new()
if (-not [AgentBroSmokeMouse]::GetCursorPos([ref]$originalCursor)) {
    throw 'Cannot read the Windows test desktop cursor'
}
$project = ConvertTo-Json -InputObject $ProjectName -Compress
$socket = [Net.WebSockets.ClientWebSocket]::new()
$connectTimeout = [Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(5))
try {
    [void]$socket.ConnectAsync($debugSocket, $connectTimeout.Token).GetAwaiter().GetResult()
    $positionExpression = @'
async (element) => {
    const appWindow = window.__TAURI__?.window?.getCurrentWindow();
    if (!element || !appWindow || !await appWindow.isVisible()) return null;
    const rect = element.getBoundingClientRect();
    if (!rect.width || !rect.height) return null;
    const x = rect.x + rect.width / 2;
    const y = rect.y + rect.height / 2;
    if (!element.contains(document.elementFromPoint(x, y))) return null;
    const origin = await appWindow.innerPosition();
    return {
        x: Math.round(origin.x + x * window.devicePixelRatio),
        y: Math.round(origin.y + y * window.devicePixelRatio)
    };
}
'@
    $islandPoint = Wait-IslandValue 'the visible native Island' {
        Read-IslandWebView "($positionExpression)(document.querySelector('.notch-panel'))"
    }
    if (-not [AgentBroSmokeMouse]::SetCursorPos($islandPoint.x, $islandPoint.y)) {
        throw 'Could not move the Windows cursor into the Island'
    }
    $sessionPoint = Wait-IslandValue 'the fixture session and its Read target in the hover list' {
        Read-IslandWebView @"
        (() => {
            const title = Array.from(document.querySelectorAll('.hover-list__session-title'))
                .find(element => element.textContent.trim() === $project);
            const card = title?.closest('.hover-list__card');
            if (!card || card.closest('[inert]') || !card.innerText.includes('smoke.txt')) return null;
            return ($positionExpression)(title);
        })()
"@
    }
    if (-not [AgentBroSmokeMouse]::SetCursorPos($sessionPoint.x, $sessionPoint.y)) {
        throw 'Could not move the Windows cursor onto the session'
    }
    [AgentBroSmokeMouse]::Click()
    $null = Wait-IslandValue 'the clicked session in the detail view' {
        Read-IslandWebView "document.querySelector('.chat-header__project')?.textContent.trim() === $project"
    }
    Write-Output 'Windows Island rendered the Claude Code activity and opened its session with native mouse input.'
} catch {
    if ($socket.State -eq 'Open') {
        try {
            Read-IslandWebView @'
            ({
                state: document.querySelector('.notch-panel')?.dataset.islandState,
                hidden: document.querySelector('.notch-hitbox')?.dataset.islandHidden,
                title: document.querySelector('.chat-header__project')?.textContent,
                sessions: Array.from(document.querySelectorAll('.hover-list__session-title'), node => node.textContent),
                size: [innerWidth, innerHeight, devicePixelRatio]
            })
'@ | ConvertTo-Json -Compress | Write-Output
        } catch {
            Write-Output "Could not read Island failure state: $_"
        }
    }
    throw
} finally {
    $connectTimeout.Dispose()
    $socket.Dispose()
    [void][AgentBroSmokeMouse]::SetCursorPos($originalCursor.X, $originalCursor.Y)
}
