import { useState, useEffect, useCallback, useRef, type KeyboardEvent as ReactKeyboardEvent } from 'react'
import { useTranslation } from 'react-i18next'
import { invoke } from '@tauri-apps/api/core'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { useConfigStore } from '../../../stores/configStore'
import type { SoundChoice, SoundRule } from '../../../stores/configStore'
import { useThemeStore, COLOR_THEMES } from '../../../stores/themeStore'
import { CUSTOM_NOTCH_HEIGHT_MAX, CUSTOM_NOTCH_HEIGHT_MIN } from '../../../utils/islandLayout'
import {
  formatShortcutKeyEvent,
  isRecordableShortcutEvent,
  shortcutDisplayParts,
} from '../../../utils/keyboardShortcuts'
import {
  listDisplays, isTauri,
  setSoundVolume, setSoundEnabled, setSoundPack, setProbeSessionFilter, setDisplayId, repositionNotch,
  previewIslandLayout, clearIslandLayoutPreview,
  setSoundQuietHours, setSoundEventRule, previewSound, importCustomSound as importCustomSoundFile, setCustomSounds,
  registerGlobalShortcut, setGlobalActionShortcuts, setIslandFeatureFlags, setIslandSurfaceOptions,
  setActiveBackendTheme, listRemoteHosts, addRemoteHost, removeRemoteHost, connectRemote,
  disconnectRemote, installRemoteHooks, uninstallRemoteHooks, getRemoteStatus, listSshConfigHosts,
  runHookDoctor,
  getConfig, updateConfig as updateBackendConfig, listUsageProviders, authorizeUsageProvider,
} from '../../../services/tauriApi'
import type { BackendDisplayInfo, ConnectionStatus, HookDoctorCheck, HookDoctorReport, HookEventStatus, RemoteHost, SshConfigHost, UsageProviderStatus } from '../../../services/tauriApi'
import type { IslandLayoutPreviewMode, IslandLayoutPreviewOptions } from '../../../services/tauriApi'
import { SettingSection } from '../SettingSection'
import { SettingGroup } from '../SettingGroup'
import { SettingRow } from '../SettingRow'
import { Toggle } from '../Toggle'
import { Dropdown } from '../Dropdown'
import { Slider } from '../Slider'
import { GlassButton, GlassInput } from '../../shared'
import { PlatformIcon } from '../../platform/PlatformIcon'
import { HookEventConfigDialog } from '../HookEventConfigDialog'
import type { IslandSettingsView } from '../../../types/capability'

function displayMatchesConfiguredValue(display: BackendDisplayInfo, value: string): boolean {
  return display.id === value || display.name === value || display.label === value
}

function normalizeDisplayMonitorValue(value: string, displays: BackendDisplayInfo[]): string {
  const normalizedValue = value.trim()
  if (normalizedValue === 'primary' || normalizedValue === 'auto' || !normalizedValue) return normalizedValue
  const display = displays.find((d) => displayMatchesConfiguredValue(d, normalizedValue))
  if (!display) {
    return displays.some((d) => d.isPrimary) && displays.every((d) => d.isPrimary) ? 'primary' : normalizedValue
  }
  return display.isPrimary ? 'primary' : display.id
}

type IslandFeatureFlag = 'tipsEnabled' | 'pixelCursorEnabled' | 'confettiEnabled' | 'followFocus'
const USAGE_PROVIDER_REFRESH_TIMEOUT_MS = 10_000
const ACCOUNT_USAGE_PROVIDER_ORDER = [
  'codex',
  'claude-code',
  'z-ai',
  'kimi',
  'gemini-cli',
  'copilot',
  'cursor',
  'cursor-cli',
  'deepseek',
  'opencode',
  'droid',
  'stepfun',
  'antigravity',
  'kiro',
]
const ACCOUNT_USAGE_PROVIDER_RANK = new Map(
  ACCOUNT_USAGE_PROVIDER_ORDER.map((provider, index) => [provider, index]),
)
const QUIET_ASSISTANT_PRESET = {
  interactionMode: 'minimal' as const,
  smartSuppression: true,
  autoHideNoSessions: true,
  idleCompactDwellSeconds: 8,
  noSessionsHideDelay: 10,
}
const PERSISTENT_MONITOR_PRESET = {
  interactionMode: 'persistent' as const,
  smartSuppression: true,
  autoHideNoSessions: false,
  idleCompactDwellSeconds: 8,
  noSessionsHideDelay: 10,
}

function withTimeout<T>(promise: Promise<T>, timeoutMs: number, message: string): Promise<T> {
  let timer: ReturnType<typeof window.setTimeout> | undefined
  const timeout = new Promise<T>((_, reject) => {
    timer = window.setTimeout(() => reject(new Error(message)), timeoutMs)
  })
  return Promise.race([
    promise.finally(() => {
      if (timer) window.clearTimeout(timer)
    }),
    timeout,
  ])
}

function persistIslandFeatureFlags(next: Partial<Record<IslandFeatureFlag, boolean>>) {
  const state = useConfigStore.getState()
  setIslandFeatureFlags({
    tipsEnabled: next.tipsEnabled ?? state.tipsEnabled,
    pixelCursorEnabled: next.pixelCursorEnabled ?? state.pixelCursorEnabled,
    confettiEnabled: next.confettiEnabled ?? state.confettiEnabled,
    followFocus: next.followFocus ?? state.followFocus,
  }).catch((err) => console.error('Failed to persist island feature flags:', err))
}

function persistIslandSurfaceOptions(next: Partial<{ islandSurfaceMode: 'island' | 'pet'; islandPetScale: number }>) {
  const state = useConfigStore.getState()
  setIslandSurfaceOptions({
    islandSurfaceMode: next.islandSurfaceMode ?? state.islandSurfaceMode,
    islandPetScale: next.islandPetScale ?? state.islandPetScale,
  }).catch((err) => console.error('Failed to persist island surface options:', err))
}

function persistUsageQuerySettings(next: Partial<{ usageQueryEnabled: boolean; showUsageQuota: boolean }>) {
  const state = useConfigStore.getState()
  getConfig()
    .then((backendConfig) => updateBackendConfig({
      ...backendConfig,
      usageQueryEnabled: next.usageQueryEnabled ?? state.usageQueryEnabled,
      showTokenUsage: next.showUsageQuota ?? state.showUsageQuota,
    }))
    .catch((err) => console.error('Failed to persist usage query settings:', err))
}

function SurfaceModeSegmentedControl({
  onChange,
  value,
}: {
  value: 'island' | 'pet'
  onChange: (value: 'island' | 'pet') => void
}) {
  const { t } = useTranslation()
  const options = [
    { value: 'island' as const, label: t('settings.surfaceIsland', { defaultValue: '灵动岛' }) },
    { value: 'pet' as const, label: t('settings.surfacePet', { defaultValue: '宠物' }) },
  ]

  return (
    <div className="surface-mode-segmented" role="radiogroup" aria-label={t('settings.islandSurfaceMode', { defaultValue: '展示模式' })}>
      {options.map((option) => (
        <button
          aria-checked={value === option.value}
          className={`surface-mode-segmented__option ${value === option.value ? 'surface-mode-segmented__option--active' : ''}`}
          key={option.value}
          onClick={() => onChange(option.value)}
          role="radio"
          type="button"
        >
          {option.label}
        </button>
      ))}
    </div>
  )
}

const PRIMARY_SHORTCUT_ACTIONS = new Set(['toggle-panel', 'collapse-panel', 'open-settings'])

function ShortcutRow({ action, label, keys }: { action: string; label: string; keys: string }) {
  const { t } = useTranslation()
  const [recording, setRecording] = useState(false)
  const [conflict, setConflict] = useState<string | null>(null)
  const updateShortcut = useConfigStore((s) => s.updateShortcut)
  const allShortcuts = useConfigStore((s) => s.shortcuts)

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (!recording) return
      e.preventDefault()
      e.stopPropagation()
      if (e.key === 'Escape' && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) {
        setRecording(false)
        setConflict(null)
        return
      }
      if (!isRecordableShortcutEvent(e)) return
      const formatted = formatShortcutKeyEvent(e)
      if (formatted) {
        const duplicate = allShortcuts.find((s) => s.keys === formatted && s.action !== action)
        if (duplicate) {
          setConflict(t('settings.alreadyUsedBy', { label: duplicate.label }))
          return
        }
        setConflict(null)
        updateShortcut(action, formatted)
        setRecording(false)
      }
    },
    [recording, action, updateShortcut, allShortcuts, t],
  )

  useEffect(() => {
    if (recording) {
      window.addEventListener('keydown', handleKeyDown, true)
      return () => window.removeEventListener('keydown', handleKeyDown, true)
    }
  }, [recording, handleKeyDown])

  return (
    <div className="shortcuts-row">
      <span className="shortcuts-row__action">{label}</span>
      <span className="shortcuts-row__meta">
        {recording ? (
          <span className="shortcuts-row__recording">
            {conflict ? <span className="shortcuts-row__conflict">{conflict}</span> : t('settings.pressKeys')}
          </span>
        ) : (
          <span className="shortcuts-row__keys">
            {keys.trim()
              ? shortcutDisplayParts(keys).map((k, i) => (<kbd key={i}>{k}</kbd>))
              : <span className="shortcuts-row__off">{t('settings.shortcutOff', { defaultValue: 'Off' })}</span>}
          </span>
        )}
        <span className="shortcuts-row__actions">
          {!recording && keys.trim() && (
            <button className="shortcuts-row__edit shortcuts-row__clear" onClick={() => updateShortcut(action, '')}>
              {t('settings.shortcutClear', { defaultValue: 'Clear' })}
            </button>
          )}
          <button className="shortcuts-row__edit" onClick={() => setRecording(!recording)}>
            {recording ? t('settings.cancel') : t('settings.edit')}
          </button>
        </span>
      </span>
    </div>
  )
}

// ── Hook helpers ──
interface ToolHookStatus {
  toolId?: string
  adapterId?: string
  profileId?: string
  name: string
  displayName: string
  installed: boolean
  installStatus?: 'installed' | 'not_installed' | 'needs_reinstall' | 'settings_corrupted' | 'error' | string
  configPath?: string
  configDir?: string
  status: string
  version?: string
  supportsEventSelection?: boolean
  events?: HookEventStatus[]
  enabledEventNames?: string[]
  isCustom?: boolean
  customId?: string
}

function hookToolId(tool: ToolHookStatus) {
  return tool.toolId || tool.name
}

type HookInstallStatus = 'installed' | 'not_installed' | 'needs_reinstall' | 'settings_corrupted' | 'error'

function hookInstallStatus(tool: ToolHookStatus): HookInstallStatus {
  if (
    tool.installStatus === 'installed'
    || tool.installStatus === 'not_installed'
    || tool.installStatus === 'needs_reinstall'
    || tool.installStatus === 'settings_corrupted'
    || tool.installStatus === 'error'
  ) return tool.installStatus
  return tool.installed ? 'installed' : 'not_installed'
}

function hookInstallStatusLabel(t: (key: string, options?: Record<string, unknown>) => string, status: HookInstallStatus): string {
  if (status === 'installed') return t('settings.hookInstalled')
  if (status === 'needs_reinstall') return t('settings.hookNeedsReinstall', { defaultValue: '需重新安装' })
  if (status === 'settings_corrupted') return t('settings.hookSettingsCorrupted', { defaultValue: '配置异常' })
  if (status === 'error') return t('settings.hookError', { defaultValue: '异常' })
  return t('settings.hookNotInstalled')
}

function hookDoctorSuggestion(t: (key: string, options?: Record<string, unknown>) => string, check: HookDoctorCheck): string | null {
  if (check.status === 'ok') return null
  if (check.id === 'bridge-binary') {
    return t('settings.hookDoctorSuggestionBridge', { defaultValue: 'Restart AgentBro. If it still fails, reinstall the app.' })
  }
  if (check.id === 'hook-server') {
    return t('settings.hookDoctorSuggestionServer', { defaultValue: 'Keep AgentBro running and check again. New CLI sessions connect to the current Hook service.' })
  }
  if (check.id === 'installed-hooks') {
    return t('settings.hookDoctorSuggestionInstall', { defaultValue: 'Click Install All Hooks, then restart the corresponding CLI sessions.' })
  }
  if (check.id === 'automation-permission') {
    return t('settings.hookDoctorSuggestionAutomation', { defaultValue: 'Allow AgentBro to control Terminal and System Events in macOS System Settings.' })
  }
  if (check.id.startsWith('binary-')) {
    return t('settings.hookDoctorSuggestionBinary', { defaultValue: 'Install this only if you use the corresponding terminal multiplexer or terminal; otherwise it can be ignored.' })
  }
  return t('settings.hookDoctorSuggestionGeneric', { defaultValue: 'Fix the issue from the detail above, then run diagnostics again.' })
}

// ── Webhook helpers ──
interface WebhookConfig {
  enabled: boolean
  url: string
  secret?: string
  events: string[]
}

interface SavedWebhookConfig {
  id: string
  platform: WebhookProvider
  url: string
  secret: string | null
  enabled: boolean
}

type WebhookProvider = 'dingtalk' | 'feishu'

const WEBHOOK_EVENT_OPTIONS = [
  'session_start', 'task_complete', 'error', 'waiting_approval',
]

function WebhookProviderSection({
  provider, labelKey, descKey, urlPlaceholder, iconEmoji,
}: {
  provider: WebhookProvider; labelKey: string; descKey: string; urlPlaceholder: string; iconEmoji: string
}) {
  const { t } = useTranslation()
  const [config, setConfig] = useState<WebhookConfig>({
    enabled: false, url: '', secret: '', events: ['task_complete', 'error', 'waiting_approval'],
  })
  const [saving, setSaving] = useState(false)
  const [testResult, setTestResult] = useState<'success' | 'error' | null>(null)
  const [saveResult, setSaveResult] = useState<'success' | 'error' | null>(null)

  useEffect(() => {
    let cancelled = false

    const loadSavedConfig = async () => {
      try {
        const webhooks = await invoke<SavedWebhookConfig[]>('list_webhooks')
        if (cancelled) return
        const saved = webhooks.find((webhook) => webhook.platform === provider || webhook.id === provider)
        if (!saved) return
        setConfig((prev) => ({
          ...prev,
          enabled: saved.enabled,
          url: saved.url,
          secret: saved.secret ?? '',
        }))
      } catch (e) {
        console.error('Failed to load webhook config:', e)
      }
    }

    loadSavedConfig()
    return () => {
      cancelled = true
    }
  }, [provider])

  const save = async () => {
    setSaveResult(null)
    setSaving(true)
    try {
      await invoke('save_webhook_config', { provider, config })
      setSaveResult('success')
    } catch (e) {
      console.error('Failed to save webhook config:', e)
      setSaveResult('error')
    } finally {
      setSaving(false)
      setTimeout(() => setSaveResult(null), 3000)
    }
  }

  const test = async () => {
    setTestResult(null)
    try { await invoke('test_webhook', { provider, url: config.url, secret: config.secret }); setTestResult('success') }
    catch { setTestResult('error') }
    setTimeout(() => setTestResult(null), 3000)
  }

  const toggleEvent = (event: string) => {
    setConfig(prev => ({
      ...prev,
      events: prev.events.includes(event) ? prev.events.filter(e => e !== event) : [...prev.events, event],
    }))
  }

  return (
    <SettingGroup label={`${iconEmoji} ${t(labelKey)}`}>
      <SettingRow label={t('settings.webhookEnabled')} description={t(descKey)}>
        <Toggle checked={config.enabled} onChange={(v) => setConfig(prev => ({ ...prev, enabled: v }))} />
      </SettingRow>
      {config.enabled && (
        <>
          <SettingRow label={t('settings.webhookUrl')}>
            <GlassInput placeholder={urlPlaceholder} value={config.url}
              onChange={(e) => setConfig(prev => ({ ...prev, url: (e.target as HTMLInputElement).value }))}
              style={{ width: 260, fontSize: 12 }} />
          </SettingRow>
          <SettingRow label={t('settings.webhookSecret')}>
            <GlassInput type="password" placeholder={t('settings.webhookSecretPlaceholder')} value={config.secret ?? ''}
              onChange={(e) => setConfig(prev => ({ ...prev, secret: (e.target as HTMLInputElement).value }))}
              style={{ width: 200, fontSize: 12 }} />
          </SettingRow>
          <SettingRow label={t('settings.webhookEvents')} description={t('settings.webhookEventsDesc')}>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 4, alignItems: 'flex-end' }}>
              {WEBHOOK_EVENT_OPTIONS.map(event => (
                <label key={event} style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 12, cursor: 'pointer' }}>
                  <input type="checkbox" checked={config.events.includes(event)} onChange={() => toggleEvent(event)} />
                  <span style={{ color: 'var(--settings-text-secondary)' }}>{t(`settings.webhookEvent_${event}`, { defaultValue: event })}</span>
                </label>
              ))}
            </div>
          </SettingRow>
          <div style={{ display: 'flex', gap: 8, paddingTop: 8, justifyContent: 'flex-end' }}>
            {testResult === 'success' && <span style={{ fontSize: 12, color: 'var(--settings-status-active)', alignSelf: 'center' }}>{t('settings.webhookTestSuccess')}</span>}
            {testResult === 'error' && <span style={{ fontSize: 12, color: 'var(--settings-danger)', alignSelf: 'center' }}>{t('settings.webhookTestError')}</span>}
            {saveResult === 'success' && <span style={{ fontSize: 12, color: 'var(--settings-status-active)', alignSelf: 'center' }}>{t('settings.saved', { defaultValue: '已保存' })}</span>}
            {saveResult === 'error' && <span style={{ fontSize: 12, color: 'var(--settings-danger)', alignSelf: 'center' }}>{t('settings.saveFailed', { defaultValue: '保存失败' })}</span>}
            <GlassButton type="button" variant="ghost" onClick={test} disabled={!config.url}>{t('settings.webhookTest')}</GlassButton>
            <GlassButton type="button" variant="primary" onClick={save} disabled={saving || !config.url}>{saving ? '...' : t('settings.save')}</GlassButton>
          </div>
        </>
      )}
    </SettingGroup>
  )
}

// ═══════════════════════════════════════════════
// Main IslandSection
// ═══════════════════════════════════════════════

interface IslandSectionProps {
  activeView: IslandSettingsView
}

export function IslandSection({ activeView }: IslandSectionProps) {
  const { t } = useTranslation()

  return (
    <SettingSection className="setting-section--compact island-settings-section" title={t('settings.island.title')} description={t('settings.island.desc')}>
      {activeView === 'overview' && <OverviewTab />}
      {activeView === 'display' && <DisplayTab />}
      {activeView === 'behavior' && <BehaviorTab />}
      {activeView === 'integration' && <IntegrationTab />}
      {activeView === 'remote' && <RemoteTab />}
      {activeView === 'notify' && <SoundTab />}
      {activeView === 'keys' && <ShortcutsTab />}
      {activeView === 'advanced' && <AdvancedTab />}
    </SettingSection>
  )
}

// ── Overview Tab ──
function OverviewTab() {
  const { t } = useTranslation()
  const config = useConfigStore()
  const activeInteractionPreset = (
    config.interactionMode === QUIET_ASSISTANT_PRESET.interactionMode
    && config.smartSuppression === QUIET_ASSISTANT_PRESET.smartSuppression
    && config.autoHideNoSessions === QUIET_ASSISTANT_PRESET.autoHideNoSessions
    && config.idleCompactDwellSeconds === QUIET_ASSISTANT_PRESET.idleCompactDwellSeconds
    && config.noSessionsHideDelay === QUIET_ASSISTANT_PRESET.noSessionsHideDelay
  ) ? 'quiet' : (
    config.interactionMode === PERSISTENT_MONITOR_PRESET.interactionMode
    && config.smartSuppression === PERSISTENT_MONITOR_PRESET.smartSuppression
    && config.autoHideNoSessions === PERSISTENT_MONITOR_PRESET.autoHideNoSessions
    && config.idleCompactDwellSeconds === PERSISTENT_MONITOR_PRESET.idleCompactDwellSeconds
    && config.noSessionsHideDelay === PERSISTENT_MONITOR_PRESET.noSessionsHideDelay
  ) ? 'persistent' : 'custom'

  const applyInteractionPreset = (preset: typeof QUIET_ASSISTANT_PRESET | typeof PERSISTENT_MONITOR_PRESET) => {
    config.updateConfig('interactionMode', preset.interactionMode)
    config.updateConfig('smartSuppression', preset.smartSuppression)
    config.updateConfig('autoHideNoSessions', preset.autoHideNoSessions)
    config.updateConfig('idleCompactDwellSeconds', preset.idleCompactDwellSeconds)
    config.updateConfig('noSessionsHideDelay', preset.noSessionsHideDelay)
  }

  const resetIslandDefaults = () => {
    config.resetIslandDefaults()
    setSoundEnabled(true)
    setSoundVolume(80)
    setSoundPack('synth')
    setProbeSessionFilter(false)
    setSoundQuietHours(false, '22:00', '08:00')
    setDisplayId('auto')
      .then(() => repositionNotch('auto', 0))
      .catch((e) => console.error('Failed to reset island position:', e))
  }

  return (
    <>
      <div className="overview-showcase">
        <div className="overview-hero" aria-hidden="true">
          <div className="overview-hero__wallpaper">
            <span className="overview-hero__stripe overview-hero__stripe--blue" />
            <span className="overview-hero__stripe overview-hero__stripe--cyan" />
            <span className="overview-hero__stripe overview-hero__stripe--warm" />
            <span className="overview-hero__stripe overview-hero__stripe--gold" />
          </div>
          <div className="overview-live-pill">
            <img src="/agentbro-app-icon.png" className="overview-live-pill__icon" alt="AgentBro" />
            <span className="overview-live-pill__copy">
              <strong>AgentBro</strong>
              <span>让Agent更好用</span>
            </span>
          </div>
        </div>

        <div className="overview-mode-grid">
          <button
            aria-pressed={activeInteractionPreset === 'quiet'}
            className={`overview-mode-card ${activeInteractionPreset === 'quiet' ? 'overview-mode-card--active' : ''}`}
            type="button"
            onClick={() => applyInteractionPreset(QUIET_ASSISTANT_PRESET)}
          >
            <span className="overview-mode-card__island" />
            <strong>{t('settings.island.overview.quietAssistant', { defaultValue: 'Quiet Assistant' })}</strong>
            <span className="overview-mode-card__description">{t('settings.island.overview.quietAssistantDesc', { defaultValue: 'Hidden while agents run; appears for approvals, questions, failures, and completion notifications.' })}</span>
          </button>
          <button
            aria-pressed={activeInteractionPreset === 'persistent'}
            className={`overview-mode-card ${activeInteractionPreset === 'persistent' ? 'overview-mode-card--active' : ''}`}
            type="button"
            onClick={() => applyInteractionPreset(PERSISTENT_MONITOR_PRESET)}
          >
            <span className="overview-mode-card__island" />
            <strong>{t('settings.island.overview.persistentMonitor', { defaultValue: 'Persistent Monitor' })}</strong>
            <span className="overview-mode-card__description">{t('settings.island.overview.persistentMonitorDesc', { defaultValue: 'Keeps the island visible while agents run, then returns to a mini island when idle.' })}</span>
          </button>
          {activeInteractionPreset === 'custom' && (
            <div className="overview-mode-custom" role="status">
              <strong>{t('settings.island.overview.customPreset', { defaultValue: 'Custom visibility' })}</strong>
              <span>{t('settings.island.overview.customPresetDesc', { defaultValue: 'One or more visibility timing settings differ from the presets below.' })}</span>
            </div>
          )}
        </div>
      </div>

      <div className="overview-section-heading">
        <h3>{t('settings.island.overview.coreSwitches', { defaultValue: 'Core Switches' })}</h3>
        <p>{t('settings.island.overview.coreSwitchesDesc', { defaultValue: 'Primary controls for visibility, focus behavior, and suppression.' })}</p>
      </div>

      <SettingGroup>
        <SettingRow label={t('settings.islandResetDefaults')} description={t('settings.islandResetDefaultsDesc')}>
          <GlassButton variant="secondary" onClick={resetIslandDefaults}>
            {t('settings.reset')}
          </GlassButton>
        </SettingRow>
      </SettingGroup>

      <SettingGroup>
        <SettingRow label={t('settings.islandEnabled', { defaultValue: 'Enable Island' })} description={t('settings.islandEnabledDesc', { defaultValue: 'Show AgentBro status, approvals, questions, and completions in the floating island.' })}>
          <Toggle checked={config.islandEnabled} onChange={(v) => {
            config.updateConfig('islandEnabled', v)
            if (v) {
              applyInteractionPreset(PERSISTENT_MONITOR_PRESET)
            }
          }} />
        </SettingRow>
        <SettingRow label={t('settings.islandMonitorSubagents', { defaultValue: 'Monitor subagents' })} description={t('settings.islandMonitorSubagentsDesc', { defaultValue: 'Surface subagent activity and completion history in the island.' })}>
          <Toggle checked={config.islandMonitorSubagents} onChange={(v) => config.updateConfig('islandMonitorSubagents', v)} />
        </SettingRow>
        <SettingRow label={t('settings.tipsEnabled')} description={t('settings.tipsEnabledDesc')}>
          <Toggle checked={config.tipsEnabled} onChange={(v) => {
            config.updateConfig('tipsEnabled', v)
            persistIslandFeatureFlags({ tipsEnabled: v })
          }} />
        </SettingRow>
        <SettingRow label={t('settings.smartSuppression')} description={t('settings.smartSuppressionDesc')}>
          <Toggle checked={config.smartSuppression} onChange={(v) => config.updateConfig('smartSuppression', v)} />
        </SettingRow>
        <SettingRow label={t('settings.autoCollapse')} description={t('settings.autoCollapseDesc')}>
          <Toggle checked={config.autoCollapse} onChange={(v) => config.updateConfig('autoCollapse', v)} />
        </SettingRow>
        <SettingRow label={t('settings.autoHideNoSessions')} description={t('settings.autoHideNoSessionsDesc')}>
          <Toggle checked={config.autoHideNoSessions} onChange={(v) => config.updateConfig('autoHideNoSessions', v)} />
        </SettingRow>
      </SettingGroup>
    </>
  )
}

// ── Behavior Tab ──
function BehaviorTab() {
  const { t } = useTranslation()
  const config = useConfigStore()

  const idleTimeoutOptions = [
    { value: '0', label: t('settings.idleTimeoutDisabled') },
    { value: '1', label: t('settings.idleTimeoutMinutes', { minutes: 1 }) },
    { value: '5', label: t('settings.idleTimeoutMinutes', { minutes: 5 }) },
    { value: '10', label: t('settings.idleTimeoutMinutes', { minutes: 10 }) },
    { value: '15', label: t('settings.idleTimeoutMinutes', { minutes: 15 }) },
    { value: '30', label: t('settings.idleTimeoutMinutes', { minutes: 30 }) },
  ]

  return (
    <>
      <SettingGroup label={t('settings.island.section.expand', { defaultValue: 'Expand' })}>
        <SettingRow label={t('settings.hoverExpandDelay')} description={t('settings.hoverExpandDelayDesc')}>
          <Slider value={config.hoverExpandDelay} min={0} max={1000} step={50}
            onChange={(v) => config.updateConfig('hoverExpandDelay', v)} unit="ms" />
        </SettingRow>
        <SettingRow label={t('settings.microHoverExpandDelay')} description={t('settings.microHoverExpandDelayDesc')}>
          <Slider value={config.microHoverExpandDelay} min={0} max={1000} step={50}
            onChange={(v) => config.updateConfig('microHoverExpandDelay', v)} unit="ms" />
        </SettingRow>
        <SettingRow label={t('settings.collapseDelay')} description={t('settings.collapseDelayDesc')}>
          <Slider value={config.collapseDelay} min={100} max={1000} step={50}
            onChange={(v) => config.updateConfig('collapseDelay', v)} unit="ms" />
        </SettingRow>
        <SettingRow label={t('settings.islandAnimationScale', { defaultValue: 'Animation Scale' })} description={t('settings.islandAnimationScaleDesc', { defaultValue: 'Adjust the speed of island open, close, and content motion.' })}>
          <Slider value={config.islandAnimationScale} min={0.25} max={6} step={0.25}
            onChange={(v) => config.updateConfig('islandAnimationScale', v)} unit="x" />
        </SettingRow>
      </SettingGroup>

      <SettingGroup label={t('settings.island.section.hide', { defaultValue: 'Hide and Collapse' })}>
        <SettingRow label={t('settings.autoHideNoSessions')} description={t('settings.autoHideNoSessionsDesc')}>
          <Toggle checked={config.autoHideNoSessions} onChange={(v) => config.updateConfig('autoHideNoSessions', v)} />
        </SettingRow>
        <SettingRow label={t('settings.idleCompactDwell')} description={t('settings.idleCompactDwellDesc')}>
          <Slider value={config.idleCompactDwellSeconds} min={0} max={60} step={1}
            onChange={(v) => config.updateConfig('idleCompactDwellSeconds', v)} unit="s" />
        </SettingRow>
        <SettingRow label={t('settings.noSessionsHideDelay')} description={t('settings.noSessionsHideDelayDesc')}>
          <Slider value={config.noSessionsHideDelay} min={1} max={30} step={1}
            onChange={(v) => config.updateConfig('noSessionsHideDelay', v)} unit="min" />
        </SettingRow>
      </SettingGroup>

      <SettingGroup label={t('settings.island.section.dwell', { defaultValue: 'Dwell Time' })}>
        <SettingRow label={t('settings.taskCompleteDwell')} description={t('settings.taskCompleteDwellDesc')}>
          <Slider value={config.taskCompleteDwellSeconds} min={1} max={30} step={1}
            onChange={(v) => config.updateConfig('taskCompleteDwellSeconds', v)} unit="s" />
        </SettingRow>
        <SettingRow label={t('settings.escSilenceDuration')} description={t('settings.escSilenceDurationDesc')}>
          <Slider value={config.escSilenceDuration} min={10} max={300} step={10}
            onChange={(v) => config.updateConfig('escSilenceDuration', v)} unit="s" />
        </SettingRow>
      </SettingGroup>

      <SettingGroup label={t('settings.island.section.sessionHandling', { defaultValue: 'Session Handling' })}>
        <SettingRow label={t('settings.clickToDetail')} description={t('settings.clickToDetailDesc')}>
          <Toggle checked={config.clickToDetail} onChange={(v) => config.updateConfig('clickToDetail', v)} />
        </SettingRow>
        <SettingRow label={t('settings.carouselInterval')} description={t('settings.carouselIntervalDesc')}>
          <Slider value={config.carouselIntervalMs} min={1000} max={10000} step={500}
            onChange={(v) => config.updateConfig('carouselIntervalMs', v)} unit="ms" />
        </SettingRow>
        <SettingRow label={t('settings.idleTimeout')} description={t('settings.idleTimeoutDesc')}>
          <Dropdown value={String(config.idleTimeoutMinutes)} options={idleTimeoutOptions}
            onChange={(v) => config.updateConfig('idleTimeoutMinutes', Number(v))} minWidth={130} />
        </SettingRow>
        <SettingRow label={t('settings.sessionTimeout')} description={t('settings.sessionTimeoutDesc')}>
          <Slider value={config.sessionTimeoutMinutes} min={1} max={120} step={1}
            onChange={(v) => config.updateConfig('sessionTimeoutMinutes', v)} unit="min" />
        </SettingRow>
      </SettingGroup>
    </>
  )
}

// ── Display Tab ──
function DisplayTab() {
  const { t, i18n } = useTranslation()
  const config = useConfigStore()
  const { themes, activeThemeName, setActiveTheme, colorTheme, setColorTheme } = useThemeStore()
  const isZh = i18n.language?.startsWith('zh')
  const [displays, setDisplays] = useState<BackendDisplayInfo[]>([])
  const previewTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const compactPillWidthValue = Math.round(config.compactPillWidth * (config.collapsedWidthScale / 100))

  useEffect(() => {
    listDisplays().then(setDisplays)
    if (!isTauri()) return
    let unlisten: (() => void) | undefined
    import('@tauri-apps/api/event').then(({ listen }) => {
      listen<BackendDisplayInfo[]>('display-changed', (e) => { setDisplays(e.payload) }).then((fn) => { unlisten = fn })
    })
    return () => { unlisten?.() }
  }, [])

  const previewLayout = useCallback((mode: IslandLayoutPreviewMode, overrides: IslandLayoutPreviewOptions = {}) => {
    const state = useConfigStore.getState()
    previewIslandLayout(mode, {
      collapsedWidthScale: state.collapsedWidthScale,
      microPillWidth: state.microPillWidth,
      compactPillWidth: state.compactPillWidth,
      panelMaxWidth: state.panelMaxWidth,
      notchHeightMode: state.notchHeightMode,
      customNotchHeight: state.customNotchHeight,
      contentFontSize: state.contentFontSize,
      completionCardHeight: state.completionCardHeight,
      maxPanelHeight: state.maxPanelHeight,
      detailPanelMaxHeight: state.detailPanelMaxHeight,
      ...overrides,
    }).catch((e) => console.error('Failed to preview island layout:', e))
    if (previewTimerRef.current) clearTimeout(previewTimerRef.current)
    previewTimerRef.current = setTimeout(() => {
      clearIslandLayoutPreview().catch(() => {})
      previewTimerRef.current = undefined
    }, 1800)
  }, [])

  useEffect(() => () => {
    if (previewTimerRef.current) clearTimeout(previewTimerRef.current)
    clearIslandLayoutPreview().catch(() => {})
  }, [])

  const themeOptions = themes.map((th) => {
    const label = th.name === 'ink-amber'
      ? isZh ? 'AgentBro 经典' : 'AgentBro Classic'
      : th.displayName
        ? th.isCodexPet ? `Codex Pet: ${th.displayName}` : th.displayName
        : th.name.charAt(0).toUpperCase() + th.name.slice(1).replace(/[-:]/g, ' ')
    return { value: th.name, label }
  })
  const fontSizeOptions = [
    { value: '11px', label: `11px - ${t('settings.fontSizeSmall', { defaultValue: 'Small' })}` },
    { value: '12px', label: `12px - ${t('settings.fontSizeCompact', { defaultValue: 'Compact' })}` },
    { value: '13px', label: `13px - ${t('settings.fontSizeDefault', { defaultValue: 'Default' })}` },
    { value: '14px', label: `14px - ${t('settings.fontSizeMedium', { defaultValue: 'Medium' })}` },
    { value: '16px', label: `16px - ${t('settings.fontSizeLarge', { defaultValue: 'Large' })}` },
  ]
  const hoverSpeedOptions = [
    { value: 'instant', label: t('settings.hoverSpeedInstant') },
    { value: 'normal', label: t('settings.hoverSpeedNormal') },
    { value: 'slow', label: t('settings.hoverSpeedSlow') },
  ]
  const maxVisibleSessionOptions = [
    { value: '3', label: '3' },
    { value: '5', label: '5' },
    { value: '8', label: '8' },
    { value: '10', label: '10' },
    { value: '0', label: t('settings.maxVisibleSessionsUnlimited') },
  ]
  const monitorOptions = [
    {
      value: 'primary',
      label: displays.find((d) => d.isPrimary)?.label
        ? `${t('settings.mainDisplay')} · ${displays.find((d) => d.isPrimary)?.label}`
        : t('settings.mainDisplay'),
    },
    { value: 'auto', label: t('settings.autoFollowFocus') },
    ...displays
      .filter((d) => !d.isPrimary)
      .map((d) => ({ value: d.id, label: d.label })),
  ]
  const displayMonitorValue = normalizeDisplayMonitorValue(config.displayMonitor, displays)

  const islandPositionLabel = config.panelHorizontalOffset === 0
    ? t('settings.islandPositionCenter', { defaultValue: 'Centered' })
    : config.panelHorizontalOffset < 0
      ? t('settings.islandPositionLeft', { defaultValue: '{{value}}px left', value: Math.abs(config.panelHorizontalOffset) })
      : t('settings.islandPositionRight', { defaultValue: '{{value}}px right', value: config.panelHorizontalOffset })

  return (
    <>
      <SettingGroup label={t('settings.island.section.surface', { defaultValue: '展示形态' })}>
        <SettingRow label={t('settings.islandSurfaceMode', { defaultValue: '展示模式' })} description={t('settings.islandSurfaceModeDesc', { defaultValue: '在灵动岛和宠物状态面板之间切换。' })}>
          <SurfaceModeSegmentedControl
            value={config.islandSurfaceMode}
            onChange={(mode) => {
              config.updateConfig('islandSurfaceMode', mode)
              config.updateConfig('islandPetWindowOrigin', null)
              persistIslandSurfaceOptions({ islandSurfaceMode: mode })
              previewLayout(mode === 'pet' ? 'expanded' : 'compact')
            }}
          />
        </SettingRow>
        <SettingRow label={t('settings.activeTheme')} description={t('settings.activeThemeDesc')}>
          <Dropdown value={activeThemeName} options={themeOptions}
            onChange={(v) => {
              setActiveTheme(v)
              setActiveBackendTheme(v).catch((e) => console.error('Failed to persist active theme:', e))
            }} minWidth={160} />
        </SettingRow>
        {config.islandSurfaceMode === 'pet' && (
          <SettingRow label={t('settings.islandPetScale', { defaultValue: '宠物大小' })} description={`${config.islandPetScale}%`}>
            <Slider
              value={config.islandPetScale}
              min={50}
              max={120}
              step={5}
              onChange={(v) => {
                config.updateConfig('islandPetScale', v)
                persistIslandSurfaceOptions({ islandPetScale: v })
                previewLayout('expanded')
              }}
              unit="%"
            />
          </SettingRow>
        )}
      </SettingGroup>

      <SettingGroup label={t('settings.colorTheme')}>
        <div className="color-theme-cards">
          {COLOR_THEMES.map((ct) => (
            <button
              key={ct.id}
              type="button"
              className={`color-theme-card ${colorTheme === ct.id ? 'color-theme-card--active' : ''}`}
              onClick={() => {
                setColorTheme(ct.id)
                previewLayout('compact')
              }}
            >
              <div className="color-theme-card__preview">
                <div className="color-theme-card__swatch" style={{ background: ct.bg }}>
                  <div className="color-theme-card__swatch-card" style={{ background: ct.card }} />
                  <div className="color-theme-card__swatch-dot" style={{ background: ct.accent }} />
                </div>
              </div>
              <div className="color-theme-card__label">{isZh ? ct.labelZh : ct.label}</div>
              <div className="color-theme-card__tag">{ct.tag}</div>
            </button>
          ))}
        </div>
      </SettingGroup>

      <SettingGroup label={t('settings.island.section.displayPlacement', { defaultValue: '显示器位置' })}>
        <SettingRow label={t('settings.displayMonitor')} description={t('settings.displayMonitorDesc')}>
          <Dropdown value={displayMonitorValue} options={monitorOptions}
            onChange={(v) => {
              config.updateConfig('displayMonitor', v)
              setDisplayId(v)
                .then(() => repositionNotch(v))
                .catch((e) => console.error('Failed to set display:', e))
            }} minWidth={180} />
        </SettingRow>
        <SettingRow label={t('settings.allowHorizontalDrag')} description={t('settings.allowHorizontalDragDesc')}>
          <Toggle checked={config.allowHorizontalDrag} onChange={(v) => config.updateConfig('allowHorizontalDrag', v)} />
        </SettingRow>
        <SettingRow label={t('settings.resetIslandPosition', { defaultValue: 'Reset Island Position' })} description={islandPositionLabel}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <button className="settings-mini-button" type="button" onClick={() => config.updateConfig('panelHorizontalOffset', 0)}>
              {t('settings.resetCenter', { defaultValue: 'Reset to Center' })}
            </button>
          </div>
        </SettingRow>
      </SettingGroup>

      <SettingGroup label={t('settings.panelSize')}>
        <SettingRow label={t('settings.maxVisibleSessions')} description={t('settings.maxVisibleSessionsDesc')}>
          <Dropdown value={String(config.maxVisibleSessions)} options={maxVisibleSessionOptions}
            onChange={(v) => { config.updateConfig('maxVisibleSessions', Number(v)); previewLayout('expanded') }} minWidth={120} />
        </SettingRow>
        <SettingRow label={t('settings.notchHeightMode')} description={t('settings.notchHeightModeDesc')}>
          <Dropdown value={config.notchHeightMode}
            options={[
              { value: 'matchNotch', label: t('settings.heightMatchNotch') },
              { value: 'matchMenuBar', label: t('settings.heightMatchMenuBar') },
              { value: 'custom', label: t('settings.heightCustom') },
            ]}
            onChange={(v) => {
              const notchHeightMode = v as 'matchNotch' | 'matchMenuBar' | 'custom'
              config.updateConfig('notchHeightMode', notchHeightMode)
              previewLayout('compact', { notchHeightMode })
            }} minWidth={160} />
        </SettingRow>
        {config.notchHeightMode === 'custom' && (
          <SettingRow label={t('settings.customNotchHeight')} description={`${config.customNotchHeight}px`}>
            <Slider value={config.customNotchHeight} min={CUSTOM_NOTCH_HEIGHT_MIN} max={CUSTOM_NOTCH_HEIGHT_MAX} step={1}
              onChange={(v) => { config.updateConfig('customNotchHeight', v); previewLayout('compact', { notchHeightMode: 'custom', customNotchHeight: v }) }} unit="px" />
          </SettingRow>
        )}
        <SettingRow label={t('settings.microPillWidth', { defaultValue: 'Micro Pill Width' })} description={`${config.microPillWidth}px`}>
          <Slider value={config.microPillWidth} min={84} max={180} step={4}
            onChange={(v) => { config.updateConfig('microPillWidth', v); previewLayout('micro', { microPillWidth: v }) }} unit="px" />
        </SettingRow>
        <SettingRow label={t('settings.compactPillWidth', { defaultValue: 'Compact Pill Width' })} description={`${compactPillWidthValue}px`}>
          <Slider value={compactPillWidthValue} min={260} max={520} step={10}
            onChange={(v) => {
              config.updateConfig('compactPillWidth', v)
              config.updateConfig('collapsedWidthScale', 100)
              previewLayout('compact', { compactPillWidth: v, collapsedWidthScale: 100 })
            }} unit="px" />
        </SettingRow>
        <SettingRow label={t('settings.panelMaxWidth', { defaultValue: 'Expanded Panel Width' })} description={`${config.panelMaxWidth}px`}>
          <Slider value={config.panelMaxWidth} min={480} max={760} step={10}
            onChange={(v) => { config.updateConfig('panelMaxWidth', v); previewLayout('expanded', { panelMaxWidth: v }) }} unit="px" />
        </SettingRow>
        <SettingRow label={t('settings.hoverSpeed')} description={t('settings.hoverSpeedDesc')}>
          <Dropdown value={config.hoverSpeed} options={hoverSpeedOptions}
            onChange={(v) => config.updateConfig('hoverSpeed', v as 'instant' | 'normal' | 'slow')} minWidth={160} />
        </SettingRow>
        <SettingRow label={t('settings.contentFontSize')}>
          <Dropdown value={config.contentFontSize} options={fontSizeOptions}
            onChange={(v) => { config.updateConfig('contentFontSize', v); previewLayout('expanded', { contentFontSize: v }) }} minWidth={160} />
        </SettingRow>
        <SettingRow label={t('settings.showToolStatus')} description={t('settings.showToolStatusDesc')}>
          <Toggle checked={config.showToolStatus} onChange={(v) => config.updateConfig('showToolStatus', v)} />
        </SettingRow>
        <SettingRow label={t('settings.completionCardHeight')} description={`${config.completionCardHeight}px`}>
          <Slider value={config.completionCardHeight} min={80} max={420} step={10}
            onChange={(v) => { config.updateConfig('completionCardHeight', v); previewLayout('completion', { completionCardHeight: v }) }} unit="px" />
        </SettingRow>
        <SettingRow label={t('settings.maxPanelHeight')} description={`${config.maxPanelHeight}px`}>
          <Slider value={config.maxPanelHeight} min={300} max={800} step={20}
            onChange={(v) => { config.updateConfig('maxPanelHeight', v); previewLayout('expanded', { maxPanelHeight: v }) }} unit="px" />
        </SettingRow>
        <SettingRow label={t('settings.detailPanelMaxHeight')} description={`${config.detailPanelMaxHeight}px`}>
          <Slider value={config.detailPanelMaxHeight} min={260} max={1200} step={20}
            onChange={(v) => { config.updateConfig('detailPanelMaxHeight', v); previewLayout('expanded', { detailPanelMaxHeight: v }) }} unit="px" />
        </SettingRow>
      </SettingGroup>

    </>
  )
}

// ── Sound Tab ──
function SoundTab() {
  const { t } = useTranslation()
  const config = useConfigStore()
  const sessionEvents = config.soundEvents.filter((e) => e.group === 'session')
  const interactionEvents = config.soundEvents.filter((e) => e.group === 'interaction')
  const systemEvents = config.soundEvents.filter((e) => e.group === 'system')
  const resolveRule = (eventId: string): SoundRule => (
    config.soundRules[eventId] ?? { enabled: config.soundEvents.find((e) => e.id === eventId)?.enabled ?? true, sound: 'default' }
  )

  const soundPackOptions = [
    { value: 'eight-bit', label: t('settings.eightBitRetro') },
    { value: 'subtle', label: t('settings.subtle') },
    { value: 'synth', label: t('settings.soundPackSynth', { defaultValue: 'Synth' }) },
    { value: 'system', label: t('settings.soundPackSystem', { defaultValue: 'System' }) },
    { value: 'none', label: t('settings.soundPackNone', { defaultValue: 'None' }) },
    { value: 'custom', label: t('settings.custom') },
  ]
  const soundChoiceOptions = [
    { value: 'default', label: t('settings.soundDefault', { defaultValue: 'Default' }) },
    { value: 'builtin:hey-bro', label: 'Hey Bro' },
    { value: 'builtin:hero', label: 'Hero' },
    { value: 'builtin:glass', label: 'Glass' },
    { value: 'builtin:ping', label: 'Ping' },
    { value: 'builtin:pop', label: 'Pop' },
    { value: 'builtin:submarine', label: 'Submarine' },
    { value: 'builtin:basso', label: 'Basso' },
    { value: 'builtin:sosumi', label: 'Sosumi' },
    { value: 'builtin:bottle', label: 'Bottle' },
    { value: 'builtin:tink', label: 'Tink' },
    { value: 'builtin:morse', label: 'Morse' },
    { value: 'builtin:funk', label: 'Funk' },
    { value: 'builtin:purr', label: 'Purr' },
    { value: 'builtin:blow', label: 'Blow' },
    { value: 'builtin:frog', label: 'Frog' },
    { value: 'synth', label: t('settings.soundPackSynth', { defaultValue: 'Synth' }) },
    { value: 'eight-bit', label: t('settings.eightBitRetro') },
    { value: 'system', label: t('settings.soundPackSystem', { defaultValue: 'System' }) },
    { value: 'off', label: t('settings.soundPackNone', { defaultValue: 'None' }) },
    ...config.customSounds.map((sound) => ({ value: `custom:${sound.id}`, label: sound.name })),
  ]
  const toggleSoundEvent = (eventId: string, enabled: boolean) => {
    const current = resolveRule(eventId)
    const next = { ...current, enabled }
    config.updateConfig('soundEvents', config.soundEvents.map((event) => (
      event.id === eventId ? { ...event, enabled } : event
    )))
    config.updateConfig('soundRules', { ...config.soundRules, [eventId]: next })
    setSoundEventRule(eventId, next.enabled, next.sound)
      .catch((e) => console.error('Failed to set sound event rule:', e))
  }
  const updateSoundChoice = (eventId: string, sound: SoundChoice) => {
    const current = resolveRule(eventId)
    const next = { ...current, sound, enabled: sound === 'off' ? false : current.enabled }
    config.updateConfig('soundEvents', config.soundEvents.map((event) => (
      event.id === eventId ? { ...event, enabled: next.enabled } : event
    )))
    config.updateConfig('soundRules', { ...config.soundRules, [eventId]: next })
    setSoundEventRule(eventId, next.enabled, next.sound)
      .catch((e) => console.error('Failed to set sound event rule:', e))
  }
  const previewSoundEvent = (eventId: string) => {
    const current = resolveRule(eventId)
    previewSound(eventId, current.sound)
      .catch((e) => console.error('Failed to preview sound:', e))
  }
  const importCustomSound = async () => {
    let selected: string | null = null
    if (isTauri()) {
      const result = await openDialog({
        multiple: false,
        filters: [{
          name: t('settings.audioFiles', { defaultValue: 'Audio Files' }),
          extensions: ['mp3', 'wav', 'ogg', 'flac'],
        }],
      })
      selected = Array.isArray(result) ? result[0] ?? null : result
    } else {
      selected = window.prompt('Audio file path')?.trim() || null
    }
    if (!selected) return
    try {
      const sound = await importCustomSoundFile(selected)
      const next = [...config.customSounds, sound]
      config.updateConfig('customSounds', next)
      setCustomSounds(next).catch((e) => console.error('Failed to set custom sounds:', e))
    } catch (e) {
      console.error('Failed to import custom sound:', e)
    }
  }
  const deleteCustomSound = (soundId: string) => {
    const nextSounds = config.customSounds.filter((sound) => sound.id !== soundId)
    const customChoice = `custom:${soundId}`
    const nextRules = Object.fromEntries(Object.entries(config.soundRules).map(([eventId, rule]) => [
      eventId,
      rule.sound === customChoice ? { ...rule, sound: 'default' as const } : rule,
    ]))
    config.updateConfig('customSounds', nextSounds)
    config.updateConfig('soundRules', nextRules)
    setCustomSounds(nextSounds).catch((e) => console.error('Failed to set custom sounds:', e))
    Object.entries(nextRules).forEach(([eventId, rule]) => {
      setSoundEventRule(eventId, rule.enabled, rule.sound).catch(() => {})
    })
  }

  const renderSoundEvent = (event: typeof config.soundEvents[number]) => {
    const rule = resolveRule(event.id)
    const eventLabel = t(`settings.soundEvents.${event.id}`, { defaultValue: event.label })
    const previewLabel = t('settings.previewSoundFor', { defaultValue: `${t('settings.previewSound')} ${eventLabel}` })
    return (
      <div key={event.id} className="sound-event-row">
        <span className="sound-event-row__label">{eventLabel}</span>
        <Dropdown value={rule.sound} options={soundChoiceOptions}
          onChange={(v) => updateSoundChoice(event.id, v as SoundChoice)} minWidth={130} />
        <button
          aria-label={previewLabel}
          className="sound-event-row__play"
          onClick={() => previewSoundEvent(event.id)}
          title={previewLabel}
          type="button"
        >
          ▶
        </button>
        <Toggle checked={rule.enabled} onChange={() => toggleSoundEvent(event.id, !rule.enabled)} disabled={!config.soundEnabled} />
      </div>
    )
  }

  return (
    <>
      <SettingGroup>
        <SettingRow label={t('settings.enableSounds')} description={t('settings.enableSoundsDesc')}>
          <Toggle checked={config.soundEnabled} onChange={(v) => { config.updateConfig('soundEnabled', v); setSoundEnabled(v) }} />
        </SettingRow>
        <SettingRow label={t('settings.volume')}>
          <Slider value={config.volume} min={0} max={100}
            onChange={(v) => { config.updateConfig('volume', v); setSoundVolume(v) }} unit="%" />
        </SettingRow>
        <SettingRow label={t('settings.soundPack')} description={t('settings.soundPackDesc')}>
          <Dropdown value={config.soundPack} options={soundPackOptions}
            onChange={(v) => {
              config.updateConfig('soundPack', v as 'eight-bit' | 'subtle' | 'synth' | 'system' | 'none' | 'custom')
              setSoundPack(v)
            }} minWidth={130} />
        </SettingRow>
      </SettingGroup>

      <SettingGroup label={t('settings.sessionEvents')}>
        {sessionEvents.map(renderSoundEvent)}
      </SettingGroup>

      <SettingGroup label={t('settings.interactionEvents')}>
        {interactionEvents.map(renderSoundEvent)}
      </SettingGroup>

      <SettingGroup label={t('settings.systemEvents')}>
        {systemEvents.map(renderSoundEvent)}
      </SettingGroup>

      <SettingGroup label={t('settings.customSounds', { defaultValue: 'Custom Sounds' })}>
        {config.customSounds.map((sound) => (
          <div key={sound.id} className="sound-event-row sound-event-row--custom">
            <span className="sound-event-row__label" title={sound.path}>{sound.name}</span>
            <button className="settings-mini-button" type="button" onClick={() => deleteCustomSound(sound.id)}>
              {t('settings.delete', { defaultValue: 'Delete' })}
            </button>
          </div>
        ))}
        <div style={{ display: 'flex', justifyContent: 'flex-end', paddingTop: 8 }}>
          <GlassButton variant="secondary" onClick={importCustomSound}>
            {t('settings.add', { defaultValue: 'Add' })}
          </GlassButton>
        </div>
      </SettingGroup>

      <SettingGroup label={t('settings.quietHours', { defaultValue: 'Quiet Hours' })}>
        <SettingRow label={t('settings.quietHoursEnabled', { defaultValue: 'Enable Quiet Hours' })}
          description={t('settings.quietHoursEnabledDesc', { defaultValue: 'Suppress sounds during specified hours' })}>
          <Toggle checked={config.quietHours.enabled}
            onChange={(v) => {
              const next = { ...config.quietHours, enabled: v }
              config.updateConfig('quietHours', next)
              setSoundQuietHours(next.enabled, next.start, next.end)
                .catch((e) => console.error('Failed to set quiet hours:', e))
            }} />
        </SettingRow>
        {config.quietHours.enabled && (
          <>
            <SettingRow label={t('settings.quietHoursStart', { defaultValue: 'Start Time' })} description={config.quietHours.start}>
              <input type="time" className="glass-input" value={config.quietHours.start}
                onChange={(e) => {
                  const next = { ...config.quietHours, start: e.target.value }
                  config.updateConfig('quietHours', next)
                  setSoundQuietHours(next.enabled, next.start, next.end)
                    .catch((err) => console.error('Failed to set quiet hours:', err))
                }} style={{ minWidth: 120 }} />
            </SettingRow>
            <SettingRow label={t('settings.quietHoursEnd', { defaultValue: 'End Time' })} description={config.quietHours.end}>
              <input type="time" className="glass-input" value={config.quietHours.end}
                onChange={(e) => {
                  const next = { ...config.quietHours, end: e.target.value }
                  config.updateConfig('quietHours', next)
                  setSoundQuietHours(next.enabled, next.start, next.end)
                    .catch((err) => console.error('Failed to set quiet hours:', err))
                }} style={{ minWidth: 120 }} />
            </SettingRow>
          </>
        )}
      </SettingGroup>

      <SettingGroup>
        <SettingRow label={t('settings.probeFilter')} description={t('settings.probeFilterDesc')}>
          <Toggle checked={config.probeSessionFilter} onChange={(v) => {
            config.updateConfig('probeSessionFilter', v)
            setProbeSessionFilter(v)
          }} />
        </SettingRow>
      </SettingGroup>
    </>
  )
}

// ── Shortcuts Tab ──
function ShortcutsTab() {
  const { t } = useTranslation()
  const shortcuts = useConfigStore((s) => s.shortcuts)
  const config = useConfigStore()
  const syncRequestRef = useRef(0)
  const [shortcutError, setShortcutError] = useState<string | null>(null)
  const [showAdvancedShortcuts, setShowAdvancedShortcuts] = useState(false)
  const primaryShortcuts = shortcuts.filter((shortcut) => PRIMARY_SHORTCUT_ACTIONS.has(shortcut.action))
  const advancedShortcuts = shortcuts.filter((shortcut) => !PRIMARY_SHORTCUT_ACTIONS.has(shortcut.action))
  const enabledAdvancedShortcutCount = advancedShortcuts.filter((shortcut) => shortcut.keys.trim()).length
  const latestShortcutDrafts = {
    globalShortcut: config.globalShortcut,
    shortcutApprove: config.shortcutApprove,
    shortcutDeny: config.shortcutDeny,
    shortcutSkip: config.shortcutSkip,
  }
  const shortcutDraftKey = [
    latestShortcutDrafts.globalShortcut,
    latestShortcutDrafts.shortcutApprove,
    latestShortcutDrafts.shortcutDeny,
    latestShortcutDrafts.shortcutSkip,
  ].join('\u0000')
  const [shortcutDraftState, setShortcutDraftState] = useState(() => ({
    key: shortcutDraftKey,
    drafts: latestShortcutDrafts,
  }))
  const shortcutDrafts = shortcutDraftState.key === shortcutDraftKey ? shortcutDraftState.drafts : latestShortcutDrafts
  const setShortcutDrafts = (
    update: typeof latestShortcutDrafts | ((drafts: typeof latestShortcutDrafts) => typeof latestShortcutDrafts),
  ) => {
    setShortcutDraftState((state) => {
      const base = state.key === shortcutDraftKey ? state.drafts : latestShortcutDrafts
      const drafts = typeof update === 'function' ? update(base) : update
      return { key: shortcutDraftKey, drafts }
    })
  }

  const formatShortcutError = (error: unknown) => {
    const message = error instanceof Error ? error.message : String(error)
    return t('settings.shortcutApplyFailed', {
      defaultValue: 'Shortcut could not be applied: {{message}}',
      message,
    })
  }

  const syncIslandGlobalShortcut = (value: string, previous: string) => {
    const requestId = ++syncRequestRef.current
    setShortcutError(null)
    registerGlobalShortcut(value).catch((error) => {
      if (requestId !== syncRequestRef.current) return
      config.updateConfig('globalShortcut', previous)
      setShortcutDrafts((drafts) => ({ ...drafts, globalShortcut: previous }))
      setShortcutError(formatShortcutError(error))
    })
  }

  const syncGlobalActions = (patch: Partial<{
    shortcutApprove: string
    shortcutApproveEnabled: boolean
    shortcutDeny: string
    shortcutDenyEnabled: boolean
    shortcutSkip: string
    shortcutSkipEnabled: boolean
  }>, rollback?: () => void) => {
    const next = {
      approve: patch.shortcutApprove ?? config.shortcutApprove,
      approveEnabled: patch.shortcutApproveEnabled ?? config.shortcutApproveEnabled,
      deny: patch.shortcutDeny ?? config.shortcutDeny,
      denyEnabled: patch.shortcutDenyEnabled ?? config.shortcutDenyEnabled,
      skip: patch.shortcutSkip ?? config.shortcutSkip,
      skipEnabled: patch.shortcutSkipEnabled ?? config.shortcutSkipEnabled,
    }
    const requestId = ++syncRequestRef.current
    setShortcutError(null)
    setGlobalActionShortcuts(next).catch((error) => {
      if (requestId !== syncRequestRef.current) return
      rollback?.()
      setShortcutError(formatShortcutError(error))
    })
  }
  const setIslandShortcut = (value: string) => {
    setShortcutDrafts((drafts) => ({ ...drafts, globalShortcut: value }))
  }
  const commitIslandShortcut = () => {
    const value = shortcutDrafts.globalShortcut.trim()
    if (value === config.globalShortcut) return
    const previous = config.globalShortcut
    config.updateConfig('globalShortcut', value)
    syncIslandGlobalShortcut(value, previous)
  }
  const setShortcut = <K extends 'shortcutApprove' | 'shortcutDeny' | 'shortcutSkip'>(key: K, value: string) => {
    setShortcutDrafts((drafts) => ({ ...drafts, [key]: value }))
  }
  const commitShortcut = <K extends 'shortcutApprove' | 'shortcutDeny' | 'shortcutSkip'>(key: K) => {
    const value = shortcutDrafts[key].trim()
    if (value === config[key]) return
    const previous = config[key]
    config.updateConfig(key, value)
    syncGlobalActions({ [key]: value }, () => {
      config.updateConfig(key, previous)
      setShortcutDrafts((drafts) => ({ ...drafts, [key]: previous }))
    })
  }
  const setShortcutEnabled = <K extends 'shortcutApproveEnabled' | 'shortcutDenyEnabled' | 'shortcutSkipEnabled'>(key: K, value: boolean) => {
    const previous = config[key]
    config.updateConfig(key, value)
    syncGlobalActions({ [key]: value }, () => config.updateConfig(key, previous))
  }
  const commitOnEnter = (event: ReactKeyboardEvent<HTMLInputElement>, commit: () => void) => {
    if (event.key !== 'Enter') return
    event.preventDefault()
    commit()
  }

  return (
    <>
      <SettingGroup label={t('settings.globalShortcuts', { defaultValue: 'Global Shortcuts' })}>
        <SettingRow
          label={t('settings.globalShortcut', { defaultValue: 'Toggle island visibility' })}
          description={t('settings.globalShortcutDesc', { defaultValue: 'Keyboard shortcut to toggle island visibility' })}
        >
          <div className="shortcut-global-control">
            <GlassInput
              className="shortcut-global-control__input"
              value={shortcutDrafts.globalShortcut}
              onChange={(e) => setIslandShortcut(e.target.value)}
              onBlur={commitIslandShortcut}
              onKeyDown={(e) => commitOnEnter(e, commitIslandShortcut)}
              placeholder="CommandOrControl+Shift+I"
            />
          </div>
        </SettingRow>
        <SettingRow
          label={t('settings.shortcutApprove', { defaultValue: 'Approve current permission' })}
          description={t('settings.shortcutApproveDesc', { defaultValue: 'Works even when the island is not focused' })}
        >
          <div className="shortcut-global-control">
            <Toggle checked={config.shortcutApproveEnabled} onChange={(v) => setShortcutEnabled('shortcutApproveEnabled', v)} />
            <GlassInput className="shortcut-global-control__input" value={shortcutDrafts.shortcutApprove} onChange={(e) => setShortcut('shortcutApprove', e.target.value)} onBlur={() => commitShortcut('shortcutApprove')} onKeyDown={(e) => commitOnEnter(e, () => commitShortcut('shortcutApprove'))} placeholder="CommandOrControl+Shift+A" />
          </div>
        </SettingRow>
        <SettingRow
          label={t('settings.shortcutDeny', { defaultValue: 'Deny current permission' })}
          description={t('settings.shortcutDenyDesc', { defaultValue: 'Sends a deny response to the oldest pending permission' })}
        >
          <div className="shortcut-global-control">
            <Toggle checked={config.shortcutDenyEnabled} onChange={(v) => setShortcutEnabled('shortcutDenyEnabled', v)} />
            <GlassInput className="shortcut-global-control__input" value={shortcutDrafts.shortcutDeny} onChange={(e) => setShortcut('shortcutDeny', e.target.value)} onBlur={() => commitShortcut('shortcutDeny')} onKeyDown={(e) => commitOnEnter(e, () => commitShortcut('shortcutDeny'))} placeholder="CommandOrControl+Shift+D" />
          </div>
        </SettingRow>
        <SettingRow
          label={t('settings.shortcutSkip', { defaultValue: 'Skip current question' })}
          description={t('settings.shortcutSkipDesc', { defaultValue: 'Selects the first answer for the oldest pending question' })}
        >
          <div className="shortcut-global-control">
            <Toggle checked={config.shortcutSkipEnabled} onChange={(v) => setShortcutEnabled('shortcutSkipEnabled', v)} />
            <GlassInput className="shortcut-global-control__input" value={shortcutDrafts.shortcutSkip} onChange={(e) => setShortcut('shortcutSkip', e.target.value)} onBlur={() => commitShortcut('shortcutSkip')} onKeyDown={(e) => commitOnEnter(e, () => commitShortcut('shortcutSkip'))} placeholder="CommandOrControl+Shift+S" />
          </div>
        </SettingRow>
        {shortcutError && <div className="shortcut-status shortcut-status--error" role="alert">{shortcutError}</div>}
      </SettingGroup>
      <SettingGroup label={t('settings.inWindowShortcuts', { defaultValue: 'In-Window Shortcuts' })}>
        <div className="shortcuts-table">
          {primaryShortcuts.map((s) => (
            <ShortcutRow
              key={s.action}
              action={s.action}
              label={t(`settings.shortcutActions.${s.action}`, { defaultValue: s.label })}
              keys={s.keys}
            />
          ))}
        </div>
        <div className="shortcuts-advanced">
          <button
            className="shortcuts-advanced__toggle"
            onClick={() => setShowAdvancedShortcuts((value) => !value)}
            type="button"
          >
            <span>{showAdvancedShortcuts
              ? t('settings.hideAdvancedShortcuts', { defaultValue: 'Hide advanced shortcuts' })
              : t('settings.showAdvancedShortcuts', { defaultValue: 'Show advanced shortcuts' })}</span>
            <span className="shortcuts-advanced__count">
              {t('settings.enabledAdvancedShortcuts', {
                defaultValue: '{{count}} enabled',
                count: enabledAdvancedShortcutCount,
              })}
            </span>
          </button>
          {showAdvancedShortcuts && (
            <div className="shortcuts-table shortcuts-table--advanced">
              {advancedShortcuts.map((s) => (
                <ShortcutRow
                  key={s.action}
                  action={s.action}
                  label={t(`settings.shortcutActions.${s.action}`, { defaultValue: s.label })}
                  keys={s.keys}
                />
              ))}
            </div>
          )}
        </div>
      </SettingGroup>
    </>
  )
}

// ── Integration Tab ──
function IntegrationTab() {
  const { t } = useTranslation()
  const config = useConfigStore()
  const [tools, setTools] = useState<ToolHookStatus[]>([])
  const [loading, setLoading] = useState(false)
  const [actionLoading, setActionLoading] = useState<Record<string, string>>({})
  const [selectedCustomProfileId, setSelectedCustomProfileId] = useState('')
  const [customInstallDir, setCustomInstallDir] = useState('')
  const [customName, setCustomName] = useState('')
  const [addingCustom, setAddingCustom] = useState(false)
  const [configuringTool, setConfiguringTool] = useState<ToolHookStatus | null>(null)
  const [bulkInstalling, setBulkInstalling] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [hookDoctorReport, setHookDoctorReport] = useState<HookDoctorReport | null>(null)
  const [hookDoctorBusy, setHookDoctorBusy] = useState(false)
  const [usageProviders, setUsageProviders] = useState<UsageProviderStatus[]>([])
  const [usageLoading, setUsageLoading] = useState(false)
  const [usageAction, setUsageAction] = useState<string | null>(null)
  const usageRequestSeq = useRef(0)

  const fetchStatus = useCallback(async () => {
    setLoading(true); setError(null); setNotice(null)
    if (!isTauri()) {
      setTools([])
      setLoading(false)
      return
    }
    try {
      const status = await invoke<ToolHookStatus[]>('get_all_hook_status')
      setTools(status)
    }
    catch (e) { setError(String(e)) }
    setLoading(false)
  }, [])

  const fetchUsageProviders = useCallback(async (options: { live?: boolean; showLoading?: boolean } = {}) => {
    const { live = true, showLoading = true } = options
    const requestSeq = ++usageRequestSeq.current
    if (!isTauri()) {
      setUsageProviders([])
      return
    }
    if (showLoading) setUsageLoading(true)
    try {
      const providers = await withTimeout(
        listUsageProviders(live),
        USAGE_PROVIDER_REFRESH_TIMEOUT_MS,
        t('settings.usageRefreshTimeout', { defaultValue: '用量查询超时，请稍后刷新。' }),
      )
      if (requestSeq === usageRequestSeq.current) {
        setUsageProviders(providers)
      }
    } catch (e) {
      if (showLoading && requestSeq === usageRequestSeq.current) {
        setError(String(e))
      }
    } finally {
      if (showLoading && requestSeq === usageRequestSeq.current) {
        setUsageLoading(false)
      }
    }
  }, [t])

  useEffect(() => {
    if (!config.islandExternalEnabled) return
    const timer = window.setTimeout(() => { fetchStatus() }, 0)
    return () => window.clearTimeout(timer)
  }, [fetchStatus, config.islandExternalEnabled])

  useEffect(() => {
    if (!config.islandExternalEnabled) return
    const timer = window.setTimeout(() => { fetchUsageProviders({ live: false, showLoading: false }) }, 0)
    return () => window.clearTimeout(timer)
  }, [fetchUsageProviders, config.islandExternalEnabled])

  const detectNow = async () => {
    if (!isTauri()) {
      setNotice(t('settings.desktopOnlyHooks', { defaultValue: 'Hook management is available in the desktop app.' }))
      return
    }
    await fetchStatus()
    await fetchUsageProviders({ live: true, showLoading: true })
    setNotice(t('settings.hookDetectDone', { defaultValue: '检测完成。' }))
  }

  const runDoctor = async () => {
    setError(null); setNotice(null)
    setHookDoctorBusy(true)
    try {
      setHookDoctorReport(await runHookDoctor())
    } catch (err) {
      setHookDoctorReport({
        generatedAt: Math.floor(Date.now() / 1000),
        checks: [{ id: 'doctor-error', label: 'Hook Doctor', status: 'error', detail: String(err) }],
      })
    } finally {
      setHookDoctorBusy(false)
    }
  }

  const setUsageQueryEnabled = (enabled: boolean) => {
    config.updateConfig('usageQueryEnabled', enabled)
    config.updateConfig('showUsageQuota', enabled)
    persistUsageQuerySettings({ usageQueryEnabled: enabled, showUsageQuota: enabled })
    window.setTimeout(() => { fetchUsageProviders({ live: false, showLoading: false }) }, 150)
  }

  const authorizeProvider = async (provider: string) => {
    setError(null); setNotice(null)
    if (!isTauri()) {
      setNotice(t('settings.desktopOnlyHooks', { defaultValue: 'Hook management is available in the desktop app.' }))
      return
    }
    setUsageAction(provider)
    try {
      await authorizeUsageProvider(provider)
      setNotice(t('settings.usageAuthStarted', { defaultValue: '已打开终端授权，完成登录后点检测刷新状态。' }))
    } catch (e) {
      setError(String(e))
    } finally {
      setUsageAction(null)
    }
  }

  const installAll = async () => {
    setError(null); setNotice(null)
    if (!isTauri()) {
      setNotice(t('settings.desktopOnlyHooks', { defaultValue: 'Hook management is available in the desktop app.' }))
      return
    }
    setBulkInstalling(true)
    try {
      const targets = visibleTools.map((tool) => hookToolId(tool))
      const errors: string[] = []
      for (const toolId of targets) {
        setToolAction(toolId, 'install')
        try {
          await invoke('install_agent_hook', { toolName: toolId })
        } catch (err) {
          errors.push(`${toolId}: ${String(err)}`)
        } finally {
          setToolAction(toolId, null)
        }
      }
      await fetchStatus()
      setNotice(errors.length > 0
        ? t('settings.hookInstallAllDoneWithErrors', { defaultValue: '部分 Hook 安装失败：{{errors}}', errors: errors.join('；') })
        : t('settings.hookInstallAllDone', { defaultValue: '全部 Hook 已安装。请重启对应 CLI 会话以加载最新配置。' }))
    } catch (e) { setError(String(e)) }
    setBulkInstalling(false)
  }

  const setToolAction = (toolId: string, action: string | null) =>
    setActionLoading(prev => { const next = { ...prev }; if (action === null) delete next[toolId]; else next[toolId] = action; return next })

  const install = async (toolId: string) => {
    setError(null); setNotice(null)
    if (!isTauri()) {
      setNotice(t('settings.desktopOnlyHooks', { defaultValue: 'Hook management is available in the desktop app.' }))
      return
    }
    setToolAction(toolId, 'install')
    try {
      await invoke('install_agent_hook', { toolName: toolId })
      await fetchStatus()
      setNotice(t('settings.hookInstallDone', { defaultValue: 'Hook installed. Restart the corresponding CLI session to load it.' }))
    } catch (e) { setError(String(e)) }
    setToolAction(toolId, null)
  }

  const uninstall = async (toolId: string) => {
    setError(null); setNotice(null)
    if (!isTauri()) {
      setNotice(t('settings.desktopOnlyHooks', { defaultValue: 'Hook management is available in the desktop app.' }))
      return
    }
    setToolAction(toolId, 'uninstall')
    try {
      await invoke('uninstall_agent_hook', { toolName: toolId })
      await fetchStatus()
      setNotice(t('settings.hookUninstallDone', { defaultValue: 'Hook uninstalled.' }))
    } catch (e) { setError(String(e)) }
    setToolAction(toolId, null)
  }

  const reinstall = async (toolId: string) => {
    setError(null); setNotice(null)
    if (!isTauri()) {
      setNotice(t('settings.desktopOnlyHooks', { defaultValue: 'Hook management is available in the desktop app.' }))
      return
    }
    setToolAction(toolId, 'reinstall')
    try {
      await invoke('install_agent_hook', { toolName: toolId })
      await fetchStatus()
      setNotice(t('settings.hookReinstallDone', { defaultValue: 'Hook reinstalled. Restart the corresponding CLI session to load it.' }))
    } catch (e) { setError(String(e)) }
    setToolAction(toolId, null)
  }

  const configureEvents = async (tool: ToolHookStatus, enabledEvents: string[]) => {
    const toolId = hookToolId(tool)
    setError(null); setNotice(null)
    if (!isTauri()) {
      setNotice(t('settings.desktopOnlyHooks', { defaultValue: 'Hook management is available in the desktop app.' }))
      return
    }
    setToolAction(toolId, 'configure')
    try {
      await invoke('configure_agent_hook_events', { toolName: toolId, enabledEvents })
      await fetchStatus()
      setConfiguringTool(null)
      setNotice(t('settings.hookConfigSaved', { defaultValue: 'Hook configuration saved. Restart the corresponding CLI session to load it.' }))
    } catch (e) { setError(String(e)) }
    setToolAction(toolId, null)
  }

  const openPath = async (path?: string) => {
    if (!path) return
    if (!isTauri()) {
      setNotice(t('settings.desktopOnlyHooks', { defaultValue: 'Hook management is available in the desktop app.' }))
      return
    }
    try { await invoke('open_system_path', { path }) }
    catch (e) { setError(String(e)) }
  }

  const addCustomHook = async () => {
    if (!selectedCustomProfileId || !customInstallDir.trim()) return
    if (!isTauri()) {
      setNotice(t('settings.desktopOnlyHooks', { defaultValue: 'Hook management is available in the desktop app.' }))
      return
    }
    setError(null); setNotice(null)
    try {
      const targetPath = await invoke<string>('install_custom_agent_hook', {
        profileId: selectedCustomProfileId,
        installDirectory: customInstallDir.trim(),
        customName: customName.trim() || null,
      })
      setSelectedCustomProfileId('')
      setCustomInstallDir('')
      setCustomName('')
      setAddingCustom(false)
      await fetchStatus()
      setNotice(t('settings.customHookInstalled', {
        defaultValue: 'Custom hook installed at {{path}}',
        path: targetPath,
      }))
    }
    catch (e) { setError(String(e)) }
  }

  const selectCustomInstallDir = async () => {
    if (!isTauri()) return
    const result = await openDialog({
      directory: true,
      multiple: false,
      title: t('settings.selectInstallDir', { defaultValue: 'Select install directory' }),
    })
    if (typeof result === 'string') setCustomInstallDir(result)
  }

  const visibleTools = config.islandExternalEnabled ? tools : []
  const accountUsageProviders = usageProviders
    .filter((provider) =>
      ACCOUNT_USAGE_PROVIDER_RANK.has(provider.provider)
      && (provider.catalogSupported || provider.implementationStatus === 'active'),
    )
    .sort((a, b) =>
      (ACCOUNT_USAGE_PROVIDER_RANK.get(a.provider) ?? Number.MAX_SAFE_INTEGER)
      - (ACCOUNT_USAGE_PROVIDER_RANK.get(b.provider) ?? Number.MAX_SAFE_INTEGER)
      || a.label.localeCompare(b.label),
    )
  const customProfileOptions = tools
    .filter((tool) => tool.adapterId || tool.name)
    .map((tool) => ({
      id: tool.adapterId || tool.name,
      label: tool.displayName || tool.name,
    }))
    .filter((tool, index, all) => all.findIndex((item) => item.id === tool.id) === index)
  const usageStatusLabel = (provider: UsageProviderStatus) => {
    if (!provider.enabled) return t('settings.disabled', { defaultValue: 'Disabled' })
    if (provider.available) return t('settings.connected', { defaultValue: 'Connected' })
    if (provider.authStatus === 'authorized') return t('settings.waitingData', { defaultValue: 'Waiting for data' })
    if (provider.authStatus === 'missing') return t('settings.needsAuth', { defaultValue: 'Needs authorization' })
    if (provider.implementationStatus === 'available') return t('settings.usageReaderAvailable', { defaultValue: '可接入' })
    if (provider.implementationStatus === 'unsupported') return t('settings.usageReaderPending', { defaultValue: '待接入' })
    return t('settings.needsAuth', { defaultValue: 'Needs authorization' })
  }

  const shouldShowUsageAuthorize = (provider: UsageProviderStatus) =>
    config.usageQueryEnabled
    && provider.canAuthorize
    && provider.authStatus !== 'authorized'
    && !provider.available

  return (
    <>
      {error && <div className="hook-error-card">{error}</div>}
      {notice && <div className="hook-notice-card">{notice}</div>}

      <SettingGroup
        actions={(
          <button className="settings-mini-button" disabled={hookDoctorBusy} onClick={runDoctor} type="button">
            {hookDoctorBusy ? t('settings.running', { defaultValue: 'Running...' }) : t('settings.runDiagnostics', { defaultValue: 'Run diagnostics' })}
          </button>
        )}
        label={t('settings.hookDoctor', { defaultValue: 'Hook Doctor' })}
      >
        <div className="hook-doctor-intro">
          {t('settings.hookDoctorInlineDesc', { defaultValue: 'Checks the AgentBro bridge, Hook service, installed Hooks, macOS automation permission, and terminal helpers.' })}
        </div>
        {hookDoctorReport && (
          <div className="hook-doctor-report">
            {hookDoctorReport.checks.map((check) => {
              const suggestion = hookDoctorSuggestion(t, check)
              return (
                <div className={`hook-doctor-check hook-doctor-check--${check.status}`} key={check.id}>
                  <strong>{check.status.toUpperCase()}</strong>
                  <div className="hook-doctor-check__body">
                    <div className="hook-doctor-check__label">{check.label}</div>
                    <div className="hook-doctor-check__detail">{check.detail}</div>
                    {suggestion && (
                      <div className="hook-doctor-check__suggestion">
                        {t('settings.hookDoctorNextStep', { defaultValue: 'Next step: ' })}{suggestion}
                      </div>
                    )}
                  </div>
                </div>
              )
            })}
          </div>
        )}
      </SettingGroup>

      <SettingGroup
        actions={config.islandExternalEnabled ? (
          <>
            <button className="settings-mini-button" disabled={loading || bulkInstalling} onClick={detectNow} type="button">
              {loading || usageLoading ? t('settings.detecting', { defaultValue: '检测中...' }) : t('settings.detectNow', { defaultValue: '一键检测' })}
            </button>
            <button className="settings-mini-button" disabled={loading || bulkInstalling} onClick={installAll} type="button">
              {bulkInstalling ? t('settings.installing', { defaultValue: '安装中...' }) : t('settings.installAllHooks', { defaultValue: '一键全部安装' })}
            </button>
          </>
        ) : null}
        label={t('settings.detectedTools')}
      >
        {!config.islandExternalEnabled && <div className="hook-empty">{t('settings.island.integration.disabled', { defaultValue: 'External tracking disabled' })}</div>}
        {config.islandExternalEnabled && visibleTools.length === 0 && !loading && <div className="hook-empty">{t('settings.noToolsDetected')}</div>}
        {config.islandExternalEnabled && loading && visibleTools.length === 0 && <div className="hook-empty">{t('settings.detectingTools')}</div>}
        {visibleTools.map((tool) => {
          const toolId = hookToolId(tool)
          const installStatus = hookInstallStatus(tool)
          const busy = actionLoading[toolId] !== undefined || bulkInstalling
          const isInstalled = installStatus === 'installed' || installStatus === 'needs_reinstall' || installStatus === 'settings_corrupted'
          const canConfigureHook = installStatus === 'installed' && tool.supportsEventSelection && tool.events && tool.events.length > 0
          return (
            <div key={toolId} className="hook-tool-row">
              <div className="hook-tool-row__icon">
                <PlatformIcon agentId={toolId} displayName={tool.displayName || tool.name} size={30} />
              </div>
              <div className="hook-tool-row__info">
                <div className="hook-tool-row__name">{tool.displayName || tool.name}</div>
                <div className="hook-tool-row__path">{tool.configPath || tool.status || toolId}</div>
              </div>
              <div className={`hook-status-badge hook-status-badge--${installStatus}`}>
                {hookInstallStatusLabel(t, installStatus)}
              </div>
              <div className="hook-tool-row__actions">
                {canConfigureHook && (
                  <GlassButton variant="ghost" onClick={() => setConfiguringTool(tool)} disabled={busy}>
                    {t('settings.configureHook', { defaultValue: '配置 Hook' })}
                  </GlassButton>
                )}
                <GlassButton variant="ghost" onClick={() => openPath(tool.configDir || tool.configPath)} disabled={busy || !tool.configPath}>
                  {t('settings.openConfigDir', { defaultValue: '打开配置目录' })}
                </GlassButton>
                {isInstalled ? (
                  <>
                    <GlassButton variant="secondary" onClick={() => reinstall(toolId)} disabled={busy}>
                      {actionLoading[toolId] === 'reinstall' ? t('settings.installing', { defaultValue: 'Installing...' }) : t('settings.reinstall', { defaultValue: 'Reinstall' })}
                    </GlassButton>
                    <GlassButton variant="secondary" onClick={() => uninstall(toolId)} disabled={busy}>
                      {actionLoading[toolId] === 'uninstall' ? t('settings.uninstalling', { defaultValue: 'Uninstalling...' }) : t('settings.uninstall', { defaultValue: 'Uninstall' })}
                    </GlassButton>
                  </>
                ) : (
                  <GlassButton variant="secondary" onClick={() => install(toolId)} disabled={busy}>
                    {actionLoading[toolId] === 'install' ? t('settings.installing', { defaultValue: 'Installing...' }) : t('settings.install', { defaultValue: 'Install' })}
                  </GlassButton>
                )}
              </div>
            </div>
          )
        })}
      </SettingGroup>

      <SettingGroup
        actions={(
          <button className="settings-mini-button" disabled={usageLoading} onClick={() => fetchUsageProviders({ live: true, showLoading: true })} type="button">
            {usageLoading ? t('settings.detecting', { defaultValue: '检测中...' }) : t('settings.refresh', { defaultValue: '刷新' })}
          </button>
        )}
        label={t('settings.accountQuota', { defaultValue: '账号配额' })}
      >
        <SettingRow
          label={t('settings.usageQueryEnabled', { defaultValue: '启用用量查询' })}
          description={t('settings.usageQueryEnabledDesc', { defaultValue: '后台读取官方账号或 CLI 的 Token 配额，用于灵动岛顶部显示。第三方 API/中转站用量后续在单独模块配置。' })}
        >
          <Toggle checked={config.usageQueryEnabled} onChange={setUsageQueryEnabled} />
        </SettingRow>
        {accountUsageProviders.length === 0 && (
          <div className="hook-empty">
            {t('settings.noAccountQuotaProviders', { defaultValue: '暂无可查询的官方账号配额。' })}
          </div>
        )}
        {accountUsageProviders.map((provider) => (
          <div className="usage-provider-row" key={provider.provider}>
            <div className="usage-provider-row__main">
              <div className="usage-provider-row__title">
                <span>{provider.label}</span>
                <strong>{usageStatusLabel(provider)}</strong>
              </div>
              <div className="usage-provider-row__detail" title={provider.authPath || provider.detail}>
                {provider.source ? `${provider.source} · ${provider.detail}` : provider.detail}
              </div>
              {provider.authPath && (
                <div className="usage-provider-row__path">{provider.authPath}</div>
              )}
            </div>
            <div className="usage-provider-row__actions">
              {provider.authPath && (
                <GlassButton variant="ghost" onClick={() => invoke('open_system_path', { path: provider.authPath })}>
                  {t('settings.openCredential', { defaultValue: '打开凭据' })}
                </GlassButton>
              )}
              {shouldShowUsageAuthorize(provider) && (
                <GlassButton
                  variant="secondary"
                  onClick={() => authorizeProvider(provider.provider)}
                  disabled={usageAction === provider.provider}
                >
                  {usageAction === provider.provider
                    ? t('settings.authorizing', { defaultValue: '授权中...' })
                    : t('settings.authorizeUsage', { defaultValue: '用量授权' })}
                </GlassButton>
              )}
            </div>
          </div>
        ))}
      </SettingGroup>

      <SettingGroup label={t('settings.customHookConfig', { defaultValue: '自定义 Hook 配置' })}>
        {!addingCustom ? (
          <button className="engine-add-btn" onClick={() => setAddingCustom(true)}>
            + {t('settings.addCustomHookConfig', { defaultValue: '添加自定义配置' })}
          </button>
        ) : (
          <div className="engine-add-form">
            <div className="engine-add-form__row">
              <label>{t('settings.customHookName', { defaultValue: '名称' })}</label>
              <GlassInput
                placeholder={t('settings.customHookNamePlaceholder', { defaultValue: '例如 CodeFuse Engine' })}
                value={customName}
                onChange={(e) => setCustomName((e.target as HTMLInputElement).value)}
                style={{ flex: 1 }}
              />
            </div>
            <div className="engine-add-form__row">
              <label>{t('settings.selectApp', { defaultValue: '选择应用' })}</label>
              <select
                className="glass-input"
                value={selectedCustomProfileId}
                onChange={(e) => setSelectedCustomProfileId(e.target.value)}
                style={{ flex: 1 }}
              >
                <option value="">{t('settings.selectPlaceholder', { defaultValue: '请选择...' })}</option>
                {customProfileOptions.map((profile) => (
                  <option key={profile.id} value={profile.id}>{profile.label}</option>
                ))}
              </select>
            </div>
            <div className="engine-add-form__row">
              <label>{t('settings.installDir', { defaultValue: '安装目录' })}</label>
              <div className="engine-add-form__path-input">
                <GlassInput
                  placeholder={t('settings.installDirPlaceholder', { defaultValue: '例如 /path/to/.claude' })}
                  value={customInstallDir}
                  onChange={(e) => setCustomInstallDir((e.target as HTMLInputElement).value)}
                  style={{ flex: 1 }}
                />
                <GlassButton variant="secondary" onClick={selectCustomInstallDir}>
                  {t('settings.selectDir', { defaultValue: '选择目录' })}
                </GlassButton>
              </div>
            </div>
            <div className="engine-add-form__actions">
              <button className="engine-add-form__cancel" onClick={() => { setAddingCustom(false); setSelectedCustomProfileId(''); setCustomInstallDir(''); setCustomName('') }}>{t('settings.cancel')}</button>
              <button className="engine-add-form__submit" disabled={!selectedCustomProfileId || !customInstallDir.trim()} onClick={addCustomHook}>{t('settings.install')}</button>
            </div>
          </div>
        )}
      </SettingGroup>
      {configuringTool && (
        <HookEventConfigDialog
          key={hookToolId(configuringTool)}
          hook={configuringTool}
          busy={actionLoading[hookToolId(configuringTool)] === 'configure'}
          onClose={() => setConfiguringTool(null)}
          onSave={(enabledEvents) => configureEvents(configuringTool, enabledEvents)}
        />
      )}
    </>
  )
}

type RemoteActionKind = 'connect' | 'disconnect' | 'installHooks' | 'uninstallHooks' | 'remove' | 'import'

// ── Remote Tab ──
function RemoteTab() {
  const { t } = useTranslation()
  const config = useConfigStore()
  const [newHostName, setNewHostName] = useState('')
  const [newHostAddr, setNewHostAddr] = useState('')
  const [remoteHosts, setRemoteHosts] = useState<RemoteHost[]>([])
  const [remoteStatuses, setRemoteStatuses] = useState<Record<string, ConnectionStatus>>({})
  const [remoteBusyAction, setRemoteBusyAction] = useState<{ id: string; action: RemoteActionKind } | null>(null)
  const [sshConfigHosts, setSshConfigHosts] = useState<SshConfigHost[]>([])
  const [sshConfigRefreshing, setSshConfigRefreshing] = useState(false)

  const refreshRemoteHosts = useCallback(async () => {
    if (!isTauri()) return
    const hosts = await listRemoteHosts()
    setRemoteHosts(hosts)
    const entries = await Promise.all(hosts.map(async (host) => {
      try {
        return [host.id, await getRemoteStatus(host.id)] as const
      } catch {
        return [host.id, { state: 'disconnected' } as ConnectionStatus] as const
      }
    }))
    setRemoteStatuses(Object.fromEntries(entries))
  }, [])

  useEffect(() => {
    refreshRemoteHosts().catch((err) => console.error('Failed to load remote hosts:', err))
  }, [refreshRemoteHosts])

  const refreshSshConfigHosts = useCallback(async () => {
    if (!isTauri()) return
    setSshConfigHosts(await listSshConfigHosts())
  }, [])

  useEffect(() => {
    refreshSshConfigHosts().catch((err) => console.error('Failed to load SSH config hosts:', err))
  }, [refreshSshConfigHosts])

  const refreshRemoteData = useCallback(async () => {
    await Promise.all([
      refreshRemoteHosts(),
      refreshSshConfigHosts(),
    ])
  }, [refreshRemoteHosts, refreshSshConfigHosts])

  useEffect(() => {
    if (!isTauri()) return
    const hasConnectingHost = Object.values(remoteStatuses).some((status) => status.state === 'connecting')
    if (!hasConnectingHost) return

    const timer = window.setInterval(() => {
      refreshRemoteHosts().catch((err) => console.error('Failed to poll remote host status:', err))
    }, 1000)
    return () => window.clearInterval(timer)
  }, [refreshRemoteHosts, remoteStatuses])

  async function refreshSshConfigImportList() {
    setSshConfigRefreshing(true)
    try {
      await refreshRemoteData()
    } catch (err) {
      console.error('Failed to refresh SSH config hosts:', err)
    } finally {
      setSshConfigRefreshing(false)
    }
  }

  function parseRemoteTarget(raw: string): { sshTarget: string; port: number | null } {
    const trimmed = raw.trim()
    const portMatch = trimmed.match(/^(.+):(\d+)$/)
    if (!portMatch) return { sshTarget: trimmed, port: null }
    return { sshTarget: portMatch[1], port: Number(portMatch[2]) }
  }

  async function addHost() {
    if (!newHostName.trim() || !newHostAddr.trim()) return
    if (!isTauri()) {
      config.addSSHHost({ id: `ssh-${Date.now()}`, name: newHostName.trim(), host: newHostAddr.trim(), enabled: true })
      setNewHostName('')
      setNewHostAddr('')
      return
    }

    const { sshTarget, port } = parseRemoteTarget(newHostAddr)
    await addRemoteHost({
      id: `remote-${Date.now()}`,
      name: newHostName.trim(),
      sshTarget,
      port,
      identityFile: null,
      authSocket: null,
      remoteSocketPath: '/tmp/agentbro-remote.sock',
      autoConnect: false,
    })
    setNewHostName('')
    setNewHostAddr('')
    await refreshRemoteData()
  }

  async function importSshConfigHost(host: SshConfigHost) {
    const hostname = host.hostname || host.name
    const sshTarget = host.user ? `${host.user}@${hostname}` : hostname
    await addRemoteHost({
      id: `remote-${Date.now()}-${host.name}`,
      name: host.name,
      sshTarget,
      port: host.port,
      identityFile: host.identityFile,
      authSocket: null,
      remoteSocketPath: '/tmp/agentbro-remote.sock',
      autoConnect: false,
    })
    await refreshRemoteData()
  }

  async function runRemoteAction(id: string, actionName: RemoteActionKind, action: () => Promise<unknown>) {
    setRemoteBusyAction({ id, action: actionName })
    try {
      await action()
      await refreshRemoteData()
    } catch (err) {
      console.error('Remote host action failed:', err)
      await refreshRemoteData().catch(() => {})
    } finally {
      setRemoteBusyAction(null)
    }
  }

  const displayedRemoteHosts = isTauri() ? remoteHosts : config.sshHosts.map((host) => ({
    id: host.id,
    name: host.name,
    sshTarget: host.host,
    port: null,
    identityFile: null,
    authSocket: null,
    remoteSocketPath: '/tmp/agentbro-remote.sock',
    autoConnect: false,
  } satisfies RemoteHost))

  function statusText(status: ConnectionStatus | undefined): string {
    if (!status) return t('settings.disconnected', { defaultValue: 'Disconnected' })
    if (status.state === 'failed') return t('settings.failed', { defaultValue: 'Failed' })
    return t(`settings.${status.state}`, { defaultValue: status.state })
  }

  function statusTone(status: ConnectionStatus | undefined): string {
    if (!status) return 'disconnected'
    return status.state
  }

  const existingRemoteNames = new Set(remoteHosts.map((remote) => remote.name))
  const seenSshConfigNames = new Set<string>()
  const importableSshConfigHosts = sshConfigHosts.filter((host) => {
    if (existingRemoteNames.has(host.name) || seenSshConfigNames.has(host.name)) return false
    seenSshConfigNames.add(host.name)
    return true
  })

  return (
    <>
      <div className="description-card">{t('settings.sshDescription')}</div>
      <div className="warning-card">
        <div className="warning-card__title">{t('settings.sshPrerequisites')}</div>
        <div className="warning-card__text">{t('settings.sshPrerequisitesText')}</div>
      </div>

      <SettingGroup label={t('settings.tcpPort')}>
        <SettingRow label={t('settings.listeningPort')} description={t('settings.listeningPortDesc')}>
          <GlassInput
            type="number"
            value={config.tcpPort}
            onChange={(e) => config.updateConfig('tcpPort', Number((e.target as HTMLInputElement).value))}
            style={{ width: 100, textAlign: 'center' }}
          />
        </SettingRow>
      </SettingGroup>

      <SettingGroup label={t('settings.remoteHosts')}>
        {displayedRemoteHosts.length === 0 && (
          <div className="ssh-empty-state">
            <div className="ssh-empty-state__title">{t('settings.noRemoteHosts')}</div>
            <div className="ssh-empty-state__text">
              {t('settings.remoteHostsEmptyHint', { defaultValue: '手动添加主机，或从本机 SSH 配置中导入。' })}
            </div>
          </div>
        )}
        {displayedRemoteHosts.map((host) => {
          const status = remoteStatuses[host.id]
          const tone = statusTone(status)
          const isConnecting = status?.state === 'connecting'
          const actionForHost = remoteBusyAction?.id === host.id ? remoteBusyAction.action : null
          const busy = actionForHost !== null || isConnecting
          const isConnected = status?.state === 'connected'
          return (
            <div key={host.id} className="ssh-host-card">
              <div className="ssh-host-card__info">
                <div className="ssh-host-card__name">{host.name}</div>
                <div className="ssh-host-card__host">
                  <span className="ssh-host-card__target">{host.sshTarget}{host.port ? `:${host.port}` : ''}</span>
                  {isTauri() && (
                    <span className={`ssh-status ssh-status--${tone}`}>
                      <span className="ssh-status__dot" aria-hidden="true" />
                      <span>{statusText(status)}</span>
                    </span>
                  )}
                  {status?.state === 'failed' && <span className="ssh-host-card__error" title={status.message}>{status.message}</span>}
                </div>
              </div>
              <div className="ssh-host-card__actions">
                {isTauri() && (
                  <>
                    <button
                      type="button"
                      className="settings-mini-button"
                      disabled={busy}
                      onClick={() => runRemoteAction(host.id, isConnected ? 'disconnect' : 'connect', () => isConnected ? disconnectRemote(host.id) : connectRemote(host.id))}
                    >
                      {isConnecting
                        ? t('settings.connecting', { defaultValue: 'Connecting...' })
                        : isConnected
                          ? t('settings.disconnect', { defaultValue: 'Disconnect' })
                          : t('settings.connect', { defaultValue: 'Connect' })}
                    </button>
                    <button
                      type="button"
                      className="settings-mini-button"
                      disabled={busy}
                      onClick={() => runRemoteAction(host.id, 'installHooks', () => installRemoteHooks(host.id))}
                    >
                      {actionForHost === 'installHooks' ? t('settings.installing', { defaultValue: 'Installing...' }) : t('settings.installHooks', { defaultValue: 'Install hooks' })}
                    </button>
                    <button
                      type="button"
                      className="settings-mini-button settings-mini-button--danger"
                      disabled={busy}
                      onClick={() => runRemoteAction(host.id, 'uninstallHooks', () => uninstallRemoteHooks(host.id))}
                    >
                      {actionForHost === 'uninstallHooks' ? t('settings.uninstalling', { defaultValue: 'Uninstalling...' }) : t('settings.uninstallHooks', { defaultValue: 'Uninstall hooks' })}
                    </button>
                  </>
                )}
                <button
                  type="button"
                  className="ssh-host-card__remove"
                  aria-label={t('settings.removeHost')}
                  disabled={busy}
                  onClick={() => {
                    if (isTauri()) {
                      runRemoteAction(host.id, 'remove', () => removeRemoteHost(host.id))
                    } else {
                      config.removeSSHHost(host.id)
                    }
                  }}
                  title={t('settings.removeHost')}
                >
                  ×
                </button>
              </div>
            </div>
          )
        })}
      </SettingGroup>

      <SettingGroup label={t('settings.addRemoteHost', { defaultValue: '添加远程主机' })}>
        <div className="ssh-add-form">
          <GlassInput
            placeholder={t('settings.name')}
            value={newHostName}
            onChange={(e) => setNewHostName((e.target as HTMLInputElement).value)}
            style={{ flex: 1 }}
          />
          <GlassInput
            placeholder="user@host"
            value={newHostAddr}
            onChange={(e) => setNewHostAddr((e.target as HTMLInputElement).value)}
            style={{ flex: 1, fontFamily: 'var(--font-mono)', fontSize: 'var(--font-size-sm)' }}
          />
          <GlassButton variant="primary" onClick={addHost} disabled={!newHostName.trim() || !newHostAddr.trim()}>
            {t('settings.add')}
          </GlassButton>
        </div>
      </SettingGroup>

      {isTauri() && (
        <SettingGroup
          label={t('settings.importFromSshConfig', { defaultValue: 'Import from ~/.ssh/config' })}
          actions={(
            <button
              type="button"
              className="settings-mini-button"
              disabled={sshConfigRefreshing}
              onClick={refreshSshConfigImportList}
            >
              {t('settings.refresh', { defaultValue: '刷新' })}
            </button>
          )}
        >
          <div className="ssh-import-list">
            {importableSshConfigHosts.length === 0 ? (
              <div className="ssh-empty-state">
                <div className="ssh-empty-state__title">
                  {t('settings.noImportableSshHosts', { defaultValue: '暂无可导入的 SSH 主机。' })}
                </div>
                <div className="ssh-empty-state__text">
                  {t('settings.refreshSshConfigHint', { defaultValue: '修改 ~/.ssh/config 后点刷新重新读取。' })}
                </div>
              </div>
            ) : (
              importableSshConfigHosts.map((host) => (
                <div key={host.name} className="ssh-host-card ssh-host-card--import">
                  <div className="ssh-host-card__info">
                    <div className="ssh-host-card__name">{host.name}</div>
                    <div className="ssh-host-card__host">
                      {host.user ? `${host.user}@` : ''}{host.hostname || host.name}{host.port ? `:${host.port}` : ''}
                    </div>
                  </div>
                  <button
                    type="button"
                    className="settings-mini-button"
                    disabled={remoteBusyAction?.id === host.name}
                    onClick={() => runRemoteAction(host.name, 'import', () => importSshConfigHost(host))}
                  >
                    {t('settings.import', { defaultValue: 'Import' })}
                  </button>
                </div>
              ))
            )}
          </div>
        </SettingGroup>
      )}
    </>
  )
}

// ── Advanced Tab ──
function AdvancedTab() {
  const { t } = useTranslation()
  const config = useConfigStore()

  return (
    <>
      <SettingGroup label={t('settings.island.section.visualSignals', { defaultValue: 'Visual Signals' })}>
        <SettingRow label={t('settings.agentActivity')} description={t('settings.agentActivityDesc')}>
          <Toggle checked={config.showAgentActivityDetails} onChange={(v) => config.updateConfig('showAgentActivityDetails', v)} />
        </SettingRow>
        <SettingRow label={t('settings.pixelCursor')} description={t('settings.pixelCursorDesc')}>
          <Toggle checked={config.pixelCursorEnabled} onChange={(v) => {
            config.updateConfig('pixelCursorEnabled', v)
            persistIslandFeatureFlags({ pixelCursorEnabled: v })
          }} />
        </SettingRow>
      </SettingGroup>

      {/* Webhooks */}
      <div className="description-card">{t('settings.webhooksInfo')}</div>

      <WebhookProviderSection provider="dingtalk" labelKey="settings.dingtalk" descKey="settings.dingtalkDesc"
        urlPlaceholder="https://oapi.dingtalk.com/robot/send?access_token=..." iconEmoji="🔔" />

      <WebhookProviderSection provider="feishu" labelKey="settings.feishu" descKey="settings.feishuDesc"
        urlPlaceholder="https://open.feishu.cn/open-apis/bot/v2/hook/..." iconEmoji="🪶" />
    </>
  )
}
