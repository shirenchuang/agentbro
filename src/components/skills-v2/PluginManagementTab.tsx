import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  skillApiV2,
  type AgentDetail,
  type PluginInventory,
  type PluginStatus,
} from '../../services/skillApiV2'
import { PluginDetailSlider } from './PluginDetailSlider'

type PluginFilter = 'all' | 'enabled' | 'disabled'

export function PluginManagementTab({ detail }: { detail: AgentDetail }) {
  const { t } = useTranslation()
  const [inventory, setInventory] = useState<PluginInventory>(() => legacyInventory(detail))
  const [filter, setFilter] = useState<PluginFilter>('all')
  const [query, setQuery] = useState('')
  const [loading, setLoading] = useState(true)
  const [busyPluginId, setBusyPluginId] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [selectedPlugin, setSelectedPlugin] = useState<PluginStatus | null>(null)

  const load = async (clearDetails = false) => {
    setLoading(true)
    setError(null)
    if (clearDetails) {
      setSelectedPlugin(null)
    }
    try {
      setInventory(await skillApiV2.listPluginInventory(detail.id))
    } catch (nextError) {
      setError(String(nextError))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    setInventory(legacyInventory(detail))
    setFilter('all')
    setQuery('')
    setBusyPluginId(null)
    setNotice(null)
    setError(null)
    setSelectedPlugin(null)
    void load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [detail.id])

  useEffect(() => {
    if (!notice) return
    const timer = window.setTimeout(() => setNotice(null), 3200)
    return () => window.clearTimeout(timer)
  }, [notice])

  const enabledCount = inventory.plugins.filter((plugin) => plugin.enabled).length
  const disabledCount = inventory.plugins.length - enabledCount
  const normalizedQuery = query.trim().toLowerCase()
  const visiblePlugins = useMemo(
    () => inventory.plugins.filter((plugin) => {
      if (filter === 'enabled' && !plugin.enabled) return false
      if (filter === 'disabled' && plugin.enabled) return false
      if (!normalizedQuery) return true
      return [plugin.name, plugin.id, plugin.source, plugin.version]
        .filter(Boolean)
        .join(' ')
        .toLowerCase()
        .includes(normalizedQuery)
    }),
    [filter, inventory.plugins, normalizedQuery],
  )

  const toggle = async (plugin: PluginStatus) => {
    setBusyPluginId(plugin.id)
    setError(null)
    try {
      const next = await skillApiV2.setPluginEnabled(
        detail.id,
        plugin.id,
        inventory.revision,
        !plugin.enabled,
      )
      setInventory(next)
      setSelectedPlugin((current) => current?.id === plugin.id
        ? { ...current, enabled: !plugin.enabled }
        : current)
      setNotice(t(
        plugin.enabled ? 'skills.pluginManagement.disabled' : 'skills.pluginManagement.enabled',
        { name: plugin.name },
      ))
    } catch (nextError) {
      setError(String(nextError))
    } finally {
      setBusyPluginId(null)
    }
  }

  return (
    <div className="sm2__plugin-manager">
      {(error || notice) && (
        <div className="sm2__agent-toast-stack sm2__agent-toast-stack--local" aria-live="polite">
          {error && (
            <div className="sm2__agent-toast sm2__agent-toast--error" role="alert">
              <span>{error}</span>
              <button
                type="button"
                className="sm2__agent-toast-close"
                onClick={() => setError(null)}
                aria-label={t('skills.pluginManagement.dismissError')}
              >
                ×
              </button>
            </div>
          )}
          {notice && (
            <div className="sm2__agent-toast sm2__agent-toast--ok" role="status">
              <span>{notice}</span>
              <button
                type="button"
                className="sm2__agent-toast-close"
                onClick={() => setNotice(null)}
                aria-label={t('skills.pluginManagement.dismissNotice')}
              >
                ×
              </button>
            </div>
          )}
        </div>
      )}

      <div className="sm2__plugin-toolbar">
        <div className="sm2__plugin-stats" aria-label={t('skills.pluginManagement.summary')}>
          <PluginStat value={inventory.plugins.length} label={t('skills.pluginManagement.total')} />
          <PluginStat value={enabledCount} label={t('skills.pluginManagement.enabledCount')} tone="ok" />
          <PluginStat value={disabledCount} label={t('skills.pluginManagement.disabledCount')} />
        </div>
        <button
          className="sm2__btn"
          type="button"
          disabled={loading || Boolean(busyPluginId)}
          onClick={() => void load(true)}
        >
          {loading
            ? t('skills.pluginManagement.scanning')
            : t('skills.pluginManagement.rescan')}
        </button>
      </div>

      {!inventory.capabilities.editable && (
        <div className="sm2__notice sm2__notice--info">
          {t('skills.pluginManagement.readOnly')}
        </div>
      )}
      {inventory.capabilities.requiresNewSession && inventory.plugins.length > 0 && (
        <div className="sm2__plugin-effect-note">
          <span aria-hidden="true">↻</span>
          <div>
            <strong>{t('skills.pluginManagement.effectTitle')}</strong>
            <small>{t('skills.pluginManagement.effectDescription')}</small>
          </div>
        </div>
      )}
      {inventory.configPath && (
        <div className="sm2__plugin-config-path">
          <span>{t('skills.pluginManagement.configPath')}</span>
          <code title={inventory.configPath}>{inventory.configPath}</code>
        </div>
      )}
      {inventory.plugins.length === 0 ? (
        <div className="sm2__empty sm2__empty--compact sm2__plugin-empty">
          <strong>{t('skills.pluginManagement.emptyTitle')}</strong>
          <span>{t('skills.pluginManagement.emptyDescription')}</span>
        </div>
      ) : (
        <>
          <div className="sm2__plugin-controls">
            <label className="sm2__plugin-search">
              <span className="sm2__sr-only">{t('skills.pluginManagement.searchLabel')}</span>
              <span aria-hidden="true">⌕</span>
              <input
                value={query}
                type="search"
                placeholder={t('skills.pluginManagement.searchPlaceholder')}
                onChange={(event) => setQuery(event.target.value)}
              />
            </label>
            <div className="sm2__plugin-filters" aria-label={t('skills.pluginManagement.filterLabel')}>
              {(['all', 'enabled', 'disabled'] as PluginFilter[]).map((value) => (
                <button
                  key={value}
                  type="button"
                  className={filter === value ? 'active' : ''}
                  aria-pressed={filter === value}
                  onClick={() => setFilter(value)}
                >
                  {t(`skills.pluginManagement.filters.${value}`)}
                </button>
              ))}
            </div>
          </div>

          {visiblePlugins.length === 0 ? (
            <div className="sm2__empty sm2__empty--compact sm2__plugin-empty">
              <strong>{t('skills.pluginManagement.noResultsTitle')}</strong>
              <span>{t('skills.pluginManagement.noResultsDescription')}</span>
            </div>
          ) : (
            <section className="sm2__panel sm2__plugin-list">
              {visiblePlugins.map((plugin) => (
                <PluginRow
                  key={plugin.id}
                  plugin={plugin}
                  editable={inventory.capabilities.editable}
                  busy={busyPluginId === plugin.id}
                  disabled={Boolean(busyPluginId)}
                  onToggle={() => void toggle(plugin)}
                  onOpenDetails={() => setSelectedPlugin(plugin)}
                />
              ))}
            </section>
          )}
        </>
      )}

      <PluginDetailSlider
        agentId={detail.id}
        plugin={selectedPlugin}
        open={Boolean(selectedPlugin)}
        onClose={() => setSelectedPlugin(null)}
      />
    </div>
  )
}

function PluginStat({
  value,
  label,
  tone = 'muted',
}: {
  value: number
  label: string
  tone?: 'ok' | 'muted'
}) {
  return (
    <div className={`sm2__plugin-stat sm2__plugin-stat--${tone}`}>
      <strong>{value}</strong>
      <span>{label}</span>
    </div>
  )
}

function PluginRow({
  plugin,
  editable,
  busy,
  disabled,
  onToggle,
  onOpenDetails,
}: {
  plugin: PluginStatus
  editable: boolean
  busy: boolean
  disabled: boolean
  onToggle: () => void
  onOpenDetails: () => void
}) {
  const { t } = useTranslation()
  const marketplace = plugin.source?.replace(/^[^:]+:/, '') || t('skills.pluginManagement.local')
  return (
    <article className={`sm2__plugin-row${plugin.enabled ? ' sm2__plugin-row--enabled' : ''}`}>
      <div className="sm2__plugin-state-rail" aria-hidden="true" />
      <div className="sm2__plugin-row-summary">
        <button
          type="button"
          className="sm2__plugin-row-open"
          aria-haspopup="dialog"
          aria-label={t('skills.pluginManagement.showDetailsLabel', { name: plugin.name })}
          onClick={onOpenDetails}
        >
          <div className="sm2__plugin-glyph" aria-hidden="true">{pluginInitials(plugin.name)}</div>
          <div className="sm2__plugin-row-main">
            <div className="sm2__plugin-row-title">
              <strong>{plugin.name}</strong>
              <span className={`sm2__tag sm2__tag--${plugin.enabled ? 'ok' : 'unmanaged'}`}>
                {t(plugin.enabled
                  ? 'skills.pluginManagement.enabledTag'
                  : 'skills.pluginManagement.disabledTag')}
              </span>
            </div>
            <code>{plugin.id}</code>
            <span>
              {marketplace}
              {plugin.version ? ` · v${plugin.version}` : ''}
            </span>
          </div>
          <span className="sm2__plugin-row-chevron" aria-hidden="true">›</span>
        </button>
        <div className="sm2__plugin-row-action">
          {editable ? (
            <div className="sm2__plugin-toggle-control">
              <span>
                {t(plugin.enabled
                  ? 'skills.pluginManagement.on'
                  : 'skills.pluginManagement.off')}
              </span>
              <button
                type="button"
                className={`sm2__mcp-switch${plugin.enabled ? ' sm2__mcp-switch--on' : ''}`}
                role="switch"
                aria-checked={plugin.enabled}
                aria-label={t('skills.pluginManagement.toggleLabel', { name: plugin.name })}
                aria-busy={busy}
                disabled={disabled}
                onClick={onToggle}
              >
                <span />
              </button>
            </div>
          ) : (
            <span className="sm2__tag sm2__tag--unmanaged">
              {t('skills.pluginManagement.readOnlyTag')}
            </span>
          )}
        </div>
      </div>
    </article>
  )
}

function pluginInitials(value: string) {
  return value
    .split(/[-_\s]+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0])
    .join('')
    .toUpperCase()
    .slice(0, 2) || 'PL'
}

function legacyInventory(detail: AgentDetail): PluginInventory {
  return {
    agentId: detail.id,
    configPath: detail.configPath,
    revision: 'legacy-read-only',
    capabilities: {
      editable: false,
      requiresNewSession: true,
    },
    plugins: detail.plugins,
  }
}
