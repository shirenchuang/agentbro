import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  getNetworkMonitorRequestDetail,
  getNetworkMonitorRequests,
  getNetworkMonitorStatus,
  getMonitorSessionDetail,
  getMonitorSessions,
  setNetworkMonitorEnabled,
  type MonitorSessionDetail,
  type MonitorSessionSummary,
  type MonitorTimelineItem,
  type NetworkMonitorStatus,
  type NetworkRequestDetail,
  type NetworkRequestSummary,
} from '../../../services/monitorApi'
import { getChatHistory, jumpToTerminal } from '../../../services/tauriApi'
import { mapParsedMessages } from '../../../hooks/useTauri'
import { selectSessionList, useSessionStore } from '../../../stores/sessionStore'
import type { BackendSession } from '../../../services/tauriApi'
import type { ChatMessage, SessionState, TokenUsage } from '../../../types/agent'
import { formatDurationShort } from '../../../utils/time'
import { formatTokens } from '../../../utils/tokens'
import { Toggle } from '../Toggle'
import './AgentMonitorSection.css'

type DetailTab = 'overview' | 'network' | 'conversation' | 'timeline' | 'approvals' | 'raw'
type DetailSession = BackendSession | SessionState

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

function shortPath(path?: string | null) {
  if (!path) return '-'
  const parts = path.split('/').filter(Boolean)
  if (parts.length <= 3) return path
  return `…/${parts.slice(-3).join('/')}`
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

function roleLabel(role: ChatMessage['role']) {
  if (role === 'tool_use') return 'tool'
  if (role === 'permission') return 'approval'
  return role
}

export function AgentMonitorSection() {
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
    return () => {
      setNetworkMonitorEnabled(false).catch(() => undefined)
    }
  }, [loadNetworkRequests, loadNetworkStatus])

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

  useEffect(() => {
    if (activeTab !== 'network' || !networkStatus.enabled) return
    loadNetworkRequests()
  }, [activeTab, loadNetworkRequests, networkStatus.enabled])

  useEffect(() => {
    if (activeTab !== 'network' || !selectedNetworkRequestId) {
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
  }, [activeTab, selectedNetworkRequestId, networkRequests])

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

      <div className={`agent-monitor__network-control ${networkStatus.enabled ? 'agent-monitor__network-control--on' : ''}`}>
        <div className="agent-monitor__network-copy">
          <strong>原生网络请求监控</strong>
          <span>默认关闭；开启后会启动本地代理并记录 request body、system prompt、messages、tools 和 response usage。</span>
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

      {networkError && <div className="agent-monitor__notice">{networkError}</div>}

      <div className="agent-monitor__stats" aria-label="Agent monitor summary">
        <div><span>Sessions</span><strong>{sessions.length}</strong></div>
        <div><span>运行中</span><strong>{runningCount}</strong></div>
        <div><span>等待用户</span><strong>{waitingCount}</strong></div>
        <div><span>网络请求</span><strong>{networkStatus.requestCount}</strong></div>
        <div><span>Raw events</span><strong>{rawEvents.length}</strong></div>
      </div>

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
            <table className="agent-monitor__table">
              <thead>
                <tr>
                  <th>Agent</th>
                  <th>项目 / cwd</th>
                  <th>状态</th>
                  <th>时长</th>
                  <th>Token</th>
                  <th>最后工具</th>
                  <th>等待</th>
                  <th>Sub</th>
                </tr>
              </thead>
              <tbody>
                {filteredSessions.map((session) => (
                  <tr
                    key={session.id}
                    className={session.id === selectedId ? 'agent-monitor__row--active' : ''}
                    onClick={() => {
                      setSelectedId(session.id)
                      setActiveTab('overview')
                    }}
                  >
                    <td>
                      <strong>{agentLabel(session.agentType, session.engineLabel)}</strong>
                      <span>{session.id.slice(0, 8)}</span>
                    </td>
                    <td>
                      <strong>{session.title || session.project || 'Unknown'}</strong>
                      <span title={session.cwd}>{shortPath(session.cwd)}</span>
                    </td>
                    <td><i className={`agent-monitor__dot agent-monitor__dot--${session.phase}`} />{phaseLabel(session.phase)}</td>
                    <td>{formatDurationShort(session.duration)}</td>
                    <td>{formatTokens(session.tokenTotal)}</td>
                    <td title={session.lastToolTarget || undefined}>{session.lastToolName || '-'}</td>
                    <td>{session.waitingUser ? pendingLabel(session.pendingKind) : '-'}</td>
                    <td>{session.subagentCount}</td>
                  </tr>
                ))}
              </tbody>
            </table>
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
                  <code>{selectedId}</code>
                </div>
                <button type="button" onClick={jumpSelected}>跳转终端</button>
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
              <strong>{subagent.agentType || 'subagent'}</strong>
              <span>{subagent.status}</span>
              <p>{subagent.description}</p>
            </div>
          ))}
        </div>
      )}
    </div>
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
  const usage = selected?.usage ?? null

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
            <small>{request.messageCount} msg · {request.toolCount} tools · {formatBytes(request.requestBytes)}</small>
          </button>
        ))}
      </div>

      <div className="agent-monitor__network-detail">
        <div className="agent-monitor__kv-grid">
          <div><span>model</span><strong>{selected.model || '-'}</strong></div>
          <div><span>status</span><strong>{selected.inProgress ? 'streaming' : selected.status ?? selected.error ?? '-'}</strong></div>
          <div><span>duration</span><strong>{selected.durationMs != null ? `${selected.durationMs}ms` : '-'}</strong></div>
          <div><span>tokens</span><strong>{usage ? formatTokens(usageTotal(usage)) : '-'}</strong></div>
        </div>

        {selected.systemPreview && (
          <section className="agent-monitor__network-section">
            <h4>System Prompt Preview</h4>
            <p>{selected.systemPreview}</p>
          </section>
        )}

        {loading ? (
          <div className="agent-monitor__inline">正在读取请求详情...</div>
        ) : detail ? (
          <>
            <section className="agent-monitor__network-section">
              <h4>Request</h4>
              <pre>{JSON.stringify(detail.requestBody, null, 2)}</pre>
            </section>
            <section className="agent-monitor__network-section">
              <h4>Response{detail.responseBodyTruncated ? ' · truncated' : ''}</h4>
              <pre>{detail.responseBody || JSON.stringify(detail.responseHeaders, null, 2)}</pre>
            </section>
            <section className="agent-monitor__network-section">
              <h4>Headers</h4>
              <pre>{JSON.stringify({ request: detail.requestHeaders, response: detail.responseHeaders }, null, 2)}</pre>
            </section>
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
