import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ApprovalBar } from '../components/notch/ApprovalBar'
import { useSessionStore } from '../stores/sessionStore'
import type { SessionState } from '../types/agent'

vi.mock('../services/tauriApi', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../services/tauriApi')>()
  return {
    ...actual,
    setNotchFocusable: vi.fn(() => Promise.resolve()),
  }
})

vi.mock('../utils/platform', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../utils/platform')>()
  return {
    ...actual,
    isWindowsPlatform: () => false,
  }
})

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, fallback?: string | { defaultValue?: string }) => {
      const translations: Record<string, string> = {
        'notch.typeMessage': 'Type message',
        'notch.typeReply': 'Type reply',
        'notch.send': 'Send',
        'notch.composerHintCodexApp': 'Codex.app cannot receive messages yet.',
        'notch.composerHintRemote': 'Remote sessions cannot receive follow-ups.',
        'notch.openHostApp': 'Open app',
      }
      if (translations[key]) return translations[key]
      if (typeof fallback === 'string') return fallback
      return fallback?.defaultValue ?? key
    },
  }),
}))

function session(overrides: Partial<SessionState> = {}): SessionState {
  return {
    id: 's-tmux',
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

function renderBar(s: SessionState, extra: Partial<React.ComponentProps<typeof ApprovalBar>> = {}) {
  return render(
    <ApprovalBar
      session={s}
      onAllow={vi.fn()}
      onAllowAlways={vi.fn()}
      onDeny={vi.fn()}
      onAutoApprove={vi.fn()}
      onSendMessage={vi.fn()}
      {...extra}
    />,
  )
}

describe('ApprovalBar composer gating', () => {
  beforeEach(() => {
    useSessionStore.getState().setCodexAppServerLive(false)
  })

  it('renders the default text input for a tmux Claude session', () => {
    renderBar(session())
    expect(screen.getByPlaceholderText('Type message')).toBeInTheDocument()
    expect(screen.queryByText(/cannot receive/i)).not.toBeInTheDocument()
  })

  it('shows a hint + Open app button for a Codex.app session', () => {
    const onJumpToHostApp = vi.fn()
    renderBar(
      session({
        agentType: 'codex',
        termBundleId: 'com.openai.codex',
        terminal: 'Codex',
        tty: undefined,
        pid: undefined,
      }),
      { onJumpToHostApp },
    )

    expect(screen.queryByPlaceholderText('Type message')).not.toBeInTheDocument()
    expect(screen.getByText('Codex.app cannot receive messages yet.')).toBeInTheDocument()
    const cta = screen.getByRole('button', { name: 'Open app' })

    fireEvent.mouseDown(cta)
    expect(onJumpToHostApp).toHaveBeenCalledTimes(1)
  })

  it('renders the default text input for a Codex.app session when app-server is live', () => {
    useSessionStore.getState().setCodexAppServerLive(true)
    const onSendMessage = vi.fn()
    renderBar(
      session({
        agentType: 'codex',
        termBundleId: 'com.openai.codex',
        codexAppServerThreadId: 'thread-1',
        terminal: 'Codex',
        tty: undefined,
        pid: undefined,
      }),
      { onSendMessage },
    )

    const input = screen.getByPlaceholderText('Type message')
    fireEvent.change(input, { target: { value: '继续' } })
    fireEvent.keyDown(input, { key: 'Enter' })

    expect(onSendMessage).toHaveBeenCalledWith('继续')
    expect(screen.queryByText('Codex.app cannot receive messages yet.')).not.toBeInTheDocument()
  })

  it('shows a hint without CTA for a remote session', () => {
    renderBar(session({ remoteHostId: 'host-1' }))
    expect(screen.getByText('Remote sessions cannot receive follow-ups.')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Open app' })).not.toBeInTheDocument()
  })

  it('still renders the question reply input when waiting_input is active', () => {
    renderBar(
      session({
        phase: 'waiting_input',
        pendingQuestion: {
          question: 'Pick',
          options: ['Yes', 'No'],
        },
      }),
    )
    expect(screen.getByPlaceholderText('Type reply')).toBeInTheDocument()
    expect(screen.queryByText(/cannot receive/i)).not.toBeInTheDocument()
  })

})
