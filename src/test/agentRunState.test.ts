import { describe, expect, it } from 'vitest'
import type { SessionState } from '../types/agent'
import {
  agentRunEventFromLegacy,
  agentRunStateFromSession,
  applyAgentRunEvent,
  filterCodexSessions,
} from '../utils/agentRunState'

function session(overrides: Partial<SessionState> = {}): SessionState {
  return {
    id: 's1',
    agentType: 'codex',
    project: 'agentbro',
    terminal: 'Codex',
    phase: 'processing',
    startedAt: 1_700_000_000_000,
    duration: 0,
    tokens: { input: 0, output: 0, cacheRead: 0, cacheCreate: 0 },
    chatHistory: [],
    subagents: [],
    activeTools: [],
    ...overrides,
  }
}

describe('agent run state normalization', () => {
  it('maps a Codex tool snapshot to a running state with a current action', () => {
    const state = agentRunStateFromSession(session({ lastToolName: 'Bash', lastToolTarget: 'pnpm test' }))

    expect(state).toMatchObject({
      agent: 'codex',
      sessionId: 's1',
      status: 'running',
      phase: 'processing',
      currentAction: 'Bash: pnpm test',
    })
    expect(() => new Date(state.startedAt || '').toISOString()).not.toThrow()
  })

  it('represents waiting permission, waiting input, errors, and completion', () => {
    expect(agentRunStateFromSession(session({ phase: 'waiting_approval' })).status).toBe('waiting_permission')
    expect(agentRunStateFromSession(session({ phase: 'waiting_input' })).status).toBe('waiting_input')
    expect(agentRunStateFromSession(session({ phase: 'error' })).status).toBe('error')
    expect(agentRunStateFromSession(session({ phase: 'done' })).status).toBe('completed')
  })

  it('maps legacy Codex hook events through the unified lifecycle', () => {
    const event = agentRunEventFromLegacy({
      type: 'tool_use',
      sessionId: 's1',
      toolName: 'Bash',
      toolInput: '{"command":"pnpm test"}',
      toolTarget: 'pnpm test',
      status: 'running',
    })
    expect(event).toEqual({ type: 'tool_started', toolName: 'Bash', toolTarget: 'pnpm test' })

    const next = applyAgentRunEvent(agentRunStateFromSession(session()), event!, 1_700_000_001_000)
    expect(next).toMatchObject({ status: 'running', currentAction: 'Bash: pnpm test' })
  })

  it('keeps only normalized Codex sessions in the Island', () => {
    const codex = session({ runState: { ...agentRunStateFromSession(session()), agent: 'codex' } })
    const claude = session({
      id: 's2',
      agentType: 'claude-code',
      runState: { ...agentRunStateFromSession(session()), agent: 'claude-code', sessionId: 's2' },
    })

    expect(filterCodexSessions([codex, claude]).map((item) => item.id)).toEqual(['s1'])
  })
})

