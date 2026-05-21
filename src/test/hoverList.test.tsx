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
      const translations: Record<string, string> = {
        'notch.tool.compactingContext': '压缩上下文',
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

  it('opens session detail on mouse down so hover state cannot swallow the click', () => {
    const onSessionClick = vi.fn()
    render(<HoverList sessions={[session()]} onSessionClick={onSessionClick} />)

    fireEvent.mouseDown(screen.getByText('agent-island · Fix island interactions'), { button: 0 })

    expect(onSessionClick).toHaveBeenCalledWith('s1')
  })

  it('opens a right-click silence menu without opening the session', () => {
    const onSessionClick = vi.fn()
    const onSilenceDirectory = vi.fn()
    const onSilencePrompt = vi.fn()
    const current = session({ cwd: '/tmp/agent-island' })
    render(
      <HoverList
        sessions={[current]}
        onSessionClick={onSessionClick}
        onSilenceDirectory={onSilenceDirectory}
        onSilencePrompt={onSilencePrompt}
      />,
    )

    fireEvent.contextMenu(screen.getByText('agent-island · Fix island interactions'))

    expect(onSessionClick).not.toHaveBeenCalled()
    expect(screen.getByRole('menu')).toBeInTheDocument()

    fireEvent.click(screen.getByText('Hide this directory'))

    expect(onSilenceDirectory).toHaveBeenCalledWith(current)
    expect(onSilencePrompt).not.toHaveBeenCalled()
  })

  it('shows named subagents and opens their history rows', () => {
    const onSubagentClick = vi.fn()
    const current = session({
      subagents: [{
        agentId: 'ae7a77784c43f40e1',
        name: 'calc-a',
        agentType: 'general-purpose',
        description: 'Calculate 1+1 (Agent A)',
        transcriptPath: '/tmp/main.jsonl',
        agentTranscriptPath: '/tmp/agent-ae7a77784c43f40e1.jsonl',
        lastAssistantMessage: '2',
        startedAt: Date.now() - 2_000,
        completedAt: Date.now() - 1_000,
        status: 'completed',
        tools: [],
      }],
    })

    render(
      <HoverList
        sessions={[current]}
        onSessionClick={vi.fn()}
        onSubagentClick={onSubagentClick}
      />,
    )

    expect(screen.getByText('Subagents (1)')).toBeInTheDocument()
    expect(screen.getByText('@calc-a')).toBeInTheDocument()
    expect(screen.getByText('Calculate 1+1 (Agent A)')).toBeInTheDocument()
    expect(screen.getByText('完成')).toHaveClass('hover-list__subagent-status--completed')

    fireEvent.click(screen.getByText('@calc-a'))

    expect(onSubagentClick).toHaveBeenCalledWith('s1', current.subagents[0])
  })

  it('hides completed subagents in the session list after a newer user message', () => {
    const now = Date.now()
    render(
      <HoverList
        sessions={[session({
          lastUserMessage: 'Next task please',
          lastUserMessageAt: now,
          subagents: [{
            agentId: 'old-agent',
            name: 'old-work',
            agentType: 'explorer',
            description: 'Explore old task',
            startedAt: now - 8_000,
            completedAt: now - 5_000,
            status: 'completed',
            tools: [],
            lastAssistantMessage: 'Old work complete.',
          }],
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.queryByText('Subagents (1)')).not.toBeInTheDocument()
    expect(screen.queryByText('@old-work')).not.toBeInTheDocument()
  })

  it('keeps running subagents visible even after a newer user message', () => {
    const now = Date.now()
    render(
      <HoverList
        sessions={[session({
          lastUserMessage: 'Next task please',
          lastUserMessageAt: now,
          subagents: [{
            agentId: 'running-agent',
            name: 'active-work',
            agentType: 'explorer',
            description: 'Explore current task',
            startedAt: now - 8_000,
            status: 'running',
            tools: ['Read'],
          }],
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByText('Subagents (1)')).toBeInTheDocument()
    expect(screen.getByText('@active-work')).toBeInTheDocument()
    expect(screen.getByText('运行中')).toHaveClass('hover-list__subagent-status--running')
  })

  it('summarizes running subagents like a child-agent team', () => {
    render(
      <HoverList
        sessions={[session({
          subagents: [
            {
              agentId: 'agent-a',
              name: 'ui-state',
              agentType: 'explorer',
              description: 'Map Vibe states',
              startedAt: Date.now() - 2_000,
              status: 'running',
              tools: ['Read'],
            },
            {
              agentId: 'agent-b',
              name: 'parser',
              agentType: 'worker',
              description: 'Parse transcript sidechains',
              startedAt: Date.now() - 3_000,
              completedAt: Date.now() - 1_000,
              status: 'completed',
              tools: [],
              lastAssistantMessage: 'Sidechain transcript is ready.',
            },
          ],
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByText('Subagents (2)')).toBeInTheDocument()
    expect(screen.getByText('@ui-state')).toBeInTheDocument()
    expect(screen.getByText('运行中')).toHaveClass('hover-list__subagent-status--running')
    expect(screen.getByText('@parser')).toBeInTheDocument()
    expect(screen.getByText('Parse transcript sidechains')).toBeInTheDocument()
    expect(screen.getByText('完成')).toHaveClass('hover-list__subagent-status--completed')
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

  it('hides jump for recovered sessions without terminal metadata', () => {
    render(
      <HoverList
        sessions={[session({ terminal: '', pid: undefined, tty: undefined, termBundleId: undefined })]}
        onSessionClick={vi.fn()}
        onJumpToTerminal={vi.fn()}
      />,
    )

    expect(screen.queryByRole('button', { name: 'notch.jumpToTerminal' })).not.toBeInTheDocument()
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
    expect(screen.getByText('Codex')).toBeInTheDocument()
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

  it('shows terminal program badges even when the terminal is not in the color table', () => {
    render(
      <HoverList
        sessions={[session({ terminal: 'Ghostty' })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByText('Ghostty')).toHaveClass('hover-list__terminal-badge')
  })

  it('shows iTerm terminal badges with the terminal style', () => {
    render(
      <HoverList
        sessions={[session({ terminal: 'iTerm' })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByText('iTerm')).toHaveClass('hover-list__terminal-badge')
  })

  it('derives terminal badges from bundle ids when terminal is only a tty', () => {
    render(
      <HoverList
        sessions={[session({ terminal: '/dev/ttys001', termBundleId: 'com.googlecode.iterm2' })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByText('iTerm2')).toHaveClass('hover-list__terminal-badge')
    expect(screen.queryByText('/dev/ttys001')).not.toBeInTheDocument()
  })

  it('falls back to TERM_PROGRAM when bundle id is missing', () => {
    render(
      <HoverList
        sessions={[session({ terminal: '/dev/ttys001', termProgram: 'Ghostty' })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByText('Ghostty')).toHaveClass('hover-list__terminal-badge')
    expect(screen.queryByText('/dev/ttys001')).not.toBeInTheDocument()
  })

  it('keeps bare tty values out of the session list badges', () => {
    render(
      <HoverList
        sessions={[session({ terminal: '/dev/ttys001', termBundleId: undefined })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.queryByText('/dev/ttys001')).not.toBeInTheDocument()
    expect(document.querySelector('.hover-list__terminal-badge')).not.toBeInTheDocument()
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

  it('shows generic working text for processing sessions without a tool', () => {
    render(
      <HoverList
        sessions={[session({
          phase: 'processing',
          lastToolName: undefined,
          description: 'Processing user input: Please fix the island',
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByText('notch.working')).toBeInTheDocument()
    expect(screen.queryByText('Processing user input: Please fix the island')).not.toBeInTheDocument()
  })

  it('shows done and needs-attention list ribbons', () => {
    render(
      <HoverList
        sessions={[
          session({ id: 'done', phase: 'done', sessionTitle: 'Completed task' }),
          session({ id: 'error', phase: 'error', sessionTitle: 'Failed task', description: 'Hook failed' }),
        ]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByText('Done')).toBeInTheDocument()
    expect(screen.getByText('Needs attention')).toBeInTheDocument()
  })

  it('localizes compacting context tool labels', () => {
    render(
      <HoverList
        sessions={[session({
          phase: 'compacting',
          lastToolName: 'Compacting context',
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByText('压缩上下文')).toHaveClass('hover-list__tool-label--compact')
    expect(screen.queryByText('Compacting context')).not.toBeInTheDocument()
  })

  it('shows compacting context from a processing description without a tool label', () => {
    render(
      <HoverList
        sessions={[session({
          phase: 'processing',
          lastToolName: undefined,
          description: 'Compacting conversation...',
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByText('压缩上下文')).toHaveClass('hover-list__tool-label--compact')
    expect(screen.queryByText('Compacting conversation...')).not.toBeInTheDocument()
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

  it('shows only file names for path-based tool targets in the session list', () => {
    render(
      <HoverList
        sessions={[session({
          lastToolName: 'Read',
          lastToolTarget: '/Users/shirenchuang/code/aidelivery/vibeIsland/agentbro/src-tauri/src/hooks/tool_processor.rs',
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByText('tool_processor.rs')).toBeInTheDocument()
    expect(screen.queryByText(/\/Users\/shirenchuang\/code/)).not.toBeInTheDocument()
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
          agentType: 'claude-code',
          phase: 'waiting_approval',
          pendingPermission: { toolName: 'Bash', toolInput: 'pnpm test' },
        })]}
        onSessionClick={onSessionClick}
      />,
    )

    const permissionCard = document.querySelector('.hover-list__inline-perm') as HTMLElement
    fireEvent.click(within(permissionCard).getByRole('button', { name: '允许一次' }))
    fireEvent.click(within(permissionCard).getByRole('button', { name: /始终允许/ }))
    fireEvent.click(within(permissionCard).getByRole('button', { name: '自动批准' }))
    fireEvent.click(within(permissionCard).getByRole('button', { name: '拒绝' }))

    expect(tauriMocks.respondPermission).toHaveBeenNthCalledWith(1, 's1', true)
    expect(tauriMocks.respondPermission).toHaveBeenNthCalledWith(2, 's1', true, true)
    expect(tauriMocks.respondAutoApprove).toHaveBeenCalledWith('s1')
    expect(tauriMocks.respondPermission).toHaveBeenNthCalledWith(3, 's1', false)
    expect(onSessionClick).not.toHaveBeenCalled()
  })

  it('hides unsupported persistent permission actions for Codex hooks', () => {
    render(
      <HoverList
        sessions={[session({
          agentType: 'codex',
          phase: 'waiting_approval',
          pendingPermission: { toolName: 'Bash', toolInput: 'pnpm test' },
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    const permissionCard = document.querySelector('.hover-list__inline-perm') as HTMLElement
    expect(within(permissionCard).getByRole('button', { name: '允许一次' })).toBeInTheDocument()
    expect(within(permissionCard).getByRole('button', { name: '拒绝' })).toBeInTheDocument()
    expect(within(permissionCard).queryByRole('button', { name: /始终允许/ })).not.toBeInTheDocument()
    expect(within(permissionCard).queryByRole('button', { name: '自动批准' })).not.toBeInTheDocument()
  })

  it('shows write permission content in the inline authorization card', () => {
    render(
      <HoverList
        sessions={[session({
          phase: 'waiting_approval',
          pendingPermission: {
            toolName: 'Write',
            toolInput: JSON.stringify({
              file_path: '/Users/demo/github/empty/package.json',
              content: '{\n  "name": "vibe-island-auth",\n  "version": "1.0.0",\n  "private": true,\n  "type": "module"\n}',
            }),
          },
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByText('.../github/empty/package.json')).toHaveClass('hover-list__inline-perm-path')
    expect(screen.getByText('new file')).toHaveClass('hover-list__inline-perm-new-badge')
    expect(screen.getByText(/"name": "vibe-island-auth"/)).toHaveClass('hover-list__inline-perm-code')
    expect(screen.queryByText('/Users/demo/github/empty/package.json')).not.toBeInTheDocument()
  })

  it('shows permission overlay content inline when returning to the session list', () => {
    useSessionStore.setState({
      activeOverlay: {
        id: 'permission-s1-overlay',
        sessionId: 's1',
        type: 'permission',
        data: {
          toolName: 'Write',
          toolInput: JSON.stringify({
            file_path: '/Users/demo/github/empty/package.json',
            content: '{\n  "name": "overlay-auth",\n  "private": true\n}',
          }),
        },
        createdAt: Date.now(),
      },
    })

    render(
      <HoverList
        sessions={[session({ phase: 'processing', pendingPermission: undefined })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByText('.../github/empty/package.json')).toHaveClass('hover-list__inline-perm-path')
    expect(screen.getByText(/"name": "overlay-auth"/)).toHaveClass('hover-list__inline-perm-code')
    expect(screen.getByRole('button', { name: '允许一次' })).toBeInTheDocument()
  })

  it('gives inline plan previews more readable content and subdued permission text', () => {
    const longPlan = `${'SwitchProviderEditor.tsx '.repeat(12)}final-visible-fragment`
    render(
      <HoverList
        sessions={[session({
          phase: 'waiting_approval',
          planTitle: 'Agent Switch UI 中文化 + 交互重设计',
          planContent: longPlan,
          planPermissions: ['Bash: run cargo check', 'Bash: run pnpm dev'],
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(document.querySelector('.hover-list__inline-plan-content')?.textContent).toContain('final-visible-fragment')
    expect(screen.getByText('请求的权限:')).toHaveClass('hover-list__inline-plan-perms-label')
    expect(screen.getAllByText('Bash')[0]).toHaveClass('hover-list__inline-plan-perm-tool')
    expect(screen.getByText(/run cargo check/)).toBeInTheDocument()
  })

  it('renders inline plan markdown and uses plan-specific action styles', () => {
    render(
      <HoverList
        sessions={[session({
          phase: 'waiting_approval',
          planTitle: 'Implementation plan',
          planContent: '### Steps\n1. **Render Markdown**\n\n```ts\nconst ok = true\n```',
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByRole('heading', { level: 3, name: 'Steps' })).toBeInTheDocument()
    expect(screen.getByText('Render Markdown').tagName).toBe('STRONG')
    expect(screen.getByText('const ok = true')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Send Feedback' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Manual Review' })).toHaveClass('hover-list__inline-plan-btn--feedback')
    expect(screen.getByRole('button', { name: 'Accept Edits' })).toHaveClass('hover-list__inline-plan-btn--accept')
    expect(screen.getByRole('button', { name: 'Auto' })).toHaveClass('hover-list__inline-plan-btn--auto')
  })

  it('keeps inline plan previews visible when the active plan overlay is folded into the list', () => {
    useSessionStore.setState({
      activeOverlay: {
        id: 'plan-s1-overlay',
        sessionId: 's1',
        type: 'plan',
        data: {
          planTitle: 'Implementation plan',
          planContent: '1. Keep the list visible',
          requestedPermissions: ['Bash: run tests'],
        },
        createdAt: Date.now(),
      },
    })

    render(
      <HoverList
        sessions={[session({
          phase: 'waiting_approval',
          planTitle: 'Implementation plan',
          planContent: '1. Keep the list visible',
          planPermissions: ['Bash: run tests'],
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(document.querySelector('.hover-list__inline-plan')?.textContent).toContain('Implementation plan')
    expect(document.querySelector('.hover-list__inline-plan')?.textContent).toContain('Keep the list visible')
    expect(screen.getByText('Bash')).toHaveClass('hover-list__inline-plan-perm-tool')
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

  it('renders terminal-routed approval notices with a jump action', () => {
    const onJumpToTerminal = vi.fn()
    const onSessionClick = vi.fn()
    render(
      <HoverList
        sessions={[session({
          phase: 'waiting_approval',
          description: 'Continue in Terminal to approve this command.',
          notice: {
            kind: 'terminal_approval',
            title: 'Continue in Terminal',
            detail: 'Approval is delegated to the active tab',
            actionLabel: 'Go to Terminal',
          },
        })]}
        onSessionClick={onSessionClick}
        onJumpToTerminal={onJumpToTerminal}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Go to Terminal' }))

    expect(screen.getByText('Continue in Terminal')).toBeInTheDocument()
    expect(screen.getByText('Approval is delegated to the active tab')).toBeInTheDocument()
    expect(onJumpToTerminal).toHaveBeenCalledWith('s1')
    expect(onSessionClick).not.toHaveBeenCalled()
  })

  it('renders compact task rows with local expand interaction', () => {
    const onSessionClick = vi.fn()
    render(
      <HoverList
        sessions={[session({
          tasks: [
            { id: '1', name: 'Map Vibe SessionCardView', status: 'completed' },
            { id: '2', name: 'Check TerminalApprovalHintView', status: 'in_progress' },
            { id: '3', name: 'Preserve question overlay', status: 'pending' },
            { id: '4', name: 'Preserve plan overlay', status: 'pending' },
            { id: '5', name: 'Preserve subagent history', status: 'pending' },
            { id: '6', name: 'Ship QA checklist', status: 'pending' },
          ],
        })]}
        onSessionClick={onSessionClick}
      />,
    )

    expect(screen.getByText('(1 已完成, 1 进行中, 4 待处理)')).toBeInTheDocument()
    expect(screen.getByText('Map Vibe SessionCardView')).toHaveClass('hover-list__task-subject--done')
    expect(screen.queryByText('Ship QA checklist')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '查看更多 (1 项)' }))

    expect(screen.getByText('Ship QA checklist')).toBeInTheDocument()
    expect(onSessionClick).not.toHaveBeenCalled()
  })

  it('renders setup and trust notices without replacing deep session rows', () => {
    render(
      <HoverList
        sessions={[
          session({
            id: 'restart',
            phase: 'idle',
            sessionTitle: 'Restart required',
            notice: {
              kind: 'restart',
              title: 'Restart your sessions',
              detail: 'Hooks just installed - restart running sessions to connect',
            },
          }),
          session({
            id: 'trust',
            phase: 'waiting_approval',
            sessionTitle: 'Codex trust',
            notice: {
              kind: 'trust',
              title: 'Codex updated - confirm authorization',
              detail: 'Confirm once to keep approvals working',
            },
          }),
        ]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByText('Restart your sessions')).toBeInTheDocument()
    expect(screen.getByText('Codex updated - confirm authorization')).toBeInTheDocument()
    expect(screen.getByText('Hooks just installed - restart running sessions to connect')).toBeInTheDocument()
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

  it('renders a compact diff preview for active edit tools without approval actions', () => {
    render(
      <HoverList
        sessions={[session({
          lastToolName: 'Edit',
          lastToolTarget: 'src/App.tsx +1 -1',
          activeTools: [{
            toolUseId: 'edit-1',
            toolName: 'Edit',
            status: 'running',
            startedAt: Date.now() - 1000,
            diff: {
              filePath: 'src/App.tsx',
              lines: [
                { type: 'remove', lineNumber: 10, content: 'const label = "old"' },
                { type: 'add', lineNumber: 10, content: 'const label = "new"' },
              ],
            },
          }],
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    expect(screen.getByText('App.tsx')).toHaveClass('hover-list__tool-target-name')
    expect(screen.getByText('src/App.tsx')).toBeInTheDocument()
    expect(screen.getByText('const label = "old"')).toBeInTheDocument()
    expect(screen.getByText('const label = "new"')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '允许一次' })).not.toBeInTheDocument()
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

  it('renders inline question options as compact question cards with descriptions', () => {
    render(
      <HoverList
        sessions={[session({
          phase: 'waiting_input',
          pendingQuestion: {
            question: 'Pick a view',
            options: ['Overlay', 'Detail', 'Compact'],
            descriptions: ['Floating prompt', 'Expanded detail', 'Dense summary'],
          },
        })]}
        onSessionClick={vi.fn()}
      />,
    )

    const detailOption = screen.getByText('Detail').closest('button')
    expect(detailOption).toHaveClass('hover-list__inline-question-opt')
    expect(detailOption?.querySelector('.hover-list__inline-question-opt-index')?.textContent).toBe('2')
    expect(screen.getByText('Expanded detail')).toHaveClass('hover-list__inline-question-opt-desc')
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
    fireEvent.mouseDown(screen.getByRole('button', { name: '✓ 提交所有回答' }))

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

  it('sends inline plan feedback from the input row without opening the row', () => {
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

    const input = screen.getByPlaceholderText('Tell Claude what to change...')
    fireEvent.mouseDown(input)
    fireEvent.change(input, { target: { value: 'Please revise the scope' } })
    fireEvent.mouseDown(screen.getByRole('button', { name: 'Send Feedback' }))

    expect(tauriMocks.respondPlan).toHaveBeenCalledWith('s1', 'feedback', 'Please revise the scope')
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
