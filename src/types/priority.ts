/* AgentBro — Priority System */

export const PRIORITY = {
  dormant: 0,
  idle: 1,
  done: 2,
  compacting: 3,
  thinking: 4,
  working: 5,
  attention: 6,
  error: 7,
} as const

export type Priority = typeof PRIORITY[keyof typeof PRIORITY]
export type PriorityName = keyof typeof PRIORITY

export function computePriority(session: {
  phase: string
  lastToolName?: string
  idleSince?: number
  startedAt: number
}): Priority {
  if (session.phase === 'error') return PRIORITY.error
  if (session.phase === 'waiting_approval' || session.phase === 'waiting_input')
    return PRIORITY.attention
  if (session.phase === 'compacting') return PRIORITY.compacting
  if (session.phase === 'done') return PRIORITY.done
  if (session.phase === 'ready') return PRIORITY.done
  if (session.phase === 'processing') {
    return session.lastToolName ? PRIORITY.working : PRIORITY.thinking
  }
  if (session.phase === 'idle') {
    return PRIORITY.idle
  }
  return PRIORITY.idle
}

export function priorityName(p: Priority): PriorityName {
  const entries = Object.entries(PRIORITY) as [PriorityName, Priority][]
  return entries.find(([, v]) => v === p)?.[0] ?? 'idle'
}
