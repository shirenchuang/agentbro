import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { NotchPanel } from '../components/notch/NotchPanel'
import { useConfigStore } from '../stores/configStore'
import { useSessionStore } from '../stores/sessionStore'
import { useThemeStore } from '../stores/themeStore'
import type { OverlayItem, SessionState } from '../types/agent'
import { MATCH_NOTCH_HEIGHT } from '../utils/islandLayout'

const tauriMocks = vi.hoisted(() => ({
  getChatHistory: vi.fn(() => Promise.resolve([])),
  isTerminalFocused: vi.fn((sessionId?: string) => Promise.resolve(Boolean(sessionId && false))),
  jumpToTerminal: vi.fn(() => Promise.resolve()),
  respondPermission: vi.fn(() => Promise.resolve()),
  respondPlan: vi.fn(() => Promise.resolve()),
  respondQuestion: vi.fn(() => Promise.resolve()),
  sendMessage: vi.fn(() => Promise.resolve()),
  setNotchFocusable: vi.fn(() => Promise.resolve()),
  setNotchIgnoreCursorEvents: vi.fn((ignore: boolean) => {
    void ignore
    return Promise.resolve()
  }),
  isCursorOverNotch: vi.fn(() => Promise.resolve(false)),
  isTauri: vi.fn(() => false),
  resizeNotch: vi.fn(() => Promise.resolve({ anchorOffsetX: 0 })),
  startNotchDrag: vi.fn(() => Promise.resolve(true)),
  endNotchDrag: vi.fn(() => Promise.resolve(null)),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

vi.mock('../services/tauriApi', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../services/tauriApi')>()
  return {
    ...actual,
    getChatHistory: tauriMocks.getChatHistory,
    isTerminalFocused: tauriMocks.isTerminalFocused,
    jumpToTerminal: tauriMocks.jumpToTerminal,
    respondPermission: tauriMocks.respondPermission,
    respondPlan: tauriMocks.respondPlan,
    respondQuestion: tauriMocks.respondQuestion,
    sendMessage: tauriMocks.sendMessage,
    setNotchFocusable: tauriMocks.setNotchFocusable,
    setNotchIgnoreCursorEvents: tauriMocks.setNotchIgnoreCursorEvents,
    isCursorOverNotch: tauriMocks.isCursorOverNotch,
    isTauri: tauriMocks.isTauri,
    resizeNotch: tauriMocks.resizeNotch,
    startNotchDrag: tauriMocks.startNotchDrag,
    endNotchDrag: tauriMocks.endNotchDrag,
  }
})

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, fallback?: string | { defaultValue?: string }) => {
      if (typeof fallback === 'string') return fallback
      return fallback?.defaultValue ?? key
    },
  }),
}))

function session(overrides: Partial<SessionState> = {}): SessionState {
  return {
    id: 's1',
    agentType: 'claude-code',
    project: 'agent-island',
    terminal: 'iTerm',
    phase: 'processing',
    startedAt: Date.now() - 10_000,
    duration: 10_000,
    tokens: { input: 0, output: 0, cacheRead: 0, cacheCreate: 0 },
    chatHistory: [],
    subagents: [],
    activeTools: [],
    sessionTitle: 'Port dynamic island',
    description: 'Running parity checks',
    pid: 1234,
    ...overrides,
  }
}

function mountIsland(activeOverlay: OverlayItem | null = null, sessionOverrides: Partial<SessionState> = {}) {
  const currentSession = session(sessionOverrides)
  useSessionStore.setState({
    sessions: { [currentSession.id]: currentSession },
    sessionList: [currentSession],
    activeSessionId: currentSession.id,
    panelState: 'hover',
    activeOverlay,
    overlayQueue: activeOverlay ? [activeOverlay] : [],
    rateLimits: undefined,
    hookNotification: null,
    wakeSilencedUntil: 0,
    focusedTerminal: null,
  })

  render(<NotchPanel />)
}

function hostWidthVar(): string {
  return (document.querySelector('.notch-container') as HTMLElement).style.getPropertyValue('--notch-host-width')
}

function hitboxWidthVar(): string {
  return (document.querySelector('.notch-container') as HTMLElement).style.getPropertyValue('--notch-hitbox-width')
}

function deferred<T = void>() {
  let resolve!: (value: T | PromiseLike<T>) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

describe('NotchPanel island shell', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    tauriMocks.getChatHistory.mockResolvedValue([])
    tauriMocks.isTerminalFocused.mockResolvedValue(false)
    tauriMocks.jumpToTerminal.mockResolvedValue(undefined)
    tauriMocks.respondPermission.mockResolvedValue(undefined)
    tauriMocks.respondPlan.mockResolvedValue(undefined)
    tauriMocks.respondQuestion.mockResolvedValue(undefined)
    tauriMocks.sendMessage.mockResolvedValue(undefined)
    tauriMocks.setNotchFocusable.mockResolvedValue(undefined)
    tauriMocks.setNotchIgnoreCursorEvents.mockResolvedValue(undefined)
    tauriMocks.isCursorOverNotch.mockResolvedValue(false)
    tauriMocks.isTauri.mockReturnValue(false)
    tauriMocks.resizeNotch.mockResolvedValue({ anchorOffsetX: 0 })
    tauriMocks.startNotchDrag.mockResolvedValue(true)
    tauriMocks.endNotchDrag.mockResolvedValue(null)
    useThemeStore.getState().loadThemes([])
    useThemeStore.getState().setActiveTheme('default')
    useConfigStore.setState({
      allowHorizontalDrag: false,
      autoCollapse: false,
      autoHideNoSessions: false,
      clickToDetail: true,
      completionCardHeight: 120,
      detailPanelMaxHeight: 500,
      confettiEnabled: false,
      followFocus: false,
      hoverExpandDelay: 0,
      microHoverExpandDelay: 0,
      interactionMode: 'persistent',
      islandAnimationScale: 1,
      islandSurfaceMode: 'island',
      maxPanelHeight: 600,
      microPillWidth: 112,
      notchStyle: 'compact',
      panelMaxWidth: 630,
      pixelCursorEnabled: false,
      showCacheTTL: false,
      taskCompleteDwellSeconds: 3,
      tipsEnabled: false,
    })
    delete (window as typeof window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__
  })

  it('renders the expanded island shell with the active session', () => {
    mountIsland()

    expect(screen.getByRole('region', { name: 'AgentBro' })).toHaveAttribute('data-island-state', 'hover')
    expect(screen.getByText('agent-island · Port dynamic island')).toBeInTheDocument()
    expect(screen.getByText('Claude')).toBeInTheDocument()
  })

  it('focuses the notch window when hover opens the session list', async () => {
    const currentSession = session()
    useSessionStore.setState({
      sessions: { [currentSession.id]: currentSession },
      sessionList: [currentSession],
      activeSessionId: currentSession.id,
      panelState: 'collapsed',
      activeOverlay: null,
      overlayQueue: [],
      rateLimits: undefined,
      hookNotification: null,
      wakeSilencedUntil: 0,
      focusedTerminal: null,
    })

    render(<NotchPanel />)
    fireEvent.pointerEnter(screen.getByRole('region', { name: 'AgentBro' }).parentElement!)

    expect(tauriMocks.setNotchFocusable).toHaveBeenCalledWith(true)
    await waitFor(() => expect(useSessionStore.getState().panelState).toBe('hover'))
  })

  it('keeps a stable native host canvas while opening from micro', () => {
    vi.useFakeTimers()
    try {
      tauriMocks.resizeNotch.mockImplementation(() => new Promise(() => {}))
      const currentSession = session({ phase: 'idle' })
      useSessionStore.setState({
        sessions: { [currentSession.id]: currentSession },
        sessionList: [currentSession],
        activeSessionId: currentSession.id,
        panelState: 'collapsed',
        activeOverlay: null,
        overlayQueue: [],
        rateLimits: undefined,
        hookNotification: null,
        wakeSilencedUntil: 0,
        focusedTerminal: null,
      })

      render(<NotchPanel />)

      expect(hostWidthVar()).toBe('754px')
      expect(hitboxWidthVar()).toBe('140px')
      expect(tauriMocks.resizeNotch).toHaveBeenCalledTimes(1)

      fireEvent.pointerEnter(screen.getByRole('region', { name: 'AgentBro' }).parentElement!)

      act(() => {
        vi.advanceTimersByTime(120)
      })

      expect(useSessionStore.getState().panelState).toBe('hover')
      expect(hostWidthVar()).toBe('754px')
      expect(tauriMocks.resizeNotch).toHaveBeenCalledTimes(1)
    } finally {
      vi.useRealTimers()
    }
  })

  it('keeps a stable native host canvas while returning to micro', () => {
    vi.useFakeTimers()
    try {
      tauriMocks.resizeNotch.mockImplementation(() => new Promise(() => {}))
      useConfigStore.setState({ autoCollapse: true, collapseDelay: 1 })
      const currentSession = session({ phase: 'idle' })
      useSessionStore.setState({
        sessions: { [currentSession.id]: currentSession },
        sessionList: [currentSession],
        activeSessionId: currentSession.id,
        panelState: 'hover',
        activeOverlay: null,
        overlayQueue: [],
        rateLimits: undefined,
        hookNotification: null,
        wakeSilencedUntil: 0,
        focusedTerminal: null,
      })

      render(<NotchPanel />)

      expect(hostWidthVar()).toBe('754px')
      expect(tauriMocks.resizeNotch).toHaveBeenCalledTimes(1)

      fireEvent.pointerLeave(screen.getByRole('region', { name: 'AgentBro' }).parentElement!)
      act(() => {
        vi.advanceTimersByTime(1)
      })

      expect(useSessionStore.getState().panelState).toBe('collapsed')
      expect(hostWidthVar()).toBe('754px')

      act(() => {
        vi.advanceTimersByTime(520)
      })

      expect(hostWidthVar()).toBe('754px')
      expect(hitboxWidthVar()).toBe('140px')
      expect(tauriMocks.resizeNotch).toHaveBeenCalledTimes(1)
    } finally {
      vi.useRealTimers()
    }
  })

  it('uses only the visible micro pill for native hover passthrough', async () => {
    tauriMocks.isTauri.mockReturnValue(true)
    const currentSession = session({ phase: 'idle' })
    useSessionStore.setState({
      sessions: { [currentSession.id]: currentSession },
      sessionList: [currentSession],
      activeSessionId: currentSession.id,
      panelState: 'collapsed',
      activeOverlay: null,
      overlayQueue: [],
      rateLimits: undefined,
      hookNotification: null,
      wakeSilencedUntil: 0,
      focusedTerminal: null,
    })

    render(<NotchPanel />)

    expect(hitboxWidthVar()).toBe('140px')
    await waitFor(() => {
      expect(tauriMocks.isCursorOverNotch).toHaveBeenCalledWith(140, MATCH_NOTCH_HEIGHT)
    })
    await waitFor(() => {
      expect(tauriMocks.setNotchIgnoreCursorEvents).toHaveBeenCalledWith(true)
    })
    expect(useSessionStore.getState().panelState).toBe('collapsed')
  })

  it('forces native cursor events back on while the hover list is interactive', async () => {
    tauriMocks.isTauri.mockReturnValue(true)
    mountIsland(null, { phase: 'idle' })

    await waitFor(() => {
      expect(tauriMocks.setNotchIgnoreCursorEvents).toHaveBeenCalledWith(false)
    })
    expect(screen.getByText('agent-island · Port dynamic island')).toBeInTheDocument()
  })

  it('keeps the hidden minimal island hotspot available for native hover', async () => {
    tauriMocks.isTauri.mockReturnValue(true)
    tauriMocks.isCursorOverNotch.mockResolvedValue(true)
    useConfigStore.setState({ interactionMode: 'minimal' })
    const currentSession = session({ phase: 'processing' })
    useSessionStore.setState({
      sessions: { [currentSession.id]: currentSession },
      sessionList: [currentSession],
      activeSessionId: currentSession.id,
      panelState: 'collapsed',
      activeOverlay: null,
      overlayQueue: [],
      rateLimits: undefined,
      hookNotification: null,
      wakeSilencedUntil: 0,
      focusedTerminal: null,
    })

    render(<NotchPanel />)

    await waitFor(() => {
      expect(tauriMocks.setNotchIgnoreCursorEvents).toHaveBeenCalledWith(false)
    })
    await waitFor(() => {
      expect(useSessionStore.getState().panelState).toBe('hover')
    })
  })

  it('serializes native cursor passthrough so a stale collapsed request cannot disable hover', async () => {
    tauriMocks.isTauri.mockReturnValue(true)
    const pendingCollapsedPassthrough = deferred()
    tauriMocks.setNotchIgnoreCursorEvents
      .mockImplementationOnce(() => pendingCollapsedPassthrough.promise)
      .mockResolvedValue(undefined)

    const currentSession = session({ phase: 'idle' })
    useSessionStore.setState({
      sessions: { [currentSession.id]: currentSession },
      sessionList: [currentSession],
      activeSessionId: currentSession.id,
      panelState: 'collapsed',
      activeOverlay: null,
      overlayQueue: [],
      rateLimits: undefined,
      hookNotification: null,
      wakeSilencedUntil: 0,
      focusedTerminal: null,
    })

    render(<NotchPanel />)

    await waitFor(() => {
      expect(tauriMocks.setNotchIgnoreCursorEvents).toHaveBeenCalledWith(true)
    })

    act(() => {
      useSessionStore.getState().setPanelState('hover')
    })

    expect(tauriMocks.setNotchIgnoreCursorEvents).toHaveBeenCalledTimes(1)

    await act(async () => {
      pendingCollapsedPassthrough.resolve()
      await pendingCollapsedPassthrough.promise
    })

    await waitFor(() => {
      expect(tauriMocks.setNotchIgnoreCursorEvents).toHaveBeenCalledWith(false)
    })
    expect(tauriMocks.setNotchIgnoreCursorEvents.mock.calls.map(([ignore]) => ignore)).toEqual([true, false])
  })

  it('starts native repositioning from the expanded top drag handle', async () => {
    useConfigStore.setState({ allowHorizontalDrag: true })
    mountIsland(null, { phase: 'idle' })

    const dragHandle = screen.getByTestId('notch-drag-handle')
    fireEvent.pointerDown(dragHandle, { button: 0, pointerId: 7, clientX: 100, clientY: 2 })
    fireEvent.pointerMove(dragHandle, { pointerId: 7, clientX: 118, clientY: 2 })

    await waitFor(() => {
      expect(tauriMocks.startNotchDrag).toHaveBeenCalledWith(0, 754, 624, 'auto')
    })
    expect(screen.getByRole('region', { name: 'AgentBro' })).toHaveAttribute('data-dragging', 'true')
    expect(screen.getByText('agent-island · Port dynamic island')).toBeInTheDocument()

    fireEvent.pointerUp(dragHandle, { pointerId: 7 })

    await waitFor(() => {
      expect(tauriMocks.endNotchDrag).toHaveBeenCalled()
    })
  })

  it('keeps native interaction enabled when opening session detail', async () => {
    tauriMocks.isTauri.mockReturnValue(true)
    mountIsland()

    fireEvent.click(screen.getByText('agent-island · Port dynamic island'))

    await waitFor(() => expect(useSessionStore.getState().panelState).toBe('expanded'))
    expect(tauriMocks.setNotchIgnoreCursorEvents).toHaveBeenCalledWith(false)
    expect(tauriMocks.setNotchFocusable).toHaveBeenCalledWith(true)
  })

  it('uses Evolab-style progressive Escape: collapse first, then hide from compact', async () => {
    mountIsland()

    fireEvent.keyDown(document.body, { key: 'Escape' })

    expect(useSessionStore.getState().panelState).toBe('collapsed')
    expect(useSessionStore.getState().wakeSilencedUntil).toBe(0)
    expect(tauriMocks.setNotchFocusable).toHaveBeenCalledWith(false)

    fireEvent.keyDown(document.body, { key: 'Escape' })

    expect(useSessionStore.getState().wakeSilencedUntil).toBeGreaterThan(Date.now())
    await waitFor(() => {
      expect(screen.getByRole('region', { name: 'AgentBro' }).parentElement).toHaveAttribute(
        'data-island-hidden',
        'true',
      )
    })
  })

  it('steps Escape from detail back to the hover list without silencing wakeups', async () => {
    mountIsland()

    fireEvent.click(screen.getByText('agent-island · Port dynamic island'))
    await waitFor(() => expect(useSessionStore.getState().panelState).toBe('expanded'))

    fireEvent.keyDown(document.body, { key: 'Escape' })

    expect(useSessionStore.getState().panelState).toBe('hover')
    expect(useSessionStore.getState().wakeSilencedUntil).toBe(0)
    expect(screen.getByText('agent-island · Port dynamic island')).toBeInTheDocument()
  })

  it('renders task-completion feedback as an Evolab-style panel', () => {
    mountIsland({
      id: 'completion-s1',
      sessionId: 's1',
      type: 'completion',
      data: { summary: 'All island parity checks passed' },
      createdAt: Date.now(),
    })

    expect(screen.getByRole('region', { name: 'AgentBro' })).toHaveAttribute('data-island-state', 'feedback')
    expect(screen.getAllByText('All island parity checks passed').length).toBeGreaterThan(0)
    expect(screen.getByText('完成')).toBeInTheDocument()
    expect(screen.getByPlaceholderText('Send a message...')).toBeInTheDocument()
  })

  it('renders assistant-response feedback as an Evolab-style panel', () => {
    mountIsland({
      id: 'response-s1',
      sessionId: 's1',
      type: 'response',
      data: {
        responseText: 'Ready for the next integration step',
        userMessage: 'Can you continue the migration?',
      },
      createdAt: Date.now(),
    })

    expect(screen.getByRole('region', { name: 'AgentBro' })).toHaveAttribute('data-island-state', 'feedback')
    expect(screen.getAllByText('Can you continue the migration?').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Ready for the next integration step').length).toBeGreaterThan(0)
    expect(screen.getByRole('button', { name: 'notch.jumpToTerminal' })).toBeInTheDocument()
    expect(screen.getByPlaceholderText('Send a message...')).toBeInTheDocument()
  })

  it('renders assistant-response feedback as a popup while collapsed', () => {
    const activeOverlay: OverlayItem = {
      id: 'response-s1-collapsed',
      sessionId: 's1',
      type: 'response',
      data: {
        responseText: 'Collapsed response should still pop up',
        userMessage: 'Did it finish?',
      },
      createdAt: Date.now(),
    }
    const currentSession = session()
    useSessionStore.setState({
      sessions: { [currentSession.id]: currentSession },
      sessionList: [currentSession],
      activeSessionId: currentSession.id,
      panelState: 'collapsed',
      activeOverlay,
      overlayQueue: [activeOverlay],
      rateLimits: undefined,
      hookNotification: null,
      wakeSilencedUntil: 0,
      focusedTerminal: null,
    })

    render(<NotchPanel />)

    expect(screen.getByRole('region', { name: 'AgentBro' })).toHaveAttribute('data-island-state', 'feedback')
    expect(screen.getByRole('region', { name: 'AgentBro' })).toHaveAttribute('aria-expanded', 'true')
    expect(screen.getAllByText('Collapsed response should still pop up').length).toBeGreaterThan(0)
  })

  it('keeps the feedback countdown running while hovered and collapses as soon as hover leaves after expiry', () => {
    vi.useFakeTimers()
    try {
      useConfigStore.setState({ autoCollapse: true, taskCompleteDwellSeconds: 3 })
      const activeOverlay: OverlayItem = {
        id: 'response-s1-countdown',
        sessionId: 's1',
        type: 'response',
        data: {
          responseText: 'Countdown should not restart on hover',
          userMessage: 'Did it finish?',
        },
        createdAt: Date.now(),
      }
      const currentSession = session()
      useSessionStore.setState({
        sessions: { [currentSession.id]: currentSession },
        sessionList: [currentSession],
        activeSessionId: currentSession.id,
        panelState: 'collapsed',
        activeOverlay,
        overlayQueue: [activeOverlay],
        rateLimits: undefined,
        hookNotification: null,
        wakeSilencedUntil: 0,
        focusedTerminal: null,
      })

      render(<NotchPanel />)
      const hitbox = screen.getByRole('region', { name: 'AgentBro' }).parentElement!

      act(() => {
        vi.advanceTimersByTime(2_000)
      })
      fireEvent.pointerEnter(hitbox)
      act(() => {
        vi.advanceTimersByTime(1_100)
      })

      expect(useSessionStore.getState().activeOverlay?.id).toBe(activeOverlay.id)

      fireEvent.pointerLeave(hitbox)

      expect(useSessionStore.getState().activeOverlay).toBeNull()
      expect(useSessionStore.getState().panelState).toBe('collapsed')
    } finally {
      vi.useRealTimers()
    }
  })

  it('can open detail again after returning to the hover list without leaving the island', async () => {
    mountIsland()

    fireEvent.click(screen.getByText('agent-island · Port dynamic island'))

    await waitFor(() => expect(useSessionStore.getState().panelState).toBe('expanded'))
    await new Promise((resolve) => setTimeout(resolve, 600))
    tauriMocks.setNotchFocusable.mockClear()

    fireEvent.click(screen.getByRole('button', { name: 'notch.back' }))

    await waitFor(() => expect(useSessionStore.getState().panelState).toBe('hover'))

    fireEvent.click(await screen.findByText('agent-island · Port dynamic island'))

    await waitFor(() => expect(useSessionStore.getState().panelState).toBe('expanded'))
  })

  it('routes the hover-list jump arrow to the terminal command without opening detail', async () => {
    mountIsland()

    fireEvent.click(screen.getByRole('button', { name: 'notch.jumpToTerminal' }))

    expect(tauriMocks.jumpToTerminal).toHaveBeenCalledWith('s1')
    expect(tauriMocks.setNotchFocusable).toHaveBeenCalledWith(false)
    expect(useSessionStore.getState().panelState).toBe('hover')
  })

  it('routes the detail jump button and message input to terminal APIs', async () => {
    mountIsland()

    fireEvent.click(screen.getByText('agent-island · Port dynamic island'))
    await waitFor(() => expect(useSessionStore.getState().panelState).toBe('expanded'))

    await screen.findByRole('button', { name: 'notch.back' })
    fireEvent.click(document.querySelector('.chat-header__jump')!)
    expect(tauriMocks.jumpToTerminal).toHaveBeenCalledWith('s1')
    expect(tauriMocks.setNotchFocusable).toHaveBeenCalledWith(false)

    const input = await screen.findByPlaceholderText('notch.typeMessage')
    fireEvent.change(input, { target: { value: 'continue please' } })
    fireEvent.mouseDown(screen.getByRole('button', { name: 'notch.send' }))

    await waitFor(() => expect(tauriMocks.sendMessage).toHaveBeenCalledWith('s1', 'continue please'))
  })

  it('routes detail question input through the hook question responder', async () => {
    mountIsland(null, {
      phase: 'waiting_input',
      pendingQuestion: { question: 'Need confirmation', options: [] },
    })

    fireEvent.click(screen.getByText('agent-island · Port dynamic island'))
    await waitFor(() => expect(useSessionStore.getState().panelState).toBe('expanded'))

    const input = await screen.findByPlaceholderText('notch.typeReply')
    fireEvent.change(input, { target: { value: 'ok' } })
    fireEvent.keyDown(input, { key: 'Enter' })

    await waitFor(() => expect(tauriMocks.respondQuestion).toHaveBeenCalledWith('s1', 'ok'))
    expect(tauriMocks.sendMessage).not.toHaveBeenCalledWith('s1', 'ok')
    expect(useSessionStore.getState().sessions.s1.pendingQuestion).toBeUndefined()
  })

  it('routes permission overlay actions to permission responses and clears the pending request', () => {
    mountIsland({
      id: 'permission-s1',
      sessionId: 's1',
      type: 'permission',
      data: { toolName: 'Bash', toolInput: 'pnpm test' },
      createdAt: Date.now(),
    }, {
      phase: 'waiting_approval',
      pendingPermission: { toolName: 'Bash', toolInput: 'pnpm test' },
    })

    fireEvent.mouseDown(screen.getByText('notch.allowOnce'))

    expect(tauriMocks.respondPermission).toHaveBeenCalledWith('s1', true)
    expect(useSessionStore.getState().sessions.s1.pendingPermission).toBeUndefined()
  })

  it('renders fresh blocking permission as the primary island content', () => {
    mountIsland({
      id: 'permission-s1',
      sessionId: 's1',
      type: 'permission',
      data: { toolName: 'Bash', toolInput: '{"command":"pnpm test"}' },
      createdAt: Date.now(),
    }, {
      phase: 'waiting_approval',
      pendingPermission: { toolName: 'Bash', toolInput: '{"command":"pnpm test"}' },
    })

    expect(screen.getByRole('region', { name: 'AgentBro' })).toHaveAttribute('data-island-state', 'alert_permission')
    expect(document.querySelector('.notch-panel__alert-content')).toBeInTheDocument()
    expect(document.querySelector('.notch-panel__overlay')).not.toBeInTheDocument()
    expect(document.querySelector('.hover-list')).not.toBeInTheDocument()
    expect(screen.queryByText('agent-island · Port dynamic island')).not.toBeInTheDocument()

    const scrollRegion = document.querySelector('.perm-card__scroll')
    const actions = document.querySelector('.perm-card__actions')
    expect(scrollRegion).toBeInTheDocument()
    expect(actions).toBeInTheDocument()
    expect(scrollRegion).not.toContainElement(actions as HTMLElement)
  })

  it('renders collapsed permission inline in the hover session list', async () => {
    mountIsland({
      id: 'permission-s1',
      sessionId: 's1',
      type: 'permission',
      data: { toolName: 'Bash', toolInput: '{"command":"pnpm test"}' },
      createdAt: Date.now(),
    }, {
      phase: 'waiting_approval',
      pendingPermission: { toolName: 'Bash', toolInput: '{"command":"pnpm test"}' },
    })

    fireEvent.keyDown(document.body, { key: 'Escape' })
    expect(useSessionStore.getState().panelState).toBe('collapsed')

    fireEvent.pointerEnter(screen.getByRole('region', { name: 'AgentBro' }).parentElement!)
    await waitFor(() => expect(useSessionStore.getState().panelState).toBe('hover'))

    expect(screen.getByRole('region', { name: 'AgentBro' })).toHaveAttribute('data-island-state', 'hover')
    expect(document.querySelector('.notch-panel__alert-content')).not.toBeInTheDocument()
    expect(document.querySelector('.hover-list')).toBeInTheDocument()
    expect(screen.getByText('agent-island · Port dynamic island')).toBeInTheDocument()
    expect(document.querySelector('.hover-list__inline-perm')).toBeInTheDocument()
  })

  it('keeps suppressed blocking overlays queued without auto-expanding the island', async () => {
    const activeOverlay: OverlayItem = {
      id: 'permission-s1-suppressed',
      sessionId: 's1',
      type: 'permission',
      data: { toolName: 'Bash', toolInput: '{"command":"pnpm test"}' },
      createdAt: Date.now(),
      suppressed: true,
    }
    const currentSession = session({
      phase: 'waiting_approval',
      pendingPermission: { toolName: 'Bash', toolInput: '{"command":"pnpm test"}' },
    })
    useSessionStore.setState({
      sessions: { [currentSession.id]: currentSession },
      sessionList: [currentSession],
      activeSessionId: currentSession.id,
      panelState: 'collapsed',
      activeOverlay,
      overlayQueue: [activeOverlay],
      rateLimits: undefined,
      hookNotification: null,
      wakeSilencedUntil: 0,
      focusedTerminal: null,
    })

    render(<NotchPanel />)

    await waitFor(() => expect(useSessionStore.getState().panelState).toBe('collapsed'))
    expect(screen.getByRole('region', { name: 'AgentBro' })).toHaveAttribute('data-island-state', 'compact')
    expect(document.querySelector('.notch-panel__alert-content')).not.toBeInTheDocument()
  })

  it('collapses a permission alert on Escape without denying the request', () => {
    mountIsland({
      id: 'permission-s1',
      sessionId: 's1',
      type: 'permission',
      data: { toolName: 'Bash', toolInput: '{"command":"pnpm test"}' },
      createdAt: Date.now(),
    }, {
      phase: 'waiting_approval',
      pendingPermission: { toolName: 'Bash', toolInput: '{"command":"pnpm test"}' },
    })

    fireEvent.keyDown(document.body, { key: 'Escape' })

    expect(tauriMocks.respondPermission).not.toHaveBeenCalled()
    expect(useSessionStore.getState().sessions.s1.pendingPermission).toBeDefined()
    expect(useSessionStore.getState().panelState).toBe('collapsed')
    expect(screen.getByRole('region', { name: 'AgentBro' })).toHaveAttribute('data-island-state', 'compact')
  })

  it('shows the same permission request inline in the session list after the alert is collapsed', async () => {
    mountIsland({
      id: 'permission-s1',
      sessionId: 's1',
      type: 'permission',
      data: {
        toolName: 'Edit',
        toolInput: JSON.stringify({ file_path: 'src/i18n/locales/zh.json' }),
        diff: {
          filePath: 'src/i18n/locales/zh.json',
          lines: [
            { type: 'remove', lineNumber: 7, content: '"noSessionsHint": "在终端中启动 Claude Code。"' },
            { type: 'add', lineNumber: 7, content: '"noSessionsHint": "在终端中启动 AI Agent。"' },
          ],
        },
      },
      createdAt: Date.now(),
    }, {
      phase: 'waiting_approval',
      pendingPermission: {
        toolName: 'Edit',
        toolInput: JSON.stringify({ file_path: 'src/i18n/locales/zh.json' }),
        diff: {
          filePath: 'src/i18n/locales/zh.json',
          lines: [
            { type: 'remove', lineNumber: 7, content: '"noSessionsHint": "在终端中启动 Claude Code。"' },
            { type: 'add', lineNumber: 7, content: '"noSessionsHint": "在终端中启动 AI Agent。"' },
          ],
        },
      },
    })

    fireEvent.keyDown(document.body, { key: 'Escape' })
    fireEvent.pointerEnter(screen.getByRole('region', { name: 'AgentBro' }).parentElement!)

    await waitFor(() => expect(document.querySelector('.hover-list')).toBeInTheDocument())
    expect(document.querySelector('.notch-panel__alert-content')).not.toBeInTheDocument()
    expect(screen.getByText('agent-island · Port dynamic island')).toBeInTheDocument()
    expect(screen.getByText('Edit')).toBeInTheDocument()
    expect(screen.getByText('src/i18n/locales/zh.json')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '允许一次' })).toBeInTheDocument()
  })

  it('routes question overlay answers to respondQuestion and clears the pending question', () => {
    mountIsland({
      id: 'question-s1',
      sessionId: 's1',
      type: 'question',
      data: { question: 'Pick one', options: ['Ship it', 'Revise'] },
      createdAt: Date.now(),
    }, {
      phase: 'waiting_input',
      pendingQuestion: { question: 'Pick one', options: ['Ship it', 'Revise'] },
    })

    fireEvent.mouseDown(screen.getByText('Ship it').closest('.question-card__option-row')!)

    expect(tauriMocks.respondQuestion).toHaveBeenCalledWith('s1', 'Ship it')
    expect(useSessionStore.getState().sessions.s1.pendingQuestion).toBeUndefined()
  })

  it('routes multi-select question answers as a joined answer string', () => {
    mountIsland({
      id: 'question-multi-s1',
      sessionId: 's1',
      type: 'question',
      data: { question: 'Pick targets', options: ['Preview', 'Production', 'Docs'], multiSelect: true },
      createdAt: Date.now(),
    }, {
      phase: 'waiting_input',
      pendingQuestion: { question: 'Pick targets', options: ['Preview', 'Production', 'Docs'], multiSelect: true },
    })

    fireEvent.mouseDown(screen.getByText('Preview').closest('.question-card__option-row')!)
    fireEvent.mouseDown(screen.getByText('Docs').closest('.question-card__option-row')!)
    fireEvent.mouseDown(screen.getByRole('button', { name: 'Confirm (2)' }))

    expect(tauriMocks.respondQuestion).toHaveBeenCalledWith('s1', 'Preview, Docs')
    expect(useSessionStore.getState().sessions.s1.pendingQuestion).toBeUndefined()
  })

  it('routes multi-question answers as JSON for the AskUserQuestion hook bridge', () => {
    mountIsland({
      id: 'question-batch-s1',
      sessionId: 's1',
      type: 'question',
      data: {
        question: '[Deploy] Choose options',
        options: ['Preview', 'Ship'],
        questions: [
          {
            header: 'Deploy',
            question: 'Which target?',
            options: [
              { label: 'Preview', description: 'Open staging' },
              { label: 'Ship', description: 'Release now' },
            ],
            multiSelect: true,
          },
          {
            question: 'Notify channel?',
            options: [{ label: 'Yes' }, { label: 'No' }],
          },
        ],
      },
      createdAt: Date.now(),
    }, {
      phase: 'waiting_input',
      pendingQuestion: {
        question: '[Deploy] Choose options',
        options: ['Preview', 'Ship'],
        questions: [
          {
            header: 'Deploy',
            question: 'Which target?',
            options: [
              { label: 'Preview', description: 'Open staging' },
              { label: 'Ship', description: 'Release now' },
            ],
            multiSelect: true,
          },
          {
            question: 'Notify channel?',
            options: [{ label: 'Yes' }, { label: 'No' }],
          },
        ],
      },
    })

    fireEvent.mouseDown(screen.getByRole('button', { name: 'Preview' }))
    fireEvent.mouseDown(screen.getByRole('button', { name: 'Ship' }))
    fireEvent.mouseDown(screen.getByRole('button', { name: 'No' }))
    fireEvent.mouseDown(screen.getByRole('button', { name: '✓ Submit All' }))

    expect(tauriMocks.respondQuestion).toHaveBeenCalledWith(
      's1',
      JSON.stringify({
        'Which target?': 'Preview, Ship',
        'Notify channel?': 'No',
      }),
    )
    expect(useSessionStore.getState().sessions.s1.pendingQuestion).toBeUndefined()
  })

  it('routes plan overlay decisions to respondPlan and dismisses the overlay', () => {
    mountIsland({
      id: 'plan-s1',
      sessionId: 's1',
      type: 'plan',
      data: {
        planTitle: 'Implementation plan',
        planContent: '# Fix jump\n\nContext\n\n1. Fix jump',
        requestedPermissions: [{ tool: 'Bash', prompt: 'run tests' }],
      },
      createdAt: Date.now(),
    }, {
      planTitle: 'Implementation plan',
      planContent: '# Fix jump\n\nContext\n\n1. Fix jump',
      sessionTitle: 'Fix jump flow',
      subagents: [{
        agentId: 'agent-1',
        agentType: 'Explore',
        description: 'inspect jump flow',
        startedAt: Date.now() - 2_000,
        completedAt: Date.now() - 1_000,
        status: 'completed',
        tools: ['Read'],
        lastAssistantMessage: 'jump path is wired',
      }],
    })

    expect(document.querySelector('.overlay-ctx__row1')?.textContent).toContain('agent-island·Fix jump flow·Claude')
    expect(screen.getByText('Subagents (1)')).toBeInTheDocument()
    expect(screen.getByText('Explore')).toBeInTheDocument()
    expect(screen.getByText('完成')).toBeInTheDocument()
    expect(document.querySelector('.plan-approval__content')?.textContent).toContain('Fix jump')
    expect(document.querySelector('.plan-approval__perms')?.textContent).toContain('Bash')

    fireEvent.click(screen.getByText('Accept Edits'))

    expect(tauriMocks.respondPlan).toHaveBeenCalledWith('s1', 'acceptEdits')
    expect(useSessionStore.getState().activeOverlay).toBeNull()
    expect(useSessionStore.getState().sessions.s1.planContent).toBeUndefined()
    expect(useSessionStore.getState().sessions.s1.phase).toBe('processing')
  })

  it('routes plan overlay manual review and auto approval actions', () => {
    mountIsland({
      id: 'plan-s1-manual',
      sessionId: 's1',
      type: 'plan',
      data: { planTitle: 'Implementation plan', planContent: '1. Fix jump' },
      createdAt: Date.now(),
    }, {
      planTitle: 'Implementation plan',
      planContent: '1. Fix jump',
    })

    fireEvent.click(screen.getByText('Manual Review'))
    expect(tauriMocks.respondPlan).toHaveBeenCalledWith('s1', 'manual')

    cleanup()
    mountIsland({
      id: 'plan-s1-auto',
      sessionId: 's1',
      type: 'plan',
      data: { planTitle: 'Implementation plan', planContent: '1. Fix jump' },
      createdAt: Date.now(),
    }, {
      planTitle: 'Implementation plan',
      planContent: '1. Fix jump',
    })

    fireEvent.click(screen.getByText('Auto'))
    expect(tauriMocks.respondPlan).toHaveBeenCalledWith('s1', 'bypassPermissions')
  })

  it('routes response overlay jump and reply actions to terminal APIs', async () => {
    mountIsland({
      id: 'response-s1',
      sessionId: 's1',
      type: 'response',
      data: {
        responseText: 'Ready for the next step',
        userMessage: 'Continue?',
      },
      createdAt: Date.now(),
    })

    expect(document.querySelector('.overlay-feedback__message--user')?.textContent).toContain('Continue?')
    expect(document.querySelector('.overlay-feedback__message--assistant')?.textContent).toContain('Ready for the next step')

    fireEvent.mouseDown(document.querySelector('.overlay-feedback__detail')!)

    expect(tauriMocks.jumpToTerminal).toHaveBeenCalledWith('s1')
    expect(useSessionStore.getState().activeOverlay).toBeNull()

    cleanup()
    mountIsland({
      id: 'response-s1-reply',
      sessionId: 's1',
      type: 'response',
      data: {
        responseText: 'Another answer',
        userMessage: 'Continue again?',
      },
      createdAt: Date.now(),
    })

    const replyInput = screen.getByPlaceholderText('Send a message...')
    tauriMocks.setNotchFocusable.mockClear()
    expect(fireEvent.mouseDown(replyInput)).toBe(false)
    expect(tauriMocks.setNotchFocusable).toHaveBeenCalledWith(true)

    fireEvent.change(replyInput, {
      target: { value: 'thanks, keep going' },
    })
    fireEvent.mouseDown(screen.getByRole('button', { name: 'Send' }))

    await waitFor(() => expect(tauriMocks.sendMessage).toHaveBeenCalledWith('s1', 'thanks, keep going'))
  })

  it('auto-collapses after the cursor leaves the detail view', async () => {
    useConfigStore.setState({ autoCollapse: true, collapseDelay: 1 })
    mountIsland()

    fireEvent.click(screen.getByText('agent-island · Port dynamic island'))
    await waitFor(() => expect(useSessionStore.getState().panelState).toBe('expanded'))

    const now = Date.now()
    const nowSpy = vi.spyOn(Date, 'now').mockReturnValue(now + 2_000)
    try {
      fireEvent.pointerLeave(screen.getByRole('region', { name: 'AgentBro' }).parentElement!)

      await waitFor(() => expect(useSessionStore.getState().panelState).toBe('collapsed'))
      expect(tauriMocks.setNotchFocusable).toHaveBeenCalledWith(false)
    } finally {
      nowSpy.mockRestore()
    }
  })

  it('dismisses feedback overlays when the feedback body jumps to the terminal', () => {
    mountIsland({
      id: 'completion-s1',
      sessionId: 's1',
      type: 'completion',
      data: { summary: 'Dismiss me from outside' },
      createdAt: Date.now(),
    })

    fireEvent.mouseDown(document.querySelector('.overlay-feedback__detail')!)

    expect(tauriMocks.jumpToTerminal).toHaveBeenCalledWith('s1')
    expect(useSessionStore.getState().activeOverlay).toBeNull()
    expect(useSessionStore.getState().overlayQueue).toEqual([])
  })

  it('keeps the feedback countdown running while hovered', () => {
    vi.useFakeTimers()
    try {
      mountIsland({
        id: 'completion-s1',
        sessionId: 's1',
        type: 'completion',
        data: { summary: 'Auto dismiss me' },
        createdAt: Date.now(),
      })

      const hitbox = screen.getByRole('region', { name: 'AgentBro' }).parentElement!
      fireEvent.pointerEnter(hitbox)
      act(() => {
        vi.advanceTimersByTime(2_000)
      })

      expect(useSessionStore.getState().activeOverlay?.id).toBe('completion-s1')

      fireEvent.mouseLeave(document.querySelector('.overlay-feedback')!)
      act(() => {
        vi.advanceTimersByTime(999)
      })

      expect(useSessionStore.getState().activeOverlay?.id).toBe('completion-s1')

      act(() => {
        vi.advanceTimersByTime(1)
      })

      expect(useSessionStore.getState().activeOverlay).toBeNull()
      expect(useSessionStore.getState().panelState).toBe('collapsed')
    } finally {
      vi.useRealTimers()
    }
  })

  it('defers collapsed feedback presentation dismissal until the cursor leaves', () => {
    vi.useFakeTimers()
    try {
      const activeOverlay: OverlayItem = {
        id: 'completion-s1-collapsed',
        sessionId: 's1',
        type: 'completion',
        data: { summary: 'Stay open while hovered' },
        createdAt: Date.now(),
      }
      const currentSession = session()
      useSessionStore.setState({
        sessions: { [currentSession.id]: currentSession },
        sessionList: [currentSession],
        activeSessionId: currentSession.id,
        panelState: 'collapsed',
        activeOverlay,
        overlayQueue: [activeOverlay],
        rateLimits: undefined,
        hookNotification: null,
        wakeSilencedUntil: 0,
        focusedTerminal: null,
      })

      render(<NotchPanel />)

      fireEvent.mouseEnter(document.querySelector('.overlay-feedback')!)
      act(() => {
        vi.advanceTimersByTime(3_000)
      })

      expect(useSessionStore.getState().activeOverlay?.id).toBe('completion-s1-collapsed')

      fireEvent.mouseLeave(document.querySelector('.overlay-feedback')!)

      expect(useSessionStore.getState().activeOverlay).toBeNull()
    } finally {
      vi.useRealTimers()
    }
  })

  it('focuses the notch when the cursor enters an already visible notification', async () => {
    const activeOverlay: OverlayItem = {
      id: 'completion-s1-collapsed',
      sessionId: 's1',
      type: 'completion',
      data: { summary: 'Do not steal focus' },
      createdAt: Date.now(),
    }
    const currentSession = session()
    useSessionStore.setState({
      sessions: { [currentSession.id]: currentSession },
      sessionList: [currentSession],
      activeSessionId: currentSession.id,
      panelState: 'collapsed',
      activeOverlay,
      overlayQueue: [activeOverlay],
      rateLimits: undefined,
      hookNotification: null,
      wakeSilencedUntil: 0,
      focusedTerminal: null,
    })

    render(<NotchPanel />)
    tauriMocks.setNotchFocusable.mockClear()

    fireEvent.pointerEnter(screen.getByRole('region', { name: 'AgentBro' }).parentElement!)

    expect(tauriMocks.setNotchFocusable).toHaveBeenCalledWith(true)
    await waitFor(() => expect(useSessionStore.getState().panelState).toBe('hover'))
  })

  it('does not let native notification hover probing steal focus', async () => {
    tauriMocks.isTauri.mockReturnValue(true)
    tauriMocks.isCursorOverNotch.mockResolvedValue(true)
    const activeOverlay: OverlayItem = {
      id: 'completion-s1-native',
      sessionId: 's1',
      type: 'completion',
      data: { summary: 'Native probe should not focus' },
      createdAt: Date.now(),
    }
    const currentSession = session()
    useSessionStore.setState({
      sessions: { [currentSession.id]: currentSession },
      sessionList: [currentSession],
      activeSessionId: currentSession.id,
      panelState: 'collapsed',
      activeOverlay,
      overlayQueue: [activeOverlay],
      rateLimits: undefined,
      hookNotification: null,
      wakeSilencedUntil: 0,
      focusedTerminal: null,
    })

    render(<NotchPanel />)
    tauriMocks.setNotchFocusable.mockClear()

    await waitFor(() => {
      expect(tauriMocks.isCursorOverNotch).toHaveBeenCalled()
    })

    expect(tauriMocks.setNotchFocusable).not.toHaveBeenCalledWith(true)
    expect(useSessionStore.getState().panelState).toBe('collapsed')
  })

  it('filters the session list to focused terminal sessions when follow focus is enabled', async () => {
    useConfigStore.setState({ followFocus: true })
    tauriMocks.isTerminalFocused.mockImplementation((sessionId?: string) => Promise.resolve(sessionId === 's2'))
    const focused = session({ id: 's2', sessionTitle: 'Focused terminal', project: 'focused', pid: 2222, terminal: 'iTerm' })
    const background = session({ id: 's1', sessionTitle: 'Background terminal', project: 'background', pid: 1111, terminal: 'Terminal' })
    useSessionStore.setState({
      sessions: { s1: background, s2: focused },
      sessionList: [background, focused],
      activeSessionId: 's2',
      panelState: 'hover',
      activeOverlay: null,
      overlayQueue: [],
      rateLimits: undefined,
      hookNotification: null,
      wakeSilencedUntil: 0,
      focusedTerminal: null,
    })

    render(<NotchPanel />)

    await waitFor(() => expect(screen.queryByText('background · Background terminal')).not.toBeInTheDocument())
    expect(screen.getByText('focused · Focused terminal')).toBeInTheDocument()
  })
})
