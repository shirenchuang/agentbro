import type { SessionPhase, SessionState } from '../types/agent'

const APP_BUNDLE_LABELS: Array<[string, string]> = [
  ['openai.codex', 'Codex App'],
  ['codex', 'Codex App'],
  ['openai.chat', 'ChatGPT'],
  ['chatgpt', 'ChatGPT'],
]

const TERMINAL_BUNDLE_LABELS: Array<[string, string]> = [
  ['googlecode.iterm2', 'iTerm2'],
  ['iterm2', 'iTerm2'],
  ['apple.terminal', 'Terminal'],
  ['mitchellh.ghostty', 'Ghostty'],
  ['ghostty', 'Ghostty'],
  ['warp', 'Warp'],
  ['commandline.waveterm', 'Wave'],
  ['waveterm', 'Wave'],
  ['wez.wezterm', 'WezTerm'],
  ['wezterm', 'WezTerm'],
  ['kovidgoyal.kitty', 'Kitty'],
  ['kitty', 'Kitty'],
  ['alacritty', 'Alacritty'],
  ['microsoft.vscodeinsiders', 'VS Code'],
  ['microsoft.vscode', 'VS Code'],
  ['vscodeinsiders', 'VS Code'],
  ['vscode', 'VS Code'],
  ['todesktop.230313mzl4w4u92', 'Cursor'],
  ['cursor', 'Cursor'],
  ['exafunction.windsurf', 'Windsurf'],
  ['windsurf', 'Windsurf'],
  ['zed.zed', 'Zed'],
  ['zed', 'Zed'],
  ['cmuxterm', 'cmux'],
  ['tw93.kaku', 'Kaku'],
  ['kapeli.kaku', 'Kaku'],
]

const TERMINAL_NAME_LABELS: Array<[string, string]> = [
  ['iterm2', 'iTerm2'],
  ['iterm', 'iTerm'],
  ['appleterminal', 'Terminal'],
  ['apple_terminal', 'Terminal'],
  ['terminalapp', 'Terminal'],
  ['terminal', 'Terminal'],
  ['ghostty', 'Ghostty'],
  ['warp', 'Warp'],
  ['waveterm', 'Wave'],
  ['wave', 'Wave'],
  ['wezterm', 'WezTerm'],
  ['kitty', 'Kitty'],
  ['alacritty', 'Alacritty'],
  ['vscodeinsiders', 'VS Code'],
  ['vscode', 'VS Code'],
  ['codeinsiders', 'VS Code'],
  ['code - insiders', 'VS Code'],
  ['cursor', 'Cursor'],
  ['windsurf', 'Windsurf'],
  ['zed', 'Zed'],
  ['cmux', 'cmux'],
  ['kaku', 'Kaku'],
]

const PASSIVE_PHASES = new Set<SessionPhase>(['ready', 'idle', 'done', 'interrupted'])

export function getAgentDisplayName(session: SessionState): string {
  if (session.agentType === 'claude-code' && session.engineLabel && session.engineLabel !== 'Claude Code') {
    return session.engineLabel
  }
  switch (session.agentType) {
    case 'claude-code': return 'Claude'
    case 'gemini-cli': return 'Gemini'
    case 'workbuddy': return 'WorkBuddy'
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

function labelFromBundle(bundleId: string | null | undefined, labels: Array<[string, string]>): string | null {
  const bundle = (bundleId || '').trim().toLowerCase()
  if (!bundle) return null
  return labels.find(([needle]) => bundle.includes(needle))?.[1] ?? null
}

function labelFromUnknownBundle(bundleId: string | null | undefined): string | null {
  const bundle = (bundleId || '').trim()
  if (!bundle || labelFromBundle(bundle, TERMINAL_BUNDLE_LABELS)) return null
  const product = bundle.split('.').filter(Boolean).pop()
  if (!product) return bundle
  return product
    .replace(/[-_]+/g, ' ')
    .replace(/\b\w/g, (char) => char.toUpperCase())
}

function normalizeTerminalLabel(value: string | null | undefined): string | null {
  const raw = (value || '').trim()
  if (!raw || isTtyLabel(raw)) return null

  const lastPathPart = raw.split(/[\\/]+/).filter(Boolean).pop() || raw
  const withoutExtension = lastPathPart.replace(/\.app$/i, '')
  const normalized = withoutExtension.toLowerCase().replace(/[^a-z0-9]+/g, '')
  const matched = TERMINAL_NAME_LABELS.find(([needle]) => normalized.includes(needle))
  return matched?.[1] ?? withoutExtension
}

export function getSessionAppLabel(session: SessionState): string | null {
  const appLabel = labelFromBundle(session.termBundleId, APP_BUNDLE_LABELS)
  if (appLabel) return appLabel

  const unknownAppLabel = labelFromUnknownBundle(session.termBundleId)
  if (unknownAppLabel) return unknownAppLabel

  const terminal = (session.terminal || '').trim()
  const terminalLower = terminal.toLowerCase()
  if (terminalLower.includes('codex')) return 'Codex App'
  if (terminalLower.includes('chatgpt')) return 'ChatGPT'

  return null
}

export function getSessionTerminalLabel(session: SessionState): string | null {
  const appLabel = getSessionAppLabel(session)
  if (appLabel) return null

  const bundleLabel = labelFromBundle(session.termBundleId, TERMINAL_BUNDLE_LABELS)
  if (bundleLabel) return bundleLabel

  const termProgramLabel = normalizeTerminalLabel(session.termProgram)
  if (termProgramLabel) return termProgramLabel

  const terminalLabel = normalizeTerminalLabel(session.terminal)
  if (!terminalLabel) return null

  return terminalLabel
}

export function getSessionTitle(session: SessionState): string {
  const title = isInternalCodexTitle(session.sessionTitle) ? '' : (session.sessionTitle || '').trim()
  const project = (session.project || '').trim()
  if (title && project && title !== project && !title.startsWith(`${project} ·`)) {
    return `${project} · ${title}`
  }
  return title || project || 'Session'
}

function isInternalCodexTitle(title: string | undefined | null): boolean {
  const normalized = (title || '').trim().replace(/\s+/g, ' ').toLowerCase()
  return normalized.startsWith('<environment_context>')
    || normalized.startsWith('you are a helpful assistant. you will be presented with a user prompt')
    || normalized.startsWith('you are codex, a coding agent')
    || normalized.startsWith('you are an ai assistant accessed via an api')
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

export function timestampToMs(timestamp: number | undefined): number | undefined {
  if (timestamp == null) return undefined
  return timestamp < 100_000_000_000 ? timestamp * 1000 : timestamp
}

export function getSessionExpiryAnchor(session: SessionState): number {
  return timestampToMs(session.taskCompletedAt)
    ?? timestampToMs(session.idleSince)
    ?? timestampToMs(session.lastActivityAt)
    ?? timestampToMs(session.startedAt)
    ?? Date.now()
}

export function isSessionPastDisplayTimeout(session: SessionState, timeoutMinutes: number, now = Date.now()): boolean {
  if (timeoutMinutes <= 0 || !isPassiveSession(session)) return false
  return now - getSessionExpiryAnchor(session) > timeoutMinutes * 60 * 1000
}

const MODEL_DISPLAY_MAP: Record<string, string> = {
  'claude-opus-4-8': 'Opus 4.8',
  'claude-opus-4-7': 'Opus 4.7',
  'claude-opus-4-6': 'Opus 4.6',
  'claude-opus-4-5-20250514': 'Opus 4.5',
  'claude-sonnet-4-6': 'Sonnet 4.6',
  'claude-sonnet-4-5-20241022': 'Sonnet 4.5',
  'claude-haiku-4-5-20251001': 'Haiku 4.5',
  'claude-3-5-sonnet-20241022': 'Sonnet 3.5',
  'claude-3-5-haiku-20241022': 'Haiku 3.5',
}

export function formatModelName(model: string): string {
  const lower = model.toLowerCase()
  const exact = MODEL_DISPLAY_MAP[lower]
  if (exact) return exact

  if (lower.startsWith('claude-')) {
    const rest = lower.slice(7)
    const match = rest.match(/^(opus|sonnet|haiku)-(.+)/)
    if (match) {
      const family = match[1].charAt(0).toUpperCase() + match[1].slice(1)
      const version = match[2].replace(/-/g, '.').replace(/\.\d{8}$/, '')
      return `${family} ${version}`
    }
  }

  if (lower.startsWith('gpt-')) return model.toUpperCase().replace('GPT', 'GPT')

  return model
}
