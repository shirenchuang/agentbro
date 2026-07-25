import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  skillApiV2,
  type McpInspectionReport,
  type McpInspectionPrompt,
  type McpInspectionTool,
  type McpOperationResult,
  type McpServerEntry,
} from '../../services/skillApiV2'
import { SlideOver } from './SlideOver'

type InspectorTab = 'overview' | 'tools' | 'resources' | 'prompts' | 'logs'

interface McpInspectorDrawerProps {
  agentId: string
  agentName: string
  server: McpServerEntry | null
  onClose: () => void
  onReport: (serverName: string, report: McpInspectionReport) => void
}

const INSPECTOR_TABS: InspectorTab[] = ['overview', 'tools', 'resources', 'prompts', 'logs']

function createInspectionId() {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
    return crypto.randomUUID()
  }
  return `mcp-${Date.now()}-${Math.random().toString(36).slice(2)}`
}

export function McpInspectorDrawer({
  agentId,
  agentName,
  server,
  onClose,
  onReport,
}: McpInspectorDrawerProps) {
  const { t } = useTranslation()
  const [activeTab, setActiveTab] = useState<InspectorTab>('overview')
  const [report, setReport] = useState<McpInspectionReport | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const inspectionIdRef = useRef<string | null>(null)

  const cancelCurrent = useCallback(() => {
    const inspectionId = inspectionIdRef.current
    inspectionIdRef.current = null
    if (inspectionId) {
      void skillApiV2.cancelMcpInspection(inspectionId).catch(() => undefined)
    }
  }, [])

  const inspect = useCallback(async () => {
    if (!server) return
    cancelCurrent()
    const inspectionId = createInspectionId()
    inspectionIdRef.current = inspectionId
    setActiveTab('overview')
    setReport(null)
    setError(null)
    setLoading(true)
    try {
      const next = await skillApiV2.inspectMcpServer(agentId, server.name, inspectionId)
      if (inspectionIdRef.current !== inspectionId) return
      setReport(next)
      onReport(server.name, next)
    } catch (nextError) {
      if (inspectionIdRef.current !== inspectionId) return
      setError(String(nextError))
    } finally {
      if (inspectionIdRef.current === inspectionId) {
        inspectionIdRef.current = null
        setLoading(false)
      }
    }
  }, [agentId, cancelCurrent, onReport, server])

  useEffect(() => {
    if (!server) {
      cancelCurrent()
      setReport(null)
      setError(null)
      setLoading(false)
      return
    }
    void inspect()
    return cancelCurrent
  }, [cancelCurrent, inspect, server])

  const close = () => {
    cancelCurrent()
    onClose()
  }

  return (
    <SlideOver
      open={Boolean(server)}
      onClose={close}
      title={t('skills.mcpManagement.inspector.title', { name: server?.name || '' })}
      subtitle={server
        ? t('skills.mcpManagement.inspector.subtitle', {
            agent: agentName,
            transport: server.transport.toUpperCase(),
          })
        : undefined}
      actions={(
        <button
          className="sm2__btn"
          type="button"
          disabled={loading || !server}
          onClick={() => void inspect()}
        >
          {loading
            ? t('skills.mcpManagement.inspector.inspecting')
            : t('skills.mcpManagement.inspector.reinspect')}
        </button>
      )}
      className="sm2__mcp-inspector-drawer"
      width={720}
    >
      <div className="sm2__mcp-inspector">
        {server && !server.enabled && (
          <div className="sm2__mcp-inspector-disabled">
            {t('skills.mcpManagement.inspector.disabledNotice')}
          </div>
        )}

        {loading && (
          <div className="sm2__mcp-inspector-loading" role="status">
            <span className="sm2__mcp-inspector-spinner" aria-hidden="true" />
            <strong>{t('skills.mcpManagement.inspector.loadingTitle')}</strong>
            <span>{t('skills.mcpManagement.inspector.loadingDescription')}</span>
          </div>
        )}

        {error && (
          <div className="sm2__mcp-inspector-fatal" role="alert">
            <strong>{t('skills.mcpManagement.inspector.unavailable')}</strong>
            <span>{error}</span>
            <button className="sm2__btn" type="button" onClick={() => void inspect()}>
              {t('skills.mcpManagement.inspector.retry')}
            </button>
          </div>
        )}

        {report && !loading && (
          <>
            <InspectorStatus report={report} />
            <nav
              className="sm2__mcp-inspector-tabs"
              aria-label={t('skills.mcpManagement.inspector.tabsLabel')}
            >
              {INSPECTOR_TABS.map((tab) => (
                <button
                  key={tab}
                  type="button"
                  className={activeTab === tab
                    ? 'sm2__mcp-inspector-tab sm2__mcp-inspector-tab--active'
                    : 'sm2__mcp-inspector-tab'}
                  aria-selected={activeTab === tab}
                  role="tab"
                  onClick={() => setActiveTab(tab)}
                >
                  {t(`skills.mcpManagement.inspector.tabs.${tab}`)}
                  {tab === 'tools' && <span>{report.tools.length}</span>}
                  {tab === 'resources' && <span>{report.resources.length}</span>}
                  {tab === 'prompts' && <span>{report.prompts.length}</span>}
                </button>
              ))}
            </nav>
            <div className="sm2__mcp-inspector-content">
              {activeTab === 'overview' && <InspectorOverview report={report} />}
              {activeTab === 'tools' && (
                <InspectorTools
                  agentId={agentId}
                  serverName={server?.name || ''}
                  report={report}
                />
              )}
              {activeTab === 'resources' && <InspectorResources report={report} />}
              {activeTab === 'prompts' && (
                <InspectorPrompts
                  agentId={agentId}
                  serverName={server?.name || ''}
                  report={report}
                />
              )}
              {activeTab === 'logs' && <InspectorLogs report={report} />}
            </div>
          </>
        )}
      </div>
    </SlideOver>
  )
}

function InspectorStatus({ report }: { report: McpInspectionReport }) {
  const { t } = useTranslation()
  return (
    <div className={`sm2__mcp-inspector-status sm2__mcp-inspector-status--${report.status}`}>
      <span className="sm2__mcp-inspector-status-dot" aria-hidden="true" />
      <div>
        <strong>{t(`skills.mcpManagement.inspector.status.${report.status}`)}</strong>
        <span>{report.summary}</span>
      </div>
      <div className="sm2__mcp-inspector-status-meta">
        <span>{report.durationMs} ms</span>
        {report.protocolVersion && <code>{report.protocolVersion}</code>}
      </div>
    </div>
  )
}

function InspectorOverview({ report }: { report: McpInspectionReport }) {
  const { i18n, t } = useTranslation()
  const metrics = [
    {
      label: t('skills.mcpManagement.inspector.overview.server'),
      value: report.serverName
        ? `${report.serverName}${report.serverVersion ? ` ${report.serverVersion}` : ''}`
        : '—',
    },
    {
      label: t('skills.mcpManagement.inspector.overview.protocol'),
      value: report.protocolVersion || '—',
    },
    {
      label: t('skills.mcpManagement.inspector.overview.transport'),
      value: report.transport.toUpperCase(),
    },
    {
      label: t('skills.mcpManagement.inspector.overview.duration'),
      value: `${report.durationMs} ms`,
    },
    {
      label: t('skills.mcpManagement.inspector.overview.inspectedAt'),
      value: new Intl.DateTimeFormat(i18n.language, {
        dateStyle: 'short',
        timeStyle: 'medium',
      }).format(new Date(report.inspectedAtMs)),
    },
  ]
  return (
    <div className="sm2__mcp-inspector-overview">
      <div className="sm2__mcp-inspector-metrics">
        {metrics.map((metric) => (
          <div key={metric.label} className="sm2__mcp-inspector-metric">
            <span>{metric.label}</span>
            <strong>{metric.value}</strong>
          </div>
        ))}
      </div>
      <section className="sm2__mcp-inspector-section">
        <h3>{t('skills.mcpManagement.inspector.overview.capabilities')}</h3>
        <div className="sm2__mcp-inspector-capabilities">
          <Capability
            label="Tools"
            supported={report.capabilities.tools}
            count={report.tools.length}
          />
          <Capability
            label="Resources"
            supported={report.capabilities.resources}
            count={report.resources.length}
          />
          <Capability
            label="Prompts"
            supported={report.capabilities.prompts}
            count={report.prompts.length}
          />
          <Capability
            label="Logging"
            supported={report.capabilities.logging}
          />
        </div>
      </section>
      {report.warnings.length > 0 && (
        <InspectorMessages
          title={t('skills.mcpManagement.inspector.overview.warnings')}
          tone="warning"
          messages={report.warnings}
        />
      )}
      {report.suggestions.length > 0 && (
        <InspectorMessages
          title={t('skills.mcpManagement.inspector.overview.suggestions')}
          tone="info"
          messages={report.suggestions}
        />
      )}
      <p className="sm2__mcp-inspector-readonly">
        {t('skills.mcpManagement.inspector.readOnlyNotice')}
      </p>
    </div>
  )
}

function Capability({
  label,
  supported,
  count,
}: {
  label: string
  supported: boolean
  count?: number
}) {
  const { t } = useTranslation()
  return (
    <div className={`sm2__mcp-inspector-capability${supported ? ' sm2__mcp-inspector-capability--on' : ''}`}>
      <span aria-hidden="true">{supported ? '✓' : '—'}</span>
      <div>
        <strong>{label}</strong>
        <small>
          {supported
            ? count === undefined
              ? t('skills.mcpManagement.inspector.supported')
              : t('skills.mcpManagement.inspector.itemCount', { count })
            : t('skills.mcpManagement.inspector.unsupported')}
        </small>
      </div>
    </div>
  )
}

function InspectorMessages({
  title,
  messages,
  tone,
}: {
  title: string
  messages: string[]
  tone: 'warning' | 'info'
}) {
  return (
    <section className={`sm2__mcp-inspector-messages sm2__mcp-inspector-messages--${tone}`}>
      <h3>{title}</h3>
      <ul>
        {messages.map((message, index) => <li key={`${message}-${index}`}>{message}</li>)}
      </ul>
    </section>
  )
}

function InspectorTools({
  agentId,
  serverName,
  report,
}: {
  agentId: string
  serverName: string
  report: McpInspectionReport
}) {
  const { t } = useTranslation()
  const [query, setQuery] = useState('')
  const [activeTool, setActiveTool] = useState<string | null>(null)
  const unknownRiskCount = report.tools.filter((tool) => !tool.hasAnnotations).length
  const filteredTools = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase()
    if (!normalized) return report.tools
    return report.tools.filter((tool) => (
      tool.name.toLocaleLowerCase().includes(normalized)
      || tool.title?.toLocaleLowerCase().includes(normalized)
      || tool.description?.toLocaleLowerCase().includes(normalized)
    ))
  }, [query, report.tools])

  if (!report.capabilities.tools) {
    return <InspectorEmpty feature="Tools" />
  }
  if (report.tools.length === 0) {
    return <InspectorEmpty feature={t('skills.mcpManagement.inspector.tabs.tools')} supported />
  }
  return (
    <div className="sm2__mcp-inspector-tool-browser">
      <div className="sm2__mcp-tool-toolbar">
        <label className="sm2__mcp-inspector-search">
          <span>{t('skills.mcpManagement.inspector.interaction.searchTools')}</span>
          <input
            type="search"
            value={query}
            placeholder={t('skills.mcpManagement.inspector.interaction.searchPlaceholder')}
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
        <div className="sm2__mcp-tool-totals" aria-live="polite">
          <span>
            <strong>{filteredTools.length}</strong>
            {' / '}
            {report.tools.length}
            {' Tools'}
          </span>
          {unknownRiskCount > 0 && (
            <span className="sm2__mcp-tool-risk-summary">
              <strong>{unknownRiskCount}</strong>
              {' '}
              {t('skills.mcpManagement.inspector.risk.unknown')}
            </span>
          )}
        </div>
      </div>
      <div className="sm2__mcp-inspector-list sm2__mcp-tool-list">
        {filteredTools.map((tool) => (
          <InspectorToolCard
            key={tool.name}
            agentId={agentId}
            serverName={serverName}
            tool={tool}
            expanded={activeTool === tool.name}
            onToggle={() => setActiveTool((current) => current === tool.name ? null : tool.name)}
          />
        ))}
        {filteredTools.length === 0 && (
          <div className="sm2__mcp-inspector-filter-empty">
            {t('skills.mcpManagement.inspector.interaction.noMatchingTools')}
          </div>
        )}
      </div>
    </div>
  )
}

function InspectorToolCard({
  agentId,
  serverName,
  tool,
  expanded,
  onToggle,
}: {
  agentId: string
  serverName: string
  tool: McpInspectionTool
  expanded: boolean
  onToggle: () => void
}) {
  const { t } = useTranslation()
  const riskTags = [
    tool.annotations.readOnly === true && ['safe', t('skills.mcpManagement.inspector.risk.readOnly')],
    tool.annotations.destructive === true && ['danger', t('skills.mcpManagement.inspector.risk.destructive')],
    tool.annotations.idempotent === true && ['neutral', t('skills.mcpManagement.inspector.risk.idempotent')],
    tool.annotations.openWorld === true && ['warning', t('skills.mcpManagement.inspector.risk.openWorld')],
  ].filter(Boolean) as [string, string][]
  const requiredCount = tool.inputs.filter((input) => input.required).length
  const visibleInputs = tool.inputs.slice(0, 5)
  const hiddenInputCount = tool.inputs.length - visibleInputs.length
  const description = formatToolDescription(tool.description)
  return (
    <article
      className={`sm2__mcp-inspector-card sm2__mcp-tool-card${expanded ? ' sm2__mcp-tool-card--expanded' : ''}`}
    >
      <div className="sm2__mcp-inspector-card-head">
        <div>
          <strong>{tool.title || tool.name}</strong>
          {tool.title && <code>{tool.name}</code>}
        </div>
        <div className="sm2__mcp-inspector-card-actions">
          {riskTags.length > 0 && (
            <div className="sm2__mcp-inspector-risk-tags">
              {riskTags.map(([tone, label]) => (
                <span key={label} className={`sm2__mcp-risk-tag sm2__mcp-risk-tag--${tone}`}>
                  {label}
                </span>
              ))}
            </div>
          )}
          <button
            className="sm2__btn sm2__btn--small"
            type="button"
            aria-expanded={expanded}
            onClick={onToggle}
          >
            {expanded
              ? t('skills.mcpManagement.inspector.interaction.closeTest')
              : t('skills.mcpManagement.inspector.interaction.testTool')}
          </button>
        </div>
      </div>
      {description && (
        <p title={description !== tool.description ? tool.description || undefined : undefined}>
          {description}
        </p>
      )}
      {tool.inputs.length > 0 && (
        <div className="sm2__mcp-tool-input-summary">
          <span>
            {tool.inputs.length}
            {' '}
            {t('skills.mcpManagement.inspector.parameters')}
            {requiredCount > 0 && (
              <>
                {' · '}
                {requiredCount}
                {' '}
                {t('skills.mcpManagement.inspector.required')}
              </>
            )}
          </span>
          <div className="sm2__mcp-tool-input-chips">
            {visibleInputs.map((input) => (
              <span
                key={input.name}
                className="sm2__mcp-tool-input-chip"
                title={input.description || undefined}
              >
                <code>{input.name}</code>
                <small>{input.valueType}</small>
                {input.required && <em aria-label={t('skills.mcpManagement.inspector.required')}>*</em>}
              </span>
            ))}
            {hiddenInputCount > 0 && (
              <span className="sm2__mcp-tool-input-more">+{hiddenInputCount}</span>
            )}
          </div>
        </div>
      )}
      {expanded && (
        <ToolRunner
          agentId={agentId}
          serverName={serverName}
          tool={tool}
        />
      )}
    </article>
  )
}

function ToolRunner({
  agentId,
  serverName,
  tool,
}: {
  agentId: string
  serverName: string
  tool: McpInspectionTool
}) {
  const { t } = useTranslation()
  const [argumentsText, setArgumentsText] = useState(() => (
    JSON.stringify(createExampleArguments(tool.inputSchema), null, 2)
  ))
  const [loading, setLoading] = useState(false)
  const [confirming, setConfirming] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [result, setResult] = useState<McpOperationResult | null>(null)
  const operationIdRef = useRef<string | null>(null)
  const needsConfirmation = !tool.hasAnnotations
    || tool.annotations.destructive !== false
    || tool.annotations.openWorld === true

  const cancel = useCallback(() => {
    const operationId = operationIdRef.current
    operationIdRef.current = null
    if (operationId) {
      void skillApiV2.cancelMcpOperation(operationId).catch(() => undefined)
    }
    setLoading(false)
  }, [])

  useEffect(() => cancel, [cancel])

  const run = async (confirmed: boolean) => {
    if (needsConfirmation && !confirmed) {
      setConfirming(true)
      return
    }
    let argumentsValue: Record<string, unknown>
    try {
      const parsed: unknown = JSON.parse(argumentsText)
      if (!isRecord(parsed)) {
        throw new Error(t('skills.mcpManagement.inspector.interaction.objectRequired'))
      }
      argumentsValue = parsed
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : String(nextError))
      return
    }

    cancel()
    const operationId = createInspectionId()
    operationIdRef.current = operationId
    setConfirming(false)
    setError(null)
    setResult(null)
    setLoading(true)
    try {
      const next = await skillApiV2.callMcpTool(
        agentId,
        serverName,
        operationId,
        tool.name,
        argumentsValue,
      )
      if (operationIdRef.current === operationId) {
        setResult(next)
      }
    } catch (nextError) {
      if (operationIdRef.current === operationId) {
        setError(String(nextError))
      }
    } finally {
      if (operationIdRef.current === operationId) {
        operationIdRef.current = null
        setLoading(false)
      }
    }
  }

  return (
    <div className="sm2__mcp-interaction">
      <div className="sm2__mcp-interaction-head">
        <div>
          <strong>{t('skills.mcpManagement.inspector.interaction.testTitle')}</strong>
          <span>{t('skills.mcpManagement.inspector.interaction.ephemeralNotice')}</span>
        </div>
        <details>
          <summary>{t('skills.mcpManagement.inspector.interaction.viewSchema')}</summary>
          <pre>{formatJson(tool.inputSchema)}</pre>
        </details>
      </div>
      <label className="sm2__mcp-interaction-editor">
        <span>{t('skills.mcpManagement.inspector.interaction.argumentsJson')}</span>
        <textarea
          value={argumentsText}
          spellCheck={false}
          disabled={loading}
          onChange={(event) => {
            setArgumentsText(event.target.value)
            setConfirming(false)
          }}
        />
      </label>
      {confirming && (
        <div className="sm2__mcp-interaction-confirm" role="alert">
          <div>
            <strong>{t('skills.mcpManagement.inspector.interaction.confirmTitle')}</strong>
            <span>{t('skills.mcpManagement.inspector.interaction.confirmDescription')}</span>
          </div>
          <button
            className="sm2__btn sm2__btn--small sm2__btn--danger"
            type="button"
            onClick={() => void run(true)}
          >
            {t('skills.mcpManagement.inspector.interaction.confirmCall')}
          </button>
        </div>
      )}
      {error && <McpInteractionError message={error} />}
      <div className="sm2__mcp-interaction-actions">
        {loading ? (
          <button className="sm2__btn sm2__btn--small" type="button" onClick={cancel}>
            {t('skills.mcpManagement.inspector.interaction.cancelCall')}
          </button>
        ) : (
          <button
            className="sm2__btn sm2__btn--small sm2__btn--primary"
            type="button"
            onClick={() => void run(false)}
          >
            {t('skills.mcpManagement.inspector.interaction.callTool')}
          </button>
        )}
      </div>
      {loading && (
        <div className="sm2__mcp-interaction-running" role="status">
          <span className="sm2__mcp-inspector-spinner" aria-hidden="true" />
          {t('skills.mcpManagement.inspector.interaction.calling')}
        </div>
      )}
      {result && <McpOperationOutput operation={result} />}
    </div>
  )
}

function InspectorResources({ report }: { report: McpInspectionReport }) {
  if (!report.capabilities.resources) {
    return <InspectorEmpty feature="Resources" />
  }
  if (report.resources.length === 0) {
    return <InspectorEmpty feature="Resources" supported />
  }
  return (
    <div className="sm2__mcp-inspector-list">
      {report.resources.map((resource) => (
        <article key={resource.uri} className="sm2__mcp-inspector-card">
          <div className="sm2__mcp-inspector-card-head">
            <strong>{resource.title || resource.name}</strong>
            {resource.mimeType && <span className="sm2__mcp-risk-tag">{resource.mimeType}</span>}
          </div>
          {resource.description && <p>{resource.description}</p>}
          <code className="sm2__mcp-inspector-uri">{resource.uri}</code>
          {resource.size !== null && (
            <small className="sm2__mcp-inspector-size">{formatBytes(resource.size)}</small>
          )}
        </article>
      ))}
    </div>
  )
}

function InspectorPrompts({
  agentId,
  serverName,
  report,
}: {
  agentId: string
  serverName: string
  report: McpInspectionReport
}) {
  const { t } = useTranslation()
  const [activePrompt, setActivePrompt] = useState<string | null>(null)
  if (!report.capabilities.prompts) {
    return <InspectorEmpty feature="Prompts" />
  }
  if (report.prompts.length === 0) {
    return <InspectorEmpty feature="Prompts" supported />
  }
  return (
    <div className="sm2__mcp-inspector-list">
      {report.prompts.map((prompt) => (
        <article key={prompt.name} className="sm2__mcp-inspector-card">
          <div className="sm2__mcp-inspector-card-head">
            <div>
              <strong>{prompt.title || prompt.name}</strong>
              {prompt.title && <code>{prompt.name}</code>}
            </div>
            <button
              className="sm2__btn sm2__btn--small"
              type="button"
              aria-expanded={activePrompt === prompt.name}
              onClick={() => setActivePrompt((current) => (
                current === prompt.name ? null : prompt.name
              ))}
            >
              {activePrompt === prompt.name
                ? t('skills.mcpManagement.inspector.interaction.closePreview')
                : t('skills.mcpManagement.inspector.interaction.previewPrompt')}
            </button>
          </div>
          {prompt.description && <p>{prompt.description}</p>}
          {prompt.arguments.length > 0 && (
            <div className="sm2__mcp-inspector-inputs">
              <span>{t('skills.mcpManagement.inspector.arguments')}</span>
              {prompt.arguments.map((argument) => (
                <div key={argument.name}>
                  <code>{argument.name}</code>
                  {argument.required && <em>{t('skills.mcpManagement.inspector.required')}</em>}
                  {argument.description && <p>{argument.description}</p>}
                </div>
              ))}
            </div>
          )}
          {activePrompt === prompt.name && (
            <PromptPreview
              agentId={agentId}
              serverName={serverName}
              prompt={prompt}
            />
          )}
        </article>
      ))}
    </div>
  )
}

function PromptPreview({
  agentId,
  serverName,
  prompt,
}: {
  agentId: string
  serverName: string
  prompt: McpInspectionPrompt
}) {
  const { t } = useTranslation()
  const [argumentsValue, setArgumentsValue] = useState<Record<string, string>>(() => (
    Object.fromEntries(prompt.arguments.map((argument) => [argument.name, '']))
  ))
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [result, setResult] = useState<McpOperationResult | null>(null)
  const operationIdRef = useRef<string | null>(null)

  const cancel = useCallback(() => {
    const operationId = operationIdRef.current
    operationIdRef.current = null
    if (operationId) {
      void skillApiV2.cancelMcpOperation(operationId).catch(() => undefined)
    }
    setLoading(false)
  }, [])

  useEffect(() => cancel, [cancel])

  const preview = async () => {
    const missing = prompt.arguments.find(
      (argument) => argument.required && !argumentsValue[argument.name]?.trim(),
    )
    if (missing) {
      setError(t('skills.mcpManagement.inspector.interaction.requiredArgument', {
        name: missing.name,
      }))
      return
    }
    cancel()
    const operationId = createInspectionId()
    operationIdRef.current = operationId
    setError(null)
    setResult(null)
    setLoading(true)
    try {
      const next = await skillApiV2.getMcpPrompt(
        agentId,
        serverName,
        operationId,
        prompt.name,
        argumentsValue,
      )
      if (operationIdRef.current === operationId) {
        setResult(next)
      }
    } catch (nextError) {
      if (operationIdRef.current === operationId) {
        setError(String(nextError))
      }
    } finally {
      if (operationIdRef.current === operationId) {
        operationIdRef.current = null
        setLoading(false)
      }
    }
  }

  return (
    <div className="sm2__mcp-interaction">
      <div className="sm2__mcp-interaction-head">
        <div>
          <strong>{t('skills.mcpManagement.inspector.interaction.previewTitle')}</strong>
          <span>{t('skills.mcpManagement.inspector.interaction.promptNotice')}</span>
        </div>
      </div>
      {prompt.arguments.length > 0 && (
        <div className="sm2__mcp-prompt-fields">
          {prompt.arguments.map((argument) => (
            <label key={argument.name}>
              <span>
                {argument.name}
                {argument.required && <em>{t('skills.mcpManagement.inspector.required')}</em>}
              </span>
              <input
                value={argumentsValue[argument.name] || ''}
                disabled={loading}
                placeholder={argument.description || ''}
                onChange={(event) => setArgumentsValue((current) => ({
                  ...current,
                  [argument.name]: event.target.value,
                }))}
              />
            </label>
          ))}
        </div>
      )}
      {error && <McpInteractionError message={error} />}
      <div className="sm2__mcp-interaction-actions">
        {loading ? (
          <button className="sm2__btn sm2__btn--small" type="button" onClick={cancel}>
            {t('skills.mcpManagement.inspector.interaction.cancelPreview')}
          </button>
        ) : (
          <button
            className="sm2__btn sm2__btn--small sm2__btn--primary"
            type="button"
            onClick={() => void preview()}
          >
            {t('skills.mcpManagement.inspector.interaction.generatePreview')}
          </button>
        )}
      </div>
      {loading && (
        <div className="sm2__mcp-interaction-running" role="status">
          <span className="sm2__mcp-inspector-spinner" aria-hidden="true" />
          {t('skills.mcpManagement.inspector.interaction.previewing')}
        </div>
      )}
      {result && <McpOperationOutput operation={result} />}
    </div>
  )
}

function McpInteractionError({ message }: { message: string }) {
  const { t } = useTranslation()
  return (
    <div className="sm2__mcp-interaction-error" role="alert">
      <strong>{t('skills.mcpManagement.inspector.interaction.failed')}</strong>
      <span>{message}</span>
    </div>
  )
}

function McpOperationOutput({ operation }: { operation: McpOperationResult }) {
  const { t } = useTranslation()
  const result = isRecord(operation.result) ? operation.result : null
  const content = result && Array.isArray(result.content) ? result.content : []
  const messages = result && Array.isArray(result.messages) ? result.messages : []
  const structuredContent = result?.structuredContent
  const failed = operation.category === 'tool_error'

  return (
    <section className={`sm2__mcp-operation-output${failed ? ' sm2__mcp-operation-output--error' : ''}`}>
      <header>
        <div>
          <span className="sm2__mcp-operation-output-dot" aria-hidden="true" />
          <strong>
            {failed
              ? t('skills.mcpManagement.inspector.interaction.toolReturnedError')
              : t('skills.mcpManagement.inspector.interaction.completed')}
          </strong>
        </div>
        <small>{operation.durationMs} ms</small>
      </header>
      {content.map((item, index) => (
        <OperationContent key={`content-${index}`} value={item} />
      ))}
      {messages.map((item, index) => (
        <PromptMessage key={`message-${index}`} value={item} />
      ))}
      {structuredContent !== undefined && (
        <div className="sm2__mcp-operation-structured">
          <span>{t('skills.mcpManagement.inspector.interaction.structuredResult')}</span>
          <pre>{formatJson(structuredContent)}</pre>
        </div>
      )}
      {operation.warnings.map((warning, index) => (
        <p key={`${warning}-${index}`} className="sm2__mcp-operation-warning">{warning}</p>
      ))}
      <details className="sm2__mcp-operation-raw">
        <summary>{t('skills.mcpManagement.inspector.interaction.rawResponse')}</summary>
        <pre>{formatJson(operation.result)}</pre>
      </details>
    </section>
  )
}

function OperationContent({ value }: { value: unknown }) {
  const { t } = useTranslation()
  if (!isRecord(value)) {
    return <pre className="sm2__mcp-operation-text">{formatJson(value)}</pre>
  }
  if (value.type === 'text' && typeof value.text === 'string') {
    return <pre className="sm2__mcp-operation-text">{value.text}</pre>
  }
  if ((value.type === 'image' || value.type === 'audio') && typeof value.mimeType === 'string') {
    return (
      <div className="sm2__mcp-operation-media">
        <strong>{value.type === 'image' ? 'Image' : 'Audio'}</strong>
        <code>{value.mimeType}</code>
        <span>{t('skills.mcpManagement.inspector.interaction.binaryResult')}</span>
      </div>
    )
  }
  return <pre className="sm2__mcp-operation-text">{formatJson(value)}</pre>
}

function PromptMessage({ value }: { value: unknown }) {
  const { t } = useTranslation()
  const message = isRecord(value) ? value : {}
  const role = typeof message.role === 'string' ? message.role : 'message'
  return (
    <div className="sm2__mcp-prompt-message">
      <span>{role}</span>
      <div>
        {renderPromptContent(message.content, t('skills.mcpManagement.inspector.interaction.emptyContent'))}
      </div>
    </div>
  )
}

function renderPromptContent(value: unknown, emptyLabel: string) {
  if (Array.isArray(value)) {
    return value.map((item, index) => (
      <OperationContent key={`prompt-content-${index}`} value={item} />
    ))
  }
  if (isRecord(value) && value.type === 'text' && typeof value.text === 'string') {
    return <pre className="sm2__mcp-operation-text">{value.text}</pre>
  }
  if (value === undefined || value === null) {
    return <span className="sm2__mcp-operation-empty">{emptyLabel}</span>
  }
  return <OperationContent value={value} />
}

function InspectorLogs({ report }: { report: McpInspectionReport }) {
  const { t } = useTranslation()
  return (
    <div className="sm2__mcp-inspector-timeline">
      {report.steps.map((step, index) => (
        <div key={`${step.phase}-${index}`} className={`sm2__mcp-inspector-step sm2__mcp-inspector-step--${step.status}`}>
          <span className="sm2__mcp-inspector-step-dot" aria-hidden="true" />
          <div>
            <div>
              <strong>
                {t(`skills.mcpManagement.inspector.phases.${step.phase}`, {
                  defaultValue: step.phase,
                })}
              </strong>
              <small>{step.durationMs} ms</small>
            </div>
            <p>{step.message}</p>
          </div>
        </div>
      ))}
    </div>
  )
}

function InspectorEmpty({
  feature,
  supported = false,
}: {
  feature: string
  supported?: boolean
}) {
  const { t } = useTranslation()
  return (
    <div className="sm2__mcp-inspector-empty">
      <span aria-hidden="true">{supported ? '0' : '—'}</span>
      <strong>
        {supported
          ? t('skills.mcpManagement.inspector.emptySupported', { feature })
          : t('skills.mcpManagement.inspector.emptyUnsupported', { feature })}
      </strong>
      <p>
        {supported
          ? t('skills.mcpManagement.inspector.emptySupportedDescription')
          : t('skills.mcpManagement.inspector.emptyUnsupportedDescription')}
      </p>
    </div>
  )
}

function createExampleArguments(schema: Record<string, unknown>): Record<string, unknown> {
  const properties = isRecord(schema.properties) ? schema.properties : {}
  const required = new Set(
    Array.isArray(schema.required)
      ? schema.required.filter((value): value is string => typeof value === 'string')
      : [],
  )
  const result: Record<string, unknown> = {}
  for (const [name, value] of Object.entries(properties)) {
    const property = isRecord(value) ? value : {}
    if (!required.has(name) && property.default === undefined) continue
    result[name] = exampleSchemaValue(property)
  }
  return result
}

function formatToolDescription(description: string | null) {
  if (!description) return null
  const marker = description.search(/\s+Parameters\s*:/i)
  const visible = marker >= 0 ? description.slice(0, marker) : description
  return visible.replace(/\s+/g, ' ').trim()
}

function exampleSchemaValue(schema: Record<string, unknown>): unknown {
  if (schema.default !== undefined) return schema.default
  if (schema.example !== undefined) return schema.example
  if (Array.isArray(schema.examples) && schema.examples.length > 0) return schema.examples[0]
  if (Array.isArray(schema.enum) && schema.enum.length > 0) return schema.enum[0]
  switch (schema.type) {
    case 'boolean':
      return false
    case 'integer':
    case 'number':
      return 0
    case 'array':
      return []
    case 'object':
      return createExampleArguments(schema)
    default:
      return ''
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function formatJson(value: unknown) {
  try {
    return JSON.stringify(value, null, 2)
  } catch {
    return String(value)
  }
}

function formatBytes(size: number) {
  if (size < 1024) return `${size} B`
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`
  return `${(size / 1024 / 1024).toFixed(1)} MB`
}
