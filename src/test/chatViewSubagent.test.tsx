import { render, screen, waitFor, within } from '@testing-library/react'
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
        'notch.typeMessage': '输入消息...',
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
})
