import { useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  skillApiV2,
  type AgentDetail,
  type McpConfigValue,
  type McpInspectionReport,
  type McpInventory,
  type McpServerDraft,
  type McpServerEntry,
  type McpTransport,
} from '../../services/skillApiV2'
import { McpInspectorDrawer } from './McpInspectorDrawer'
import { PreviewDialog } from './PreviewDialog'
import { SlideOver } from './SlideOver'

interface EditorState {
  originalName: string | null
  draft: McpServerDraft
}

const SECRET_PLACEHOLDER = '••••••••'

export function McpManagementTab({ detail }: { detail: AgentDetail }) {
  const { t } = useTranslation()
  const [inventory, setInventory] = useState<McpInventory>(() => legacyInventory(detail))
  const [loading, setLoading] = useState(true)
  const [busyAction, setBusyAction] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [editor, setEditor] = useState<EditorState | null>(null)
  const [editorError, setEditorError] = useState<string | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<McpServerEntry | null>(null)
  const [inspectorTarget, setInspectorTarget] = useState<McpServerEntry | null>(null)
  const [inspectionResults, setInspectionResults] = useState<Record<string, McpInspectionReport>>({})

  const load = async () => {
    setLoading(true)
    setError(null)
    try {
      const next = await skillApiV2.listMcpInventory(detail.id)
      setInventory(mergeLegacyInventory(next, detail))
    } catch (nextError) {
      setError(String(nextError))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    setInventory(legacyInventory(detail))
    setEditor(null)
    setDeleteTarget(null)
    setInspectorTarget(null)
    setInspectionResults({})
    void load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [detail.id])

  useEffect(() => {
    if (!notice) return
    const timer = window.setTimeout(() => setNotice(null), 3200)
    return () => window.clearTimeout(timer)
  }, [notice])

  const enabledCount = inventory.servers.filter((server) => server.enabled).length
  const invalidCount = inventory.servers.filter((server) => !server.valid).length

  const startAdd = () => {
    setEditorError(null)
    setEditor({
      originalName: null,
      draft: emptyDraft(firstSupportedTransport(inventory)),
    })
  }

  const startEdit = (server: McpServerEntry) => {
    setEditorError(null)
    setEditor({
      originalName: server.name,
      draft: {
        name: server.name,
        transport: server.transport,
        command: server.command,
        args: [...server.args],
        env: server.env.map((value) => ({ ...value })),
        cwd: server.cwd,
        url: server.url,
        headers: server.headers.map((value) => ({ ...value })),
      },
    })
  }

  const save = async () => {
    if (!editor) return
    const draft = normalizeDraft(editor.draft)
    setBusyAction('save')
    setEditorError(null)
    try {
      const validation = await skillApiV2.validateMcpServerDraft(
        detail.id,
        draft,
        editor.originalName,
      )
      if (!validation.valid) {
        setEditorError(validation.message)
        return
      }
      const next = await skillApiV2.saveMcpServer(
        detail.id,
        draft,
        inventory.revision,
        editor.originalName,
      )
      setInventory(next)
      setEditor(null)
      setNotice(t('skills.mcpManagement.saved', { name: draft.name }))
      setError(null)
    } catch (nextError) {
      setEditorError(String(nextError))
    } finally {
      setBusyAction(null)
    }
  }

  const toggle = async (server: McpServerEntry) => {
    const action = `toggle:${server.name}`
    setBusyAction(action)
    setError(null)
    try {
      const next = await skillApiV2.setMcpServerEnabled(
        detail.id,
        server.name,
        inventory.revision,
        !server.enabled,
      )
      setInventory(next)
      setNotice(t(
        server.enabled ? 'skills.mcpManagement.disabled' : 'skills.mcpManagement.enabled',
        { name: server.name },
      ))
    } catch (nextError) {
      setError(String(nextError))
    } finally {
      setBusyAction(null)
    }
  }

  const remove = async () => {
    if (!deleteTarget) return
    const action = `delete:${deleteTarget.name}`
    setBusyAction(action)
    setError(null)
    try {
      const next = await skillApiV2.deleteMcpServer(
        detail.id,
        deleteTarget.name,
        inventory.revision,
      )
      setInventory(next)
      setInspectionResults((current) => {
        const nextResults = { ...current }
        delete nextResults[deleteTarget.name]
        return nextResults
      })
      setNotice(t('skills.mcpManagement.deleted', { name: deleteTarget.name }))
      setDeleteTarget(null)
    } catch (nextError) {
      setError(String(nextError))
    } finally {
      setBusyAction(null)
    }
  }

  const storeInspectionReport = useCallback((serverName: string, report: McpInspectionReport) => {
    setInspectionResults((current) => ({ ...current, [serverName]: report }))
  }, [])

  return (
    <div className="sm2__mcp-manager">
      <div className="sm2__mcp-toolbar">
        <div className="sm2__mcp-stats" aria-label={t('skills.mcpManagement.summary')}>
          <McpStat value={inventory.servers.length} label={t('skills.mcpManagement.total')} />
          <McpStat value={enabledCount} label={t('skills.mcpManagement.enabledCount')} tone="ok" />
          <McpStat value={invalidCount} label={t('skills.mcpManagement.invalidCount')} tone={invalidCount ? 'warn' : 'muted'} />
        </div>
        <div className="sm2__btn-row sm2__mcp-toolbar-actions">
          <button className="sm2__btn" type="button" disabled={loading || Boolean(busyAction)} onClick={() => void load()}>
            {loading ? t('skills.mcpManagement.scanning') : t('skills.mcpManagement.rescan')}
          </button>
          {inventory.capabilities.editable && (
            <button className="sm2__btn sm2__btn--primary" type="button" disabled={Boolean(busyAction)} onClick={startAdd}>
              {t('skills.mcpManagement.add')}
            </button>
          )}
        </div>
      </div>

      {!inventory.capabilities.editable && (
        <div className="sm2__notice sm2__notice--info">
          {t('skills.mcpManagement.readOnly')}
        </div>
      )}
      {inventory.configPath && (
        <div className="sm2__mcp-config-path">
          <span>{t('skills.mcpManagement.configPath')}</span>
          <code>{inventory.configPath}</code>
        </div>
      )}
      {error && <div className="sm2__error" role="alert">{error}</div>}
      {notice && <div className="sm2__notice sm2__notice--ok" role="status">{notice}</div>}

      {inventory.servers.length === 0 ? (
        <div className="sm2__empty sm2__empty--compact sm2__mcp-empty">
          <strong>{t('skills.mcpManagement.emptyTitle')}</strong>
          <span>{t('skills.mcpManagement.emptyDescription')}</span>
          {inventory.capabilities.editable && (
            <button className="sm2__btn sm2__btn--primary" type="button" onClick={startAdd}>
              {t('skills.mcpManagement.addFirst')}
            </button>
          )}
        </div>
      ) : (
        <section className="sm2__panel sm2__mcp-list">
          {inventory.servers.map((server) => (
            <McpServerRow
              key={server.name}
              server={server}
              result={inspectionResults[server.name]}
              toggleBusy={busyAction === `toggle:${server.name}`}
              actionsDisabled={Boolean(busyAction)}
              onToggle={() => void toggle(server)}
              onInspect={() => setInspectorTarget(server)}
              onEdit={() => startEdit(server)}
              onDelete={() => setDeleteTarget(server)}
            />
          ))}
        </section>
      )}

      <SlideOver
        open={Boolean(editor)}
        onClose={() => {
          if (busyAction !== 'save') setEditor(null)
        }}
        title={editor?.originalName
          ? t('skills.mcpManagement.editor.editTitle', { name: editor.originalName })
          : t('skills.mcpManagement.editor.addTitle')}
        subtitle={detail.displayName}
        className="sm2__mcp-editor-drawer"
        width={620}
      >
        {editor && (
          <McpEditor
            editor={editor}
            capabilities={inventory.capabilities}
            error={editorError}
            busy={busyAction === 'save'}
            onChange={setEditor}
            onCancel={() => setEditor(null)}
            onSave={() => void save()}
          />
        )}
      </SlideOver>

      <McpInspectorDrawer
        agentId={detail.id}
        agentName={detail.displayName}
        server={inspectorTarget}
        onClose={() => setInspectorTarget(null)}
        onReport={storeInspectionReport}
      />

      {deleteTarget && (
        <PreviewDialog
          title={t('skills.mcpManagement.deleteTitle', { name: deleteTarget.name })}
          confirmLabel={t('skills.mcpManagement.deleteConfirm')}
          busyLabel={t('skills.mcpManagement.deleting')}
          destructive
          busy={busyAction === `delete:${deleteTarget.name}`}
          onCancel={() => setDeleteTarget(null)}
          onConfirm={() => void remove()}
        >
          <p>{t('skills.mcpManagement.deleteDescription', {
            name: deleteTarget.name,
            agent: detail.displayName,
          })}</p>
          <code>{deleteTarget.configPath}</code>
        </PreviewDialog>
      )}
    </div>
  )
}

function McpStat({
  value,
  label,
  tone = 'muted',
}: {
  value: number
  label: string
  tone?: 'ok' | 'warn' | 'muted'
}) {
  return (
    <div className={`sm2__mcp-stat sm2__mcp-stat--${tone}`}>
      <strong>{value}</strong>
      <span>{label}</span>
    </div>
  )
}

function McpServerRow({
  server,
  result,
  toggleBusy,
  actionsDisabled,
  onToggle,
  onInspect,
  onEdit,
  onDelete,
}: {
  server: McpServerEntry
  result?: McpInspectionReport
  toggleBusy: boolean
  actionsDisabled: boolean
  onToggle: () => void
  onInspect: () => void
  onEdit: () => void
  onDelete: () => void
}) {
  const { t } = useTranslation()
  const connection = server.transport === 'stdio'
    ? [server.command, ...server.args].filter(Boolean).join(' ')
    : server.url || ''
  return (
    <article className={`sm2__mcp-row${server.enabled ? '' : ' sm2__mcp-row--disabled'}`}>
      <div className="sm2__mcp-row-main">
        <div className="sm2__mcp-row-title">
          <strong>{server.name}</strong>
          <span className={`sm2__tag sm2__mcp-transport sm2__mcp-transport--${server.transport}`}>
            {server.transport.toUpperCase()}
          </span>
          <span className={`sm2__tag sm2__tag--${server.valid ? 'ok' : 'conflict'}`}>
            {server.valid
              ? t('skills.mcpManagement.valid')
              : t('skills.mcpManagement.invalid')}
          </span>
          {server.disabledByAgentbro && (
            <span className="sm2__tag sm2__tag--unmanaged">
              {t('skills.mcpManagement.disabledByAgentbro')}
            </span>
          )}
        </div>
        <code className="sm2__mcp-command">{connection}</code>
        <span className="sm2__mcp-message">{server.message}</span>
        {server.warnings.length > 0 && (
          <span className="sm2__mcp-warning">{server.warnings.join('；')}</span>
        )}
        {result && (
          <div className={`sm2__mcp-test-result sm2__mcp-test-result--${result.status}`}>
            <strong>{t(`skills.mcpManagement.inspector.status.${result.status}`)}</strong>
            <span>{result.summary}</span>
            <small>{result.durationMs} ms</small>
          </div>
        )}
      </div>
      <div className="sm2__mcp-row-actions">
        {server.editable ? (
          <button
            type="button"
            className={`sm2__mcp-switch${server.enabled ? ' sm2__mcp-switch--on' : ''}`}
            role="switch"
            aria-checked={server.enabled}
            aria-label={t('skills.mcpManagement.toggleLabel', { name: server.name })}
            disabled={actionsDisabled && !toggleBusy}
            onClick={onToggle}
          >
            <span />
          </button>
        ) : (
          <span className="sm2__tag sm2__tag--unmanaged">{t('skills.mcpManagement.readOnlyTag')}</span>
        )}
        {server.editable && (
          <>
            <button className="sm2__btn" type="button" disabled={actionsDisabled} onClick={onInspect}>
              {t('skills.mcpManagement.inspector.inspect')}
            </button>
            <button className="sm2__btn" type="button" disabled={actionsDisabled} onClick={onEdit}>
              {t('skills.mcpManagement.edit')}
            </button>
            <button className="sm2__btn sm2__btn--danger" type="button" disabled={actionsDisabled} onClick={onDelete}>
              {t('skills.mcpManagement.delete')}
            </button>
          </>
        )}
      </div>
    </article>
  )
}

function McpEditor({
  editor,
  capabilities,
  error,
  busy,
  onChange,
  onCancel,
  onSave,
}: {
  editor: EditorState
  capabilities: McpInventory['capabilities']
  error: string | null
  busy: boolean
  onChange: (editor: EditorState) => void
  onCancel: () => void
  onSave: () => void
}) {
  const { t } = useTranslation()
  const draft = editor.draft
  const setDraft = (patch: Partial<McpServerDraft>) =>
    onChange({ ...editor, draft: { ...draft, ...patch } })
  const transports = useMemo(() => ([
    capabilities.supportsStdio && 'stdio',
    capabilities.supportsHttp && 'http',
    capabilities.supportsSse && 'sse',
  ].filter(Boolean) as McpTransport[]), [capabilities])

  return (
    <div className="sm2__mcp-editor">
      <div className="sm2__mcp-editor-content">
        <div className="sm2__field">
          <label htmlFor="mcp-name">{t('skills.mcpManagement.editor.name')}</label>
          <input
            id="mcp-name"
            value={draft.name}
            autoComplete="off"
            disabled={busy}
            placeholder="context7"
            onChange={(event) => setDraft({ name: event.target.value })}
          />
        </div>

        <fieldset className="sm2__mcp-transport-field">
          <legend>{t('skills.mcpManagement.editor.transport')}</legend>
          <div className="sm2__mcp-transport-rail">
            {transports.map((transport) => (
              <button
                key={transport}
                className={draft.transport === transport ? 'sm2__mcp-transport-option sm2__mcp-transport-option--active' : 'sm2__mcp-transport-option'}
                type="button"
                aria-pressed={draft.transport === transport}
                disabled={busy}
                onClick={() => setDraft(transportDraft(draft, transport))}
              >
                {t(`skills.mcpManagement.editor.transportShortOptions.${transport}`)}
              </button>
            ))}
          </div>
        </fieldset>

        <div className="sm2__mcp-config-card">
          {draft.transport === 'stdio' ? (
            <>
              <div className="sm2__field">
                <label htmlFor="mcp-command">{t('skills.mcpManagement.editor.command')}</label>
                <input
                  id="mcp-command"
                  value={draft.command || ''}
                  autoComplete="off"
                  disabled={busy}
                  placeholder="npx"
                  onChange={(event) => setDraft({ command: event.target.value })}
                />
              </div>
              <StringListEditor
                label={t('skills.mcpManagement.editor.arguments')}
                addLabel={t('skills.mcpManagement.editor.addArgument')}
                values={draft.args}
                disabled={busy}
                onChange={(args) => setDraft({ args })}
              />
              <div className="sm2__field">
                <label htmlFor="mcp-cwd">{t('skills.mcpManagement.editor.cwd')}</label>
                <input
                  id="mcp-cwd"
                  value={draft.cwd || ''}
                  autoComplete="off"
                  disabled={busy}
                  placeholder="~/code/project"
                  onChange={(event) => setDraft({ cwd: event.target.value })}
                />
              </div>
              <ConfigValueEditor
                label={t('skills.mcpManagement.editor.environment')}
                addLabel={t('skills.mcpManagement.editor.addEnvironment')}
                values={draft.env}
                disabled={busy}
                onChange={(env) => setDraft({ env })}
              />
            </>
          ) : (
            <>
              <div className="sm2__field">
                <label htmlFor="mcp-url">{t('skills.mcpManagement.editor.url')}</label>
                <input
                  id="mcp-url"
                  type="url"
                  value={draft.url || ''}
                  autoComplete="off"
                  disabled={busy}
                  placeholder={draft.transport === 'sse' ? 'https://example.com/sse' : 'https://example.com/mcp'}
                  onChange={(event) => setDraft({ url: event.target.value })}
                />
              </div>
              <ConfigValueEditor
                label={t('skills.mcpManagement.editor.headers')}
                addLabel={t('skills.mcpManagement.editor.addHeader')}
                values={draft.headers}
                disabled={busy}
                onChange={(headers) => setDraft({ headers })}
              />
            </>
          )}
        </div>

        <div className="sm2__mcp-secret-note">
          <span aria-hidden="true">i</span>
          {t('skills.mcpManagement.editor.secretNote')}
        </div>
        {error && <div className="sm2__error" role="alert">{error}</div>}
      </div>
      <div className="sm2__mcp-editor-actions">
        <button className="sm2__btn" type="button" disabled={busy} onClick={onCancel}>
          {t('skills.cancel')}
        </button>
        <button className="sm2__btn sm2__btn--primary" type="button" disabled={busy} onClick={onSave}>
          {busy ? t('skills.mcpManagement.editor.saving') : t('skills.save')}
        </button>
      </div>
    </div>
  )
}

function TrashIcon() {
  return (
    <svg aria-hidden="true" viewBox="0 0 16 16">
      <path d="M5.25 2.75h5.5M6.5 2.75v-1h3v1m-6 1.5h9m-8.25 0 .5 9h6.5l.5-9M6.5 6.5v4.75m3-4.75v4.75" />
    </svg>
  )
}

function StringListEditor({
  label,
  addLabel,
  values,
  disabled,
  onChange,
}: {
  label: string
  addLabel: string
  values: string[]
  disabled: boolean
  onChange: (values: string[]) => void
}) {
  const { t } = useTranslation()
  return (
    <div className="sm2__mcp-repeat-field">
      <label>{label}</label>
      {values.map((value, index) => (
        <div className="sm2__mcp-repeat-row" key={index}>
          <input
            value={value}
            disabled={disabled}
            aria-label={`${label} ${index + 1}`}
            onChange={(event) => onChange(values.map((item, itemIndex) => itemIndex === index ? event.target.value : item))}
          />
          <button
            className="sm2__icon-btn"
            type="button"
            disabled={disabled}
            aria-label={t('skills.delete')}
            onClick={() => onChange(values.filter((_, itemIndex) => itemIndex !== index))}
          >
            <TrashIcon />
          </button>
        </div>
      ))}
      <button className="sm2__btn sm2__btn--small sm2__mcp-add-row" type="button" disabled={disabled} onClick={() => onChange([...values, ''])}>
        {addLabel}
      </button>
    </div>
  )
}

function ConfigValueEditor({
  label,
  addLabel,
  values,
  disabled,
  onChange,
}: {
  label: string
  addLabel: string
  values: McpConfigValue[]
  disabled: boolean
  onChange: (values: McpConfigValue[]) => void
}) {
  const { t } = useTranslation()
  const update = (index: number, patch: Partial<McpConfigValue>) =>
    onChange(values.map((item, itemIndex) => itemIndex === index ? { ...item, ...patch } : item))
  return (
    <div className="sm2__mcp-repeat-field">
      <label>{label}</label>
      {values.map((item, index) => (
        <div className="sm2__mcp-kv-row" key={`${index}-${item.key}`}>
          <input
            value={item.key}
            disabled={disabled}
            aria-label={`${label} ${t('skills.mcpManagement.editor.key')} ${index + 1}`}
            placeholder={t('skills.mcpManagement.editor.key')}
            onChange={(event) => update(index, { key: event.target.value })}
          />
          <input
            type={item.secret ? 'password' : 'text'}
            value={item.value ?? ''}
            disabled={disabled}
            aria-label={`${label} ${t('skills.mcpManagement.editor.value')} ${index + 1}`}
            placeholder={item.secret && item.configured
              ? t('skills.mcpManagement.editor.secretConfigured')
              : t('skills.mcpManagement.editor.value')}
            onChange={(event) => update(index, {
              value: event.target.value || null,
              configured: Boolean(event.target.value),
            })}
          />
          <button
            className="sm2__icon-btn"
            type="button"
            disabled={disabled}
            aria-label={t('skills.delete')}
            onClick={() => onChange(values.filter((_, itemIndex) => itemIndex !== index))}
          >
            <TrashIcon />
          </button>
        </div>
      ))}
      <button
        className="sm2__btn sm2__btn--small sm2__mcp-add-row"
        type="button"
        disabled={disabled}
        onClick={() => onChange([...values, {
          key: '',
          value: '',
          secret: false,
          configured: false,
        }])}
      >
        {addLabel}
      </button>
    </div>
  )
}

function emptyDraft(transport: McpTransport): McpServerDraft {
  return {
    name: '',
    transport,
    command: transport === 'stdio' ? '' : null,
    args: [],
    env: [],
    cwd: null,
    url: transport === 'stdio' ? null : '',
    headers: [],
  }
}

function firstSupportedTransport(inventory: McpInventory): McpTransport {
  if (inventory.capabilities.supportsStdio) return 'stdio'
  if (inventory.capabilities.supportsHttp) return 'http'
  return 'sse'
}

function transportDraft(draft: McpServerDraft, transport: McpTransport): Partial<McpServerDraft> {
  return transport === 'stdio'
    ? { transport, command: draft.command || '', url: null, headers: [] }
    : { transport, command: null, args: [], env: [], cwd: null, url: draft.url || '' }
}

function normalizeDraft(draft: McpServerDraft): McpServerDraft {
  const normalizeValues = (values: McpConfigValue[]) => values
    .map((item) => ({
      ...item,
      key: item.key.trim(),
      value: item.value === SECRET_PLACEHOLDER ? null : item.value,
    }))
    .filter((item) => item.key)
  return {
    ...draft,
    name: draft.name.trim(),
    command: draft.transport === 'stdio' ? draft.command?.trim() || null : null,
    args: draft.transport === 'stdio' ? draft.args.filter((value) => value.length > 0) : [],
    env: draft.transport === 'stdio' ? normalizeValues(draft.env) : [],
    cwd: draft.transport === 'stdio' ? draft.cwd?.trim() || null : null,
    url: draft.transport === 'stdio' ? null : draft.url?.trim() || null,
    headers: draft.transport === 'stdio' ? [] : normalizeValues(draft.headers),
  }
}

function legacyInventory(detail: AgentDetail): McpInventory {
  return {
    agentId: detail.id,
    configPath: detail.mcpConfigPath,
    revision: 'legacy-read-only',
    capabilities: {
      editable: false,
      supportsStdio: false,
      supportsHttp: false,
      supportsSse: false,
      supportsNativeToggle: false,
    },
    servers: detail.mcpServers.map((server) => ({
      name: server.name,
      transport: 'stdio',
      command: server.command,
      args: server.args,
      env: [],
      cwd: null,
      url: null,
      headers: [],
      enabled: true,
      disabledByAgentbro: false,
      valid: server.valid,
      message: server.message,
      warnings: [],
      configPath: detail.mcpConfigPath || '',
      editable: false,
      sourceKind: 'legacy',
    })),
  }
}

function mergeLegacyInventory(inventory: McpInventory, detail: AgentDetail): McpInventory {
  const configuredNames = new Set(inventory.servers.map((server) => server.name))
  const supplemental = legacyInventory(detail).servers.filter(
    (server) => !configuredNames.has(server.name),
  )
  return supplemental.length > 0
    ? { ...inventory, servers: [...inventory.servers, ...supplemental] }
    : inventory
}
