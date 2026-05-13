import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'
import { CollapsedBar } from '../components/notch/CollapsedBar'
import { useConfigStore } from '../stores/configStore'
import type { SessionState } from '../types/agent'

function session(overrides: Partial<SessionState> = {}): SessionState {
  return {
    id: 's1',
    agentType: 'claude-code',
    project: 'project',
    terminal: 'iTerm',
    phase: 'idle',
    startedAt: Date.now(),
    duration: 0,
    tokens: { input: 0, output: 0, cacheRead: 0, cacheCreate: 0 },
    chatHistory: [],
    subagents: [],
    activeTools: [],
    ...overrides,
  }
}

describe('CollapsedBar idle tips', () => {
  beforeEach(() => {
    useConfigStore.setState({ tipsEnabled: true })
  })

  it('shows tips when there are no sessions', () => {
    render(<CollapsedBar sessions={[]} panelState="collapsed" onCollapse={() => {}} />)

    expect(screen.getByText('Tips:')).toBeInTheDocument()
  })

  it('hides tips when the setting is disabled', () => {
    useConfigStore.setState({ tipsEnabled: false })

    render(<CollapsedBar sessions={[session()]} panelState="collapsed" onCollapse={() => {}} />)

    expect(screen.queryByText('Tips:')).not.toBeInTheDocument()
  })

  it('shows follow-focus empty text instead of idle tips', () => {
    render(<CollapsedBar sessions={[]} panelState="collapsed" onCollapse={() => {}} focusFilteredEmpty />)

    expect(screen.getByText('notch.noSessionInFocus')).toBeInTheDocument()
    expect(screen.queryByText('Tips:')).not.toBeInTheDocument()
  })
})
