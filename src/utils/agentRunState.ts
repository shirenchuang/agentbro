import type { AgentEvent, AgentRunEvent, AgentRunState, AgentRunStatus, SessionState, ToolStatus } from '../types/agent'

const TERMINAL_TOOL_STATUSES = new Set<Extract<ToolStatus, 'success' | 'error' | 'interrupted'>>([
  'success',
  'error',
  'interrupted',
])

function epochMs(value: number | undefined): number | undefined {
  if (value == null || !Number.isFinite(value)) return undefined
  return value < 1_000_000_000_000 ? value * 1000 : value
}

function isoTimestamp(value: number | undefined, fallback = Date.now()): string {
  return new Date(epochMs(value) ?? fallback).toISOString()
}

function actionLabel(toolName?: string, toolTarget?: string): string | undefined {
  if (!toolName) return undefined
  return toolTarget ? `${toolName}: ${toolTarget}` : toolName
}

export function statusFromSession(session: Pick<SessionState, 'phase' | 'pendingPermission' | 'pendingQuestion' | 'planTitle' | 'planContent'>): AgentRunStatus {
  if (session.pendingPermission || session.planTitle || session.planContent || session.phase === 'waiting_approval') {
    return 'waiting_permission'
  }
  if (session.pendingQuestion || session.phase === 'waiting_input') return 'waiting_input'
  if (session.phase === 'processing' || session.phase === 'compacting') return 'running'
  if (session.phase === 'error') return 'error'
  if (session.phase === 'done') return 'completed'
  if (session.phase === 'interrupted') return 'cancelled'
  if (session.phase === 'ready' || session.phase === 'idle') return 'idle'
  return 'unknown'
}

export function agentRunStateFromSession(session: SessionState, now = Date.now()): AgentRunState {
  const currentAction = actionLabel(session.lastToolName, session.lastToolTarget)
    || session.description
    || undefined
  const activityAt = session.lastActivityAt
    ?? session.taskCompletedAt
    ?? session.lastMainAgentAt
    ?? session.startedAt

  return {
    agent: session.agentType,
    sessionId: session.id,
    status: statusFromSession(session),
    phase: session.phase,
    currentAction,
    startedAt: isoTimestamp(session.startedAt, now),
    updatedAt: isoTimestamp(activityAt, now),
  }
}

export function agentRunEventFromLegacy(event: AgentEvent): AgentRunEvent | undefined {
  switch (event.type) {
    case 'session_start':
      return { type: 'session_started', agent: event.agentType, sessionId: event.sessionId }
    case 'processing':
      return { type: 'status_changed', status: 'running', phase: 'processing', currentAction: event.description }
    case 'user_message':
      return { type: 'status_changed', status: 'running', phase: 'processing', currentAction: event.content }
    case 'tool_use':
      return TERMINAL_TOOL_STATUSES.has(event.status as Extract<ToolStatus, 'success' | 'error' | 'interrupted'>)
        ? { type: 'tool_finished', toolName: event.toolName, toolTarget: event.toolTarget, status: event.status as Extract<ToolStatus, 'success' | 'error' | 'interrupted'> }
        : { type: 'tool_started', toolName: event.toolName, toolTarget: event.toolTarget }
    case 'permission_request':
      return { type: 'permission_requested', toolName: event.toolName }
    case 'ask_question':
      return { type: 'waiting_input', question: event.question }
    case 'plan_request':
      return { type: 'permission_requested' }
    case 'task_complete':
      return { type: 'completed', summary: event.summary }
    case 'error':
      return { type: 'error', message: event.message }
    case 'interrupt':
      return { type: 'status_changed', status: 'cancelled', phase: 'interrupted' }
    case 'context_compact':
      return event.phase === 'pre'
        ? { type: 'status_changed', status: 'running', phase: 'compacting', currentAction: 'Compacting context' }
        : { type: 'status_changed', status: 'running', phase: 'processing' }
    case 'token_usage':
      return { type: 'usage_updated' }
    default:
      return undefined
  }
}

export function applyAgentRunEvent(
  previous: AgentRunState,
  event: AgentRunEvent,
  now = Date.now(),
): AgentRunState {
  const updatedAt = new Date(now).toISOString()
  switch (event.type) {
    case 'session_started':
      return {
        ...previous,
        agent: event.agent,
        sessionId: event.sessionId,
        status: 'starting',
        phase: 'ready',
        currentAction: undefined,
        startedAt: event.startedAt ?? updatedAt,
        updatedAt,
      }
    case 'status_changed':
      return {
        ...previous,
        status: event.status,
        phase: event.phase ?? previous.phase,
        currentAction: event.currentAction ?? previous.currentAction,
        updatedAt,
      }
    case 'tool_started':
      return {
        ...previous,
        status: 'running',
        phase: 'processing',
        currentAction: actionLabel(event.toolName, event.toolTarget),
        updatedAt,
      }
    case 'tool_finished':
      return {
        ...previous,
        status: event.status === 'error' ? 'error' : event.status === 'interrupted' ? 'cancelled' : 'running',
        phase: event.status === 'error' ? 'error' : event.status === 'interrupted' ? 'interrupted' : 'processing',
        currentAction: actionLabel(event.toolName, event.toolTarget),
        updatedAt,
      }
    case 'waiting_input':
      return { ...previous, status: 'waiting_input', phase: 'waiting_input', currentAction: event.question, updatedAt }
    case 'permission_requested':
      return { ...previous, status: 'waiting_permission', phase: 'waiting_approval', currentAction: event.toolName, updatedAt }
    case 'rate_limited':
      return { ...previous, status: 'rate_limited', phase: event.phase ?? previous.phase, currentAction: event.currentAction ?? previous.currentAction, updatedAt }
    case 'error':
      return { ...previous, status: 'error', phase: 'error', currentAction: event.message, updatedAt }
    case 'completed':
      return { ...previous, status: 'completed', phase: 'done', currentAction: event.summary, updatedAt }
    case 'usage_updated':
      return { ...previous, updatedAt }
  }
}

export function runStateForLegacyEvent(
  event: AgentEvent,
  previous: AgentRunState | undefined,
  session?: SessionState,
  now = Date.now(),
): AgentRunState | undefined {
  const normalized = agentRunEventFromLegacy(event)
  if (!normalized) return session ? agentRunStateFromSession(session, now) : previous
  const base = previous
    ?? (session ? agentRunStateFromSession(session, now) : {
      agent: 'unknown',
      sessionId: 'sessionId' in event ? event.sessionId : undefined,
      status: 'unknown' as const,
      updatedAt: new Date(now).toISOString(),
    })
  return applyAgentRunEvent(base, normalized, now)
}

export function filterCodexSessions(sessions: SessionState[]): SessionState[] {
  return sessions.filter((session) => (session.runState?.agent ?? session.agentType) === 'codex')
}
