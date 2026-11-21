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

function Write-IslandPointAccessibility($State, [IntPtr]$WindowHandle) {
    if ($State.zoomed) {
        Write-Host 'Skipping UI Automation hit tests for a panned or scaled visual viewport.'
        return
    }
    $scale = [double]$State.devicePixelRatio
    $width = [double]$State.viewport[0] * $scale
    $height = [double]$State.viewport[1] * $scale
    $clients = @(foreach ($handle in @($WindowHandle) + @([AgentBroSmokeInput]::WebContentWindows($WindowHandle))) {
        $rect = [AgentBroSmokeInput+Rect]::new()
        if ([AgentBroSmokeInput]::GetClientScreenRect($handle, [ref]$rect) -and
            [Math]::Abs(($rect.Right - $rect.Left) - $width) -le 2 -and
            [Math]::Abs(($rect.Bottom - $rect.Top) - $height) -le 2) {
            $rect
        }
    })
    $origins = @($clients | ForEach-Object { "$($_.Left),$($_.Top)" } | Sort-Object -Unique)
    if ($origins.Count -ne 1) {
        Write-Host "Could not map renderer viewport ${width}x$height to a unique native client origin."
        return
    }
    $rect = $clients[0]
    foreach ($control in $State.controls) {
        if (-not $control.atCenter) { continue }
        $x = [int][Math]::Round($rect.Left + $control.center[0] * $scale)
        $y = [int][Math]::Round($rect.Top + $control.center[1] * $scale)
        $owner = [AgentBroSmokeInput]::WindowAtScreenPoint($x, $y)
        $ancestors = [Collections.Generic.List[object]]::new()
        if ($owner -eq $WindowHandle) {
            try {
                $point = [Windows.Point]::new($x, $y)
                $element = [Windows.Automation.AutomationElement]::FromPoint($point)
                $walker = [Windows.Automation.TreeWalker]::RawViewWalker
                for ($depth = 0; $element -and $depth -lt 10; $depth += 1) {
                    $current = $element.Current
                    $ancestors.Add([PSCustomObject]@{
                        name = $current.Name
                        role = $current.ControlType.ProgrammaticName
                        offscreen = $current.IsOffscreen
                    })
                    if ($current.NativeWindowHandle -eq $WindowHandle.ToInt64()) { break }
                    $element = $walker.GetParent($element)
                }
            } catch {
                Write-Host "Could not query UI Automation at ($x, $y): $_"
            }
        }
        $hit = [PSCustomObject]@{
            target = $control.label
            point = @($x, $y)
            owned = $owner -eq $WindowHandle
            ancestors = $ancestors.ToArray()
        }
        Write-Host "Island UI Automation hit at $([DateTimeOffset]::UtcNow.ToString('O')): $($hit | ConvertTo-Json -Depth 4 -Compress)"
    }
}

function Write-IslandAccessibilityAfterProbe([IntPtr]$WindowHandle, [string]$Probe) {
    try {
        $snapshot = Get-IslandAccessibilitySnapshot $WindowHandle.ToInt64() $ProjectName
        Write-Host "Island UI Automation after ${Probe}: $($snapshot | ConvertTo-Json -Depth 4 -Compress)"
    } catch {
        Write-Host "Could not query Windows accessibility after ${Probe}: $_"
    }
}

function Write-IslandRendererState([int]$Port, [IntPtr]$WindowHandle) {
    $expression = @'
(() => {
    const panel = document.querySelector('.notch-panel');
    if (!panel) return null;
    const bounds = ({ x, y, width, height }) => [x, y, width, height].map(Math.round);
    const markers = /^(Back to sessions|Show all \d+ sessions|Deny|Allow Once|Make agents easier to use)$|smoke\.txt/;
    const controls = Array.from(panel.querySelectorAll('button, [role="button"]')).flatMap(button => {
        const label = (button.getAttribute('aria-label') || button.textContent).trim().replace(/\s+/g, ' ');
        if (!markers.test(label)) return [];
        const rect = button.getBoundingClientRect();
        const center = [rect.x + rect.width / 2, rect.y + rect.height / 2];
        return [{
            label, center, bounds: bounds(rect),
            hidden: Boolean(button.closest('[inert], [aria-hidden="true"]')),
            atCenter: rect.width > 0 && rect.height > 0 && button.contains(document.elementFromPoint(...center)),
        }];
    });
    const visual = window.visualViewport;
    return {
        panel: panel.dataset.islandState, bounds: bounds(panel.getBoundingClientRect()), controls,
        viewport: [window.innerWidth, window.innerHeight], devicePixelRatio: window.devicePixelRatio,
        zoomed: Boolean(visual && (visual.scale !== 1 || visual.offsetLeft !== 0 || visual.offsetTop !== 0)),
    };
})()
'@
    $targets = Invoke-RestMethod "http://127.0.0.1:$Port/json/list" -UseBasicParsing -TimeoutSec 3
    $targetNumber = 0
    foreach ($target in $targets) {
        if ($target.type -ne 'page' -or -not $target.webSocketDebuggerUrl) { continue }
        $endpoint = [uri]$target.webSocketDebuggerUrl
        if (-not $endpoint.IsLoopback -or $endpoint.Port -ne $Port) { continue }
        $targetNumber += 1
        $socket = [Net.WebSockets.ClientWebSocket]::new()
        $timeout = [Threading.CancellationTokenSource]::new(12000)
        $stage = 'connect'
        $state = $null
        try {
            $null = $socket.ConnectAsync($endpoint, $timeout.Token).GetAwaiter().GetResult()
            $stage = 'read DOM'
            $evaluated = Invoke-IslandRendererRequest $socket $timeout.Token 'Runtime.evaluate' @{
                expression = $expression
                returnByValue = $true
            }
            $state = $evaluated.result.value
            if (-not $state) { continue }
            Write-Host "Island renderer DOM at $([DateTimeOffset]::UtcNow.ToString('O')): $($state | ConvertTo-Json -Depth 5 -Compress)"

            # Direct hit-testing and the Chromium request can each refresh provider state.
            $stage = 'hit-test Windows accessibility'
            Write-IslandPointAccessibility $state $WindowHandle
            Write-IslandAccessibilityAfterProbe $WindowHandle 'hit testing'

            $stage = 'read accessibility'
            $timeout.Dispose()
            $timeout = [Threading.CancellationTokenSource]::new(12000)
            $tree = Invoke-IslandRendererRequest $socket $timeout.Token 'Accessibility.getFullAXTree' @{}
            $buttons = @($tree.nodes | Where-Object {
                $_.role.value -eq 'button' -and
                    $_.name.value -match '^(Back to sessions|Show all \d+ sessions|Deny|Allow Once|Make agents easier to use)$|smoke\.txt'
            } | ForEach-Object { @{ label = $_.name.value; ignored = $_.ignored } })
            Write-Host "Island renderer accessibility at $([DateTimeOffset]::UtcNow.ToString('O')): $(ConvertTo-Json -InputObject $buttons -Compress)"
            Write-IslandAccessibilityAfterProbe $WindowHandle 'Chromium accessibility'
            return
        } catch {
            Write-Host "Could not $stage for WebView2 page $($targetNumber): $_"
            if ($state) { return }
        } finally {
            $timeout.Dispose()
            $socket.Dispose()
        }
    }
    throw 'The debugger did not find the Island renderer'
}
