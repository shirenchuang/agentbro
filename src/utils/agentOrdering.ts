import type { AgentSummary } from '../services/skillApiV2'
import type { SessionState } from '../types/agent'

export const AGENT_ORDER_STORAGE_KEY = 'agentbro.agentManagement.agentOrder.v1'

const RECENT_WINDOW_MS = 7 * 24 * 60 * 60 * 1000

export function readStoredAgentOrder(): string[] {
  if (typeof window === 'undefined') return []
  try {
    const raw = window.localStorage.getItem(AGENT_ORDER_STORAGE_KEY)
    const parsed = raw ? JSON.parse(raw) : []
    return Array.isArray(parsed) ? parsed.filter((id): id is string => typeof id === 'string' && id.length > 0) : []
  } catch {
    return []
  }
}

export function writeStoredAgentOrder(order: string[]) {
  if (typeof window === 'undefined') return
  try {
    window.localStorage.setItem(AGENT_ORDER_STORAGE_KEY, JSON.stringify(order))
  } catch {
    // Local preference only; ignore storage failures.
  }
}

export function normalizeAgentOrder(order: string[], agentIds: string[]): string[] {
  const available = new Set(agentIds)
  const seen = new Set<string>()
  const normalized = order.filter((id) => {
    if (!available.has(id) || seen.has(id)) return false
    seen.add(id)
    return true
  })
  for (const id of agentIds) {
    if (!seen.has(id)) normalized.push(id)
  }
  return normalized
}

export function moveAgentInOrder(
  currentOrder: string[],
  displayedAgentIds: string[],
  agentId: string,
  direction: 'up' | 'down',
): string[] {
  const base = currentOrder.length > 0
    ? normalizeAgentOrder(currentOrder, displayedAgentIds)
    : [...displayedAgentIds]
  const index = base.indexOf(agentId)
  if (index < 0) return base
  const nextIndex = direction === 'up' ? index - 1 : index + 1
  if (nextIndex < 0 || nextIndex >= base.length) return base
  const next = [...base]
  const target = next[nextIndex]
  next[nextIndex] = next[index]
  next[index] = target
  return next
}

export function buildAgentUsageScores(
  sessions: SessionState[],
  activeSessionId: string | null,
  now = Date.now(),
): Map<string, number> {
  const scores = new Map<string, number>()
  for (const session of sessions) {
    const activityAt = session.lastActivityAt
      ?? session.lastUserMessageAt
      ?? session.taskCompletedAt
      ?? session.startedAt
      ?? 0
    const age = activityAt > 0 ? Math.max(0, now - activityAt) : RECENT_WINDOW_MS
    const recency = Math.max(0, RECENT_WINDOW_MS - age) / RECENT_WINDOW_MS
    const activeBoost = session.id === activeSessionId ? 10_000 : 0
    const runningBoost = session.phase === 'processing' || session.phase === 'waiting_approval' || session.phase === 'waiting_input'
      ? 2_000
      : 0
    scores.set(session.agentType, (scores.get(session.agentType) ?? 0) + activeBoost + runningBoost + 1_000 + recency * 1_000)
  }
  return scores
}

export function sortAgentSummaries(
  agents: AgentSummary[],
  options: {
    manualOrder?: string[]
    usageScores?: Map<string, number>
  } = {},
): AgentSummary[] {
  const manualOrder = options.manualOrder ?? []
  const usageScores = options.usageScores ?? new Map<string, number>()
  const normalizedManualOrder = manualOrder.length > 0
    ? normalizeAgentOrder(manualOrder, agents.map((agent) => agent.id))
    : []
  const manualRank = new Map(normalizedManualOrder.map((id, index) => [id, index]))

  return [...agents].sort((a, b) => {
    if (normalizedManualOrder.length > 0) {
      const manualDelta = (manualRank.get(a.id) ?? Number.MAX_SAFE_INTEGER) - (manualRank.get(b.id) ?? Number.MAX_SAFE_INTEGER)
      if (manualDelta !== 0) return manualDelta
    }

    const usageDelta = (usageScores.get(b.id) ?? 0) - (usageScores.get(a.id) ?? 0)
    if (usageDelta !== 0) return usageDelta

    const skillDelta = agentSkillWeight(b) - agentSkillWeight(a)
    if (skillDelta !== 0) return skillDelta

    return a.displayName.localeCompare(b.displayName)
  })
}

function agentSkillWeight(agent: AgentSummary): number {
  return agent.managedSkillCount * 2 + agent.unmanagedSkillCount + (agent.readOnlySkillCount ?? 0)
}
