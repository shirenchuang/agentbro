import { fireEvent, render, screen, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { HoverList } from '../components/notch/HoverList'
import { useConfigStore } from '../stores/configStore'
import { useSessionStore } from '../stores/sessionStore'
import type { SessionState } from '../types/agent'

const tauriMocks = vi.hoisted(() => ({
  respondAutoApprove: vi.fn(() => Promise.resolve()),
  respondPermission: vi.fn(() => Promise.resolve()),
  respondPlan: vi.fn(() => Promise.resolve()),
  respondQuestion: vi.fn(() => Promise.resolve()),
}))

vi.mock('../services/tauriApi', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../services/tauriApi')>()
  return {
    ...actual,
    respondAutoApprove: tauriMocks.respondAutoApprove,
    respondPermission: tauriMocks.respondPermission,
    respondPlan: tauriMocks.respondPlan,
    respondQuestion: tauriMocks.respondQuestion,
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
    vi.clearAllMocks()
    useSessionStore.setState({ activeOverlay: null })
    useConfigStore.setState({ hoverSpeed: 'instant', maxVisibleSessions: 5, showCacheTTL: false })
  })

  it('opens session detail from the session row', () => {
    const onSessionClick = vi.fn()
    render(<HoverList sessions={[session()]} onSessionClick={onSessionClick} />)

    fireEvent.click(screen.getByText('agent-island · Fix island interactions'))

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

  it('still jumps from the arrow when terminal metadata is not ready', () => {
    const onSessionClick = vi.fn()
    const onJumpToTerminal = vi.fn()
    render(
      <HoverList
        sessions={[session({ agentType: 'claude-code', pid: undefined, tty: undefined })]}
        onSessionClick={onSessionClick}
        onJumpToTerminal={onJumpToTerminal}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'notch.jumpToTerminal' }))

    expect(onJumpToTerminal).toHaveBeenCalledWith('s1')
    expect(onSessionClick).not.toHaveBeenCalled()
  })

  it('allows Codex Desktop sessions without terminal metadata to jump', () => {
    const onSessionClick = vi.fn()
    const onJumpToTerminal = vi.fn()
    render(
      <HoverList
        sessions={[session({ pid: undefined, tty: undefined, terminal: 'Codex' })]}
        onSessionClick={onSessionClick}
        onJumpToTerminal={onJumpToTerminal}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'notch.jumpToTerminal' }))

    expect(onJumpToTerminal).toHaveBeenCalledWith('s1')
    expect(onSessionClick).not.toHaveBeenCalled()
  })

  it('labels Codex App sessions by app instead of cwd-derived Evolab', () => {
    const { container } = render(
      <HoverList
        sessions={[session({
          project: 'free-chat',
          cwd: '/Users/me/Library/Application Support/.evolab-desktop/free-chat',
          terminal: 'Codex',
          termBundleId: 'com.openai.codex',
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByText('Codex App')).toBeInTheDocument()
    expect(screen.queryByText('Evolab')).not.toBeInTheDocument()
    expect(container.querySelector('.mascot-image')).toHaveAttribute('data-mascot-source', 'codex')
    expect(container.querySelector('.pixel-indicator')).not.toBeInTheDocument()
  })

  it('infers terminal source from bundle metadata when terminal is a tty', () => {
    render(
      <HoverList
        sessions={[session({
          agentType: 'claude-code',
          terminal: '/dev/ttys001',
          termBundleId: 'com.googlecode.iterm2',
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByText('Claude')).toBeInTheDocument()
    expect(screen.getByText('iTerm2')).toBeInTheDocument()
  })

  it('uses a subdued dot for inactive sessions', () => {
    const { container } = render(
      <HoverList
        sessions={[session({
          phase: 'idle',
          idleSince: Date.now() - 60_000,
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(container.querySelector('.hover-list__expired-dot')).toBeInTheDocument()
    expect(container.querySelector('.mascot-image')).not.toBeInTheDocument()
  })

  it('renders Codex list items with title, latest user prompt, and response fallback rows', () => {
    render(
      <HoverList
        sessions={[session({
          phase: 'idle',
          sessionTitle: undefined,
          lastUserMessage: undefined,
          responseText: undefined,
          description: undefined,
          chatHistory: [
            { role: 'user', content: '第三个自定义高度怎么都不生效', timestamp: 1 },
            { role: 'assistant', content: '确实，第三个自定义高度之前没有正确同步。', timestamp: 2 },
          ],
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByText('agent-island · 第三个自定义高度怎么都不生效')).toBeInTheDocument()
    expect(screen.getByText('第三个自定义高度怎么都不生效')).toBeInTheDocument()
    expect(screen.getByText('确实，第三个自定义高度之前没有正确同步。')).toBeInTheDocument()
  })

  it('shows generic thinking text for processing sessions without a tool', () => {
    render(
      <HoverList
        sessions={[session({
          phase: 'processing',
          lastToolName: undefined,
          description: 'Processing user input',
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByText('notch.thinking')).toBeInTheDocument()
    expect(screen.queryByText('Processing user input')).not.toBeInTheDocument()
  })

  it('renders Codex edit targets with change counts', () => {
    const { container } = render(
      <HoverList
        sessions={[session({
          lastToolName: 'Edit',
          lastToolTarget: 'OverlayFeedbackPanel.tsx +68 -41',
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByText('notch.tool.editing')).toBeInTheDocument()
    expect(screen.getByText('OverlayFeedbackPanel.tsx')).toHaveClass('hover-list__tool-target-name')
    expect(screen.getByText('+68')).toHaveClass('hover-list__tool-count--add')
    expect(screen.getByText('-41')).toHaveClass('hover-list__tool-count--del')
    expect(container.querySelector('.hover-list__tool-target--changes')).toBeInTheDocument()
  })

  it('supports keyboard navigation and Enter jump like the island panel', () => {
    const onSessionClick = vi.fn()
    const onJumpToTerminal = vi.fn()
    render(
      <HoverList
        sessions={[
          session({ id: 's1', phase: 'idle', sessionTitle: 'Idle session' }),
          session({ id: 's2', phase: 'waiting_approval', sessionTitle: 'Approval needed' }),
          session({ id: 's3', phase: 'processing', sessionTitle: 'Working session' }),
        ]}
        onSessionClick={onSessionClick}
        onJumpToTerminal={onJumpToTerminal}
      />,
    )

    fireEvent.keyDown(window, { key: 'ArrowDown' })
    fireEvent.keyDown(window, { key: 'Enter' })

    expect(onJumpToTerminal).toHaveBeenCalledWith('s2')
    expect(onSessionClick).not.toHaveBeenCalled()
  })

  it('routes inline permission actions without opening the row', () => {
    const onSessionClick = vi.fn()
    render(
      <HoverList
        sessions={[session({
          phase: 'waiting_approval',
          pendingPermission: { toolName: 'Bash', toolInput: 'pnpm test' },
        })]}
        onSessionClick={onSessionClick}
      />,
    )

    const permissionCard = document.querySelector('.hover-list__inline-perm') as HTMLElement
    fireEvent.mouseDown(within(permissionCard).getByRole('button', { name: '允许一次' }))
    fireEvent.mouseDown(within(permissionCard).getByRole('button', { name: /始终允许/ }))
    fireEvent.mouseDown(within(permissionCard).getByRole('button', { name: '自动批准' }))
    fireEvent.mouseDown(within(permissionCard).getByRole('button', { name: '拒绝' }))

    expect(tauriMocks.respondPermission).toHaveBeenNthCalledWith(1, 's1', true)
    expect(tauriMocks.respondPermission).toHaveBeenNthCalledWith(2, 's1', true, true)
    expect(tauriMocks.respondAutoApprove).toHaveBeenCalledWith('s1')
    expect(tauriMocks.respondPermission).toHaveBeenNthCalledWith(3, 's1', false)
    expect(onSessionClick).not.toHaveBeenCalled()
  })

  it('renders the generic authorization card from pendingPermission with a diff preview', () => {
    render(
      <HoverList
        sessions={[session({
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
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByText('Edit')).toBeInTheDocument()
    expect(screen.getByText('src/i18n/locales/zh.json')).toBeInTheDocument()
    expect(screen.getByText('+1')).toBeInTheDocument()
    expect(screen.getByText('-1')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '允许一次' })).toBeInTheDocument()
  })

  it('does not render an authorization card for a normal edit tool row', () => {
    render(
      <HoverList
        sessions={[session({
          lastToolName: 'Edit',
          lastToolTarget: 'src/App.tsx +1 -1',
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByText('notch.tool.editing')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '允许一次' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '拒绝' })).not.toBeInTheDocument()
  })

  it('routes inline question options without opening the row', async () => {
    const onSessionClick = vi.fn()
    render(
      <HoverList
        sessions={[session({
          phase: 'waiting_input',
          pendingQuestion: { question: 'Pick a target', options: ['Preview', 'Ship'] },
        })]}
        onSessionClick={onSessionClick}
      />,
    )

    fireEvent.mouseDown(screen.getByText('Ship').closest('button')!)

    expect(tauriMocks.respondQuestion).toHaveBeenCalledWith('s1', 'Ship')
    expect(onSessionClick).not.toHaveBeenCalled()
  })

  it('routes inline multi-select question answers as joined text', () => {
    render(
      <HoverList
        sessions={[session({
          phase: 'waiting_input',
          pendingQuestion: {
            question: 'Pick targets',
            options: ['Preview', 'Docs', 'Production'],
            multiSelect: true,
          },
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    fireEvent.mouseDown(screen.getByText('Preview').closest('button')!)
    fireEvent.mouseDown(screen.getByText('Production').closest('button')!)
    fireEvent.mouseDown(screen.getByRole('button', { name: '确认 (2)' }))

    expect(tauriMocks.respondQuestion).toHaveBeenCalledWith('s1', 'Preview, Production')
  })

  it('routes inline multi-question answers as JSON', () => {
    render(
      <HoverList
        sessions={[session({
          phase: 'waiting_input',
          pendingQuestion: {
            question: '[Deploy] Choose options',
            options: ['Preview', 'Ship'],
            questions: [
              {
                question: 'Which target?',
                options: [{ label: 'Preview' }, { label: 'Ship' }],
                multiSelect: true,
              },
              {
                question: 'Notify channel?',
                options: [{ label: 'Yes' }, { label: 'No' }],
              },
            ],
          },
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    fireEvent.mouseDown(screen.getByText('Preview').closest('button')!)
    fireEvent.mouseDown(screen.getByText('Ship').closest('button')!)
    fireEvent.mouseDown(screen.getByText('No').closest('button')!)
    fireEvent.mouseDown(screen.getByRole('button', { name: '提交所有回答' }))

    expect(tauriMocks.respondQuestion).toHaveBeenCalledWith(
      's1',
      JSON.stringify({
        'Which target?': 'Preview, Ship',
        'Notify channel?': 'No',
      }),
    )
  })

  it('routes inline plan actions without opening the row', () => {
    const onSessionClick = vi.fn()
    render(
      <HoverList
        sessions={[session({
          phase: 'waiting_approval',
          planTitle: 'Implementation plan',
          planContent: '1. Fix the island',
        })]}
        onSessionClick={onSessionClick}
      />,
    )

    fireEvent.mouseDown(screen.getByRole('button', { name: 'Accept Edits' }))

    expect(tauriMocks.respondPlan).toHaveBeenCalledWith('s1', 'acceptEdits', undefined)
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
