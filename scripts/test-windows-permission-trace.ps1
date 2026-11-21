$permissionStateExpression = @'
(() => {
    const island = document.querySelector('.notch-panel[aria-label="AgentBro"]');
    const detail = island?.querySelector('.notch-panel__detail-content');
    const deny = detail?.querySelector('button.approval-bar__btn--deny');
    const composer = detail?.querySelector('input.approval-bar__input');
    const visible = element => Boolean(element?.getClientRects().length);
    return {
        island: visible(island),
        detail: visible(detail?.querySelector('.chat-header__back')),
        deny: visible(deny),
        composer: visible(composer),
        denyFocused: Boolean(deny && document.activeElement === deny),
        visibility: document.visibilityState,
    };
})()
'@

function Read-IslandPermissionDom([System.Net.WebSockets.ClientWebSocket]$Socket) {
    $deadline = [System.Threading.CancellationTokenSource]::new(2500)
    try {
        $request = @{
            id = 1
            method = 'Runtime.evaluate'
            params = @{ expression = $permissionStateExpression; returnByValue = $true }
        } | ConvertTo-Json -Depth 4 -Compress
        $bytes = [Text.Encoding]::UTF8.GetBytes($request)
        $null = $Socket.SendAsync([ArraySegment[byte]]::new($bytes),
            [System.Net.WebSockets.WebSocketMessageType]::Text, $true, $deadline.Token).GetAwaiter().GetResult()
        $buffer = [ArraySegment[byte]]::new([byte[]]::new(4096))
        do {
            $message = [IO.MemoryStream]::new()
            try {
                do {
                    $frame = $Socket.ReceiveAsync($buffer, $deadline.Token).GetAwaiter().GetResult()
                    if ($frame.MessageType -ne [System.Net.WebSockets.WebSocketMessageType]::Text) {
                        throw 'The WebView2 permission trace stopped returning text responses'
                    }
                    $message.Write($buffer.Array, 0, $frame.Count)
                } until ($frame.EndOfMessage)
                $response = [Text.Encoding]::UTF8.GetString($message.ToArray()) | ConvertFrom-Json
            } finally {
                $message.Dispose()
            }
        } until ($response.id -eq 1)
        if ($response.error -or $response.result.exceptionDetails -or -not $response.result.result.value) {
            throw 'The WebView2 permission state could not be evaluated'
        }
        return $response.result.result.value
    } finally {
        $deadline.Dispose()
    }
}

function Open-IslandPermissionTrace([int]$Port) {
    $targets = Invoke-RestMethod -Uri "http://127.0.0.1:$Port/json/list" -TimeoutSec 3 -UseBasicParsing
    foreach ($target in $targets) {
        if ($target.type -ne 'page' -or -not $target.webSocketDebuggerUrl) { continue }
        $address = [Uri]$target.webSocketDebuggerUrl
        if (-not $address.IsLoopback -or $address.Scheme -ne 'ws' -or $address.Port -ne $Port) { continue }
        $socket = [System.Net.WebSockets.ClientWebSocket]::new()
        $deadline = [System.Threading.CancellationTokenSource]::new(2500)
        $selected = $false
        try {
            $null = $socket.ConnectAsync($address, $deadline.Token).GetAwaiter().GetResult()
            $selected = (Read-IslandPermissionDom $socket).island
            if ($selected) { return $socket }
        } finally {
            $deadline.Dispose()
            if (-not $selected) { $socket.Dispose() }
        }
    }
    throw 'The loopback WebView2 endpoint did not contain a visible Island page'
}
