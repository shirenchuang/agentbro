import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ChatView } from '../components/notch/ChatView'
import { useConfigStore } from '../stores/configStore'
import { useSessionStore } from '../stores/sessionStore'
import type { SessionState } from '../types/agent'

const tauriMocks = vi.hoisted(() => ({
  getChatHistory: vi.fn(() => Promise.resolve([])),
  getSubagentChatHistory: vi.fn(() => Promise.resolve([
    {
      id: 'u1',
      role: 'user',
      timestamp: null,
      blocks: [{ type: 'text', text: '请计算 1+1 等于几？直接给出答案即可。' }],
    },
    {
      id: 'a1',
      role: 'assistant',
      timestamp: null,
      blocks: [{ type: 'text', text: '2' }],
    },
  ])),
  jumpToTerminal: vi.fn(() => Promise.resolve()),
  respondAutoApprove: vi.fn(() => Promise.resolve()),
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
    getSubagentChatHistory: tauriMocks.getSubagentChatHistory,
    jumpToTerminal: tauriMocks.jumpToTerminal,
    respondAutoApprove: tauriMocks.respondAutoApprove,
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
      const translations: Record<string, string> = {
        'notch.subagents': '子代理',
        'notch.expandSubagents': '展开',
        'notch.collapseSubagents': '收起',
        'notch.typeMessage': '输入消息...',
        'notch.typeReply': 'Type reply...',
        'notch.planFeedback': 'Tell Claude what to change...',
        'notch.manualReview': 'Manual Review',
        'notch.acceptEdits': 'Accept Edits',
        'notch.autoApprovePerms': 'Auto',
        'notch.questionsCount': 'questions',
        'notch.multiSelect': 'Multi-select',
        'notch.submitAll': 'Submit All',
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
    startedAt: Date.now() - 10_000,
    duration: 10_000,
    tokens: { input: 0, output: 0, cacheRead: 0, cacheCreate: 0 },
    chatHistory: [],
    subagents: [],
    activeTools: [],
    sessionTitle: 'hi',
    pid: 1234,
    ...overrides,
  }
}

describe('ChatView subagent history', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    tauriMocks.getChatHistory.mockResolvedValue([])
    tauriMocks.getSubagentChatHistory.mockResolvedValue([
      {
        id: 'u1',
        role: 'user',
        timestamp: null,
        blocks: [{ type: 'text', text: '请计算 1+1 等于几？直接给出答案即可。' }],
      },
      {
        id: 'a1',
        role: 'assistant',
        timestamp: null,
        blocks: [{ type: 'text', text: '2' }],
      },
    ])
    useConfigStore.setState({
      contentFontSize: '13px',
      islandMonitorSubagents: true,
      showAgentActivityDetails: true,
    })
    useSessionStore.setState({
      sessions: {},
      sessionList: [],
      activeSessionId: null,
      panelState: 'expanded',
      activeOverlay: null,
      overlayQueue: [],
      rateLimits: undefined,
      hookNotification: null,
      wakeSilencedUntil: 0,
      focusedTerminal: null,
    })
  })

  it('keeps subagent identity visible and hides the main-session composer in readonly history', async () => {
    const current = session({
      subagents: [{
        agentId: 'agent-2',
        agentType: 'general-purpose',
        description: '计算1+1 #2',
        transcriptPath: '/tmp/main.jsonl',
        agentTranscriptPath: '/tmp/agent-2.jsonl',
        lastAssistantMessage: '2',
        startedAt: Date.now() - 2_000,
        completedAt: Date.now() - 1_000,
        status: 'completed',
        tools: [],
      }],
    })
    useSessionStore.setState({
      sessions: { [current.id]: current },
      sessionList: [current],
      activeSessionId: current.id,
    })

    render(<ChatView onBack={vi.fn()} initialSubagentId="agent-2" />)

    await waitFor(() => {
      expect(screen.getAllByText('计算1+1 #2').length).toBeGreaterThan(0)
    })
    expect(screen.getByText('general-purpose')).toBeInTheDocument()
    expect(screen.getByText('readonly')).toBeInTheDocument()
    expect(screen.queryByPlaceholderText('输入消息...')).not.toBeInTheDocument()

    const list = screen.getByText('1 子代理').closest('.subagent-list')
    expect(list).not.toBeNull()
    expect(within(list as HTMLElement).getByText('展开')).toBeInTheDocument()
    expect(within(list as HTMLElement).queryByText('2')).not.toBeInTheDocument()

    fireEvent.click(within(list as HTMLElement).getByRole('button', { name: 'Expand subagents' }))

    expect(within(list as HTMLElement).getByText('收起')).toBeInTheDocument()
    expect(within(list as HTMLElement).getByText('计算1+1 #2')).toBeInTheDocument()
    expect(within(list as HTMLElement).getByText('2')).toBeInTheDocument()
  })

  it('uses explicit Claude subagent names when available', async () => {
    const current = session({
      subagents: [{
        agentId: 'ae7a77784c43f40e1',
        name: 'calc-a',
        agentType: 'general-purpose',
        description: 'Calculate 1+1 (Agent A)',
        transcriptPath: '/tmp/main.jsonl',
        agentTranscriptPath: '/tmp/agent-a.jsonl',
        lastAssistantMessage: '2',
        startedAt: Date.now() - 2_000,
        completedAt: Date.now() - 1_000,
        status: 'completed',
        tools: [],
      }],
    })
    useSessionStore.setState({
      sessions: { [current.id]: current },
      sessionList: [current],
      activeSessionId: current.id,
    })

    render(<ChatView onBack={vi.fn()} initialSubagentId="ae7a77784c43f40e1" />)

    await waitFor(() => {
      expect(screen.getAllByText('@calc-a').length).toBeGreaterThan(0)
    })
    expect(screen.getAllByText('Calculate 1+1 (Agent A)').length).toBeGreaterThan(0)
  })

  it('shows plan-specific detail actions and routes them to plan modes', () => {
    const current = session({
      phase: 'waiting_approval',
      planTitle: 'Implementation plan',
      planContent: '1. Fix detail approval',
    })
    useSessionStore.setState({
      sessions: { [current.id]: current },
      sessionList: [current],
      activeSessionId: current.id,
    })

    render(<ChatView onBack={vi.fn()} />)

    expect(screen.getByRole('button', { name: 'Manual Review' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Accept Edits' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Auto' })).toBeInTheDocument()
    expect(screen.queryByText('notch.allowOnce')).not.toBeInTheDocument()

    fireEvent.mouseDown(screen.getByRole('button', { name: 'Accept Edits' }))

    expect(tauriMocks.respondPlan).toHaveBeenCalledWith('s1', 'acceptEdits')
    expect(tauriMocks.respondPermission).not.toHaveBeenCalled()
  })

  it('confirms detail multi-select questions instead of submitting the first clicked option', () => {
    const current = session({
      phase: 'waiting_input',
      pendingQuestion: {
        question: 'Pick targets',
        options: ['Preview', 'Docs', 'Production'],
        multiSelect: true,
      },
    })
    useSessionStore.setState({
      sessions: { [current.id]: current },
      sessionList: [current],
      activeSessionId: current.id,
    })

    render(<ChatView onBack={vi.fn()} />)

    fireEvent.mouseDown(screen.getByRole('button', { name: /Preview/ }))
    fireEvent.mouseDown(screen.getByRole('button', { name: /Production/ }))
    expect(tauriMocks.respondQuestion).not.toHaveBeenCalled()

    fireEvent.mouseDown(screen.getByRole('button', { name: 'Confirm (2)' }))

    expect(tauriMocks.respondQuestion).toHaveBeenCalledWith('s1', 'Preview, Production')
  })

  it('submits detail multi-question answers as AskUserQuestion JSON', () => {
    const current = session({
      phase: 'waiting_input',
      pendingQuestion: {
        question: '[Deploy] Choose options',
        options: ['Preview', 'Ship'],
        questions: [
          {
            header: 'Deploy',
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
    })
    useSessionStore.setState({
      sessions: { [current.id]: current },
      sessionList: [current],
      activeSessionId: current.id,
    })

    render(<ChatView onBack={vi.fn()} />)

    expect(screen.getByRole('button', { name: 'Submit All' })).toBeDisabled()
    fireEvent.mouseDown(screen.getByRole('button', { name: 'Preview' }))
    fireEvent.mouseDown(screen.getByRole('button', { name: 'Ship' }))
    fireEvent.mouseDown(screen.getByRole('button', { name: 'No' }))
    fireEvent.mouseDown(screen.getByRole('button', { name: 'Submit All' }))

    expect(tauriMocks.respondQuestion).toHaveBeenCalledWith(
      's1',
      JSON.stringify({
        'Which target?': 'Preview, Ship',
        'Notify channel?': 'No',
      }),
    )
  })
})
