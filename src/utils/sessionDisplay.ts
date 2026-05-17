import type { SessionState } from '../types/agent'

const BUNDLE_LABELS: Array<[string, string]> = [
  ['openai.codex', 'Codex App'],
  ['codex', 'Codex App'],
  ['openai.chat', 'ChatGPT'],
  ['chatgpt', 'ChatGPT'],
  ['evolab', 'Evolab'],
]

export function getAgentDisplayName(session: SessionState): string {
  if (session.agentType === 'claude-code' && session.engineLabel && session.engineLabel !== 'Claude Code') {
    return session.engineLabel
  }
  switch (session.agentType) {
    case 'claude-code': return 'Claude'
    case 'gemini-cli': return 'Gemini'
    default: return session.agentType.charAt(0).toUpperCase() + session.agentType.slice(1)
  }
}

function normalizeProductLabel(label: string): string {
  return label
    .toLowerCase()
    .replace(/\b(openai|app|cli|code)\b/g, '')
    .replace(/[^a-z0-9]+/g, '')
}

export function shouldShowAgentBadge(session: SessionState): boolean {
  const appLabel = getSessionAppLabel(session)
  if (!appLabel) return true

  const appProduct = normalizeProductLabel(appLabel)
  const agentProduct = normalizeProductLabel(getAgentDisplayName(session))
  return !appProduct || !agentProduct || appProduct !== agentProduct
}

export function isTtyLabel(value: string | null | undefined): boolean {
  const label = (value || '').trim()
  return /^\/dev\/tty/.test(label) || /^ttys\d+$/i.test(label) || /^tty[A-Za-z0-9]+$/i.test(label)
}

export function getSessionAppLabel(session: SessionState): string | null {
  const bundle = (session.termBundleId || '').trim().toLowerCase()
  if (bundle) {
    const matched = BUNDLE_LABELS.find(([needle]) => bundle.includes(needle))
    if (matched) return matched[1]
  }

  const terminal = (session.terminal || '').trim()
  const terminalLower = terminal.toLowerCase()
  if (terminalLower.includes('codex')) return 'Codex App'
  if (terminalLower.includes('chatgpt')) return 'ChatGPT'
  if (terminalLower.includes('evolab')) return 'Evolab'

  return null
}

export function getSessionTerminalLabel(session: SessionState): string | null {
  const terminal = (session.terminal || '').trim()
  if (!terminal || isTtyLabel(terminal)) return null

  const appLabel = getSessionAppLabel(session)
  if (appLabel && terminal.toLowerCase().includes(appLabel.replace(/\s+app$/i, '').toLowerCase())) {
    return null
  }

  return terminal
}

export function getSessionTitle(session: SessionState): string {
  const title = (session.sessionTitle || '').trim()
  const project = (session.project || '').trim()
  if (title && project && title !== project && !title.startsWith(`${project} ·`)) {
    return `${project} · ${title}`
  }
  return title || project || 'Session'
}
