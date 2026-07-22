import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { NotchPanel } from '../components/notch/NotchPanel'
import { useConfigStore } from '../stores/configStore'
import { useSessionStore } from '../stores/sessionStore'
import { useThemeStore } from '../stores/themeStore'
import type { OverlayItem, SessionState } from '../types/agent'
import { MATCH_NOTCH_HEIGHT } from '../utils/islandLayout'
import { isApplePlatform } from '../utils/platform'

const tauriMocks = vi.hoisted(() => ({
  getChatHistory: vi.fn(() => Promise.resolve([])),
  getChatHistoryTail: vi.fn(() => Promise.resolve({ messages: [], hasMore: false, firstMessageId: null, totalCount: 0, transcriptPath: null })),
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

function primaryModifierKeyEvent() {
  return isApplePlatform() ? { metaKey: true } : { ctrlKey: true }
}

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

vi.mock('../services/tauriApi', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../services/tauriApi')>()
  return {
    ...actual,
    getChatHistory: tauriMocks.getChatHistory,
    getChatHistoryTail: tauriMocks.getChatHistoryTail,
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
    project: 'agentbro',
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
      panelHorizontalOffset: 0,
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
    expect(screen.getByText('agentbro · Port dynamic island')).toBeInTheDocument()
    expect(screen.getByText('Claude')).toBeInTheDocument()
  })

  it('collapses the island before opening settings so the transparent host cannot block it', () => {
    mountIsland()

    fireEvent.click(screen.getByTitle('notch.settings'))

    expect(useSessionStore.getState().panelState).toBe('collapsed')
    expect(tauriMocks.setNotchFocusable).toHaveBeenCalledWith(false)
  })

  it('does not render the pet companion inside the notch window in pet mode', () => {
    useConfigStore.setState({ islandSurfaceMode: 'pet' })
    mountIsland()

    expect(document.querySelector('.pet-surface')).not.toBeInTheDocument()
    expect(document.querySelector('.notch-hitbox')).toHaveAttribute('data-island-hidden', 'true')
    expect(document.querySelector('.notch-panel')).toHaveStyle({ display: 'none' })
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

  it('opens the collapsed island immediately on pointer down even when hover is delayed', async () => {
    vi.useFakeTimers()
    try {
      useConfigStore.setState({ hoverExpandDelay: 1000 })
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

      fireEvent.pointerDown(screen.getByRole('region', { name: 'AgentBro' }).parentElement!, { button: 0 })

      expect(tauriMocks.setNotchFocusable).toHaveBeenCalledWith(true)
      act(() => {
        vi.advanceTimersByTime(120)
      })

      expect(useSessionStore.getState().panelState).toBe('hover')
    } finally {
      vi.useRealTimers()
    }
  })

  it('keeps a stable native host canvas while collapsed, then sizes to hitbox on hover', () => {
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

      expect(hostWidthVar()).toBe('754px')
      expect(tauriMocks.resizeNotch).toHaveBeenCalledTimes(1)

      act(() => {
        vi.advanceTimersByTime(120)
      })

      expect(useSessionStore.getState().panelState).toBe('hover')
      expect(hostWidthVar()).toBe('686px')
      expect(tauriMocks.resizeNotch).toHaveBeenCalledTimes(2)
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

      expect(hostWidthVar()).toBe('686px')
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
      expect(tauriMocks.resizeNotch).toHaveBeenCalledTimes(2)
    } finally {
      vi.useRealTimers()
    }
  })

  it('keeps the visible collapsed pill inside a passthrough stable native host', async () => {
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
    expect(hostWidthVar()).toBe('754px')
    await waitFor(() => {
      expect(tauriMocks.isCursorOverNotch).toHaveBeenCalledWith(140, MATCH_NOTCH_HEIGHT, 0)
    })
    await waitFor(() => {
      expect(tauriMocks.setNotchIgnoreCursorEvents).toHaveBeenCalledWith(true)
    })
    expect(useSessionStore.getState().panelState).toBe('collapsed')
  })

  it('aligns native hover probing with the collapsed shell anchor offset', async () => {
    tauriMocks.isTauri.mockReturnValue(true)
    tauriMocks.resizeNotch.mockResolvedValue({ anchorOffsetX: 36 })
    useConfigStore.setState({ allowHorizontalDrag: true, panelHorizontalOffset: 480 })
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
      expect(tauriMocks.resizeNotch).toHaveBeenCalled()
    })
    await waitFor(() => {
      expect(tauriMocks.isCursorOverNotch).toHaveBeenCalledWith(140, MATCH_NOTCH_HEIGHT, 36)
    })
  })

  it('forces native cursor events back on while the hover list is interactive', async () => {
    tauriMocks.isTauri.mockReturnValue(true)
    mountIsland(null, { phase: 'idle' })

    await waitFor(() => {
      expect(tauriMocks.setNotchIgnoreCursorEvents).toHaveBeenCalledWith(false)
    })
    expect(screen.getByText('agentbro · Port dynamic island')).toBeInTheDocument()
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

  it('serializes native cursor passthrough so a stale hidden request cannot disable hover', async () => {
    tauriMocks.isTauri.mockReturnValue(true)
    const pendingCollapsedPassthrough = deferred()
    tauriMocks.setNotchIgnoreCursorEvents
      .mockImplementationOnce(() => pendingCollapsedPassthrough.promise)
      .mockResolvedValue(undefined)

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

  it('cancels delayed collapsed cursor passthrough when the hover list opens first', async () => {
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

    await waitFor(() => {
      expect(tauriMocks.isCursorOverNotch).toHaveBeenCalled()
    })
    expect(tauriMocks.setNotchIgnoreCursorEvents).not.toHaveBeenCalledWith(true)

    act(() => {
      useSessionStore.getState().setPanelState('hover')
    })

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 150))
    })

    expect(tauriMocks.setNotchIgnoreCursorEvents).not.toHaveBeenCalledWith(true)
    expect(tauriMocks.setNotchIgnoreCursorEvents).toHaveBeenCalledWith(false)
  })

  it('starts native repositioning from the expanded top drag handle', async () => {
    useConfigStore.setState({ allowHorizontalDrag: true })
    mountIsland(null, { phase: 'idle' })

    const dragHandle = screen.getByTestId('notch-drag-handle')
    fireEvent.pointerDown(dragHandle, { button: 0, pointerId: 7, clientX: 100, clientY: 2 })
    fireEvent.pointerMove(dragHandle, { pointerId: 7, clientX: 118, clientY: 2 })

    await waitFor(() => {
      expect(tauriMocks.startNotchDrag).toHaveBeenCalledWith(0, 686, 192, 'auto')
    })
    expect(screen.getByRole('region', { name: 'AgentBro' })).toHaveAttribute('data-dragging', 'true')
    expect(screen.getByText('agentbro · Port dynamic island')).toBeInTheDocument()

    fireEvent.pointerUp(dragHandle, { pointerId: 7 })

    await waitFor(() => {
      expect(tauriMocks.endNotchDrag).toHaveBeenCalled()
    })
  })

  it('keeps native interaction enabled when opening session detail', async () => {
    tauriMocks.isTauri.mockReturnValue(true)
    mountIsland()

    fireEvent.click(screen.getByText('agentbro · Port dynamic island'))

    await waitFor(() => expect(useSessionStore.getState().panelState).toBe('expanded'))
    expect(tauriMocks.setNotchIgnoreCursorEvents).toHaveBeenCalledWith(false)
    expect(tauriMocks.setNotchFocusable).toHaveBeenCalledWith(true)
  })

  it('does not auto-collapse while the detail input has draft text', async () => {
    useConfigStore.setState({ autoCollapse: true, collapseDelay: 1 })
    mountIsland(null, { phase: 'idle' })

    fireEvent.click(screen.getByText('agentbro · Port dynamic island'))
    await waitFor(() => expect(useSessionStore.getState().panelState).toBe('expanded'))

    const input = await screen.findByRole('textbox') as HTMLInputElement
    fireEvent.change(input, { target: { value: 'keep this draft' } })
    expect(input).toHaveAttribute('data-has-draft', 'true')

    fireEvent.pointerLeave(screen.getByRole('region', { name: 'AgentBro' }).parentElement!)
    await new Promise((resolve) => setTimeout(resolve, 20))
    expect(useSessionStore.getState().panelState).toBe('expanded')
    await new Promise((resolve) => setTimeout(resolve, 1200))

    fireEvent.change(input, { target: { value: '' } })
    expect(input).toHaveAttribute('data-has-draft', 'false')
    fireEvent.pointerLeave(screen.getByRole('region', { name: 'AgentBro' }).parentElement!)

    await waitFor(() => expect(useSessionStore.getState().panelState).toBe('collapsed'))
  })

  it('uses AgentBro-style progressive Escape: collapse first, then hide from compact', async () => {
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

    fireEvent.click(screen.getByText('agentbro · Port dynamic island'))
    await waitFor(() => expect(useSessionStore.getState().panelState).toBe('expanded'))

    fireEvent.keyDown(document.body, { key: 'Escape' })

    expect(useSessionStore.getState().panelState).toBe('hover')
    expect(useSessionStore.getState().wakeSilencedUntil).toBe(0)
    expect(screen.getByText('agentbro · Port dynamic island')).toBeInTheDocument()
  })

  it('renders task-completion feedback as an AgentBro-style panel', () => {
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

  it('renders assistant-response feedback as an AgentBro-style panel', () => {
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
    expect(screen.getByText('New reply')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'notch.jumpToTerminal' })).toBeInTheDocument()
    expect(screen.getByPlaceholderText('Send a message...')).toBeInTheDocument()
  })

  it('dismisses feedback from the close button without jumping to terminal', () => {
    mountIsland({
      id: 'response-s1-close',
      sessionId: 's1',
      type: 'response',
      data: {
        responseText: 'Close this feedback panel',
        userMessage: 'Did it finish?',
      },
      createdAt: Date.now(),
    })

    fireEvent.mouseDown(screen.getByRole('button', { name: 'Dismiss' }))

    expect(tauriMocks.jumpToTerminal).not.toHaveBeenCalled()
    expect(useSessionStore.getState().activeOverlay).toBeNull()
    expect(useSessionStore.getState().panelState).toBe('collapsed')
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

  it('focuses a collapsed feedback popup when native hover enters it', async () => {
    tauriMocks.isTauri.mockReturnValue(true)
    tauriMocks.isCursorOverNotch.mockResolvedValue(true)
    const activeOverlay: OverlayItem = {
      id: 'response-s1-native-hover',
      sessionId: 's1',
      type: 'response',
      data: {
        responseText: 'Native hover should make this reply box interactive',
        userMessage: 'Can I reply here?',
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

    await waitFor(() => {
      expect(tauriMocks.setNotchIgnoreCursorEvents).toHaveBeenCalledWith(false)
      expect(tauriMocks.setNotchFocusable).toHaveBeenCalledWith(true)
    })
    expect(useSessionStore.getState().panelState).toBe('collapsed')
    expect(screen.getByPlaceholderText('Send a message...')).toBeInTheDocument()
  })

  it('keeps collapsed feedback popups interactive without delayed cursor passthrough', async () => {
    tauriMocks.isTauri.mockReturnValue(true)
    tauriMocks.isCursorOverNotch.mockResolvedValue(false)
    const activeOverlay: OverlayItem = {
      id: 'response-s1-no-passthrough',
      sessionId: 's1',
      type: 'response',
      data: {
        responseText: 'Visible feedback should stay clickable',
        userMessage: 'Can I click it?',
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

    await waitFor(() => {
      expect(tauriMocks.isCursorOverNotch).toHaveBeenCalled()
    })
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 260))
    })

    expect(tauriMocks.setNotchIgnoreCursorEvents).toHaveBeenCalledWith(false)
    expect(tauriMocks.setNotchIgnoreCursorEvents).not.toHaveBeenCalledWith(true)
    expect(screen.getByPlaceholderText('Send a message...')).toBeInTheDocument()
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

    fireEvent.click(screen.getByText('agentbro · Port dynamic island'))

    await waitFor(() => expect(useSessionStore.getState().panelState).toBe('expanded'))
    await new Promise((resolve) => setTimeout(resolve, 600))
    tauriMocks.setNotchFocusable.mockClear()

    fireEvent.click(screen.getByRole('button', { name: 'notch.back' }))

    await waitFor(() => expect(useSessionStore.getState().panelState).toBe('hover'))

    fireEvent.click(await screen.findByText('agentbro · Port dynamic island'))

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

    fireEvent.click(screen.getByText('agentbro · Port dynamic island'))
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

    fireEvent.click(screen.getByText('agentbro · Port dynamic island'))
    await waitFor(() => expect(useSessionStore.getState().panelState).toBe('expanded'))

    const input = await screen.findByPlaceholderText('notch.typeReply')
    fireEvent.change(input, { target: { value: 'ok' } })
    fireEvent.keyDown(input, { key: 'Enter' })

    await waitFor(() => expect(tauriMocks.respondQuestion).toHaveBeenCalledWith('s1', 'ok'))
    expect(tauriMocks.sendMessage).not.toHaveBeenCalledWith('s1', 'ok')
    expect(useSessionStore.getState().sessions.s1.pendingQuestion).toBeUndefined()
  })

  it('routes detail approval bar actions on mouse down', async () => {
    mountIsland(null, {
      phase: 'waiting_approval',
      pendingPermission: { toolName: 'Bash', toolInput: '{"command":"pnpm test"}' },
    })

    act(() => {
      useSessionStore.getState().setPanelState('expanded')
    })
    await waitFor(() => expect(document.querySelector('.chat-view')).toBeInTheDocument())

    const allowButton = document.querySelector('.approval-bar__btn--allow') as HTMLElement
    expect(allowButton).toBeInTheDocument()
    fireEvent.mouseDown(allowButton)

    expect(tauriMocks.respondPermission).toHaveBeenCalledWith('s1', true, false)
    await waitFor(() => {
      expect(useSessionStore.getState().sessions.s1.pendingPermission).toBeUndefined()
    })
  })

  it('routes permission overlay actions to permission responses and clears the pending request', async () => {
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

    const allowButton = document.querySelector('.perm-card__btn--allow') as HTMLElement
    expect(allowButton).toBeInTheDocument()
    fireEvent.click(allowButton)

    expect(tauriMocks.respondPermission).toHaveBeenCalledWith('s1', true)
    await waitFor(() => {
      expect(useSessionStore.getState().sessions.s1.pendingPermission).toBeUndefined()
    })
  })

  it('opens the island when backend snapshots introduce a permission request', async () => {
    tauriMocks.isTauri.mockReturnValue(true)
    useSessionStore.setState({
      sessions: {},
      sessionList: [],
      activeSessionId: null,
      panelState: 'collapsed',
      activeOverlay: null,
      overlayQueue: [],
      rateLimits: undefined,
      hookNotification: null,
      wakeSilencedUntil: 0,
      focusedTerminal: null,
    })

    render(<NotchPanel />)

    act(() => {
      useSessionStore.getState().replaceAllSessions([
        session({
          id: 'backend-permission',
          phase: 'waiting_approval',
          pendingPermission: {
            toolName: 'Bash',
            toolInput: '{"command":"echo smoke"}',
          },
        }),
      ])
    })

    await waitFor(() => {
      expect(useSessionStore.getState().activeOverlay?.type).toBe('permission')
    })
    await waitFor(() => {
      expect(useSessionStore.getState().panelState).toBe('hover')
    })
    await waitFor(() => {
      expect(tauriMocks.resizeNotch).toHaveBeenCalledWith(
        expect.any(Number),
        expect.any(Number),
        expect.any(Number),
        expect.anything(),
      )
    })
  })

  it('ignores legacy single-letter permission shortcuts', () => {
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

    fireEvent.keyDown(document.body, { key: 'y' })
    fireEvent.keyDown(document.body, { key: 'A' })
    fireEvent.keyDown(document.body, { key: 'n' })

    expect(tauriMocks.respondPermission).not.toHaveBeenCalled()
    expect(useSessionStore.getState().sessions.s1.pendingPermission).toBeDefined()
  })

  it('does not approve permission requests with the old default window shortcut', () => {
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

    fireEvent.keyDown(window, { key: 'Enter', metaKey: true })

    expect(tauriMocks.respondPermission).not.toHaveBeenCalled()
    expect(useSessionStore.getState().sessions.s1.pendingPermission).toBeDefined()
  })

  it('routes numbered question shortcuts through respondQuestion', () => {
    mountIsland(null, {
      phase: 'waiting_input',
      pendingQuestion: { question: 'Pick one', options: ['Overlay', 'Detail', 'Compact'] },
    })

    fireEvent.keyDown(window, { key: '2', ...primaryModifierKeyEvent() })

    expect(tauriMocks.respondQuestion).toHaveBeenCalledWith('s1', 'Detail')
    expect(tauriMocks.sendMessage).not.toHaveBeenCalledWith('s1', 'Detail')
    expect(useSessionStore.getState().sessions.s1.pendingQuestion).toBeUndefined()
  })

  it('does not collapse multi-select questions through numbered shortcuts', () => {
    mountIsland(null, {
      phase: 'waiting_input',
      pendingQuestion: { question: 'Pick targets', options: ['Preview', 'Docs', 'Production'], multiSelect: true },
    })

    fireEvent.keyDown(window, { key: '2', ...primaryModifierKeyEvent() })

    expect(tauriMocks.respondQuestion).not.toHaveBeenCalled()
    expect(useSessionStore.getState().sessions.s1.pendingQuestion).toBeDefined()
  })

  it('routes configured approval shortcuts to plan modes when a plan is pending', () => {
    useConfigStore.setState({
      shortcuts: [
        { action: 'approve-action', label: 'Approve Action', keys: '⌘+a' },
        { action: 'reject-action', label: 'Reject Action', keys: '⌘+d' },
      ],
    })
    mountIsland(null, {
      phase: 'waiting_approval',
      planTitle: 'Implementation plan',
      planContent: '1. Fix shortcut routing',
    })

    fireEvent.keyDown(window, { key: 'a', metaKey: true })

    expect(tauriMocks.respondPlan).toHaveBeenCalledWith('s1', 'acceptEdits')
    expect(tauriMocks.respondPermission).not.toHaveBeenCalled()

    cleanup()
    tauriMocks.respondPlan.mockClear()
    mountIsland(null, {
      phase: 'waiting_approval',
      planTitle: 'Implementation plan',
      planContent: '1. Fix shortcut routing',
    })

    fireEvent.keyDown(window, { key: 'd', metaKey: true })

    expect(tauriMocks.respondPlan).toHaveBeenCalledWith('s1', 'manual')
    expect(tauriMocks.respondPermission).not.toHaveBeenCalled()
  })

  it('routes recorded symbol shortcuts to permission actions', () => {
    useConfigStore.setState({
      shortcuts: [
        { action: 'approve-action', label: 'Approve Action', keys: '⌘+⇧+A' },
        { action: 'reject-action', label: 'Reject Action', keys: '' },
      ],
    })
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

    fireEvent.keyDown(window, { key: 'a', metaKey: true, shiftKey: true })

    expect(tauriMocks.respondPermission).toHaveBeenCalledWith('s1', true)
  })

  it('routes configured panel and session navigation shortcuts', () => {
    useConfigStore.setState({
      shortcuts: [
        { action: 'toggle-panel', label: 'Toggle Panel', keys: '⌘+⇧+I' },
        { action: 'expand-panel', label: 'Expand Panel', keys: '⌘+⇧+E' },
        { action: 'next-session', label: 'Next Session', keys: '⌘+]' },
        { action: 'prev-session', label: 'Previous Session', keys: '⌘+[' },
      ],
    })
    const first = session({ id: 's1', sessionTitle: 'First session' })
    const second = session({ id: 's2', sessionTitle: 'Second session', startedAt: first.startedAt + 1 })
    useSessionStore.setState({
      sessions: { s1: first, s2: second },
      sessionList: [first, second],
      activeSessionId: 's1',
      panelState: 'collapsed',
      activeOverlay: null,
      overlayQueue: [],
      rateLimits: undefined,
      hookNotification: null,
      wakeSilencedUntil: 0,
      focusedTerminal: null,
    })

    render(<NotchPanel />)

    fireEvent.keyDown(window, { key: 'e', metaKey: true, shiftKey: true })
    expect(useSessionStore.getState().panelState).toBe('hover')

    fireEvent.keyDown(window, { key: ']', metaKey: true })
    expect(useSessionStore.getState().activeSessionId).toBe('s2')

    fireEvent.keyDown(window, { key: '[', metaKey: true })
    expect(useSessionStore.getState().activeSessionId).toBe('s1')

    fireEvent.keyDown(window, { key: 'i', metaKey: true, shiftKey: true })
    expect(useSessionStore.getState().panelState).toBe('collapsed')
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
    expect(hostWidthVar()).toBe('656px')
    expect(hitboxWidthVar()).toBe('656px')
    expect(document.querySelector('.notch-panel__alert-content')).toBeInTheDocument()
    expect(document.querySelector('.notch-panel__overlay')).not.toBeInTheDocument()
    expect(document.querySelector('.hover-list')).not.toBeInTheDocument()
    expect(screen.queryByText('agentbro · Port dynamic island')).not.toBeInTheDocument()

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
    expect(screen.getByText('agentbro · Port dynamic island')).toBeInTheDocument()
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
    expect(screen.getByText('agentbro · Port dynamic island')).toBeInTheDocument()
    expect(screen.getByText('Edit')).toBeInTheDocument()
    expect(screen.getByText('src/i18n/locales/zh.json')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '允许一次' })).toBeInTheDocument()
  })

  it('shows the same plan request inline in the session list after the alert is collapsed', async () => {
    mountIsland({
      id: 'plan-s1',
      sessionId: 's1',
      type: 'plan',
      data: {
        planTitle: 'Implementation plan',
        planContent: '1. Preserve session list hover',
        requestedPermissions: ['Bash: run tests'],
      },
      createdAt: Date.now(),
    }, {
      phase: 'waiting_approval',
      planTitle: 'Implementation plan',
      planContent: '1. Preserve session list hover',
      planPermissions: ['Bash: run tests'],
    })

    fireEvent.keyDown(document.body, { key: 'Escape' })
    fireEvent.pointerEnter(screen.getByRole('region', { name: 'AgentBro' }).parentElement!)

    await waitFor(() => expect(document.querySelector('.hover-list')).toBeInTheDocument())
    expect(document.querySelector('.notch-panel__alert-content')).not.toBeInTheDocument()
    expect(document.querySelector('.plan-approval__content')).not.toBeInTheDocument()
    expect(document.querySelector('.hover-list__inline-plan')?.textContent).toContain('Implementation plan')
    expect(document.querySelector('.hover-list__inline-plan')?.textContent).toContain('Preserve session list hover')
    expect(screen.getByText('Bash')).toHaveClass('hover-list__inline-plan-perm-tool')
    expect(screen.getByRole('button', { name: 'Accept Edits' })).toBeInTheDocument()
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

    expect(screen.getByText('让 Agent 更好用')).toBeInTheDocument()
    fireEvent.mouseDown(screen.getByText('Ship it').closest('.question-card__option-row')!)

    expect(tauriMocks.respondQuestion).toHaveBeenCalledWith('s1', 'Ship it')
    expect(useSessionStore.getState().sessions.s1.pendingQuestion).toBeUndefined()
  })

  it('opens custom input from the question chat action', () => {
    mountIsland({
      id: 'question-chat-s1',
      sessionId: 's1',
      type: 'question',
      data: { question: 'Pick one', options: ['Ship it', 'Revise'] },
      createdAt: Date.now(),
    }, {
      phase: 'waiting_input',
      pendingQuestion: { question: 'Pick one', options: ['Ship it', 'Revise'] },
    })

    fireEvent.mouseDown(screen.getByRole('button', { name: 'Chat about this' }))

    expect(screen.getByPlaceholderText('Type your response...')).toBeInTheDocument()
    expect(tauriMocks.setNotchFocusable).toHaveBeenCalledWith(true)
    expect(tauriMocks.respondQuestion).not.toHaveBeenCalled()
  })

  it('routes skip interview as a free-form question answer', () => {
    mountIsland({
      id: 'question-skip-s1',
      sessionId: 's1',
      type: 'question',
      data: { question: 'Pick one', options: ['Ship it', 'Revise'] },
      createdAt: Date.now(),
    }, {
      phase: 'waiting_input',
      pendingQuestion: { question: 'Pick one', options: ['Ship it', 'Revise'] },
    })

    fireEvent.mouseDown(screen.getByRole('button', { name: 'Skip interview' }))

    expect(tauriMocks.respondQuestion).toHaveBeenCalledWith('s1', 'Skip interview and plan immediately')
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
        requestedPermissions: [{ tool: 'Bash', prompt: 'run tests' }, 'Edit: src/App.tsx'],
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

    expect(document.querySelector('.overlay-ctx__row1')?.textContent).toContain('agentbro·Fix jump flow·Claude')
    expect(screen.getByText('Subagents (1)')).toBeInTheDocument()
    expect(screen.getByText('Explore')).toBeInTheDocument()
    expect(screen.getByText('完成')).toBeInTheDocument()
    expect(document.querySelector('.plan-approval__content')?.textContent).toContain('Fix jump')
    expect(document.querySelector('.plan-approval__perms')?.textContent).toContain('Bash')
    expect(screen.getByText('请求的权限:')).toHaveClass('plan-approval__perms-label')
    expect(screen.getByText('Bash')).toHaveClass('plan-approval__perm-tool')
    expect(screen.getByText('Edit')).toHaveClass('plan-approval__perm-tool')
    expect(screen.getByText('让 Agent 更好用')).toBeInTheDocument()

    fireEvent.click(document.querySelector('.plan-approval__content')!)
    expect(tauriMocks.jumpToTerminal).toHaveBeenCalledWith('s1')

    fireEvent.click(screen.getByText('Accept Edits'))

    expect(tauriMocks.respondPlan).toHaveBeenCalledWith('s1', 'acceptEdits')
    expect(tauriMocks.jumpToTerminal).toHaveBeenCalledTimes(1)
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

  it('routes plan overlay feedback through feedback mode', () => {
    mountIsland({
      id: 'plan-s1-feedback',
      sessionId: 's1',
      type: 'plan',
      data: { planTitle: 'Implementation plan', planContent: '1. Fix jump' },
      createdAt: Date.now(),
    }, {
      planTitle: 'Implementation plan',
      planContent: '1. Fix jump',
    })

    fireEvent.change(screen.getByPlaceholderText('Tell Claude what to change...'), {
      target: { value: 'Revise the migration order' },
    })
    fireEvent.click(screen.getByText('Send Feedback'))

    expect(tauriMocks.respondPlan).toHaveBeenCalledWith(
      's1',
      'feedback',
      'Revise the migration order',
    )
    expect(useSessionStore.getState().activeOverlay).toBeNull()
  })

  it('focuses plan feedback inputs through the island hitbox capture path', () => {
    mountIsland({
      id: 'plan-s1-input-focus',
      sessionId: 's1',
      type: 'plan',
      data: { planTitle: 'Implementation plan', planContent: '1. Fix input focus' },
      createdAt: Date.now(),
    }, {
      planTitle: 'Implementation plan',
      planContent: '1. Fix input focus',
    })

    const input = screen.getByPlaceholderText('Tell Claude what to change...')
    tauriMocks.setNotchFocusable.mockClear()

    fireEvent.pointerDown(input, { button: 0 })

    expect(tauriMocks.setNotchFocusable).toHaveBeenCalledWith(true)
    expect(tauriMocks.jumpToTerminal).not.toHaveBeenCalled()
  })

  it('routes response overlay jump and reply actions to terminal APIs', async () => {
    mountIsland({
      id: 'response-s1',
      sessionId: 's1',
      type: 'response',
      data: {
        responseText: [
          'Ready for the next step',
          '',
          '| 级别 | 问题 | 修复 |',
          '| --- | --- | --- |',
          '| Critical | `set_current` 非原子 | 事务 + 行数校验 |',
          '| Medium | `count_table` 格式化 SQL | 白名单校验 |',
        ].join('\n'),
        userMessage: 'Continue?',
      },
      createdAt: Date.now(),
    })

    expect(document.querySelector('.overlay-feedback__message--user')?.textContent).toContain('Continue?')
    expect(document.querySelector('.overlay-feedback__message--assistant')?.textContent).toContain('Ready for the next step')
    expect(document.querySelector('.overlay-feedback__markdown table')).toBeInTheDocument()
    expect(screen.getByText('级别')).toBeInTheDocument()
    expect(screen.getByText('让 Agent 更好用')).toBeInTheDocument()

    tauriMocks.jumpToTerminal.mockClear()
    fireEvent.mouseDown(document.querySelector('.overlay-feedback__session')!)

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
    tauriMocks.jumpToTerminal.mockClear()
    expect(fireEvent.mouseDown(replyInput)).toBe(true)
    expect(tauriMocks.setNotchFocusable).toHaveBeenCalledWith(true)
    expect(tauriMocks.jumpToTerminal).not.toHaveBeenCalled()

    fireEvent.change(replyInput, {
      target: { value: 'thanks, keep going' },
    })
    fireEvent.mouseDown(screen.getByRole('button', { name: 'Send' }))

    await waitFor(() => expect(tauriMocks.sendMessage).toHaveBeenCalledWith('s1', 'thanks, keep going'))
  })

  it('does not send response replies while an IME composition is active', async () => {
    mountIsland({
      id: 'response-s1-ime',
      sessionId: 's1',
      type: 'response',
      data: {
        responseText: 'Another answer',
        userMessage: 'Continue again?',
      },
      createdAt: Date.now(),
    })

    const replyInput = screen.getByPlaceholderText('Send a message...')
    fireEvent.change(replyInput, { target: { value: 'ni' } })
    fireEvent.compositionStart(replyInput)
    fireEvent.keyDown(replyInput, { key: 'Enter' })

    expect(tauriMocks.sendMessage).not.toHaveBeenCalled()

    fireEvent.compositionEnd(replyInput, { data: '你' })
    fireEvent.change(replyInput, { target: { value: '你' } })
    fireEvent.keyDown(replyInput, { key: 'Enter' })

    await waitFor(() => expect(tauriMocks.sendMessage).toHaveBeenCalledWith('s1', '你'))
  })

  it('keeps response feedback open after sending a reply until hover leaves', async () => {
    vi.useFakeTimers()
    try {
      mountIsland({
        id: 'response-s1-reply-stays-open',
        sessionId: 's1',
        type: 'response',
        data: {
          responseText: 'Still working on it',
          userMessage: 'Confirm?',
        },
        createdAt: Date.now(),
      })

      const hitbox = screen.getByRole('region', { name: 'AgentBro' }).parentElement!
      const overlay = document.querySelector('.overlay-feedback')!
      fireEvent.pointerEnter(hitbox)
      fireEvent.mouseEnter(overlay)

      const replyInput = screen.getByPlaceholderText('Send a message...')
      fireEvent.change(replyInput, { target: { value: 'confirmed, continue' } })

      await act(async () => {
        fireEvent.mouseDown(screen.getByRole('button', { name: 'Send' }))
        await Promise.resolve()
      })

      expect(tauriMocks.sendMessage).toHaveBeenCalledWith('s1', 'confirmed, continue')

      act(() => {
        vi.advanceTimersByTime(500)
      })

      expect(useSessionStore.getState().activeOverlay?.id).toBe('response-s1-reply-stays-open')
      expect(screen.getByRole('region', { name: 'AgentBro' })).toHaveAttribute('data-island-state', 'feedback')

      act(() => {
        vi.advanceTimersByTime(2_500)
      })

      expect(useSessionStore.getState().activeOverlay?.id).toBe('response-s1-reply-stays-open')

      fireEvent.mouseLeave(overlay)

      expect(useSessionStore.getState().activeOverlay).toBeNull()
      expect(useSessionStore.getState().panelState).toBe('collapsed')
    } finally {
      vi.useRealTimers()
    }
  })

  it('does not dismiss feedback overlays on mouse leave while the reply input has draft text', () => {
    vi.useFakeTimers()
    try {
      mountIsland({
        id: 'response-s1-draft',
        sessionId: 's1',
        type: 'response',
        data: {
          responseText: 'Draft should hold this open',
          userMessage: 'Continue?',
        },
        createdAt: Date.now(),
      })

      const hitbox = screen.getByRole('region', { name: 'AgentBro' }).parentElement!
      const overlay = document.querySelector('.overlay-feedback')!
      const replyInput = screen.getByPlaceholderText('Send a message...')

      fireEvent.pointerEnter(hitbox)
      fireEvent.mouseEnter(overlay)
      fireEvent.change(replyInput, { target: { value: 'not ready yet' } })
      expect(replyInput).toHaveAttribute('data-has-draft', 'true')

      act(() => {
        vi.advanceTimersByTime(3_000)
      })
      fireEvent.mouseLeave(overlay)
      fireEvent.pointerLeave(hitbox)

      expect(useSessionStore.getState().activeOverlay?.id).toBe('response-s1-draft')
      expect(useSessionStore.getState().panelState).toBe('hover')

      fireEvent.change(replyInput, { target: { value: '' } })
      expect(replyInput).toHaveAttribute('data-has-draft', 'false')
      fireEvent.mouseLeave(overlay)

      expect(useSessionStore.getState().activeOverlay).toBeNull()
      expect(useSessionStore.getState().panelState).toBe('collapsed')
    } finally {
      vi.useRealTimers()
    }
  })

  it('renders a dedicated compacting overlay while context compaction is running', () => {
    mountIsland({
      id: 'compacting-s1',
      sessionId: 's1',
      type: 'compacting',
      data: {},
      createdAt: Date.now(),
    }, {
      phase: 'compacting',
      lastUserMessage: 'Please compact the context',
    })

    expect(screen.getByText('Compacting context...')).toBeInTheDocument()
    expect(screen.getByText('Please compact the context')).toBeInTheDocument()
    expect(document.querySelector('.overlay-compacting')).toBeInTheDocument()

    fireEvent.mouseDown(document.querySelector('.overlay-compacting__body')!)

    expect(tauriMocks.jumpToTerminal).toHaveBeenCalledWith('s1')
    expect(useSessionStore.getState().activeOverlay).toBeNull()
  })

  it('auto-collapses after the cursor leaves the detail view', async () => {
    useConfigStore.setState({ autoCollapse: true, collapseDelay: 1 })
    mountIsland()

    fireEvent.click(screen.getByText('agentbro · Port dynamic island'))
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
    expect(useSessionStore.getState().overlayQueue.map((overlay) => overlay.id)).not.toContain('completion-s1')
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

  it('lets native notification hover probing focus visible feedback controls', async () => {
    tauriMocks.isTauri.mockReturnValue(true)
    tauriMocks.isCursorOverNotch.mockResolvedValue(true)
    const activeOverlay: OverlayItem = {
      id: 'completion-s1-native',
      sessionId: 's1',
      type: 'completion',
      data: { summary: 'Native probe should focus feedback controls' },
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

    expect(tauriMocks.setNotchFocusable).toHaveBeenCalledWith(true)
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
