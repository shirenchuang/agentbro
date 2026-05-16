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

  it('uses backend response text instead of generic completion text', () => {
    useSessionStore.getState().replaceAllSessions([session({ phase: 'processing' })])
    useSessionStore.getState().replaceAllSessions([
      session({
        phase: 'done',
        description: 'Task completed',
        responseText: 'AntWalle 能做，我们当然也能做。它本质是桌面 UI + 本地 opencode server。',
      }),
    ])

    const overlay = useSessionStore.getState().activeOverlay
    expect(overlay?.type).toBe('completion')
    expect(overlay?.data).toMatchObject({
      summary: 'AntWalle 能做，我们当然也能做。它本质是桌面 UI + 本地 opencode server。',
    })
  })

  it('falls back to the last assistant message when done summary is generic', () => {
    useSessionStore.getState().replaceAllSessions([session({ phase: 'processing' })])
    useSessionStore.getState().replaceAllSessions([
      session({
        phase: 'done',
        description: 'Task completed',
        chatHistory: [
          { role: 'user', content: '怎么实现？', timestamp: Date.now() - 1000 },
          { role: 'assistant', content: '', trailingContent: '完整回答内容应该展示在通知里。', timestamp: Date.now() },
        ],
      }),
    ])

    const overlay = useSessionStore.getState().activeOverlay
    expect(overlay?.type).toBe('completion')
    expect(overlay?.data).toMatchObject({ summary: '完整回答内容应该展示在通知里。' })
  })

  it('preserves explicit session-ended summaries instead of replaying the last assistant response', () => {
    useSessionStore.getState().replaceAllSessions([session({ phase: 'processing' })])
    useSessionStore.getState().replaceAllSessions([
      session({
        phase: 'done',
        description: 'Session ended',
        responseText: 'Session ended',
        chatHistory: [
          { role: 'assistant', content: 'Hi! How can I help you today?', timestamp: Date.now() },
        ],
      }),
    ])

    const overlay = useSessionStore.getState().activeOverlay
    expect(overlay?.type).toBe('completion')
    expect(overlay?.data).toMatchObject({ summary: 'Session ended' })
  })

  it('hides ended sessions from the visible session list', () => {
    useSessionStore.getState().replaceAllSessions([
      session({
        phase: 'done',
        description: 'Session ended',
        responseText: 'Session ended',
        chatHistory: [
          { role: 'assistant', content: 'Hi! How can I help you today?', timestamp: Date.now() },
        ],
      }),
    ])

    const state = useSessionStore.getState()
    expect(state.sessionList).toEqual([])
    expect(state.sessions.s1).toBeDefined()
    expect(state.activeOverlay?.type).toBe('completion')
  })

  it('hides expired Claude rows that only have a title or last user message', () => {
    useSessionStore.getState().replaceAllSessions([
      session({
        phase: 'idle',
        sessionTitle: 'hi',
        lastUserMessage: 'hi',
        description: undefined,
        responseText: undefined,
        lastToolName: undefined,
      }),
    ])

    const state = useSessionStore.getState()
    expect(state.sessionList).toEqual([])
    expect(state.activeSessionId).toBeNull()
    expect(state.activeOverlay).toBeNull()
  })

  it('hides completed Codex internal prompt sessions from the visible list', () => {
    useSessionStore.getState().replaceAllSessions([
      session({
        agentType: 'codex',
        project: 'evolab',
        terminal: '',
        tty: undefined,
        pid: undefined,
        termBundleId: undefined,
        phase: 'idle',
        sessionTitle: 'You are a helpful assistant. You will be presented with a user prompt, and your job is to help.',
        lastUserMessage: 'You are a helpful assistant. You will be presented with a user prompt, and your job is to help.',
        description: 'Task completed',
        responseText: 'Task completed',
      }),
    ])

    const state = useSessionStore.getState()
    expect(state.sessionList).toEqual([])
    expect(state.activeSessionId).toBeNull()
  })

  it('keeps Codex prompt-looking sessions when they still have a detail anchor', () => {
    useSessionStore.getState().replaceAllSessions([
      session({
        agentType: 'codex',
        project: 'evolab',
        terminal: 'Codex',
        termBundleId: 'com.openai.codex',
        phase: 'idle',
        sessionTitle: 'You are a helpful assistant. You will be presented with a user prompt, and your job is to help.',
        lastUserMessage: 'You are a helpful assistant. You will be presented with a user prompt, and your job is to help.',
        description: 'Task completed',
        responseText: 'Task completed',
      }),
    ])

    expect(useSessionStore.getState().sessionList).toHaveLength(1)
  })

  it('keeps normal Codex sessions even when their completion text is generic', () => {
    useSessionStore.getState().replaceAllSessions([
      session({
        agentType: 'codex',
        project: 'agent-island',
        phase: 'idle',
        sessionTitle: 'Fix session filtering',
        lastUserMessage: 'Fix session filtering',
        description: 'Task completed',
        responseText: 'Task completed',
      }),
    ])

    expect(useSessionStore.getState().sessionList).toHaveLength(1)
  })

  it('ignores transient processing text when deriving completion summary', () => {
    useSessionStore.getState().replaceAllSessions([session({ phase: 'processing' })])
    useSessionStore.getState().replaceAllSessions([
      session({
        phase: 'done',
        description: 'Processing user input',
        responseText: 'Hi! How can I help you today?',
      }),
    ])

    const overlay = useSessionStore.getState().activeOverlay
    expect(overlay?.type).toBe('completion')
    expect(overlay?.data).toMatchObject({ summary: 'Hi! How can I help you today?' })
  })

  it('does not duplicate done-state backend responses as a response overlay', () => {
    useSessionStore.getState().replaceAllSessions([session({ phase: 'processing' })])
    useSessionStore.getState().replaceAllSessions([
      session({
        phase: 'done',
        description: 'All checks passed',
        responseText: 'All checks passed',
        lastUserMessage: 'Run checks',
      }),
    ])

    const state = useSessionStore.getState()
    expect(state.activeOverlay?.type).toBe('completion')
    expect(state.overlayQueue.map((overlay) => overlay.type)).toEqual(['completion'])
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

  it('hides empty Codex App startup placeholders from the visible session list', () => {
    useSessionStore.getState().replaceAllSessions([
      session({
        agentType: 'codex',
        project: 'free-chat',
        cwd: '/Users/me/Library/Application Support/.evolab-desktop/free-chat',
        terminal: 'Codex',
        termBundleId: 'com.openai.codex',
        phase: 'idle',
      }),
    ])

    expect(useSessionStore.getState().sessionList).toEqual([])

    useSessionStore.getState().replaceAllSessions([
      session({
        agentType: 'codex',
        project: 'free-chat',
        cwd: '/Users/me/Library/Application Support/.evolab-desktop/free-chat',
        terminal: 'Codex',
        termBundleId: 'com.openai.codex',
        phase: 'processing',
        lastUserMessage: 'Help me inspect this project',
      }),
    ])

    expect(useSessionStore.getState().sessionList).toHaveLength(1)
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
