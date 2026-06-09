import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { OverlayFeedbackPanel } from '../components/overlay/OverlayFeedbackPanel'
import { useSessionStore } from '../stores/sessionStore'
import type { SessionState } from '../types/agent'

const tauriMocks = vi.hoisted(() => ({
  sendMessage: vi.fn(() => Promise.resolve()),
  setNotchFocusable: vi.fn(() => Promise.resolve()),
}))

vi.mock('../services/tauriApi', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../services/tauriApi')>()
  return {
    ...actual,
    sendMessage: tauriMocks.sendMessage,
    setNotchFocusable: tauriMocks.setNotchFocusable,
  }
})

vi.mock('../utils/platform', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../utils/platform')>()
  return {
    ...actual,
    isWindowsPlatform: () => false,
  }
})

vi.mock('../components/notch/mascots/MascotRouter', () => ({
  MascotRouter: () => <div data-testid="mascot" />,
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, fallback?: string | { defaultValue?: string }) => {
      const translations: Record<string, string> = {
        'notch.typeMessage': 'Type message',
        'notch.send': 'Send',
        'notch.composerHintCodexApp': 'Codex.app cannot receive messages yet.',
        'notch.openHostApp': 'Open app',
        'notch.jumpToTerminal': 'Jump to terminal',
        'notch.you': 'You',
      }
      if (translations[key]) return translations[key]
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
    terminal: 'iTerm2',
    phase: 'idle',
    startedAt: 0,
    duration: 0,
    tokens: { input: 0, output: 0, cacheRead: 0, cacheCreate: 0 },
    chatHistory: [],
    subagents: [],
    activeTools: [],
    pid: 1234,
    tty: '/dev/ttys001',
    ...overrides,
  }
}

function renderPanel(s: SessionState, onJumpToTerminal = vi.fn(), onDismiss = vi.fn()) {
  const result = render(
    <OverlayFeedbackPanel
      session={s}
      text="Done"
      dwellMs={6000}
      statusLabel="New reply"
      onJumpToTerminal={onJumpToTerminal}
      onDismiss={onDismiss}
    />,
  )
  return { ...result, onJumpToTerminal, onDismiss }
}

describe('OverlayFeedbackPanel composer gating', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useSessionStore.getState().setCodexAppServerLive(false)
  })

  it('shows the normal reply input for sendable sessions', () => {
    renderPanel(session())
    expect(screen.getByPlaceholderText('Type message')).toBeInTheDocument()
    expect(screen.queryByText('Codex.app cannot receive messages yet.')).not.toBeInTheDocument()
  })

  it('shows a hint and Open app button for Codex.app sessions', () => {
    const { onJumpToTerminal, onDismiss } = renderPanel(
      session({
        agentType: 'codex',
        termBundleId: 'com.openai.codex',
        codexAppServerThreadId: 'thread-1',
        terminal: 'Codex',
        tty: undefined,
        pid: undefined,
      }),
    )

    expect(screen.queryByPlaceholderText('Type message')).not.toBeInTheDocument()
    expect(screen.getByText('Codex.app cannot receive messages yet.')).toBeInTheDocument()

    fireEvent.mouseDown(screen.getByRole('button', { name: 'Open app' }))
    expect(onJumpToTerminal).toHaveBeenCalledTimes(1)
    expect(onDismiss).toHaveBeenCalledTimes(1)
    expect(tauriMocks.sendMessage).not.toHaveBeenCalled()
  })

  it('dismisses after using the jump button', () => {
    const { onJumpToTerminal, onDismiss } = renderPanel(session())

    fireEvent.mouseDown(screen.getByRole('button', { name: 'Jump to terminal' }))

    expect(onJumpToTerminal).toHaveBeenCalledTimes(1)
    expect(onDismiss).toHaveBeenCalledTimes(1)
  })

  it('dismisses after clicking the notification panel to jump', () => {
    const { container, onJumpToTerminal, onDismiss } = renderPanel(session())
    const panel = container.querySelector('.overlay-feedback')

    fireEvent.mouseDown(panel!, { button: 0 })

    expect(onJumpToTerminal).toHaveBeenCalledTimes(1)
    expect(onDismiss).toHaveBeenCalledTimes(1)
  })

  it('allows Codex.app replies when the app-server bridge is live', () => {
    useSessionStore.getState().setCodexAppServerLive(true)
    renderPanel(
      session({
        agentType: 'codex',
        termBundleId: 'com.openai.codex',
        codexAppServerThreadId: 'thread-1',
        terminal: 'Codex',
        tty: undefined,
        pid: undefined,
      }),
    )

    expect(screen.getByPlaceholderText('Type message')).toBeInTheDocument()
    expect(screen.queryByText('Codex.app cannot receive messages yet.')).not.toBeInTheDocument()
  })
})
