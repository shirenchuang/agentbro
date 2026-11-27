const [port, sessionId, answer] = process.argv.slice(2)
if (process.env.RUNNER_ENVIRONMENT !== 'github-hosted' || !/^\d+$/.test(port) || !sessionId || !answer) {
  throw new Error('The question trace requires an isolated GitHub-hosted runner and a fixture session')
}

const origin = `http://127.0.0.1:${port}`
const pages = await fetch(`${origin}/json/list`, { signal: AbortSignal.timeout(2_500) }).then(response => {
  if (!response.ok) throw new Error(`WebView2 target lookup returned ${response.status}`)
  return response.json()
})

const expression = `(${async (id, expectedAnswer) => {
  const island = document.querySelector('[role="region"][aria-label="AgentBro"]')
  if (!island) return null
  const snapshot = await window.__TAURI__.core.invoke('get_session_snapshot')
  const session = snapshot.sessions.find(candidate => candidate.id === id)
  const visible = element => Boolean(element?.getClientRects().length && getComputedStyle(element).visibility !== 'hidden')
  const answerButtons = [...island.querySelectorAll('button')].filter(button => button.textContent.trim().endsWith(expectedAnswer))
  return {
    revision: snapshot.revision,
    backendPhase: session?.phase ?? null,
    backendQuestion: Boolean(session?.pendingQuestion),
    expanded: island.getAttribute('aria-expanded'),
    hidden: island.getAttribute('data-island-hidden'),
    focused: document.hasFocus(),
    questionCard: visible(island.querySelector('[data-overlay-type="question"]')),
    inlineAnswer: answerButtons.some(visible),
    sessionList: visible(island.querySelector('.notch-panel__hover-content')),
    detail: visible(island.querySelector('.notch-panel__detail-content')),
  }
}})(${JSON.stringify(sessionId)}, ${JSON.stringify(answer)})`

function inspectPage(target) {
  const url = new URL(target.webSocketDebuggerUrl)
  if (url.protocol !== 'ws:' || url.host !== `127.0.0.1:${port}` || !url.pathname.startsWith('/devtools/page/')) {
    return null
  }
  const socket = new WebSocket(url)
  let timer
  return new Promise((resolve, reject) => {
    timer = setTimeout(() => reject(new Error('WebView2 question snapshot timed out')), 3_000)
    socket.addEventListener('open', () => {
      socket.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression, awaitPromise: true, returnByValue: true } }))
    }, { once: true })
    socket.addEventListener('error', () => reject(new Error('Could not inspect the WebView2 page')), { once: true })
    socket.addEventListener('message', ({ data }) => {
      const response = JSON.parse(data)
      if (response.id !== 1) return
      if (response.error || response.result?.exceptionDetails) {
        reject(new Error('WebView2 question evaluation failed'))
        return
      }
      resolve(response.result?.result?.value)
    })
    socket.addEventListener('close', () => reject(new Error('WebView2 closed before returning a snapshot')), { once: true })
  }).finally(() => {
    clearTimeout(timer)
    socket.close()
  })
}

let state
for (const target of pages) {
  if (target.type !== 'page' || !target.webSocketDebuggerUrl) continue
  state = await inspectPage(target)
  if (state) break
}
if (!state) throw new Error('The AgentBro Island page was not found in WebView2')
console.log(`Pending question renderer snapshot: ${JSON.stringify(state)}`)
