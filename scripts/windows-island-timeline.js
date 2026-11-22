(() => {
  const key = Symbol.for('agentbro.windowsSmokeTimeline')
  if (window[key]) return true
  const panel = document.querySelector('.notch-panel')
  if (!panel) return false

  const frames = []
  window[key] = frames
  const buttons = 'button, [role="button"]'
  const buttonKind = (element) => {
    const button = element instanceof Element ? element.closest(buttons) : null
    if (!button || !panel.contains(button)) return null
    if (button.classList.contains('hover-list__session-title')) return 'session'
    const name = (button.getAttribute('aria-label') || button.textContent).trim()
    if (name === 'Back to sessions') return 'back'
    if (name === 'Deny') return 'deny'
    if (name === 'Allow Once') return 'allowOnce'
    if (name.startsWith('Mute notifications')) return 'mute'
    if (name === 'Make agents easier to use' || /^Show all \d+ sessions$/.test(name)) return 'showSessions'
    return null
  }

  const record = (event, detail) => {
    frames.push({
      event,
      at: new Date().toISOString(),
      panel: panel.dataset.islandState,
      focused: document.hasFocus(),
      active: buttonKind(document.activeElement),
      ...detail,
    })
    if (frames.length > 96) frames.shift()
  }

  let previous = ''
  const recordView = () => {
    const controls = Array.from(panel.querySelectorAll(buttons)).flatMap((button) => {
      const kind = buttonKind(button)
      if (!kind) return []
      return [{ kind, hidden: Boolean(button.closest('[inert], [aria-hidden="true"]')) }]
    })
    const current = JSON.stringify([panel.dataset.islandState, controls])
    if (current === previous) return
    previous = current
    record('view', { controls })
  }
  new MutationObserver(recordView).observe(panel, {
    subtree: true,
    childList: true,
    attributes: true,
    attributeFilter: ['data-island-state', 'aria-hidden', 'inert', 'aria-label'],
  })
  recordView()

  panel.addEventListener('click', (event) => {
    const target = buttonKind(event.target)
    if (target) record('click', { target, trusted: event.isTrusted })
  }, true)
  for (const type of ['keydown', 'keyup']) {
    document.addEventListener(type, (event) => {
      if (event.code !== 'Space' && event.code !== 'Tab') return
      record(type, { code: event.code, trusted: event.isTrusted })
    }, true)
  }
  document.addEventListener('focusin', (event) => {
    const target = buttonKind(event.target)
    if (target) record('focusin', { target })
  })
  for (const type of ['focus', 'blur']) {
    window.addEventListener(type, () => record(type))
  }
  return true
})()
