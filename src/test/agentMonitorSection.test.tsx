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
  getClaudeWrapperStatus: vi.fn(),
  installClaudeWrapper: vi.fn(),
  removeClaudeWrapper: vi.fn(),
  setNetworkMonitorEnabled: vi.fn(),
  getNetworkMonitorRequests: vi.fn(),
  getNetworkMonitorRequestDetail: vi.fn(),
}))

const tauriMocks = vi.hoisted(() => ({
  getChatHistory: vi.fn(),
  jumpToTerminal: vi.fn(() => Promise.resolve()),
  openSystemPath: vi.fn(() => Promise.resolve()),
}))

vi.mock('../services/monitorApi', () => ({
  getMonitorSessions: monitorMocks.getMonitorSessions,
  getMonitorSessionDetail: monitorMocks.getMonitorSessionDetail,
  getMonitorTimeline: monitorMocks.getMonitorTimeline,
  getNetworkMonitorStatus: monitorMocks.getNetworkMonitorStatus,
  getClaudeWrapperStatus: monitorMocks.getClaudeWrapperStatus,
  installClaudeWrapper: monitorMocks.installClaudeWrapper,
  removeClaudeWrapper: monitorMocks.removeClaudeWrapper,
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
    openSystemPath: tauriMocks.openSystemPath,
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
    termProgram: null,
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
      name: null,
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
  transcriptPath: '/Users/me/.claude/projects/agentbro/session-1.jsonl',
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
    monitorMocks.getClaudeWrapperStatus.mockResolvedValue({
      installed: false,
      shimPath: '/Users/me/.agentbro/bin/claude',
      pathHintInstalled: false,
      shellConfigPath: '/Users/me/.zshrc',
    })
    monitorMocks.installClaudeWrapper.mockResolvedValue({
      installed: true,
      shimPath: '/Users/me/.agentbro/bin/claude',
      pathHintInstalled: true,
      shellConfigPath: '/Users/me/.zshrc',
    })
    monitorMocks.removeClaudeWrapper.mockResolvedValue({
      installed: false,
      shimPath: '/Users/me/.agentbro/bin/claude',
      pathHintInstalled: false,
      shellConfigPath: '/Users/me/.zshrc',
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
    await waitFor(() => expect(screen.getByRole('button', { name: '打开 JSON' })).toBeEnabled())

    fireEvent.click(screen.getByRole('button', { name: '打开 JSON' }))
    await waitFor(() => expect(tauriMocks.openSystemPath).toHaveBeenCalledWith('/Users/me/.claude/projects/agentbro/session-1.jsonl'))

    fireEvent.click(screen.getByRole('button', { name: '打开目录' }))
    await waitFor(() => expect(tauriMocks.openSystemPath).toHaveBeenCalledWith('/Users/me/.claude/projects/agentbro'))

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

    render(<AgentMonitorSection activeView="access" />)

    await waitFor(() => expect(screen.getByRole('switch')).toHaveAttribute('aria-checked', 'false'))
    expect(screen.getByText('关闭')).toBeInTheDocument()
    expect(screen.getByText('Claude 命令无感接入')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('switch'))
    await waitFor(() => expect(monitorMocks.setNetworkMonitorEnabled).toHaveBeenCalledWith(true, 'https://api.anthropic.com'))
    expect(await screen.findByText('已开启')).toBeInTheDocument()
    expect(screen.getByText('ANTHROPIC_BASE_URL=http://127.0.0.1:4567 claude')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '安装接入' }))
    await waitFor(() => expect(monitorMocks.installClaudeWrapper).toHaveBeenCalled())
    expect(await screen.findByText('移除接入')).toBeInTheDocument()
  })

  it('renders classified network request detail tabs', async () => {
    monitorMocks.getNetworkMonitorStatus.mockResolvedValue({
      enabled: true,
      proxyUrl: 'http://127.0.0.1:4567',
      upstreamBaseUrl: 'https://api.anthropic.com',
      requestCount: 1,
      activeRequestCount: 0,
    })
    monitorMocks.getNetworkMonitorRequests.mockResolvedValue([{
      id: 'req-1',
      timestampMs: 1_700_000_052_000,
      provider: 'anthropic',
      method: 'POST',
      url: '/v1/messages',
      upstreamUrl: 'https://api.anthropic.com/v1/messages',
      sessionId: null,
      project: null,
      model: 'claude-sonnet-4',
      status: 200,
      durationMs: 321,
      requestBytes: 1234,
      responseBytes: 2345,
      isStream: true,
      mainAgent: true,
      requestType: 'MainAgent',
      requestSubType: null,
      messageCount: 2,
      toolCount: 1,
      systemPreview: 'You are Claude Code.',
      usage: {
        input_tokens: 100,
        output_tokens: 20,
        cache_creation_input_tokens: 10,
        cache_read_input_tokens: 40,
      },
      usageSummary: {
        inputTokens: 100,
        outputTokens: 20,
        cacheCreationInputTokens: 10,
        cacheReadInputTokens: 40,
        totalTokens: 170,
        cacheHitRate: 80,
      },
      error: null,
      inProgress: false,
    }])
    monitorMocks.getNetworkMonitorRequestDetail.mockResolvedValue({
      summary: {
        id: 'req-1',
        timestampMs: 1_700_000_052_000,
        provider: 'anthropic',
        method: 'POST',
        url: '/v1/messages',
        upstreamUrl: 'https://api.anthropic.com/v1/messages',
        sessionId: null,
        project: null,
        model: 'claude-sonnet-4',
        status: 200,
        durationMs: 321,
        requestBytes: 1234,
        responseBytes: 2345,
        isStream: true,
        mainAgent: true,
        requestType: 'MainAgent',
        requestSubType: null,
        messageCount: 2,
        toolCount: 1,
        systemPreview: 'You are Claude Code.',
        usage: null,
        usageSummary: null,
        error: null,
        inProgress: false,
      },
      requestHeaders: { authorization: '****' },
      requestBody: {
        model: 'claude-sonnet-4',
        system: [{ type: 'text', text: 'You are Claude Code.' }],
        messages: [{ role: 'user', content: 'hello' }],
        tools: [{ name: 'Edit', description: 'edit files' }],
      },
      responseHeaders: { 'content-type': 'text/event-stream' },
      responseBody: 'data: {"type":"message_delta","usage":{"output_tokens":20}}\n',
      responseBodyTruncated: false,
      streamEventCount: 1,
    })

    render(<AgentMonitorSection activeView="capture" />)

    await waitFor(() => expect(screen.getByText('已开启')).toBeInTheDocument())
    await waitFor(() => expect(screen.getAllByText('MainAgent').length).toBeGreaterThan(0))
    expect(screen.getByText('cache read 40')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Tools' }))
    await waitFor(() => expect(screen.getAllByText('Edit').length).toBeGreaterThan(0))

    fireEvent.click(screen.getByRole('button', { name: 'Response' }))
    expect(await screen.findByText('event #1')).toBeInTheDocument()
  })

  it('is reachable from the top-level settings menu and has an empty state', async () => {
    monitorMocks.getMonitorSessions.mockResolvedValue([])
    monitorMocks.getMonitorSessionDetail.mockResolvedValue(null)

    render(<SettingsApp onClose={vi.fn()} />)

    fireEvent.click(screen.getByText('settings.agentMonitor'))
    await waitFor(() => expect(screen.getByText('请求抓包')).toBeInTheDocument())
    fireEvent.click(screen.getByText('会话追踪'))
    await waitFor(() => expect(screen.getByText('暂无匹配的 Agent 会话。')).toBeInTheDocument())

    fireEvent.click(screen.getByText('‹ settings.title'))
    fireEvent.click(screen.getByText('settings.agents'))
    expect(screen.queryByRole('button', { name: /Agent监控|settings\.agentMonitor/ })).not.toBeInTheDocument()
  })
})
