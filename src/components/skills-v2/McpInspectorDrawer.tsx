import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  skillApiV2,
  type McpInspectionReport,
  type McpInspectionTool,
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
              {activeTab === 'tools' && <InspectorTools report={report} />}
              {activeTab === 'resources' && <InspectorResources report={report} />}
              {activeTab === 'prompts' && <InspectorPrompts report={report} />}
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

function InspectorTools({ report }: { report: McpInspectionReport }) {
  const { t } = useTranslation()
  if (!report.capabilities.tools) {
    return <InspectorEmpty feature="Tools" />
  }
  if (report.tools.length === 0) {
    return <InspectorEmpty feature={t('skills.mcpManagement.inspector.tabs.tools')} supported />
  }
  return (
    <div className="sm2__mcp-inspector-list">
      {report.tools.map((tool) => <InspectorToolCard key={tool.name} tool={tool} />)}
    </div>
  )
}

function InspectorToolCard({ tool }: { tool: McpInspectionTool }) {
  const { t } = useTranslation()
  const riskTags = [
    tool.annotations.readOnly === true && ['safe', t('skills.mcpManagement.inspector.risk.readOnly')],
    tool.annotations.destructive === true && ['danger', t('skills.mcpManagement.inspector.risk.destructive')],
    tool.annotations.idempotent === true && ['neutral', t('skills.mcpManagement.inspector.risk.idempotent')],
    tool.annotations.openWorld === true && ['warning', t('skills.mcpManagement.inspector.risk.openWorld')],
  ].filter(Boolean) as [string, string][]
  return (
    <article className="sm2__mcp-inspector-card">
      <div className="sm2__mcp-inspector-card-head">
        <div>
          <strong>{tool.title || tool.name}</strong>
          {tool.title && <code>{tool.name}</code>}
        </div>
        <div className="sm2__mcp-inspector-risk-tags">
          {tool.hasAnnotations ? (
            riskTags.length > 0
              ? riskTags.map(([tone, label]) => (
                  <span key={label} className={`sm2__mcp-risk-tag sm2__mcp-risk-tag--${tone}`}>
                    {label}
                  </span>
                ))
              : <span className="sm2__mcp-risk-tag">{t('skills.mcpManagement.inspector.risk.declared')}</span>
          ) : (
            <span className="sm2__mcp-risk-tag sm2__mcp-risk-tag--warning">
              {t('skills.mcpManagement.inspector.risk.unknown')}
            </span>
          )}
        </div>
      </div>
      {tool.description && <p>{tool.description}</p>}
      {tool.inputs.length > 0 && (
        <div className="sm2__mcp-inspector-inputs">
          <span>{t('skills.mcpManagement.inspector.parameters')}</span>
          {tool.inputs.map((input) => (
            <div key={input.name}>
              <code>{input.name}</code>
              <small>{input.valueType}</small>
              {input.required && <em>{t('skills.mcpManagement.inspector.required')}</em>}
              {input.description && <p>{input.description}</p>}
            </div>
          ))}
        </div>
      )}
    </article>
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

function InspectorPrompts({ report }: { report: McpInspectionReport }) {
  const { t } = useTranslation()
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
        </article>
      ))}
    </div>
  )
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

function formatBytes(size: number) {
  if (size < 1024) return `${size} B`
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`
  return `${(size / 1024 / 1024).toFixed(1)} MB`
}
