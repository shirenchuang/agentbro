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
    useConfigStore.setState({ idleTimeoutMinutes: 0, sessionTimeoutMinutes: 30, sessionSilenceRules: [], excludedHookCwdSubstrings: '' })
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

  it('does not show response overlays for generic session-ended text', () => {
    useSessionStore.getState().replaceAllSessions([session({ phase: 'processing' })])
    useSessionStore.getState().replaceAllSessions([
      session({ phase: 'idle', responseText: 'Session ended', lastUserMessage: 'Question?' }),
    ])

    const state = useSessionStore.getState()
    expect(state.overlayQueue).toEqual([])
    expect(state.activeOverlay).toBeNull()
  })

  it('does not show response overlays for transient processing text with a prompt preview', () => {
    useSessionStore.getState().replaceAllSessions([session({ phase: 'processing' })])
    useSessionStore.getState().replaceAllSessions([
      session({ phase: 'idle', responseText: 'Processing user input: hi', lastUserMessage: 'hi' }),
    ])

    const state = useSessionStore.getState()
    expect(state.overlayQueue).toEqual([])
    expect(state.activeOverlay).toBeNull()
  })

  it('does not show response overlays for transient compacting conversation text', () => {
    useSessionStore.getState().replaceAllSessions([session({ phase: 'processing' })])
    useSessionStore.getState().replaceAllSessions([
      session({ phase: 'idle', responseText: 'Compacting conversation...', lastUserMessage: '/compact' }),
    ])

    const state = useSessionStore.getState()
    expect(state.overlayQueue).toEqual([])
    expect(state.activeOverlay).toBeNull()
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

  it('suppresses explicit session-ended completion overlays instead of replaying stale assistant text', () => {
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

    const state = useSessionStore.getState()
    expect(state.overlayQueue).toEqual([])
    expect(state.activeOverlay).toBeNull()
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
    expect(state.activeOverlay).toBeNull()
  })

  it('does not show direct task-complete overlays for generic session-ended summaries', async () => {
    useSessionStore.getState().updateSession({
      type: 'session_start',
      sessionId: 's1',
      project: 'project',
      terminal: 'iTerm',
      agentType: 'claude-code',
    })

    useSessionStore.getState().updateSession({
      type: 'task_complete',
      sessionId: 's1',
      summary: 'Session ended',
    })
    await new Promise((resolve) => setTimeout(resolve, 0))

    const state = useSessionStore.getState()
    expect(state.sessions.s1.phase).toBe('done')
    expect(state.sessions.s1.responseText).toBe('Session ended')
    expect(state.overlayQueue).toEqual([])
    expect(state.activeOverlay).toBeNull()
  })

  it('shows and clears compacting overlays around context compaction', async () => {
    useSessionStore.getState().updateSession({
      type: 'session_start',
      sessionId: 's1',
      project: 'project',
      terminal: 'iTerm',
      agentType: 'claude-code',
    })

    useSessionStore.getState().updateSession({
      type: 'context_compact',
      sessionId: 's1',
      phase: 'pre',
    })
    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(useSessionStore.getState().sessions.s1.phase).toBe('compacting')
    expect(useSessionStore.getState().activeOverlay?.type).toBe('compacting')

    useSessionStore.getState().updateSession({
      type: 'context_compact',
      sessionId: 's1',
      phase: 'post',
    })

    const state = useSessionStore.getState()
    expect(state.sessions.s1.phase).toBe('processing')
    expect(state.sessions.s1.description).toBeUndefined()
    expect(state.sessions.s1.lastToolName).toBeUndefined()
    expect(state.overlayQueue.some((overlay) => overlay.type === 'compacting')).toBe(false)
  })

  it('creates a compacting overlay when backend snapshots enter compacting', () => {
    useSessionStore.getState().replaceAllSessions([session({ phase: 'processing' })])
    useSessionStore.getState().replaceAllSessions([session({ phase: 'compacting' })])

    expect(useSessionStore.getState().activeOverlay?.type).toBe('compacting')
  })

  it('promotes backend compacting conversation snapshots to compacting', () => {
    useSessionStore.getState().replaceAllSessions([session({ phase: 'processing' })])
    useSessionStore.getState().replaceAllSessions([
      session({
        phase: 'processing',
        description: 'Compacting conversation...',
        lastToolName: undefined,
      }),
    ])

    const state = useSessionStore.getState()
    expect(state.sessions.s1.phase).toBe('compacting')
    expect(state.sessions.s1.description).toBe('Compacting conversation...')
    expect(state.activeOverlay?.type).toBe('compacting')
  })

  it('clears stale compacting labels when backend snapshots leave compacting', () => {
    useSessionStore.getState().replaceAllSessions([
      session({
        phase: 'compacting',
        description: 'Compacting context',
        lastToolName: 'Compacting',
        lastToolTarget: 'context',
      }),
    ])
    useSessionStore.getState().replaceAllSessions([
      session({
        phase: 'processing',
        description: 'Compacting context',
        lastToolName: 'Compacting',
        lastToolTarget: 'context',
      }),
    ])

    const state = useSessionStore.getState()
    expect(state.sessions.s1.phase).toBe('processing')
    expect(state.sessions.s1.description).toBeUndefined()
    expect(state.sessions.s1.lastToolName).toBeUndefined()
    expect(state.overlayQueue.some((overlay) => overlay.type === 'compacting')).toBe(false)
  })

  it('keeps inactive Claude rows visible until the session timeout', () => {
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
    expect(state.sessionList.map((item) => item.id)).toEqual(['s1'])
    expect(state.sessionList[0].idleSince).toEqual(expect.any(Number))
    expect(state.activeOverlay).toBeNull()
  })

  it('hides inactive sessions after the configured session timeout', () => {
    const now = Date.now()
    useConfigStore.setState({ sessionTimeoutMinutes: 5 })
    useSessionStore.getState().replaceAllSessions([
      session({
        phase: 'idle',
        sessionTitle: 'old work',
        lastUserMessage: 'old work',
        idleSince: now - 6 * 60 * 1000,
        lastActivityAt: now - 6 * 60 * 1000,
      }),
    ])

    const state = useSessionStore.getState()
    expect(state.sessionList).toEqual([])
    expect(state.activeSessionId).toBeNull()
  })

  it('refreshes visible sessions when an inactive row crosses the session timeout', () => {
    const now = Date.now()
    useConfigStore.setState({ idleTimeoutMinutes: 0, sessionTimeoutMinutes: 5 })
    const inactive = session({
      phase: 'idle',
      sessionTitle: 'recent work',
      lastUserMessage: 'recent work',
      idleSince: now - 4 * 60 * 1000,
      lastActivityAt: now - 4 * 60 * 1000,
    })
    useSessionStore.setState({
      sessions: { s1: inactive },
      sessionList: [inactive],
    })

    useSessionStore.getState().applyIdleTimeout(now + 2 * 60 * 1000)

    expect(useSessionStore.getState().sessionList).toEqual([])
    expect(useSessionStore.getState().sessions.s1).toBeDefined()
  })

  it('keeps an opened detail session selected when a backend refresh marks it idle', () => {
    useSessionStore.getState().replaceAllSessions([
      session({ id: 'selected', phase: 'processing', sessionTitle: 'Selected Claude session' }),
      session({ id: 'other', phase: 'processing', sessionTitle: 'Other session' }),
    ])
    useSessionStore.getState().setActiveSession('selected')

    useSessionStore.getState().replaceAllSessions([
      session({
        id: 'selected',
        phase: 'idle',
        sessionTitle: 'Selected Claude session',
        lastUserMessage: 'continue',
        description: undefined,
        responseText: undefined,
        lastToolName: undefined,
      }),
      session({ id: 'other', phase: 'processing', sessionTitle: 'Other session' }),
    ])

    const state = useSessionStore.getState()
    expect(state.sessionList.map((item) => item.id)).toEqual(['selected', 'other'])
    expect(state.activeSessionId).toBe('selected')
    expect(state.sessions.selected).toBeDefined()
  })

  it('marks locally sent messages as processing activity', () => {
    const before = Date.now()
    useSessionStore.getState().replaceAllSessions([
      session({ phase: 'idle', sessionTitle: 'Selected Claude session' }),
    ])

    useSessionStore.getState().updateSession({
      type: 'user_message',
      sessionId: 's1',
      content: 'continue please',
    })

    const selected = useSessionStore.getState().sessions.s1
    expect(selected.phase).toBe('processing')
    expect(selected.lastUserMessage).toBe('continue please')
    expect(selected.lastUserMessageAt).toBeGreaterThanOrEqual(before)
    expect(selected.idleSince).toBeUndefined()
    expect(selected.chatHistory.at(-1)).toMatchObject({ role: 'user', content: 'continue please' })
  })

  it('keeps locally sent messages when stale parsed history reloads', () => {
    const baseMessages = [
      { role: 'user' as const, content: 'initial request', timestamp: Date.now() - 2000 },
      { role: 'assistant' as const, content: 'initial answer', timestamp: Date.now() - 1000 },
    ]
    useSessionStore.getState().replaceAllSessions([
      session({ chatHistory: baseMessages }),
    ])

    useSessionStore.getState().updateSession({
      type: 'user_message',
      sessionId: 's1',
      content: 'continue please',
    })

    useSessionStore.getState().setChatHistory('s1', baseMessages)

    const selected = useSessionStore.getState().sessions.s1
    expect(selected.chatHistory.map((message) => message.role === 'user' ? message.content : message.role)).toEqual([
      'initial request',
      'assistant',
      'continue please',
    ])
  })

  it('does not duplicate locally sent messages after parsed history catches up', () => {
    const now = Date.now()
    useSessionStore.getState().replaceAllSessions([
      session({ chatHistory: [] }),
    ])

    useSessionStore.getState().updateSession({
      type: 'user_message',
      sessionId: 's1',
      content: 'continue please',
    })

    useSessionStore.getState().setChatHistory('s1', [
      { role: 'user', content: 'continue please', timestamp: now + 1000 },
    ])

    const selected = useSessionStore.getState().sessions.s1
    expect(selected.chatHistory).toHaveLength(1)
    expect(selected.chatHistory[0]).toMatchObject({ role: 'user', content: 'continue please' })
  })

  it('timestamps backend user prompt changes', () => {
    const oldPromptAt = Date.now() - 10_000
    useSessionStore.getState().replaceAllSessions([
      session({ phase: 'processing', lastUserMessage: 'old prompt', lastUserMessageAt: oldPromptAt }),
    ])

    useSessionStore.getState().replaceAllSessions([
      session({ phase: 'processing', lastUserMessage: 'new prompt' }),
    ])

    const selected = useSessionStore.getState().sessions.s1
    expect(selected.lastUserMessage).toBe('new prompt')
    expect(selected.lastUserMessageAt).toBeGreaterThan(oldPromptAt)
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

  it('hides Codex internal prompt sessions even when Codex App metadata is present', () => {
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

    const state = useSessionStore.getState()
    expect(state.sessionList).toEqual([])
    expect(state.activeSessionId).toBeNull()
    expect(state.activeOverlay).toBeNull()
  })

  it('does not show completion overlays for Codex internal prompt sessions', () => {
    useSessionStore.getState().replaceAllSessions([
      session({
        agentType: 'codex',
        project: 'agentbro',
        terminal: 'Codex',
        termBundleId: 'com.openai.codex',
        phase: 'processing',
        sessionTitle: 'You are a helpful assistant. You will be presented with a user prompt, and your job is to help.',
        lastUserMessage: 'You are a helpful assistant. You will be presented with a user prompt, and your job is to help.',
        description: 'Processing user input',
      }),
    ])
    useSessionStore.getState().replaceAllSessions([
      session({
        agentType: 'codex',
        project: 'agentbro',
        terminal: 'Codex',
        termBundleId: 'com.openai.codex',
        phase: 'done',
        sessionTitle: 'You are a helpful assistant. You will be presented with a user prompt, and your job is to help.',
        lastUserMessage: 'You are a helpful assistant. You will be presented with a user prompt, and your job is to help.',
        description: 'Task completed',
        responseText: 'Task completed',
      }),
    ])

    const state = useSessionStore.getState()
    expect(state.sessionList).toEqual([])
    expect(state.overlayQueue).toEqual([])
    expect(state.activeOverlay).toBeNull()
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
        description: 'Processing user input: hi',
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

  it('hides Codex App title metadata placeholders and suppresses overlays', () => {
    useSessionStore.getState().replaceAllSessions([
      session({
        agentType: 'codex',
        project: 'agentbro',
        terminal: 'Codex',
        termBundleId: 'com.openai.codex',
        phase: 'idle',
        responseText: '{"title":"修复灵动岛乱码"}',
        description: '{"title":"修复灵动岛乱码"}',
      }),
    ])

    const state = useSessionStore.getState()
    expect(state.sessionList).toEqual([])
    expect(state.overlayQueue).toEqual([])
    expect(state.activeOverlay).toBeNull()
  })

  it('does not show response overlays for Codex title metadata on real sessions', () => {
    useSessionStore.getState().replaceAllSessions([
      session({
        agentType: 'codex',
        project: 'agentbro',
        terminal: 'Codex',
        termBundleId: 'com.openai.codex',
        phase: 'processing',
        sessionTitle: '修复灵动岛乱码',
        lastUserMessage: '帮我修一下灵动岛乱码',
      }),
    ])

    useSessionStore.getState().replaceAllSessions([
      session({
        agentType: 'codex',
        project: 'agentbro',
        terminal: 'Codex',
        termBundleId: 'com.openai.codex',
        phase: 'idle',
        sessionTitle: '修复灵动岛乱码',
        lastUserMessage: '帮我修一下灵动岛乱码',
        responseText: '{"title":"修复灵动岛乱码"}',
      }),
    ])

    expect(useSessionStore.getState().sessionList).toHaveLength(1)
    expect(useSessionStore.getState().overlayQueue).toEqual([])
  })

  it('hides internal probe sessions from the visible session list', () => {
    useSessionStore.getState().replaceAllSessions([
      session({
        agentType: 'claude-code',
        project: 'ClaudeProbe',
        terminal: 'Codex',
        termBundleId: 'com.openai.codex',
        phase: 'ready',
        sessionTitle: 'ClaudeProbe',
        lastUserMessage: 'health check',
        responseText: 'ok',
      }),
    ])

    expect(useSessionStore.getState().sessionList).toEqual([])
    expect(useSessionStore.getState().activeOverlay).toBeNull()
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

  it('clears pending permission overlays when a live terminal tool result arrives', async () => {
    useSessionStore.getState().updateSession({
      type: 'session_start',
      sessionId: 's1',
      project: 'project',
      terminal: 'iTerm',
      agentType: 'claude-code',
    })
    useSessionStore.getState().updateSession({
      type: 'permission_request',
      sessionId: 's1',
      toolName: 'Bash',
      toolInput: 'pnpm test',
    })
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(useSessionStore.getState().sessions.s1.pendingPermission).toBeDefined()
    expect(useSessionStore.getState().activeOverlay?.type).toBe('permission')

    useSessionStore.getState().updateSession({
      type: 'tool_use',
      sessionId: 's1',
      toolName: 'Bash',
      toolInput: 'pnpm test',
      status: 'error',
    })

    const state = useSessionStore.getState()
    expect(state.sessions.s1.pendingPermission).toBeUndefined()
    expect(state.sessions.s1.phase).toBe('processing')
    expect(state.overlayQueue.some((overlay) => overlay.type === 'permission')).toBe(false)
    expect(state.activeOverlay).toBeNull()
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

  it('does not refresh recovered ready sessions to the current time', () => {
    const now = Date.now()
    useConfigStore.setState({ idleTimeoutMinutes: 5 })
    useSessionStore.getState().replaceAllSessions([
      session({
        id: 'recovered',
        phase: 'ready',
        startedAt: Math.floor((now - 6 * 60 * 1000) / 1000),
        lastActivityAt: undefined,
        sessionTitle: 'Recovered transcript',
      }),
    ])

    useSessionStore.getState().applyIdleTimeout(now)

    const recovered = useSessionStore.getState().sessions.recovered
    expect(recovered.phase).toBe('idle')
    expect(recovered.lastActivityAt).toBeUndefined()
  })

  it('does not revive an idle session when backend repeats ready without new activity', () => {
    const now = Date.now()
    const old = now - 8 * 60 * 60 * 1000
    useConfigStore.setState({ idleTimeoutMinutes: 5, sessionTimeoutMinutes: 30 })
    const expired = session({
      id: 'stale',
      phase: 'idle',
      startedAt: old,
      idleSince: old,
      lastActivityAt: old,
      sessionTitle: 'Old transcript',
      description: 'No new activity',
    })
    useSessionStore.setState({
      sessions: { stale: expired },
      sessionList: [expired],
    })

    useSessionStore.getState().replaceAllSessions([
      session({
        id: 'stale',
        phase: 'ready',
        startedAt: old,
        lastActivityAt: undefined,
        idleSince: undefined,
        sessionTitle: 'Old transcript',
        description: 'No new activity',
      }),
    ])

    const stale = useSessionStore.getState().sessions.stale
    expect(stale.phase).toBe('idle')
    expect(stale.idleSince).toBe(old)
    expect(stale.lastActivityAt).toBe(old)
    expect(useSessionStore.getState().sessionList).toEqual([])
  })

  it('hides non-blocking sessions that match a custom directory silence rule', () => {
    useConfigStore.setState({
      sessionSilenceRules: [{
        id: 'rule-cwd',
        kind: 'cwd',
        pattern: '/tmp/noisy-project',
        enabled: true,
        createdAt: 1,
      }],
    })

    useSessionStore.getState().replaceAllSessions([
      session({ id: 'hidden', cwd: '/tmp/noisy-project/packages/app', project: 'noisy' }),
      session({ id: 'visible', cwd: '/tmp/active-project', project: 'active' }),
    ])

    expect(useSessionStore.getState().sessionList.map((item) => item.id)).toEqual(['visible'])
  })

  it('hides non-blocking sessions that match a prompt silence rule', () => {
    useConfigStore.setState({
      sessionSilenceRules: [{
        id: 'rule-prompt',
        kind: 'prompt',
        pattern: 'refresh generated docs',
        enabled: true,
        createdAt: 1,
      }],
    })

    useSessionStore.getState().replaceAllSessions([
      session({ id: 'hidden', lastUserMessage: 'Refresh generated docs for status page' }),
      session({ id: 'visible', lastUserMessage: 'Ship the island UI' }),
    ])

    expect(useSessionStore.getState().sessionList.map((item) => item.id)).toEqual(['visible'])
  })

  it('keeps blocking sessions visible even when they match a silence rule', () => {
    useConfigStore.setState({
      sessionSilenceRules: [{
        id: 'rule-cwd',
        kind: 'cwd',
        pattern: '/tmp/noisy-project',
        enabled: true,
        createdAt: 1,
      }],
    })

    useSessionStore.getState().replaceAllSessions([
      session({
        id: 'approval',
        cwd: '/tmp/noisy-project',
        phase: 'waiting_approval',
        pendingPermission: { toolName: 'Edit', toolInput: '{}' },
      }),
    ])

    expect(useSessionStore.getState().sessionList.map((item) => item.id)).toEqual(['approval'])
  })
})
