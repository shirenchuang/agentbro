import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { NotchPanel } from '../components/notch/NotchPanel'
import { useConfigStore } from '../stores/configStore'
import { useSessionStore } from '../stores/sessionStore'
import { useThemeStore } from '../stores/themeStore'
import type { OverlayItem, SessionState } from '../types/agent'

const tauriMocks = vi.hoisted(() => ({
  getChatHistory: vi.fn(() => Promise.resolve([])),
  jumpToTerminal: vi.fn(() => Promise.resolve()),
  respondPermission: vi.fn(() => Promise.resolve()),
  respondPlan: vi.fn(() => Promise.resolve()),
  respondQuestion: vi.fn(() => Promise.resolve()),
  sendMessage: vi.fn(() => Promise.resolve()),
  setNotchFocusable: vi.fn(() => Promise.resolve()),
}))

vi.mock('../services/tauriApi', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../services/tauriApi')>()
  return {
    ...actual,
    getChatHistory: tauriMocks.getChatHistory,
    jumpToTerminal: tauriMocks.jumpToTerminal,
    respondPermission: tauriMocks.respondPermission,
    respondPlan: tauriMocks.respondPlan,
    respondQuestion: tauriMocks.respondQuestion,
    sendMessage: tauriMocks.sendMessage,
    setNotchFocusable: tauriMocks.setNotchFocusable,
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

describe('NotchPanel island shell', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    tauriMocks.getChatHistory.mockResolvedValue([])
    tauriMocks.jumpToTerminal.mockResolvedValue(undefined)
    tauriMocks.respondPermission.mockResolvedValue(undefined)
    tauriMocks.respondPlan.mockResolvedValue(undefined)
    tauriMocks.respondQuestion.mockResolvedValue(undefined)
    tauriMocks.sendMessage.mockResolvedValue(undefined)
    tauriMocks.setNotchFocusable.mockResolvedValue(undefined)
    useThemeStore.getState().loadThemes([])
    useThemeStore.getState().setActiveTheme('default')
    useConfigStore.setState({
      allowHorizontalDrag: false,
      autoCollapse: false,
      autoHideNoSessions: false,
      clickToDetail: true,
      dismissOnOutsideClick: false,
      completionCardHeight: 120,
      confettiEnabled: false,
      followFocus: false,
      hoverExpandDelay: 0,
      interactionMode: 'persistent',
      islandSurfaceMode: 'island',
      maxPanelHeight: 600,
      panelMaxWidth: 630,
      pixelCursorEnabled: false,
      showCacheTTL: false,
      taskCompleteDwellSeconds: 3,
      tipsEnabled: false,
    })
  })

  it('renders the expanded island shell with the active session', () => {
    mountIsland()

    expect(screen.getByRole('region', { name: 'AgentBro' })).toHaveAttribute('data-island-state', 'hover')
    expect(screen.getByText('agent-island · Port dynamic island')).toBeInTheDocument()
    expect(screen.getByText('Claude')).toBeInTheDocument()
  })

  it('focuses the notch window when hover opens the session list', () => {
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
    expect(useSessionStore.getState().panelState).toBe('hover')
  })

  it('renders task-completion feedback over the session list', () => {
    mountIsland({
      id: 'completion-s1',
      sessionId: 's1',
      type: 'completion',
      data: { summary: 'All island parity checks passed' },
      createdAt: Date.now(),
    })

    expect(screen.getByRole('region', { name: 'AgentBro' })).toHaveAttribute('data-island-state', 'feedback')
    expect(screen.getByText('All island parity checks passed')).toBeInTheDocument()
  })

  it('renders assistant-response feedback over the session list', () => {
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
    expect(screen.getByText('Can you continue the migration?')).toBeInTheDocument()
    expect(screen.getByText('Ready for the next integration step')).toBeInTheDocument()
    expect(screen.getByText('Jump to terminal →')).toBeInTheDocument()
    expect(screen.getByPlaceholderText('Send a message... (Enter)')).toBeInTheDocument()
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
    expect(screen.getByText('Collapsed response should still pop up')).toBeInTheDocument()
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

  it('routes plan overlay decisions to respondPlan and dismisses the overlay', () => {
    mountIsland({
      id: 'plan-s1',
      sessionId: 's1',
      type: 'plan',
      data: { planTitle: 'Implementation plan', planContent: '1. Fix jump' },
      createdAt: Date.now(),
    }, {
      planTitle: 'Implementation plan',
      planContent: '1. Fix jump',
    })

    fireEvent.click(screen.getByText('Accept Edits'))

    expect(tauriMocks.respondPlan).toHaveBeenCalledWith('s1', 'acceptEdits')
    expect(useSessionStore.getState().activeOverlay).toBeNull()
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

    fireEvent.click(screen.getByText('Jump to terminal →').closest('.overlay-response__bubble')!)

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

    fireEvent.change(screen.getByPlaceholderText('Send a message... (Enter)'), {
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

  it('dismisses feedback overlays with the outside-click layer when enabled', () => {
    useConfigStore.setState({ dismissOnOutsideClick: true })
    mountIsland({
      id: 'completion-s1',
      sessionId: 's1',
      type: 'completion',
      data: { summary: 'Dismiss me from outside' },
      createdAt: Date.now(),
    })

    fireEvent.mouseDown(screen.getByTestId('notch-outside-dismiss'))

    expect(useSessionStore.getState().activeOverlay).toBeNull()
    expect(useSessionStore.getState().overlayQueue).toEqual([])
  })
})
