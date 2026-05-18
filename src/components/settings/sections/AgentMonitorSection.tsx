import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  getNetworkMonitorRequestDetail,
  getNetworkMonitorRequests,
  getNetworkMonitorStatus,
  getClaudeWrapperStatus,
  getMonitorSessionDetail,
  getMonitorSessions,
  installClaudeWrapper,
  removeClaudeWrapper,
  setNetworkMonitorEnabled,
  type ClaudeWrapperStatus,
  type MonitorSessionDetail,
  type MonitorSessionSummary,
  type MonitorTimelineItem,
  type NetworkMonitorStatus,
  type NetworkRequestDetail,
  type NetworkRequestSummary,
} from '../../../services/monitorApi'
import { getChatHistory, jumpToTerminal, openSystemPath } from '../../../services/tauriApi'
import { mapParsedMessages } from '../../../hooks/useTauri'
import { selectSessionList, useSessionStore } from '../../../stores/sessionStore'
import type { BackendSession } from '../../../services/tauriApi'
import type { ChatMessage, SessionState, TokenUsage } from '../../../types/agent'
import { formatDurationShort } from '../../../utils/time'
import { formatTokens } from '../../../utils/tokens'
import type { MonitorSettingsView } from '../../../types/capability'
import { Toggle } from '../Toggle'
import './AgentMonitorSection.css'

type DetailTab = 'overview' | 'network' | 'conversation' | 'timeline' | 'approvals' | 'raw'
type DetailSession = BackendSession | SessionState
type NetworkDetailTab = 'system' | 'messages' | 'tools' | 'response' | 'headers' | 'raw'

const DETAIL_TABS: Array<{ id: DetailTab; label: string }> = [
  { id: 'overview', label: '概览' },
  { id: 'network', label: '网络请求' },
  { id: 'conversation', label: '对话' },
  { id: 'timeline', label: '工具时间线' },
  { id: 'approvals', label: '审批与问题' },
  { id: 'raw', label: 'Raw 事件' },
]

const DEFAULT_NETWORK_STATUS: NetworkMonitorStatus = {
  enabled: false,
  proxyUrl: null,
  upstreamBaseUrl: 'https://api.anthropic.com',
  requestCount: 0,
  activeRequestCount: 0,
}

const DEFAULT_WRAPPER_STATUS: ClaudeWrapperStatus = {
  installed: false,
  shimPath: '~/.agentbro/bin/claude',
  pathHintInstalled: false,
  shellConfigPath: '~/.zshrc',
}

function agentLabel(agentType: string, engineLabel?: string | null) {
  if (engineLabel && agentType === 'claude-code' && engineLabel !== 'Claude Code') return engineLabel
  if (agentType === 'claude-code') return 'Claude'
  if (agentType === 'gemini-cli') return 'Gemini'
  return agentType
    .split(/[-_]/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ')
}

function phaseLabel(phase?: string) {
  switch (phase) {
    case 'processing': return '运行中'
    case 'waiting_approval': return '等审批'
    case 'waiting_input': return '等输入'
    case 'compacting': return '压缩上下文'
    case 'done': return '完成'
    case 'error': return '错误'
    case 'interrupted': return '已中断'
    case 'idle':
    default: return '空闲'
  }
}

function pendingLabel(kind?: string | null) {
  if (kind === 'permission') return '权限'
  if (kind === 'question') return '问题'
  if (kind === 'plan') return '计划'
  return kind || ''
}

function totalTokens(tokens: TokenUsage | BackendSession['tokens'] | undefined) {
  if (!tokens) return 0
  return tokens.input + tokens.output + tokens.cacheRead + tokens.cacheCreate
}

function summaryFromSession(session: SessionState): MonitorSessionSummary {
  const pendingKind = session.pendingPermission
    ? 'permission'
    : session.pendingQuestion
      ? 'question'
      : session.planContent
        ? 'plan'
        : null
  return {
    id: session.id,
    agentType: session.agentType,
    engineLabel: session.engineLabel ?? null,
    project: session.project,
    cwd: session.cwd ?? '',
    terminal: session.terminal,
    phase: session.phase,
    startedAt: session.startedAt,
    duration: session.duration,
    tokenTotal: totalTokens(session.tokens),
    lastToolName: session.lastToolName ?? null,
    lastToolTarget: session.lastToolTarget ?? null,
    lastToolStatus: session.lastToolStatus ?? null,
    waitingUser: pendingKind !== null,
    pendingKind,
    subagentCount: session.subagents.length,
    activeToolCount: session.activeTools.filter((tool) => tool.status === 'running').length,
    title: session.sessionTitle ?? null,
  }
}

function formatTime(timestampMs?: number | null) {
  if (!timestampMs) return '-'
  return new Date(timestampMs).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })
}

function formatBytes(bytes: number) {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B'
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}

function usageValue(usage: Record<string, unknown> | null | undefined, key: string) {
  const value = usage?.[key]
  return typeof value === 'number' ? value : 0
}

function usageTotal(usage: Record<string, unknown> | null | undefined) {
  return usageValue(usage, 'input_tokens')
    + usageValue(usage, 'output_tokens')
    + usageValue(usage, 'cache_creation_input_tokens')
    + usageValue(usage, 'cache_read_input_tokens')
}

function requestTypeLabel(request: NetworkRequestSummary) {
  return request.requestSubType ? `${request.requestType}:${request.requestSubType}` : request.requestType
}

function jsonObject(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {}
}

function jsonArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : []
}

function requestSystem(body: unknown) {
  return jsonObject(body).system
}

function requestMessages(body: unknown) {
  return jsonArray(jsonObject(body).messages)
}

function requestTools(body: unknown) {
  return jsonArray(jsonObject(body).tools)
}

function responseEvents(responseBody: string | null | undefined) {
  if (!responseBody) return []
  return responseBody
    .split('\n')
    .filter((line) => line.startsWith('data:'))
    .map((line) => line.slice(5).trim())
    .filter((line) => line && line !== '[DONE]')
    .map((line) => {
      try {
        return JSON.parse(line)
      } catch {
        return line
      }
    })
}

function shortPath(path?: string | null) {
  if (!path) return '-'
  const parts = path.split('/').filter(Boolean)
  if (parts.length <= 3) return path
  return `…/${parts.slice(-3).join('/')}`
}

function parentPath(path?: string | null) {
  if (!path) return null
  const normalized = path.replace(/\/+$/, '')
  const index = normalized.lastIndexOf('/')
  if (index <= 0) return null
  return normalized.slice(0, index)
}

function sessionPlan(session?: DetailSession) {
  if (!session) return null
  if ('pendingPlan' in session && session.pendingPlan) return session.pendingPlan
  if ('planContent' in session && session.planContent) {
    return {
      title: session.planTitle || 'Plan',
      content: session.planContent,
      permissions: session.planPermissions || [],
    }
  }
  return null
}

function timelineKindLabel(kind: string) {
  if (kind === 'tool') return 'tool'
  if (kind === 'hook_tool') return 'hook'
  if (kind === 'approval') return '审批'
  if (kind === 'question') return '问题'
  if (kind === 'plan') return '计划'
  if (kind === 'subagent') return 'subagent'
  if (kind === 'session') return 'session'
  return kind
}

function messagePreview(message: ChatMessage) {
  if (message.role === 'assistant') {
    return message.content || message.trailingContent || message.thinking || `${message.toolCalls?.length ?? 0} tool calls`
  }
  if (message.role === 'tool_use') return `${message.toolName}${message.toolInput ? `\n${message.toolInput}` : ''}`
  if (message.role === 'permission') return `${message.toolName}${message.toolInput ? `\n${message.toolInput}` : ''}`
  if (message.role === 'thinking') return message.content
  if (message.role === 'error') return message.message
  return message.content
}

interface AgentMonitorSectionProps {
  activeView?: MonitorSettingsView
}

function roleLabel(role: ChatMessage['role']) {
  if (role === 'tool_use') return 'tool'
  if (role === 'permission') return 'approval'
  return role
}

interface RequestStats {
  key: string
  requestCount: number
  inputTokens: number
  outputTokens: number
  cacheCreate: number
  cacheRead: number
  mainAgentCount: number
  subAgentCount: number
}

function emptyRequestStats(key: string): RequestStats {
  return {
    key,
    requestCount: 0,
    inputTokens: 0,
    outputTokens: 0,
    cacheCreate: 0,
    cacheRead: 0,
    mainAgentCount: 0,
    subAgentCount: 0,
  }
}

function aggregateRequestStats(requests: NetworkRequestSummary[], by: 'model' | 'project') {
  const groups = new Map<string, RequestStats>()
  for (const request of requests) {
    const key = by === 'model'
      ? request.model || request.provider || 'unknown'
      : request.project || '未关联项目'
    const stats = groups.get(key) ?? emptyRequestStats(key)
    stats.requestCount += 1
    stats.inputTokens += request.usageSummary?.inputTokens ?? usageValue(request.usage, 'input_tokens')
    stats.outputTokens += request.usageSummary?.outputTokens ?? usageValue(request.usage, 'output_tokens')
    stats.cacheCreate += request.usageSummary?.cacheCreationInputTokens ?? usageValue(request.usage, 'cache_creation_input_tokens')
    stats.cacheRead += request.usageSummary?.cacheReadInputTokens ?? usageValue(request.usage, 'cache_read_input_tokens')
    if (request.requestType === 'MainAgent') stats.mainAgentCount += 1
    if (request.requestType === 'SubAgent') stats.subAgentCount += 1
    groups.set(key, stats)
  }
  return Array.from(groups.values()).sort((a, b) => b.requestCount - a.requestCount)
}

function cacheHitRate(stats: RequestStats) {
  const cacheTotal = stats.cacheCreate + stats.cacheRead
  return cacheTotal > 0 ? Math.round((stats.cacheRead / cacheTotal) * 100) : null
}

export function AgentMonitorSection({ activeView = 'sessions' }: AgentMonitorSectionProps) {
  const liveSessions = useSessionStore(selectSessionList)
  const [sessions, setSessions] = useState<MonitorSessionSummary[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [agentFilter, setAgentFilter] = useState('all')
  const [statusFilter, setStatusFilter] = useState('all')
  const [projectFilter, setProjectFilter] = useState('all')
  const [activeTab, setActiveTab] = useState<DetailTab>('overview')
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [detail, setDetail] = useState<MonitorSessionDetail | null>(null)
  const [detailLoading, setDetailLoading] = useState(false)
  const [detailError, setDetailError] = useState('')
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [chatLoading, setChatLoading] = useState(false)
  const [chatError, setChatError] = useState('')
  const [networkStatus, setNetworkStatus] = useState<NetworkMonitorStatus>(DEFAULT_NETWORK_STATUS)
  const [networkUpstream, setNetworkUpstream] = useState(DEFAULT_NETWORK_STATUS.upstreamBaseUrl)
  const [networkRequests, setNetworkRequests] = useState<NetworkRequestSummary[]>([])
  const [selectedNetworkRequestId, setSelectedNetworkRequestId] = useState<string | null>(null)
  const [networkDetail, setNetworkDetail] = useState<NetworkRequestDetail | null>(null)
  const [networkLoading, setNetworkLoading] = useState(false)
  const [networkBusy, setNetworkBusy] = useState(false)
  const [networkError, setNetworkError] = useState('')
  const [wrapperStatus, setWrapperStatus] = useState<ClaudeWrapperStatus>(DEFAULT_WRAPPER_STATUS)
  const [wrapperBusy, setWrapperBusy] = useState(false)

  const loadSessions = useCallback(async (showSpinner = false) => {
    if (showSpinner) setLoading(true)
    setError('')
    try {
      const remoteSessions = await getMonitorSessions()
      const nextSessions = remoteSessions.length > 0 ? remoteSessions : liveSessions.map(summaryFromSession)
      setSessions(nextSessions)
      setSelectedId((current) => {
        if (current && nextSessions.some((session) => session.id === current)) return current
        return nextSessions[0]?.id ?? null
      })
    } catch (err) {
      setError(String(err))
      setSessions(liveSessions.map(summaryFromSession))
    } finally {
      setLoading(false)
    }
  }, [liveSessions])

  const loadNetworkStatus = useCallback(async () => {
    try {
      const status = await getNetworkMonitorStatus()
      setNetworkStatus(status)
      setNetworkUpstream((current) => current.trim() ? current : status.upstreamBaseUrl)
      return status
    } catch (err) {
      setNetworkError(String(err))
      return null
    }
  }, [])

  const loadWrapperStatus = useCallback(async () => {
    try {
      const status = await getClaudeWrapperStatus()
      setWrapperStatus(status)
      return status
    } catch (err) {
      setNetworkError(String(err))
      return null
    }
  }, [])

  const loadNetworkRequests = useCallback(async () => {
    try {
      const requests = await getNetworkMonitorRequests()
      setNetworkRequests(requests)
      setSelectedNetworkRequestId((current) => {
        if (current && requests.some((request) => request.id === current)) return current
        return requests[0]?.id ?? null
      })
      return requests
    } catch (err) {
      setNetworkError(String(err))
      return []
    }
  }, [])

  useEffect(() => {
    loadSessions(true)
  }, [loadSessions])

  useEffect(() => {
    loadNetworkStatus().then((status) => {
      if (status?.enabled) loadNetworkRequests()
    })
    loadWrapperStatus()
  }, [loadNetworkRequests, loadNetworkStatus, loadWrapperStatus])

  useEffect(() => {
    const timer = window.setInterval(() => loadSessions(false), 3000)
    return () => window.clearInterval(timer)
  }, [loadSessions])

  useEffect(() => {
    if (!networkStatus.enabled) return
    const timer = window.setInterval(() => {
      loadNetworkStatus()
      loadNetworkRequests()
    }, 2000)
    return () => window.clearInterval(timer)
  }, [loadNetworkRequests, loadNetworkStatus, networkStatus.enabled])

  useEffect(() => {
    if (!selectedId) {
      setDetail(null)
      return
    }

    let cancelled = false
    setDetailLoading(true)
    setDetailError('')
    getMonitorSessionDetail(selectedId)
      .then((result) => {
        if (cancelled) return
        setDetail(result)
      })
      .catch((err) => {
        if (cancelled) return
        setDetail(null)
        setDetailError(String(err))
      })
      .finally(() => {
        if (!cancelled) setDetailLoading(false)
      })

    return () => {
      cancelled = true
    }
  }, [selectedId, sessions])

  useEffect(() => {
    if (!selectedId || activeTab !== 'conversation') return

    let cancelled = false
    setChatLoading(true)
    setChatError('')
    getChatHistory(selectedId)
      .then((parsed) => {
        if (cancelled) return
        setMessages(mapParsedMessages(parsed))
      })
      .catch((err) => {
        if (cancelled) return
        setMessages([])
        setChatError(String(err))
      })
      .finally(() => {
        if (!cancelled) setChatLoading(false)
      })

    return () => {
      cancelled = true
    }
  }, [activeTab, selectedId])

  const networkViewActive = activeTab === 'network' || activeView === 'capture'

  useEffect(() => {
    if (!networkViewActive || !networkStatus.enabled) return
    loadNetworkRequests()
  }, [loadNetworkRequests, networkStatus.enabled, networkViewActive])

  useEffect(() => {
    if (!networkViewActive || !selectedNetworkRequestId) {
      setNetworkDetail(null)
      return
    }

    let cancelled = false
    setNetworkLoading(true)
    setNetworkError('')
    getNetworkMonitorRequestDetail(selectedNetworkRequestId)
      .then((result) => {
        if (cancelled) return
        setNetworkDetail(result)
      })
      .catch((err) => {
        if (cancelled) return
        setNetworkDetail(null)
        setNetworkError(String(err))
      })
      .finally(() => {
        if (!cancelled) setNetworkLoading(false)
      })

    return () => {
      cancelled = true
    }
  }, [networkViewActive, selectedNetworkRequestId, networkRequests])

  const agentOptions = useMemo(() => Array.from(new Set(sessions.map((session) => session.agentType))).sort(), [sessions])
  const statusOptions = useMemo(() => Array.from(new Set(sessions.map((session) => session.phase))).sort(), [sessions])
  const projectOptions = useMemo(() => Array.from(new Set(sessions.map((session) => session.project || 'Unknown'))).sort(), [sessions])

  const filteredSessions = useMemo(() => sessions.filter((session) => {
    if (agentFilter !== 'all' && session.agentType !== agentFilter) return false
    if (statusFilter !== 'all' && session.phase !== statusFilter) return false
    if (projectFilter !== 'all' && (session.project || 'Unknown') !== projectFilter) return false
    return true
  }), [agentFilter, projectFilter, sessions, statusFilter])

  const selectedSummary = sessions.find((session) => session.id === selectedId) ?? null
  const liveSelected = liveSessions.find((session) => session.id === selectedId)
  const detailSession: DetailSession | undefined = detail?.session ?? liveSelected
  const plan = sessionPlan(detailSession)
  const timeline = detail?.timeline ?? []
  const toolTimeline = timeline.filter((item) => ['tool', 'hook_tool', 'approval', 'question', 'plan', 'subagent'].includes(item.kind))
  const rawEvents = detail?.rawEvents ?? []
  const pendingPermission = detailSession?.pendingPermission
  const pendingQuestion = detailSession?.pendingQuestion
  const waitingCount = sessions.filter((session) => session.waitingUser).length
  const runningCount = sessions.filter((session) => session.phase === 'processing' || session.phase === 'compacting').length

  const jumpSelected = () => {
    if (!selectedId) return
    jumpToTerminal(selectedId).catch((err) => setDetailError(String(err)))
  }

  const openSelectedTranscript = () => {
    if (!detail?.transcriptPath) return
    openSystemPath(detail.transcriptPath).catch((err) => setDetailError(String(err)))
  }

  const openSelectedTranscriptDirectory = () => {
    const directory = parentPath(detail?.transcriptPath)
    if (!directory) return
    openSystemPath(directory).catch((err) => setDetailError(String(err)))
  }

  const toggleNetworkMonitor = async (enabled: boolean) => {
    setNetworkBusy(true)
    setNetworkError('')
    try {
      const status = await setNetworkMonitorEnabled(enabled, networkUpstream)
      setNetworkStatus(status)
      setNetworkUpstream(status.upstreamBaseUrl)
      if (status.enabled) {
        await loadNetworkRequests()
      } else {
        setNetworkRequests([])
        setSelectedNetworkRequestId(null)
        setNetworkDetail(null)
      }
    } catch (err) {
      setNetworkError(String(err))
    } finally {
      setNetworkBusy(false)
    }
  }

  const toggleClaudeWrapper = async () => {
    setWrapperBusy(true)
    setNetworkError('')
    try {
      const status = wrapperStatus.installed
        ? await removeClaudeWrapper()
        : await installClaudeWrapper()
      setWrapperStatus(status)
    } catch (err) {
      setNetworkError(String(err))
    } finally {
      setWrapperBusy(false)
    }
  }

  const accessControls = (
    <>
      <div className={`agent-monitor__network-control ${networkStatus.enabled ? 'agent-monitor__network-control--on' : ''}`}>
        <div className="agent-monitor__network-copy">
          <strong>原生网络请求监控</strong>
          <span>开启后启动本地 inspector 代理，记录 request body、system prompt、messages、tools、response、usage 和 KV cache 数据。</span>
          <label>
            上游地址
            <input
              value={networkUpstream}
              onChange={(event) => setNetworkUpstream(event.target.value)}
              disabled={networkStatus.enabled || networkBusy}
              placeholder="https://api.anthropic.com"
            />
          </label>
          {networkStatus.proxyUrl && (
            <code>ANTHROPIC_BASE_URL={networkStatus.proxyUrl} claude</code>
          )}
        </div>
        <div className="agent-monitor__network-actions">
          <Toggle checked={networkStatus.enabled} onChange={toggleNetworkMonitor} disabled={networkBusy} />
          <span>{networkStatus.enabled ? '已开启' : '关闭'}</span>
        </div>
      </div>

      <div className={`agent-monitor__wrapper-control ${wrapperStatus.installed ? 'agent-monitor__wrapper-control--on' : ''}`}>
        <div>
          <strong>Claude 命令无感接入</strong>
          <span>
            安装一次后，新开的 iTerm、Terminal、Cursor/VS Code 终端里继续输入 claude，会先进入 AgentBro inspector，再启动真实 Claude。只注入进程级环境变量，不覆盖 Claude settings 或 hooks。
          </span>
          <code>{wrapperStatus.shimPath}</code>
          <em>{wrapperStatus.pathHintInstalled ? `PATH 已写入 ${wrapperStatus.shellConfigPath} · hooks preserved` : `PATH 尚未写入 ${wrapperStatus.shellConfigPath}`}</em>
        </div>
        <button type="button" onClick={toggleClaudeWrapper} disabled={wrapperBusy}>
          {wrapperStatus.installed ? '移除接入' : '安装接入'}
        </button>
      </div>
    </>
  )

  const summaryStats = (
    <div className="agent-monitor__stats" aria-label="Agent monitor summary">
      <div><span>Sessions</span><strong>{sessions.length}</strong></div>
      <div><span>运行中</span><strong>{runningCount}</strong></div>
      <div><span>等待用户</span><strong>{waitingCount}</strong></div>
      <div><span>网络请求</span><strong>{networkStatus.requestCount}</strong></div>
      <div><span>Raw events</span><strong>{rawEvents.length}</strong></div>
    </div>
  )

  const quickControls = (
    <div className="agent-monitor__quick-controls">
      <section className={networkStatus.enabled ? 'agent-monitor__quick-control agent-monitor__quick-control--on' : 'agent-monitor__quick-control'}>
        <div>
          <span>Inspector 代理</span>
          <strong>{networkStatus.enabled ? '运行中' : '未开启'}</strong>
          <em>{networkStatus.proxyUrl || networkStatus.upstreamBaseUrl}</em>
        </div>
        <Toggle checked={networkStatus.enabled} onChange={toggleNetworkMonitor} disabled={networkBusy} />
      </section>
      <section className={wrapperStatus.installed ? 'agent-monitor__quick-control agent-monitor__quick-control--on' : 'agent-monitor__quick-control'}>
        <div>
          <span>Claude 命令接入</span>
          <strong>{wrapperStatus.installed ? '已安装' : '未安装'}</strong>
          <em>{wrapperStatus.pathHintInstalled ? 'PATH 已配置 · 保留 hooks' : '等待安装'}</em>
        </div>
        <button type="button" onClick={toggleClaudeWrapper} disabled={wrapperBusy}>
          {wrapperStatus.installed ? '移除' : '安装'}
        </button>
      </section>
    </div>
  )

  const modelStats = aggregateRequestStats(networkRequests, 'model')
  const projectStats = aggregateRequestStats(networkRequests, 'project')
  const networkTokenTotal = networkRequests.reduce((total, request) => (
    total + (request.usageSummary?.totalTokens ?? usageTotal(request.usage))
  ), 0)

  if (activeView === 'access') {
    return (
      <section className="agent-monitor">
        <header className="agent-monitor__header">
          <div>
            <h2>接入设置</h2>
            <p>管理本地 inspector 代理和 Claude 命令无感接入。provider 仍按当前 shell、项目或全局 Claude 配置决定，Claude hooks/settings 会被保留。</p>
          </div>
          <button type="button" className="agent-monitor__refresh" onClick={() => {
            loadNetworkStatus()
            loadWrapperStatus()
          }}>
            重新加载
          </button>
        </header>
        {accessControls}
        {networkError && <div className="agent-monitor__notice">{networkError}</div>}
      </section>
    )
  }

  if (activeView === 'capture') {
    return (
      <section className="agent-monitor">
        <header className="agent-monitor__header">
          <div>
            <h2>请求抓包</h2>
            <p>按 MainAgent、SubAgent、count、preflight 分类查看 Claude Code 的 system、messages、tools、response 和 KV cache。</p>
          </div>
          <button type="button" className="agent-monitor__refresh" onClick={() => {
            loadNetworkStatus()
            loadNetworkRequests()
          }}>
            刷新请求
          </button>
        </header>
        {accessControls}
        {networkError && <div className="agent-monitor__notice">{networkError}</div>}
        <div className="agent-monitor__capture-workbench">
          <NetworkRequestsTab
            status={networkStatus}
            requests={networkRequests}
            selectedRequestId={selectedNetworkRequestId}
            detail={networkDetail}
            loading={networkLoading}
            onSelect={setSelectedNetworkRequestId}
            onRefresh={() => {
              loadNetworkStatus()
              loadNetworkRequests()
            }}
          />
        </div>
      </section>
    )
  }

  if (activeView === 'stats') {
    return (
      <section className="agent-monitor">
        <header className="agent-monitor__header">
          <div>
            <h2>项目统计</h2>
            <p>按项目和模型聚合请求量、Main/SubAgent 占比、token 消耗和 KV cache 命中。</p>
          </div>
          <button type="button" className="agent-monitor__refresh" onClick={loadNetworkRequests}>刷新统计</button>
        </header>
        {summaryStats}
        <div className="agent-monitor__metric-row agent-monitor__metric-row--wide">
          <div>
            <span>Captured tokens</span>
            <strong>{formatTokens(networkTokenTotal)}</strong>
            <em>来自已捕获网络请求</em>
          </div>
          <div>
            <span>MainAgent requests</span>
            <strong>{networkRequests.filter((request) => request.requestType === 'MainAgent').length}</strong>
            <em>主会话推理请求</em>
          </div>
          <div>
            <span>SubAgent requests</span>
            <strong>{networkRequests.filter((request) => request.requestType === 'SubAgent').length}</strong>
            <em>子任务请求</em>
          </div>
        </div>
        <StatsPanel title="模型使用统计" items={modelStats} />
        <StatsPanel title="项目请求统计" items={projectStats} />
      </section>
    )
  }

  if (activeView === 'overview') {
    return (
      <section className="agent-monitor">
        <header className="agent-monitor__header">
          <div>
            <h2>监控总览</h2>
            <p>集中查看 Agent 会话、Claude 原生请求、KV cache 和接入状态。</p>
          </div>
          <button type="button" className="agent-monitor__refresh" onClick={() => {
            loadSessions(true)
            loadNetworkStatus()
            loadNetworkRequests()
          }}>
            重新加载
          </button>
        </header>
        {quickControls}
        {networkError && <div className="agent-monitor__notice">{networkError}</div>}
        {summaryStats}
        <div className="agent-monitor__overview-grid">
          <section>
            <h3>抓包状态</h3>
            <div className="agent-monitor__kv-grid">
              <div><span>Inspector</span><strong>{networkStatus.enabled ? '运行中' : '未开启'}</strong></div>
              <div><span>无感接入</span><strong>{wrapperStatus.installed ? '已安装' : '未安装'}</strong></div>
              <div><span>Proxy</span><strong>{networkStatus.proxyUrl || '-'}</strong></div>
              <div><span>Upstream</span><strong>{networkStatus.upstreamBaseUrl}</strong></div>
            </div>
          </section>
          <section>
            <h3>请求结构</h3>
            <div className="agent-monitor__metric-row">
              <div><span>MainAgent</span><strong>{networkRequests.filter((request) => request.requestType === 'MainAgent').length}</strong><em>主请求</em></div>
              <div><span>SubAgent</span><strong>{networkRequests.filter((request) => request.requestType === 'SubAgent').length}</strong><em>子请求</em></div>
              <div><span>Tokens</span><strong>{formatTokens(networkTokenTotal)}</strong><em>已捕获</em></div>
            </div>
          </section>
        </div>
        <StatsPanel title="模型使用统计" items={modelStats.slice(0, 3)} compact />
      </section>
    )
  }

  return (
    <section className="agent-monitor">
      <header className="agent-monitor__header">
        <div>
          <h2>Agent监控</h2>
          <p>查看 Agent 会话状态，并在手动开启后捕获 Claude Code 原生网络请求。</p>
        </div>
        <button type="button" className="agent-monitor__refresh" onClick={() => loadSessions(true)}>
          重新加载
        </button>
      </header>

      {networkError && <div className="agent-monitor__notice">{networkError}</div>}

      {summaryStats}

      <div className="agent-monitor__filters">
        <label>
          Agent
          <select value={agentFilter} onChange={(event) => setAgentFilter(event.target.value)}>
            <option value="all">全部</option>
            {agentOptions.map((agent) => <option key={agent} value={agent}>{agentLabel(agent)}</option>)}
          </select>
        </label>
        <label>
          状态
          <select value={statusFilter} onChange={(event) => setStatusFilter(event.target.value)}>
            <option value="all">全部</option>
            {statusOptions.map((status) => <option key={status} value={status}>{phaseLabel(status)}</option>)}
          </select>
        </label>
        <label>
          项目
          <select value={projectFilter} onChange={(event) => setProjectFilter(event.target.value)}>
            <option value="all">全部</option>
            {projectOptions.map((project) => <option key={project} value={project}>{project}</option>)}
          </select>
        </label>
      </div>

      {error && <div className="agent-monitor__notice">Monitor IPC 读取失败，已回退到前端 sessionStore：{error}</div>}

      <div className="agent-monitor__layout">
        <div className="agent-monitor__sessions" aria-label="Agent session list">
          {loading && sessions.length === 0 ? (
            <div className="agent-monitor__empty">正在读取 Agent 会话...</div>
          ) : filteredSessions.length === 0 ? (
            <div className="agent-monitor__empty">暂无匹配的 Agent 会话。</div>
          ) : (
            <div className="agent-monitor__session-list">
              {filteredSessions.map((session) => (
                <button
                  key={session.id}
                  type="button"
                  className={session.id === selectedId ? 'agent-monitor__session-row agent-monitor__session-row--active' : 'agent-monitor__session-row'}
                  onClick={() => {
                    setSelectedId(session.id)
                    setActiveTab('overview')
                  }}
                >
                  <span className="agent-monitor__session-agent">
                    <strong>{agentLabel(session.agentType, session.engineLabel)}</strong>
                    <em>{session.id.slice(0, 8)}</em>
                  </span>
                  <span className="agent-monitor__session-main">
                    <strong>{session.title || session.project || 'Unknown'}</strong>
                    <em title={session.cwd}>{shortPath(session.cwd)}</em>
                  </span>
                  <span className="agent-monitor__session-state">
                    <i className={`agent-monitor__dot agent-monitor__dot--${session.phase}`} />
                    {phaseLabel(session.phase)}
                  </span>
                  <span className="agent-monitor__session-metrics">
                    <em>{formatDurationShort(session.duration)}</em>
                    <em>{formatTokens(session.tokenTotal)} tok</em>
                    <em>{session.subagentCount} sub</em>
                  </span>
                  <span className="agent-monitor__session-foot">
                    <em>{session.lastToolName || '无工具'}</em>
                    <em>{session.waitingUser ? pendingLabel(session.pendingKind) : '无等待'}</em>
                  </span>
                </button>
              ))}
            </div>
          )}
        </div>

        <aside className="agent-monitor__detail">
          {!selectedId ? (
            <div className="agent-monitor__empty agent-monitor__empty--detail">选择一个 session 查看详情。</div>
          ) : (
            <>
              <div className="agent-monitor__detail-header">
                <div>
                  <span>{selectedSummary ? agentLabel(selectedSummary.agentType, selectedSummary.engineLabel) : 'Session'}</span>
                  <h3>{selectedSummary?.title || selectedSummary?.project || selectedId}</h3>
                  <code title={detail?.transcriptPath || undefined}>{selectedId}</code>
                </div>
                <div className="agent-monitor__detail-actions">
                  <button type="button" onClick={openSelectedTranscript} disabled={!detail?.transcriptPath}>
                    打开 JSON
                  </button>
                  <button type="button" onClick={openSelectedTranscriptDirectory} disabled={!parentPath(detail?.transcriptPath)}>
                    打开目录
                  </button>
                  <button type="button" onClick={jumpSelected}>跳转终端</button>
                </div>
              </div>

              <div className="agent-monitor__tabs">
                {DETAIL_TABS.map((tab) => (
                  <button
                    key={tab.id}
                    type="button"
                    className={activeTab === tab.id ? 'active' : ''}
                    onClick={() => setActiveTab(tab.id)}
                  >
                    {tab.label}
                  </button>
                ))}
              </div>

              {detailLoading && <div className="agent-monitor__inline">正在刷新详情...</div>}
              {detailError && <div className="agent-monitor__notice">{detailError}</div>}

              {activeTab === 'overview' && (
                <OverviewTab session={detailSession} summary={selectedSummary} rawCount={rawEvents.length} timelineCount={timeline.length} />
              )}

              {activeTab === 'network' && (
                <NetworkRequestsTab
                  status={networkStatus}
                  requests={networkRequests}
                  selectedRequestId={selectedNetworkRequestId}
                  detail={networkDetail}
                  loading={networkLoading}
                  onSelect={setSelectedNetworkRequestId}
                  onRefresh={() => {
                    loadNetworkStatus()
                    loadNetworkRequests()
                  }}
                />
              )}

              {activeTab === 'conversation' && (
                <ConversationTab loading={chatLoading} error={chatError} messages={messages} />
              )}

              {activeTab === 'timeline' && (
                <TimelineTab items={toolTimeline} />
              )}

              {activeTab === 'approvals' && (
                <ApprovalsTab
                  permission={pendingPermission}
                  question={pendingQuestion}
                  plan={plan}
                  onJump={jumpSelected}
                />
              )}

              {activeTab === 'raw' && (
                <RawEventsTab events={rawEvents} />
              )}
            </>
          )}
        </aside>
      </div>
    </section>
  )
}

function OverviewTab({
  session,
  summary,
  rawCount,
  timelineCount,
}: {
  session?: DetailSession
  summary: MonitorSessionSummary | null
  rawCount: number
  timelineCount: number
}) {
  if (!session && !summary) return <div className="agent-monitor__empty">暂无详情数据。</div>

  const tokens = session?.tokens
  const contextWindow = session?.contextWindow
  const rateLimits = session?.rateLimits

  return (
    <div className="agent-monitor__overview">
      <div className="agent-monitor__kv-grid">
        <div><span>phase</span><strong>{phaseLabel(session?.phase ?? summary?.phase)}</strong></div>
        <div><span>cwd</span><strong title={session?.cwd ?? summary?.cwd}>{shortPath(session?.cwd ?? summary?.cwd)}</strong></div>
        <div><span>terminal</span><strong>{session?.terminal || summary?.terminal || '-'}</strong></div>
        <div><span>pid</span><strong>{session?.pid ?? '-'}</strong></div>
        <div><span>tools</span><strong>{timelineCount}</strong></div>
        <div><span>raw events</span><strong>{rawCount}</strong></div>
      </div>

      <div className="agent-monitor__metric-row">
        <div>
          <span>Token usage</span>
          <strong>{formatTokens(totalTokens(tokens))}</strong>
          <em>{formatTokens(tokens?.input ?? 0)} in · {formatTokens(tokens?.output ?? 0)} out · {formatTokens(tokens?.cacheRead ?? 0)} cache</em>
        </div>
        <div>
          <span>Context window</span>
          <strong>{contextWindow?.usedPercentage != null ? `${Math.round(contextWindow.usedPercentage)}%` : '-'}</strong>
          <em>{formatTokens(contextWindow?.totalInputTokens ?? 0)} / {formatTokens(contextWindow?.contextWindowSize ?? 0)}</em>
        </div>
        <div>
          <span>Rate limit</span>
          <strong>{rateLimits ? `${Math.round(rateLimits.fiveHourUsage)}%` : '-'}</strong>
          <em>{rateLimits ? `5h ${rateLimits.fiveHourRemaining} · 7d ${rateLimits.sevenDayRemaining}` : 'no statusline data'}</em>
        </div>
      </div>

      {session?.subagents && session.subagents.length > 0 && (
        <div className="agent-monitor__subagents">
          <h4>Subagents</h4>
          {session.subagents.map((subagent) => (
            <div key={subagent.agentId} className="agent-monitor__subagent">
              <strong>{subagent.name ? `@${subagent.name}` : (subagent.agentType || 'subagent')}</strong>
              <span>{subagent.status}</span>
              <p>{subagent.description}</p>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

function StatsPanel({
  title,
  items,
  compact = false,
}: {
  title: string
  items: RequestStats[]
  compact?: boolean
}) {
  return (
    <section className={`agent-monitor__stats-panel ${compact ? 'agent-monitor__stats-panel--compact' : ''}`}>
      <h3>{title}</h3>
      {items.length === 0 ? (
        <div className="agent-monitor__empty">暂无可统计的网络请求。开启请求抓包并运行 Claude 后会在这里聚合。</div>
      ) : (
        <div className="agent-monitor__stats-list">
          {items.map((item) => {
            const hitRate = cacheHitRate(item)
            return (
              <article key={item.key} className="agent-monitor__stats-item">
                <header>
                  <strong>{item.key}</strong>
                  <span>{item.requestCount} reqs</span>
                </header>
                <div>
                  <span>Token</span>
                  <strong>{formatTokens(item.inputTokens + item.outputTokens)}</strong>
                  <em>{formatTokens(item.inputTokens)} in · {formatTokens(item.outputTokens)} out</em>
                </div>
                <div>
                  <span>KV Cache</span>
                  <strong>{hitRate == null ? '-' : `${hitRate}%`}</strong>
                  <em>{formatTokens(item.cacheCreate)} create · {formatTokens(item.cacheRead)} read</em>
                </div>
                <div>
                  <span>Agent</span>
                  <strong>{item.mainAgentCount}/{item.subAgentCount}</strong>
                  <em>Main/Sub</em>
                </div>
              </article>
            )
          })}
        </div>
      )}
    </section>
  )
}

function NetworkRequestsTab({
  status,
  requests,
  selectedRequestId,
  detail,
  loading,
  onSelect,
  onRefresh,
}: {
  status: NetworkMonitorStatus
  requests: NetworkRequestSummary[]
  selectedRequestId: string | null
  detail: NetworkRequestDetail | null
  loading: boolean
  onSelect: (requestId: string) => void
  onRefresh: () => void
}) {
  const [detailTab, setDetailTab] = useState<NetworkDetailTab>('system')

  if (!status.enabled) {
    return (
      <div className="agent-monitor__empty">
        开启上方“原生网络请求监控”后，这里会显示 Claude Code API 请求、system prompt、messages、tools 和 usage。
      </div>
    )
  }

  if (requests.length === 0) {
    return (
      <div className="agent-monitor__empty">
        代理已启动，等待请求进入。外部终端可用 <code>ANTHROPIC_BASE_URL={status.proxyUrl} claude</code> 启动。
      </div>
    )
  }

  const selected = requests.find((request) => request.id === selectedRequestId) ?? requests[0]
  const usage = selected?.usageSummary ?? null
  const rawUsage = selected?.usage ?? null
  const networkTabs: Array<{ id: NetworkDetailTab; label: string }> = [
    { id: 'system', label: 'System' },
    { id: 'messages', label: 'Messages' },
    { id: 'tools', label: 'Tools' },
    { id: 'response', label: 'Response' },
    { id: 'headers', label: 'Headers' },
    { id: 'raw', label: 'Raw' },
  ]

  return (
    <div className="agent-monitor__network-tab">
      <div className="agent-monitor__network-toolbar">
        <span>{status.activeRequestCount > 0 ? `${status.activeRequestCount} 个请求进行中` : '无进行中请求'}</span>
        <button type="button" onClick={onRefresh}>刷新请求</button>
      </div>

      <div className="agent-monitor__network-list">
        {requests.map((request) => (
          <button
            key={request.id}
            type="button"
            className={request.id === selected.id ? 'agent-monitor__network-row agent-monitor__network-row--active' : 'agent-monitor__network-row'}
            onClick={() => onSelect(request.id)}
          >
            <span>{formatTime(request.timestampMs)}</span>
            <strong>{request.model || request.provider}</strong>
            <em>{request.inProgress ? 'streaming' : request.status ? `HTTP ${request.status}` : request.error ? 'error' : 'pending'}</em>
            <small>{requestTypeLabel(request)} · {request.messageCount} msg · {request.toolCount} tools · {formatBytes(request.requestBytes)}</small>
          </button>
        ))}
      </div>

      <div className="agent-monitor__network-detail">
        <div className="agent-monitor__kv-grid">
          <div><span>model</span><strong>{selected.model || '-'}</strong></div>
          <div><span>type</span><strong>{requestTypeLabel(selected)}</strong></div>
          <div><span>status</span><strong>{selected.inProgress ? 'streaming' : selected.status ?? selected.error ?? '-'}</strong></div>
          <div><span>duration</span><strong>{selected.durationMs != null ? `${selected.durationMs}ms` : '-'}</strong></div>
          <div><span>tokens</span><strong>{usage ? formatTokens(usage.totalTokens) : rawUsage ? formatTokens(usageTotal(rawUsage)) : '-'}</strong></div>
          <div><span>cache hit</span><strong>{usage?.cacheHitRate != null ? `${Math.round(usage.cacheHitRate)}%` : '-'}</strong></div>
        </div>

        {loading ? (
          <div className="agent-monitor__inline">正在读取请求详情...</div>
        ) : detail ? (
          <>
            {usage && (
              <div className="agent-monitor__network-usage" aria-label="Network token usage">
                <span>input {formatTokens(usage.inputTokens)}</span>
                <span>output {formatTokens(usage.outputTokens)}</span>
                <span>cache create {formatTokens(usage.cacheCreationInputTokens)}</span>
                <span>cache read {formatTokens(usage.cacheReadInputTokens)}</span>
              </div>
            )}

            <div className="agent-monitor__subtabs">
              {networkTabs.map((tab) => (
                <button
                  key={tab.id}
                  type="button"
                  className={detailTab === tab.id ? 'active' : ''}
                  onClick={() => setDetailTab(tab.id)}
                >
                  {tab.label}
                </button>
              ))}
            </div>

            {detailTab === 'system' && (
              <section className="agent-monitor__network-section">
                <h4>System Prompt</h4>
                {requestSystem(detail.requestBody) ? (
                  <pre>{JSON.stringify(requestSystem(detail.requestBody), null, 2)}</pre>
                ) : (
                  <p>{selected.systemPreview || 'No system prompt captured.'}</p>
                )}
              </section>
            )}

            {detailTab === 'messages' && (
              <section className="agent-monitor__network-section">
                <h4>Messages</h4>
                {requestMessages(detail.requestBody).length > 0 ? (
                  <div className="agent-monitor__network-block-list">
                    {requestMessages(detail.requestBody).map((message, index) => {
                      const item = jsonObject(message)
                      return (
                        <details key={index} open={index >= requestMessages(detail.requestBody).length - 2}>
                          <summary>{String(item.role ?? 'message')} #{index + 1}</summary>
                          <pre>{JSON.stringify(message, null, 2)}</pre>
                        </details>
                      )
                    })}
                  </div>
                ) : (
                  <p>No messages captured.</p>
                )}
              </section>
            )}

            {detailTab === 'tools' && (
              <section className="agent-monitor__network-section">
                <h4>Tools</h4>
                {requestTools(detail.requestBody).length > 0 ? (
                  <div className="agent-monitor__network-block-list">
                    {requestTools(detail.requestBody).map((tool, index) => {
                      const item = jsonObject(tool)
                      return (
                        <details key={index}>
                          <summary>{String(item.name ?? `tool-${index + 1}`)}</summary>
                          <pre>{JSON.stringify(tool, null, 2)}</pre>
                        </details>
                      )
                    })}
                  </div>
                ) : (
                  <p>No tools captured.</p>
                )}
              </section>
            )}

            {detailTab === 'response' && (
              <section className="agent-monitor__network-section">
                <h4>Response{detail.responseBodyTruncated ? ' · truncated' : ''}</h4>
                {detail.responseBody && responseEvents(detail.responseBody).length > 0 ? (
                  <div className="agent-monitor__network-block-list">
                    {responseEvents(detail.responseBody).map((event, index) => (
                      <details key={index} open={index === responseEvents(detail.responseBody).length - 1}>
                        <summary>event #{index + 1}</summary>
                        <pre>{typeof event === 'string' ? event : JSON.stringify(event, null, 2)}</pre>
                      </details>
                    ))}
                  </div>
                ) : (
                  <pre>{detail.responseBody || JSON.stringify(detail.responseHeaders, null, 2)}</pre>
                )}
              </section>
            )}

            {detailTab === 'headers' && (
              <section className="agent-monitor__network-section">
                <h4>Headers</h4>
                <pre>{JSON.stringify({ request: detail.requestHeaders, response: detail.responseHeaders }, null, 2)}</pre>
              </section>
            )}

            {detailTab === 'raw' && (
              <section className="agent-monitor__network-section">
                <h4>Raw Request / Response</h4>
                <pre>{JSON.stringify({
                  summary: detail.summary,
                  requestBody: detail.requestBody,
                  responseBody: detail.responseBody,
                }, null, 2)}</pre>
              </section>
            )}
          </>
        ) : (
          <div className="agent-monitor__empty">选择一个请求查看原始 request / response。</div>
        )}
      </div>
    </div>
  )
}

function ConversationTab({ loading, error, messages }: { loading: boolean; error: string; messages: ChatMessage[] }) {
  if (loading) return <div className="agent-monitor__empty">正在解析对话历史...</div>
  if (error) return <div className="agent-monitor__empty">未找到可解析的 transcript：{error}</div>
  if (messages.length === 0) return <div className="agent-monitor__empty">暂无对话历史。</div>

  return (
    <div className="agent-monitor__conversation">
      {messages.map((message, index) => (
        <div key={`${message.role}:${message.timestamp}:${index}`} className={`agent-monitor__message agent-monitor__message--${message.role}`}>
          <span>{roleLabel(message.role)}</span>
          <p>{messagePreview(message)}</p>
          {'toolCalls' in message && message.toolCalls && message.toolCalls.length > 0 && (
            <div className="agent-monitor__tool-chips">
              {message.toolCalls.map((tool) => (
                <em key={`${tool.toolUseId || tool.toolName}:${tool.toolName}`}>{tool.toolName} · {tool.status}</em>
              ))}
            </div>
          )}
        </div>
      ))}
    </div>
  )
}

function TimelineTab({ items }: { items: MonitorTimelineItem[] }) {
  if (items.length === 0) return <div className="agent-monitor__empty">暂无工具或 hook 时间线。</div>

  return (
    <div className="agent-monitor__timeline">
      {items.map((item) => (
        <div key={item.id} className="agent-monitor__timeline-item">
          <time>{formatTime(item.timestampMs)}</time>
          <span>{timelineKindLabel(item.kind)}</span>
          <div>
            <strong>{item.title}</strong>
            {item.detail && <p>{item.detail}</p>}
          </div>
          <em>{item.status || (item.rawEventSeq != null ? `#${item.rawEventSeq}` : '')}</em>
        </div>
      ))}
    </div>
  )
}

function ApprovalsTab({
  permission,
  question,
  plan,
  onJump,
}: {
  permission?: DetailSession['pendingPermission']
  question?: DetailSession['pendingQuestion']
  plan: { title: string; content: string; permissions: string[] } | null
  onJump: () => void
}) {
  if (!permission && !question && !plan) {
    return <div className="agent-monitor__empty">当前 session 没有 pending approval、question 或 plan。</div>
  }

  return (
    <div className="agent-monitor__pending">
      {permission && (
        <section>
          <h4>Pending Permission</h4>
          <strong>{permission.toolName}</strong>
          <pre>{permission.toolInput || 'no input'}</pre>
        </section>
      )}
      {question && (
        <section>
          <h4>Pending Question</h4>
          <strong>{question.header || 'AskUserQuestion'}</strong>
          <p>{question.question}</p>
          {question.options.length > 0 && (
            <div className="agent-monitor__tool-chips">
              {question.options.map((option) => <em key={option}>{option}</em>)}
            </div>
          )}
        </section>
      )}
      {plan && (
        <section>
          <h4>Pending Plan</h4>
          <strong>{plan.title}</strong>
          <pre>{plan.content}</pre>
          {plan.permissions.length > 0 && (
            <div className="agent-monitor__tool-chips">
              {plan.permissions.map((permission) => <em key={permission}>{permission}</em>)}
            </div>
          )}
        </section>
      )}
      <button type="button" onClick={onJump}>跳转到对应终端处理</button>
    </div>
  )
}

function RawEventsTab({ events }: { events: MonitorSessionDetail['rawEvents'] }) {
  if (events.length === 0) return <div className="agent-monitor__empty">暂无 raw hook event。</div>

  return (
    <div className="agent-monitor__raw">
      {events.slice().reverse().map((event) => (
        <details key={event.seq} className="agent-monitor__raw-item">
          <summary>
            <span>{formatTime(event.timestampMs)}</span>
            <strong>{event.eventName}</strong>
            <em>#{event.seq}</em>
          </summary>
          <pre>{JSON.stringify(event.raw, null, 2)}</pre>
        </details>
      ))}
    </div>
  )
}
