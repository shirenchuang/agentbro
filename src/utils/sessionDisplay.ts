import type { SessionPhase, SessionState } from '../types/agent'

const APP_BUNDLE_LABELS: Array<[string, string]> = [
  ['openai.codex', 'Codex App'],
  ['codex', 'Codex App'],
  ['openai.chat', 'ChatGPT'],
  ['chatgpt', 'ChatGPT'],
  ['evolab', 'Evolab'],
]

const TERMINAL_BUNDLE_LABELS: Array<[string, string]> = [
  ['googlecode.iterm2', 'iTerm2'],
  ['iterm2', 'iTerm2'],
  ['apple.terminal', 'Terminal'],
  ['mitchellh.ghostty', 'Ghostty'],
  ['ghostty', 'Ghostty'],
  ['wez.wezterm', 'WezTerm'],
  ['wezterm', 'WezTerm'],
  ['kovidgoyal.kitty', 'Kitty'],
  ['kitty', 'Kitty'],
  ['warp', 'Warp'],
  ['alacritty', 'Alacritty'],
  ['vscodeinsiders', 'VS Code'],
  ['vscode', 'VS Code'],
  ['todesktop.230313mzl4w4u92', 'Cursor'],
  ['cursor', 'Cursor'],
  ['windsurf', 'Windsurf'],
  ['zed', 'Zed'],
  ['cmuxterm', 'cmux'],
]

const TERMINAL_TEXT_LABELS: Array<[string, string]> = [
  ['iterm2', 'iTerm2'],
  ['iterm', 'iTerm2'],
  ['terminal.app', 'Terminal'],
  ['apple_terminal', 'Terminal'],
  ['terminal', 'Terminal'],
  ['ghostty', 'Ghostty'],
  ['wezterm', 'WezTerm'],
  ['kitty', 'Kitty'],
  ['warp', 'Warp'],
  ['alacritty', 'Alacritty'],
  ['vs code', 'VS Code'],
  ['vscode', 'VS Code'],
  ['code - insiders', 'VS Code'],
  ['cursor', 'Cursor'],
  ['windsurf', 'Windsurf'],
  ['zed', 'Zed'],
  ['cmux', 'cmux'],
]

const PASSIVE_PHASES = new Set<SessionPhase>(['idle', 'done', 'interrupted'])

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

function labelFromPairs(value: string | null | undefined, pairs: Array<[string, string]>): string | null {
  const normalized = (value || '').trim().toLowerCase()
  if (!normalized) return null
  const matched = pairs.find(([needle]) => normalized.includes(needle))
  return matched?.[1] ?? null
}

export function getSessionAppLabel(session: SessionState): string | null {
  const bundleLabel = labelFromPairs(session.termBundleId, APP_BUNDLE_LABELS)
  if (bundleLabel) return bundleLabel

  const terminal = (session.terminal || '').trim()
  const terminalLower = terminal.toLowerCase()
  if (terminalLower.includes('codex')) return 'Codex App'
  if (terminalLower.includes('chatgpt')) return 'ChatGPT'
  if (terminalLower.includes('evolab')) return 'Evolab'

  return null
}

export function getSessionTerminalLabel(session: SessionState): string | null {
  const terminal = (session.terminal || '').trim()
  const bundleLabel = labelFromPairs(session.termBundleId, TERMINAL_BUNDLE_LABELS)
  if (bundleLabel) return bundleLabel

  if (!terminal || isTtyLabel(terminal)) return null

  const appLabel = getSessionAppLabel(session)
  if (appLabel && terminal.toLowerCase().includes(appLabel.replace(/\s+app$/i, '').toLowerCase())) {
    return null
  }

  return labelFromPairs(terminal, TERMINAL_TEXT_LABELS) ?? terminal
}

export function getSessionTitle(session: SessionState): string {
  const title = (session.sessionTitle || '').trim()
  const project = (session.project || '').trim()
  if (title && project && title !== project && !title.startsWith(`${project} ·`)) {
    return `${project} · ${title}`
  }
  return title || project || 'Session'
}

export function isPassiveSession(session: SessionState): boolean {
  return PASSIVE_PHASES.has(session.phase)
    && !session.pendingPermission
    && !session.pendingQuestion
    && !session.planTitle
    && !session.planContent
    && !session.activeTools.some((tool) => tool.status === 'running')
    && !session.subagents.some((agent) => agent.status === 'running')
}

export function getSessionExpiryAnchor(session: SessionState): number {
  return session.taskCompletedAt
    ?? session.idleSince
    ?? session.lastActivityAt
    ?? session.startedAt
}

export function isSessionPastDisplayTimeout(session: SessionState, timeoutMinutes: number, now = Date.now()): boolean {
  if (timeoutMinutes <= 0 || !isPassiveSession(session)) return false
  return now - getSessionExpiryAnchor(session) > timeoutMinutes * 60 * 1000
}
