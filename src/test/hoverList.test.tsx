import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { HoverList } from '../components/notch/HoverList'
import { useConfigStore } from '../stores/configStore'
import { useSessionStore } from '../stores/sessionStore'
import type { SessionState } from '../types/agent'

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
    agentType: 'codex',
    project: 'agent-island',
    terminal: 'Terminal',
    phase: 'processing',
    startedAt: Date.now() - 10_000,
    duration: 10_000,
    tokens: { input: 0, output: 0, cacheRead: 0, cacheCreate: 0 },
    chatHistory: [],
    subagents: [],
    activeTools: [],
    sessionTitle: 'Fix island interactions',
    lastUserMessage: 'Please fix the island',
    pid: 1234,
    ...overrides,
  }
}

describe('HoverList interactions', () => {
  beforeEach(() => {
    useSessionStore.setState({ activeOverlay: null })
    useConfigStore.setState({ hoverSpeed: 'instant', maxVisibleSessions: 5, showCacheTTL: false })
  })

  it('opens session detail from the session row', () => {
    const onSessionClick = vi.fn()
    render(<HoverList sessions={[session()]} onSessionClick={onSessionClick} />)

    fireEvent.click(screen.getByText('Fix island interactions'))

    expect(onSessionClick).toHaveBeenCalledWith('s1')
  })

  it('jumps from the arrow without opening detail', () => {
    const onSessionClick = vi.fn()
    const onJumpToTerminal = vi.fn()
    render(
      <HoverList
        sessions={[session()]}
        onSessionClick={onSessionClick}
        onJumpToTerminal={onJumpToTerminal}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'notch.jumpToTerminal' }))

    expect(onJumpToTerminal).toHaveBeenCalledWith('s1')
    expect(onSessionClick).not.toHaveBeenCalled()
  })

  it('does not treat an unavailable jump arrow as a session click', () => {
    const onSessionClick = vi.fn()
    const onJumpToTerminal = vi.fn()
    render(
      <HoverList
        sessions={[session({ pid: undefined, tty: undefined })]}
        onSessionClick={onSessionClick}
        onJumpToTerminal={onJumpToTerminal}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: '等待终端信息' }))

    expect(onJumpToTerminal).not.toHaveBeenCalled()
    expect(onSessionClick).not.toHaveBeenCalled()
  })

  it('only displays cache TTL when the setting is enabled', () => {
    const cacheSession = session({
      lastMainAgentAt: Date.now() - 60_000,
      cacheTtlMs: 300_000,
    })

    const { rerender } = render(<HoverList sessions={[cacheSession]} onSessionClick={vi.fn()} />)
    expect(screen.queryByText(/cache /)).not.toBeInTheDocument()

    useConfigStore.setState({ showCacheTTL: true })
    rerender(<HoverList sessions={[cacheSession]} onSessionClick={vi.fn()} />)
    expect(screen.getByText(/cache \d+m/)).toBeInTheDocument()
  })
})
