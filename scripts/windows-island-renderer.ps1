function Invoke-IslandRendererRequest(
    [Net.WebSockets.ClientWebSocket]$Socket,
    [Threading.CancellationToken]$Cancellation,
    [string]$Method,
    [hashtable]$Parameters
) {
    $request = @{ id = 1; method = $Method; params = $Parameters } | ConvertTo-Json -Depth 5 -Compress
    $send = [ArraySegment[byte]]::new([Text.Encoding]::UTF8.GetBytes($request))
    $buffer = [ArraySegment[byte]]::new([byte[]]::new(8192))
    $body = [IO.MemoryStream]::new()
    try {
        $null = $Socket.SendAsync($send, [Net.WebSockets.WebSocketMessageType]::Text, $true, $Cancellation).GetAwaiter().GetResult()
        do {
            $body.SetLength(0)
            do {
                $frame = $Socket.ReceiveAsync($buffer, $Cancellation).GetAwaiter().GetResult()
                if ($frame.MessageType -eq [Net.WebSockets.WebSocketMessageType]::Close) {
                    throw 'The WebView2 debugger disconnected'
                }
                $body.Write($buffer.Array, 0, $frame.Count)
            } until ($frame.EndOfMessage)
            $response = [Text.Encoding]::UTF8.GetString($body.ToArray()) | ConvertFrom-Json
        } until ($response.id -eq 1)
        if ($response.error) { throw "$Method failed: $($response.error.message)" }
        $response.result
    } finally {
        $body.Dispose()
    }
}

function Write-IslandRendererState([int]$Port) {
    $expression = @'
(() => {
    const panel = document.querySelector('.notch-panel');
    if (!panel) return null;
    const bounds = ({ x, y, width, height }) => [x, y, width, height].map(Math.round);
    const markers = /^(Back to sessions|Show all \d+ sessions|Deny|Allow Once|Make agents easier to use)$|smoke\.txt/;
    const controls = Array.from(panel.querySelectorAll('button, [role="button"]')).flatMap(button => {
        const label = (button.getAttribute('aria-label') || button.textContent).trim().replace(/\s+/g, ' ');
        return markers.test(label) ? [{
            label,
            bounds: bounds(button.getBoundingClientRect()),
            hidden: Boolean(button.closest('[inert], [aria-hidden="true"]')),
        }] : [];
    });
    return { panel: panel.dataset.islandState, bounds: bounds(panel.getBoundingClientRect()), controls };
})()
'@
    $targets = Invoke-RestMethod "http://127.0.0.1:$Port/json/list" -UseBasicParsing -TimeoutSec 3
    foreach ($target in $targets) {
        if ($target.type -ne 'page' -or -not $target.webSocketDebuggerUrl) { continue }
        $endpoint = [uri]$target.webSocketDebuggerUrl
        if (-not $endpoint.IsLoopback -or $endpoint.Port -ne $Port) { continue }
        $socket = [Net.WebSockets.ClientWebSocket]::new()
        $timeout = [Threading.CancellationTokenSource]::new(5000)
        try {
            $null = $socket.ConnectAsync($endpoint, $timeout.Token).GetAwaiter().GetResult()
            $evaluated = Invoke-IslandRendererRequest $socket $timeout.Token 'Runtime.evaluate' @{
                expression = $expression
                returnByValue = $true
            }
            $state = $evaluated.result.value
            if (-not $state) { continue }
            Write-Host "Island renderer DOM at $([DateTimeOffset]::UtcNow.ToString('O')): $($state | ConvertTo-Json -Depth 5 -Compress)"

            # Querying Chromium's accessibility tree can refresh it. Collect the DOM and Windows samples first.
            $tree = Invoke-IslandRendererRequest $socket $timeout.Token 'Accessibility.getFullAXTree' @{}
            $buttons = @($tree.nodes | Where-Object {
                $_.role.value -eq 'button' -and
                    $_.name.value -match '^(Back to sessions|Show all \d+ sessions|Deny|Allow Once|Make agents easier to use)$|smoke\.txt'
            } | ForEach-Object { @{ label = $_.name.value; ignored = $_.ignored } })
            Write-Host "Island renderer accessibility at $([DateTimeOffset]::UtcNow.ToString('O')): $(ConvertTo-Json -InputObject $buttons -Compress)"
            return
        } finally {
            $timeout.Dispose()
            $socket.Dispose()
        }
    }
    throw 'The debugger did not find the Island renderer'
}
