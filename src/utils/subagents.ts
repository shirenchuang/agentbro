import type { SessionState, SubagentInfo } from '../types/agent'
import { timestampToMs } from './sessionDisplay'

const FOLLOW_UP_HIDE_TOLERANCE_MS = 1000

function latestUserMessageAt(session: SessionState): number | undefined {
  const explicit = timestampToMs(session.lastUserMessageAt)
  const chatTimestamp = [...session.chatHistory]
    .reverse()
    .find((message) => message.role === 'user')?.timestamp
  const chat = timestampToMs(chatTimestamp)

  if (explicit == null) return chat
  if (chat == null) return explicit
  return Math.max(explicit, chat)
}

function subagentActivityAt(subagent: SubagentInfo): number | undefined {
  return timestampToMs(subagent.completedAt ?? subagent.startedAt)
    ?? timestampToMs(subagent.startedAt)
}

export function getSessionListSubagents(session: SessionState): SubagentInfo[] {
  if (session.subagents.length === 0) return []

  const userMessageAt = latestUserMessageAt(session)
  if (userMessageAt == null) return session.subagents

  return session.subagents.filter((subagent) => {
    if (subagent.status === 'running') return true

    const activityAt = subagentActivityAt(subagent)
    return activityAt == null || activityAt + FOLLOW_UP_HIDE_TOLERANCE_MS >= userMessageAt
  })
}
