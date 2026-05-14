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

  it('marks blocking overlays from suppressed backend updates', () => {
    useSessionStore.getState().replaceAllSessions([
      session({
        phase: 'waiting_approval',
        pendingPermission: { toolName: 'Bash', toolInput: 'pnpm test' },
      }),
    ], { suppressed: true })

    const overlay = useSessionStore.getState().activeOverlay
    expect(overlay?.type).toBe('permission')
    expect(overlay?.suppressed).toBe(true)
  })

  it('marks plan requests as waiting for approval', async () => {
    useSessionStore.getState().updateSession({
      type: 'session_start',
      sessionId: 's1',
      project: 'project',
      terminal: 'iTerm',
      agentType: 'claude-code',
    })

    useSessionStore.getState().updateSession({
      type: 'plan_request',
      sessionId: 's1',
      planTitle: 'Implementation plan',
      planContent: '1. Align details',
      requestedPermissions: ['Edit'],
    })
    await new Promise((resolve) => setTimeout(resolve, 0))

    const state = useSessionStore.getState()
    expect(state.sessions.s1.phase).toBe('waiting_approval')
    expect(state.sessions.s1.planPermissions).toEqual(['Edit'])
    expect(state.sessions.s1.unattendedSince).toEqual(expect.any(Number))
    expect(state.activeOverlay?.type).toBe('plan')
  })

  it('removes stale blocking overlays when backend clears pending state', () => {
    useSessionStore.getState().replaceAllSessions([
      session({
        pendingPermission: { toolName: 'Bash', toolInput: 'pnpm test' },
        pendingQuestion: { question: 'Continue?', options: ['Yes'] },
        planTitle: 'Plan',
        planContent: '1. Fix',
      }),
    ])
    expect(useSessionStore.getState().overlayQueue.map((overlay) => overlay.type)).toEqual([
      'permission',
      'plan',
      'question',
    ])

    useSessionStore.getState().replaceAllSessions([session({ phase: 'processing' })])

    expect(useSessionStore.getState().overlayQueue).toEqual([])
    expect(useSessionStore.getState().activeOverlay).toBeNull()
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
