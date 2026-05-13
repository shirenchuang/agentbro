import { beforeEach, describe, expect, it } from 'vitest'
import { useConfigStore } from '../stores/configStore'
import { useSessionStore } from '../stores/sessionStore'
import type { SessionState } from '../types/agent'

function session(overrides: Partial<SessionState>): SessionState {
  return {
    id: 's1',
    agentType: 'claude-code',
    project: 'project',
    terminal: 'iTerm',
    phase: 'processing',
    startedAt: Date.now(),
    duration: 0,
    tokens: { input: 0, output: 0, cacheRead: 0, cacheCreate: 0 },
    chatHistory: [],
    subagents: [],
    activeTools: [],
    ...overrides,
  }
}

describe('sessionStore backend overlays', () => {
  beforeEach(() => {
    useConfigStore.setState({ idleTimeoutMinutes: 0 })
    useSessionStore.setState({
      sessions: {},
      sessionList: [],
      activeSessionId: null,
      panelState: 'collapsed',
      baseLayer: 'compact',
      hookNotification: null,
      overlayQueue: [],
      activeOverlay: null,
      mutedSessions: {},
      wakeSilencedUntil: 0,
      focusedTerminal: null,
    })
  })

  it('creates a response overlay when backend response text changes', () => {
    useSessionStore.getState().replaceAllSessions([
      session({ responseText: 'Done with task', lastUserMessage: 'Continue the migration' }),
    ])

    const overlay = useSessionStore.getState().activeOverlay
    expect(overlay?.type).toBe('response')
    expect(overlay?.data).toMatchObject({
      responseText: 'Done with task',
      userMessage: 'Continue the migration',
    })
  })

  it('treats backend idle response as response feedback, not completion feedback', () => {
    useSessionStore.getState().replaceAllSessions([session({ phase: 'processing' })])
    useSessionStore.getState().replaceAllSessions([
      session({ phase: 'idle', responseText: 'Assistant reply', lastUserMessage: 'Question?' }),
    ])

    const state = useSessionStore.getState()
    expect(state.activeOverlay?.type).toBe('response')
    expect(state.overlayQueue.map((overlay) => overlay.type)).toEqual(['response'])
  })

  it('creates a completion overlay on transition to done', () => {
    useSessionStore.getState().replaceAllSessions([session({ phase: 'processing' })])
    useSessionStore.getState().replaceAllSessions([
      session({ phase: 'done', description: 'All checks passed' }),
    ])

    const overlay = useSessionStore.getState().activeOverlay
    expect(overlay?.type).toBe('completion')
    expect(overlay?.data).toMatchObject({ summary: 'All checks passed' })
  })

  it('marks inactive processing sessions idle after the configured idle timeout', () => {
    const now = Date.now()
    useConfigStore.setState({ idleTimeoutMinutes: 5 })
    useSessionStore.setState({
      sessions: {
        idle: session({ id: 'idle', lastActivityAt: now - 6 * 60 * 1000 }),
        activeTool: session({
          id: 'activeTool',
          lastActivityAt: now - 6 * 60 * 1000,
          activeTools: [{ toolUseId: 't1', toolName: 'Bash', status: 'running', startedAt: now - 1000 }],
        }),
      },
      sessionList: [
        session({ id: 'idle', lastActivityAt: now - 6 * 60 * 1000 }),
        session({
          id: 'activeTool',
          lastActivityAt: now - 6 * 60 * 1000,
          activeTools: [{ toolUseId: 't1', toolName: 'Bash', status: 'running', startedAt: now - 1000 }],
        }),
      ],
    })

    useSessionStore.getState().applyIdleTimeout(now)

    expect(useSessionStore.getState().sessions.idle.phase).toBe('idle')
    expect(useSessionStore.getState().sessions.idle.idleSince).toBe(now)
    expect(useSessionStore.getState().sessions.activeTool.phase).toBe('processing')
  })
})
