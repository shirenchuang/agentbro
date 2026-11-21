function Write-IslandRendererState([int]$Port, [string]$SessionId) {
    $session = ConvertTo-Json -InputObject $SessionId -Compress
    $expression = @"
(async () => {
    const panel = document.querySelector('.notch-panel');
    if (!panel) return null;
    const deny = panel.querySelector('.approval-bar__btn--deny');
    const element = document.activeElement;
    const state = {
        panel: panel.dataset.islandState,
        viewport: [window.innerWidth, window.innerHeight],
        focused: document.hasFocus(),
        activeElement: element?.tagName,
        activeClass: typeof element?.className === 'string' ? element.className : null,
        deny: deny ? { top: Math.round(deny.getBoundingClientRect().top), focused: deny === element } : null,
        showSessions: Boolean(panel.querySelector('button.overlay-card__show-sessions')),
    };
    state.backend = await Promise.race([
        new Promise(resolve => setTimeout(() => resolve('timeout'), 650)),
        Promise.resolve().then(() => window.__TAURI__.core.invoke('get_sessions'))
            .then(sessions => {
                const fixture = sessions.find(item => item.id === $session);
                return fixture ? { phase: fixture.phase, pending: Boolean(fixture.pendingPermission) } : 'absent';
            }).catch(() => 'unavailable'),
    ]);
    return state;
})()
"@
    $request = @{
        id = 1
        method = 'Runtime.evaluate'
        params = @{ expression = $expression; awaitPromise = $true; returnByValue = $true }
    } | ConvertTo-Json -Depth 5 -Compress
    $bytes = [Text.Encoding]::UTF8.GetBytes($request)
    $send = [ArraySegment[byte]]::new($bytes)
    $buffer = [ArraySegment[byte]]::new([byte[]]::new(8192))
    $targets = Invoke-RestMethod "http://127.0.0.1:$Port/json/list" -UseBasicParsing -TimeoutSec 3
    foreach ($target in $targets) {
        $endpoint = [uri]$target.webSocketDebuggerUrl
        if ($target.type -ne 'page' -or -not $endpoint.IsLoopback -or $endpoint.Port -ne $Port) { continue }
        $socket = [Net.WebSockets.ClientWebSocket]::new()
        $timeout = [Threading.CancellationTokenSource]::new(3000)
        $body = [IO.MemoryStream]::new()
        try {
            $null = $socket.ConnectAsync($endpoint, $timeout.Token).GetAwaiter().GetResult()
            $null = $socket.SendAsync($send, [Net.WebSockets.WebSocketMessageType]::Text, $true, $timeout.Token).GetAwaiter().GetResult()
            do {
                $body.SetLength(0)
                do {
                    $frame = $socket.ReceiveAsync($buffer, $timeout.Token).GetAwaiter().GetResult()
                    if ($frame.MessageType -eq [Net.WebSockets.WebSocketMessageType]::Close) {
                        throw 'The WebView2 debugger disconnected'
                    }
                    $body.Write($buffer.Array, 0, $frame.Count)
                } until ($frame.EndOfMessage)
                $response = [Text.Encoding]::UTF8.GetString($body.ToArray()) | ConvertFrom-Json
            } until ($response.id -eq 1)
            $state = $response.result.result.value
            if ($state) {
                Write-Output "Island renderer state: $($state | ConvertTo-Json -Depth 5 -Compress)"
                return
            }
        } finally {
            $body.Dispose()
            $timeout.Dispose()
            $socket.Dispose()
        }
    }
    Write-Output 'The debugger did not find the Island renderer.'
}
