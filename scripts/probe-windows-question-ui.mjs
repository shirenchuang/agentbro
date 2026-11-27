const [port, sessionId, answer, mode] = process.argv.slice(2)
if (process.env.RUNNER_ENVIRONMENT !== 'github-hosted' || !/^\d+$/.test(port) || !sessionId || !answer) {
  throw new Error('The question trace requires an isolated GitHub-hosted runner and a fixture session')
}

const origin = `http://127.0.0.1:${port}`
const pages = await fetch(`${origin}/json/list`, { signal: AbortSignal.timeout(2_500) }).then(response => {
  if (!response.ok) throw new Error(`WebView2 target lookup returned ${response.status}`)
  return response.json()
})

const expression = `(${async (id, expectedAnswer, action) => {
  const island = document.querySelector('[role="region"][aria-label="AgentBro"]')
  if (!island) return null
  const key = Symbol.for('agentbro.windowsQuestionTrace')
  const visible = element => Boolean(element?.getClientRects().length && !element.closest('[inert], [aria-hidden="true"]'))
  const currentView = () => ({
    expanded: island.getAttribute('aria-expanded'),
    focused: document.hasFocus(),
    questionCard: visible(island.querySelector('[data-overlay-type="question"]')),
    answer: [...island.querySelectorAll('button')].some(button => visible(button) && button.textContent.trim().endsWith(expectedAnswer)),
    list: visible(island.querySelector('.notch-panel__hover-content')),
    detail: visible(island.querySelector('.notch-panel__detail-content')),
  })
  if (action === 'start') {
    const frames = []
    window[key] = frames
    const record = (event, detail) => {
      frames.push({ at: new Date().toISOString(), event, ...detail })
      if (frames.length > 160) frames.shift()
    }
    const timers = new Map()
    const nativeSetTimeout = window.setTimeout.bind(window)
    const nativeClearTimeout = window.clearTimeout.bind(window)
    window.setTimeout = (callback, delay, ...args) => {
      if (typeof callback !== 'function' || !callback.toString().includes('collapsed')) {
        return nativeSetTimeout(callback, delay, ...args)
      }
      const id = nativeSetTimeout((...values) => {
        timers.delete(id)
        record('timer-fired', { id, delay })
        callback.apply(window, values)
      }, delay, ...args)
      timers.set(id, delay)
      record('timer-scheduled', { id, delay })
      return id
    }
    window.clearTimeout = id => {
      if (timers.has(id)) {
        record('timer-cleared', { id, delay: timers.get(id) })
        timers.delete(id)
      }
      nativeClearTimeout(id)
    }
    let lastView = ''
    const traceView = () => {
      const view = currentView()
      const serialized = JSON.stringify(view)
      if (serialized !== lastView) {
        lastView = serialized
        record('view', view)
      }
    }
    new MutationObserver(traceView).observe(island, {
      subtree: true, childList: true, attributes: true, attributeFilter: ['aria-expanded', 'aria-hidden', 'inert'],
    })
    window.addEventListener('focus', traceView)
    window.addEventListener('blur', traceView)
    let lastHook = ''
    await window.__TAURI__.event.listen('session-update', ({ payload }) => {
      const session = payload.sessions.find(candidate => candidate.id === id)
      const hook = { phase: session?.phase, question: session?.pendingQuestion?.toolUseId, suppressed: payload.suppressed }
      const serialized = JSON.stringify(hook)
      if (serialized !== lastHook) {
        lastHook = serialized
        record('session', { revision: payload.revision, ...hook })
      }
    })
    traceView()
    return { ready: true }
  }
  const snapshot = await window.__TAURI__.core.invoke('get_session_snapshot')
  const session = snapshot.sessions.find(candidate => candidate.id === id)
  return {
    revision: snapshot.revision,
    backendPhase: session?.phase ?? null,
    backendQuestion: Boolean(session?.pendingQuestion),
    ...currentView(),
    frames: window[key] ?? [],
  }
}})(${JSON.stringify(sessionId)}, ${JSON.stringify(answer)}, ${JSON.stringify(mode)})`

function inspectPage(target) {
  const url = new URL(target.webSocketDebuggerUrl)
  if (url.protocol !== 'ws:' || url.port !== port || !['127.0.0.1', 'localhost'].includes(url.hostname) || !url.pathname.startsWith('/devtools/page/')) {
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
