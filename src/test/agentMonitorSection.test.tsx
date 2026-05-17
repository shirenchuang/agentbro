import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { SettingsApp } from '../components/settings'
import { AgentMonitorSection } from '../components/settings/sections/AgentMonitorSection'
import type { MonitorSessionDetail, MonitorSessionSummary } from '../services/monitorApi'
import { useSessionStore } from '../stores/sessionStore'

const monitorMocks = vi.hoisted(() => ({
  getMonitorSessions: vi.fn(),
  getMonitorSessionDetail: vi.fn(),
  getMonitorTimeline: vi.fn(),
  getNetworkMonitorStatus: vi.fn(),
  setNetworkMonitorEnabled: vi.fn(),
  getNetworkMonitorRequests: vi.fn(),
  getNetworkMonitorRequestDetail: vi.fn(),
}))

const tauriMocks = vi.hoisted(() => ({
  getChatHistory: vi.fn(),
  jumpToTerminal: vi.fn(() => Promise.resolve()),
}))

vi.mock('../services/monitorApi', () => ({
  getMonitorSessions: monitorMocks.getMonitorSessions,
  getMonitorSessionDetail: monitorMocks.getMonitorSessionDetail,
  getMonitorTimeline: monitorMocks.getMonitorTimeline,
  getNetworkMonitorStatus: monitorMocks.getNetworkMonitorStatus,
  setNetworkMonitorEnabled: monitorMocks.setNetworkMonitorEnabled,
  getNetworkMonitorRequests: monitorMocks.getNetworkMonitorRequests,
  getNetworkMonitorRequestDetail: monitorMocks.getNetworkMonitorRequestDetail,
}))

vi.mock('../services/tauriApi', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../services/tauriApi')>()
  return {
    ...actual,
    getChatHistory: tauriMocks.getChatHistory,
    jumpToTerminal: tauriMocks.jumpToTerminal,
  }
})

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: { defaultValue?: string }) => options?.defaultValue ?? key,
    i18n: { language: 'zh' },
  }),
}))

const summary: MonitorSessionSummary = {
  id: 'session-1',
  agentType: 'claude-code',
  engineLabel: null,
  project: 'agentbro',
  cwd: '/Users/me/code/agentbro',
  terminal: 'iTerm',
  phase: 'waiting_approval',
  startedAt: 1_700_000_000,
  duration: 125,
  tokenTotal: 4200,
  lastToolName: 'Edit',
  lastToolTarget: 'src/App.tsx',
  lastToolStatus: 'running',
  waitingUser: true,
  pendingKind: 'permission',
  subagentCount: 1,
  activeToolCount: 1,
  title: 'agentbro · monitor',
}

const detail = {
  session: {
    id: 'session-1',
    agentType: 'claude-code',
    engineLabel: null,
    engineConfigRoot: null,
    project: 'agentbro',
    cwd: '/Users/me/code/agentbro',
    terminal: 'iTerm',
    phase: 'waiting_approval',
    startedAt: 1_700_000_000,
    duration: 125,
    tokens: { input: 2000, output: 1200, cacheRead: 900, cacheCreate: 100 },
    rateLimits: { fiveHourUsage: 23, fiveHourRemaining: '4h', sevenDayUsage: 12, sevenDayRemaining: '6d' },
    statusLineText: null,
    contextWindow: { totalInputTokens: 2000, totalOutputTokens: 1200, contextWindowSize: 200000, usedPercentage: 2 },
    lastMainAgentAt: null,
    cacheTtlMs: null,
    pendingPermission: { toolName: 'Edit', toolInput: '{"file":"src/App.tsx"}', diff: null, options: null },
    pendingQuestion: null,
    pendingPlan: null,
    lastToolName: 'Edit',
    lastToolTarget: 'src/App.tsx',
    lastToolStatus: 'running',
    description: null,
    sessionTitle: 'agentbro · monitor',
    pid: 1234,
    tty: '/dev/ttys001',
    termBundleId: null,
    weztermPane: null,
    zellijPaneId: null,
    zellijSessionName: null,
    cmuxSurfaceId: null,
    cmuxWorkspaceId: null,
    subagents: [{
      agentId: 'sub-1',
      agentType: 'explorer',
      description: 'Inspect monitor wiring',
      transcriptPath: null,
      agentTranscriptPath: null,
      lastAssistantMessage: null,
      startedAt: 1_700_000_030,
      completedAt: null,
      status: 'running',
      tools: ['Read'],
    }],
    activeTools: [{
      toolUseId: 'tool-1',
      toolName: 'Edit',
      status: 'running',
      startedAt: 1_700_000_050,
      completedAt: null,
      error: null,
    }],
    tasks: [],
    isYoloMode: false,
    lastUserMessage: null,
    lastResponse: null,
    lastThought: null,
  },
  timeline: [{
    id: 'tool:session-1:tool-1',
    timestampMs: 1_700_000_050_000,
    kind: 'tool',
    title: 'Edit',
    detail: 'running',
    status: 'running',
    toolName: 'Edit',
    rawEventSeq: null,
  }],
  rawEvents: [{
    seq: 7,
    timestampMs: 1_700_000_051_000,
    sessionId: 'session-1',
    agent: 'claude-code',
    eventName: 'PreToolUse',
    raw: { event: 'PreToolUse', session_id: 'session-1', tool_name: 'Edit' },
  }],
} as MonitorSessionDetail

describe('AgentMonitorSection', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useSessionStore.setState({ sessions: {}, sessionList: [], activeSessionId: null })
    monitorMocks.getMonitorSessions.mockResolvedValue([summary])
    monitorMocks.getMonitorSessionDetail.mockResolvedValue(detail)
    monitorMocks.getMonitorTimeline.mockResolvedValue(detail.timeline)
    monitorMocks.getNetworkMonitorStatus.mockResolvedValue({
      enabled: false,
      proxyUrl: null,
      upstreamBaseUrl: 'https://api.anthropic.com',
      requestCount: 0,
      activeRequestCount: 0,
    })
    monitorMocks.setNetworkMonitorEnabled.mockResolvedValue({
      enabled: false,
      proxyUrl: null,
      upstreamBaseUrl: 'https://api.anthropic.com',
      requestCount: 0,
      activeRequestCount: 0,
    })
    monitorMocks.getNetworkMonitorRequests.mockResolvedValue([])
    monitorMocks.getNetworkMonitorRequestDetail.mockResolvedValue(null)
    tauriMocks.getChatHistory.mockResolvedValue([
      {
        id: 'msg-1',
        role: 'user',
        timestamp: '2026-05-17T00:00:00Z',
        blocks: [{ type: 'text', text: '请检查监控模块' }],
      },
    ])
  })

  it('shows monitor sessions and loads detail tabs', async () => {
    render(<AgentMonitorSection />)

    await waitFor(() => expect(screen.getAllByText('agentbro · monitor').length).toBeGreaterThan(0))
    expect(screen.getAllByText('Edit').length).toBeGreaterThan(0)
    expect(screen.getByText('权限')).toBeInTheDocument()
    expect(screen.getByText('原生网络请求监控')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '工具时间线' }))
    await waitFor(() => expect(screen.getAllByText('running').length).toBeGreaterThan(0))

    fireEvent.click(screen.getByRole('button', { name: '对话' }))
    await waitFor(() => expect(screen.getByText('请检查监控模块')).toBeInTheDocument())
    expect(tauriMocks.getChatHistory).toHaveBeenCalledWith('session-1')

    fireEvent.click(screen.getByRole('button', { name: 'Raw 事件' }))
    await waitFor(() => expect(screen.getByText('PreToolUse')).toBeInTheDocument())
  })

  it('keeps native request monitoring off by default until manually enabled', async () => {
    monitorMocks.setNetworkMonitorEnabled.mockResolvedValue({
      enabled: true,
      proxyUrl: 'http://127.0.0.1:4567',
      upstreamBaseUrl: 'https://api.anthropic.com',
      requestCount: 0,
      activeRequestCount: 0,
    })

    render(<AgentMonitorSection />)

    await waitFor(() => expect(screen.getByRole('switch')).toHaveAttribute('aria-checked', 'false'))
    expect(screen.getByText('关闭')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('switch'))
    await waitFor(() => expect(monitorMocks.setNetworkMonitorEnabled).toHaveBeenCalledWith(true, 'https://api.anthropic.com'))
    expect(await screen.findByText('已开启')).toBeInTheDocument()
    expect(screen.getByText('ANTHROPIC_BASE_URL=http://127.0.0.1:4567 claude')).toBeInTheDocument()
  })

  it('is reachable from the top-level settings menu and has an empty state', async () => {
    monitorMocks.getMonitorSessions.mockResolvedValue([])
    monitorMocks.getMonitorSessionDetail.mockResolvedValue(null)

    render(<SettingsApp onClose={vi.fn()} />)

    fireEvent.click(screen.getByText('settings.agentMonitor'))
    await waitFor(() => expect(screen.getByText('暂无匹配的 Agent 会话。')).toBeInTheDocument())

    fireEvent.click(screen.getByText('settings.agents'))
    expect(screen.queryByRole('button', { name: /Agent监控|settings\.agentMonitor/ })).not.toBeInTheDocument()
  })
})
