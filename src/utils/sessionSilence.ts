import type { SessionSilenceRule } from '../stores/configStore'
import type { SessionState } from '../types/agent'

export function normalizeSilencePattern(value: string | undefined | null): string {
  return (value || '').trim().replace(/\s+/g, ' ').toLowerCase()
}

export function getSessionDirectorySilenceTarget(session: SessionState): string {
  return (session.cwd || session.project || '').trim()
}

export function getSessionPromptSilenceTarget(session: SessionState): string {
  const direct = (session.lastUserMessage || session.sessionTitle || '').trim()
  if (direct) return direct

  const firstUserMessage = session.chatHistory.find((message) => message.role === 'user')
  return (firstUserMessage?.content || '').trim()
}

export function sessionMatchesSilenceRule(session: SessionState, rule: SessionSilenceRule): boolean {
  if (!rule.enabled) return false
  const pattern = normalizeSilencePattern(rule.pattern)
  if (!pattern) return false

  if (rule.kind === 'cwd') {
    const target = normalizeSilencePattern(getSessionDirectorySilenceTarget(session))
    return Boolean(target && target.includes(pattern))
  }

  const prompt = normalizeSilencePattern(getSessionPromptSilenceTarget(session))
  return Boolean(prompt && prompt.startsWith(pattern))
}

export function sessionMatchesLegacyCwdExclusion(session: SessionState, exclusions: string): boolean {
  const target = normalizeSilencePattern(getSessionDirectorySilenceTarget(session))
  if (!target) return false
  return exclusions
    .split(/[\n,]/)
    .map((item) => normalizeSilencePattern(item))
    .filter(Boolean)
    .some((pattern) => target.includes(pattern))
}
