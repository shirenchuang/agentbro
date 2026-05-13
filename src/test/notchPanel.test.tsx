import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { NotchPanel } from '../components/notch/NotchPanel'
import { useConfigStore } from '../stores/configStore'
import { useSessionStore } from '../stores/sessionStore'
import type { OverlayItem, SessionState } from '../types/agent'

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

function mountIsland(activeOverlay: OverlayItem | null = null) {
  const currentSession = session()
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
    expect(screen.getByText('Port dynamic island')).toBeInTheDocument()
    expect(screen.getByText('Claude')).toBeInTheDocument()
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
