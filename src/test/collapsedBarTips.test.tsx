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
    useConfigStore.setState({ tipsEnabled: true, showToolStatus: true, showUsageQuota: true, usageQueryEnabled: true })
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

  it('counts unfinished task sessions in WAIT like evolab', () => {
    const { container } = render(
      <CollapsedBar
        sessions={[
          session({ tasks: [{ id: '1', name: 'Finish parity', status: 'pending' }] }),
        ]}
        panelState="hover"
        onCollapse={() => {}}
      />,
    )

    expect(container.querySelector('.collapsed-bar__counter-pill--wait')?.textContent).toBe('WAIT1')
  })

  it('shows 5h and 7d usage in the expanded header when usage quota is enabled', () => {
    const { container } = render(
      <CollapsedBar
        sessions={[session()]}
        panelState="hover"
        rateLimits={{
          fiveHourUsage: 36,
          fiveHourRemaining: '1h1m',
          sevenDayUsage: 50,
          sevenDayRemaining: '5d8h',
        }}
        onCollapse={() => {}}
      />,
    )

    expect(screen.getByText('5h 36% 1h1m')).toBeInTheDocument()
    expect(screen.getByText('7d 50% 5d8h')).toBeInTheDocument()
    expect(container.querySelector('.collapsed-bar__counter-pills')).not.toBeInTheDocument()
  })

  it('uses the lead agent provider snapshot before the global fallback', () => {
    render(
      <CollapsedBar
        sessions={[session({ agentType: 'codex' })]}
        panelState="hover"
        rateLimits={{
          fiveHourUsage: 5,
          fiveHourRemaining: '4h',
          sevenDayUsage: 6,
          sevenDayRemaining: '6d',
        }}
        usageSnapshots={{
          codex: {
            provider: 'codex',
            providerLabel: 'Codex',
            source: 'codex-jsonl',
            fiveHourUsage: 66,
            fiveHourRemaining: '31m',
            sevenDayUsage: 22,
            sevenDayRemaining: '4d',
            windows: [
              { id: 'five_hour', title: '5h', usedPercent: 66, remainingLabel: '31m' },
              { id: 'seven_day', title: '7d', usedPercent: 22, remainingLabel: '4d' },
            ],
          },
          'claude-code': {
            provider: 'claude-code',
            providerLabel: 'Claude',
            fiveHourUsage: 11,
            fiveHourRemaining: '3h',
            sevenDayUsage: 12,
            sevenDayRemaining: '5d',
          },
        }}
        onCollapse={() => {}}
      />,
    )

    expect(screen.getByText('5h 66% 31m')).toBeInTheDocument()
    expect(screen.getByText('7d 22% 4d')).toBeInTheDocument()
    expect(screen.queryByText('5h 5% 4h')).not.toBeInTheDocument()
  })

  it('does not show another provider snapshot for the active agent', () => {
    const { container } = render(
      <CollapsedBar
        sessions={[session({ agentType: 'codex' })]}
        panelState="hover"
        rateLimits={{
          provider: 'claude-code',
          providerLabel: 'Claude',
          fiveHourUsage: 88,
          fiveHourRemaining: '2m',
          sevenDayUsage: 44,
          sevenDayRemaining: '5d',
        }}
        onCollapse={() => {}}
      />,
    )

    expect(container.querySelector('.collapsed-bar__counter-pills')).toBeInTheDocument()
    expect(screen.queryByText('5h 88% 2m')).not.toBeInTheDocument()
  })

  it('keeps ALL ACT WAIT in the expanded header when usage quota is disabled', () => {
    useConfigStore.setState({ showUsageQuota: false })

    const { container } = render(
      <CollapsedBar
        sessions={[session({ phase: 'processing' })]}
        panelState="hover"
        rateLimits={{
          fiveHourUsage: 36,
          fiveHourRemaining: '1h1m',
          sevenDayUsage: 50,
          sevenDayRemaining: '5d8h',
        }}
        onCollapse={() => {}}
      />,
    )

    expect(container.querySelector('.collapsed-bar__counter-pills')).toBeInTheDocument()
    expect(screen.getByText('ALL')).toBeInTheDocument()
    expect(screen.getByText('ACT')).toBeInTheDocument()
    expect(screen.getByText('WAIT')).toBeInTheDocument()
    expect(screen.queryByText('5h 36% 1h1m')).not.toBeInTheDocument()
  })

  it('keeps ALL ACT WAIT when usage querying is disabled', () => {
    useConfigStore.setState({ usageQueryEnabled: false, showUsageQuota: true })

    const { container } = render(
      <CollapsedBar
        sessions={[session({ phase: 'processing' })]}
        panelState="hover"
        rateLimits={{
          fiveHourUsage: 36,
          fiveHourRemaining: '1h1m',
          sevenDayUsage: 50,
          sevenDayRemaining: '5d8h',
        }}
        onCollapse={() => {}}
      />,
    )

    expect(container.querySelector('.collapsed-bar__counter-pills')).toBeInTheDocument()
    expect(screen.queryByText('5h 36% 1h1m')).not.toBeInTheDocument()
  })

  it('counts error sessions as alerts above ordinary attention sessions', () => {
    const { container } = render(
      <CollapsedBar
        sessions={[
          session({ id: 'error', phase: 'error', description: 'Tool failed' }),
          session({ id: 'approval', phase: 'waiting_approval', pendingPermission: { toolName: 'Bash', toolInput: 'pnpm test' } }),
        ]}
        panelState="collapsed"
        onCollapse={() => {}}
      />,
    )

    expect(container.querySelector('.collapsed-bar__alert-badge')?.textContent).toBe('2')
  })

  it('uses thinking text instead of internal processing prompt text', () => {
    render(
      <CollapsedBar
        sessions={[
          session({ project: '', phase: 'processing', description: 'Processing user input' }),
        ]}
        panelState="collapsed"
        onCollapse={() => {}}
      />,
    )

    expect(screen.getByText('notch.thinking')).toBeInTheDocument()
    expect(screen.queryByText('Processing user input')).not.toBeInTheDocument()
  })

  it('shows live tool status as the primary collapsed island text when enabled', () => {
    const { container } = render(
      <CollapsedBar
        sessions={[
          session({
            phase: 'processing',
            lastToolName: 'Edit',
            lastToolTarget: 'OverlayFeedbackPanel.tsx +68 -41',
          }),
        ]}
        panelState="collapsed"
        onCollapse={() => {}}
      />,
    )

    expect(screen.getByText('notch.tool.editing')).toBeInTheDocument()
    expect(screen.getByText('project')).toBeInTheDocument()
    expect(screen.getByText('OverlayFeedbackPanel.tsx')).toBeInTheDocument()
    expect(screen.getByText('+68')).toHaveClass('collapsed-bar__tool-count--add')
    expect(screen.getByText('-41')).toHaveClass('collapsed-bar__tool-count--del')
    expect(container.querySelector('.collapsed-bar__tool-inline')).toBeInTheDocument()
  })

  it('compacts full file paths in collapsed live tool status', () => {
    const { container } = render(
      <CollapsedBar
        sessions={[
          session({
            project: 'agentbro',
            phase: 'processing',
            lastToolName: 'Read',
            lastToolTarget: '/Users/shirenchuang/code/shizhen/agentbro/src/components/notch/CollapsedBar.tsx',
          }),
        ]}
        panelState="collapsed"
        onCollapse={() => {}}
      />,
    )

    expect(screen.getByText('agentbro')).toBeInTheDocument()
    expect(screen.getByText('notch.tool.reading')).toBeInTheDocument()
    expect(screen.getByText('CollapsedBar.tsx')).toBeInTheDocument()
    expect(container.querySelector('.collapsed-bar__tool-inline')).toHaveAttribute(
      'title',
      'notch.tool.reading /Users/shirenchuang/code/shizhen/agentbro/src/components/notch/CollapsedBar.tsx',
    )
    expect(screen.queryByText('/Users/shirenchuang/code/shizhen/agentbro/src/components/notch/CollapsedBar.tsx')).not.toBeInTheDocument()
  })

  it('hides live tool status from the collapsed island when disabled', () => {
    useConfigStore.setState({ showToolStatus: false })

    render(
      <CollapsedBar
        sessions={[
          session({
            phase: 'processing',
            lastToolName: 'Edit',
            lastToolTarget: 'OverlayFeedbackPanel.tsx +68 -41',
          }),
        ]}
        panelState="collapsed"
        onCollapse={() => {}}
      />,
    )

    expect(screen.queryByText('notch.tool.editing')).not.toBeInTheDocument()
    expect(screen.queryByText('OverlayFeedbackPanel.tsx')).not.toBeInTheDocument()
    expect(screen.getByText('project')).toBeInTheDocument()
  })
})
