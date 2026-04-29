import { useState, useEffect, useCallback } from 'react'
import { useTranslation } from 'react-i18next'
import { useConfigStore } from '../../../stores/configStore'
import type { EngineInstance } from '../../../stores/configStore'
import {
  installHooks, removeHooks, listDisplays, isTauri,
  addEngineInstance, removeEngineInstance, verifyEnginePath,
} from '../../../services/tauriApi'
import type { BackendDisplayInfo } from '../../../services/tauriApi'
import { SettingSection } from '../SettingSection'
import { SettingGroup } from '../SettingGroup'
import { SettingRow } from '../SettingRow'
import { Toggle } from '../Toggle'
import { Dropdown } from '../Dropdown'
import { Slider } from '../Slider'

export function GeneralSection() {
  const { t, i18n } = useTranslation()
  const config = useConfigStore()

  const [displays, setDisplays] = useState<BackendDisplayInfo[]>([])

  useEffect(() => {
    listDisplays().then(setDisplays)

    if (!isTauri()) return
    let unlisten: (() => void) | undefined
    import('@tauri-apps/api/event').then(({ listen }) => {
      listen<BackendDisplayInfo[]>('display-changed', (e) => {
        setDisplays(e.payload)
      }).then((fn) => { unlisten = fn })
    })
    return () => { unlisten?.() }
  }, [])

  const monitorOptions = [
    { value: 'auto', label: t('settings.autoFollowFocus') },
    ...displays.map((d) => ({
      value: d.name,
      label: d.name,
    })),
  ]

  const durationOptions = [
    { value: '3s', label: t('settings.duration3s') },
    { value: '5s', label: t('settings.duration5s') },
    { value: '10s', label: t('settings.duration10s') },
    { value: 'persistent', label: t('settings.durationPersistent') },
  ]

  const idleTimeoutOptions = [
    { value: '0', label: t('settings.idleTimeoutDisabled') },
    { value: '5', label: t('settings.idleTimeoutMinutes', { minutes: 5 }) },
    { value: '10', label: t('settings.idleTimeoutMinutes', { minutes: 10 }) },
    { value: '15', label: t('settings.idleTimeoutMinutes', { minutes: 15 }) },
    { value: '30', label: t('settings.idleTimeoutMinutes', { minutes: 30 }) },
  ]

  const notificationModeOptions = [
    { value: 'turnEnd', label: t('settings.notificationTurnEnd') },
    { value: 'every', label: t('settings.notificationEvery') },
  ]

  const languageOptions = [
    { value: 'en', label: 'English' },
    { value: 'zh', label: '中文' },
  ]

  return (
    <SettingSection title={t('settings.general')} description={t('settings.generalDesc')}>
      <SettingGroup>
        <SettingRow label={t('settings.language')} description={t('settings.languageDesc')}>
          <Dropdown
            value={i18n.language.startsWith('zh') ? 'zh' : 'en'}
            options={languageOptions}
            onChange={(v) => i18n.changeLanguage(v)}
            minWidth={120}
          />
        </SettingRow>
        <SettingRow label={t('settings.launchAtLogin')} description={t('settings.launchAtLoginDesc') + ' (Coming soon)'}>
          <Toggle checked={config.launchAtLogin} onChange={() => {}} disabled />
        </SettingRow>
        <SettingRow label={t('settings.displayMonitor')} description={t('settings.displayMonitorDesc')}>
          <Dropdown
            value={config.displayMonitor}
            options={monitorOptions}
            onChange={(v) => config.updateConfig('displayMonitor', v)}
            minWidth={160}
          />
        </SettingRow>
      </SettingGroup>

      <SettingGroup label={t('settings.behavior')}>
        <SettingRow label={t('settings.hideInFullscreen')} description={t('settings.hideInFullscreenDesc')}>
          <Toggle checked={config.hideInFullscreen} onChange={(v) => config.updateConfig('hideInFullscreen', v)} />
        </SettingRow>
        <SettingRow label={t('settings.autoHide')} description={t('settings.autoHideDesc')}>
          <Toggle checked={config.autoHideNoSessions} onChange={(v) => config.updateConfig('autoHideNoSessions', v)} />
        </SettingRow>
        <SettingRow label={t('settings.smartSuppression')} description={t('settings.smartSuppressionDesc')}>
          <Toggle checked={config.smartSuppression} onChange={(v) => config.updateConfig('smartSuppression', v)} />
        </SettingRow>
        <SettingRow label={t('settings.autoCollapse')} description={t('settings.autoCollapseDesc')}>
          <Toggle checked={config.autoCollapse} onChange={(v) => config.updateConfig('autoCollapse', v)} />
        </SettingRow>
        <SettingRow label={t('settings.completionDuration')} description={t('settings.completionDurationDesc')}>
          <Dropdown
            value={config.completionPopupDuration}
            options={durationOptions}
            onChange={(v) => config.updateConfig('completionPopupDuration', v)}
            minWidth={140}
          />
        </SettingRow>
        <SettingRow label={t('settings.hoverDelay')} description={t('settings.hoverDelayDesc')}>
          <Slider
            value={config.dwellDuration}
            min={100}
            max={1000}
            step={50}
            onChange={(v) => config.updateConfig('dwellDuration', v)}
            unit="ms"
          />
        </SettingRow>
        <SettingRow label={t('settings.idleTimeout')} description={t('settings.idleTimeoutDesc')}>
          <Dropdown
            value={String(config.idleTimeoutMinutes)}
            options={idleTimeoutOptions}
            onChange={(v) => config.updateConfig('idleTimeoutMinutes', Number(v))}
            minWidth={130}
          />
        </SettingRow>
        <SettingRow label={t('settings.notificationMode')} description={t('settings.notificationModeDesc')}>
          <Dropdown
            value={config.notificationMode}
            options={notificationModeOptions}
            onChange={(v) => config.updateConfig('notificationMode', v as 'turnEnd' | 'every')}
            minWidth={150}
          />
        </SettingRow>
      </SettingGroup>

      <SettingGroup label={t('settings.interface')}>
        <SettingRow label={t('settings.showUsageQuota')} description={t('settings.showUsageQuotaDesc')}>
          <Toggle checked={config.showUsageQuota} onChange={(v) => config.updateConfig('showUsageQuota', v)} />
        </SettingRow>
      </SettingGroup>

      <SettingGroup label={t('settings.cliHooks')}>
        {/* Default ~/.claude instance */}
        <div className="agent-hook-row">
          <span className="agent-hook-row__status agent-hook-row__status--connected" />
          <div style={{ flex: 1 }}>
            <span className="agent-hook-row__label">Claude Code</span>
            <span className="agent-hook-row__path">~/.claude</span>
          </div>
        </div>

        {/* Custom engine instances */}
        {config.engineInstances.map((inst: EngineInstance) => (
          <div key={inst.id} className="agent-hook-row">
            <span className={`agent-hook-row__status ${inst.enabled ? 'agent-hook-row__status--connected' : 'agent-hook-row__status--disconnected'}`} />
            <div style={{ flex: 1 }}>
              <span className="agent-hook-row__label">{inst.label}</span>
              <span className="agent-hook-row__path">{inst.configRoot}</span>
            </div>
            <Toggle checked={inst.enabled} onChange={(v) => {
              const updated = config.engineInstances.map((i: EngineInstance) =>
                i.id === inst.id ? { ...i, enabled: v } : i
              )
              config.updateConfig('engineInstances', updated)
            }} />
            <button
              className="agent-hook-row__remove"
              onClick={async () => {
                try {
                  await removeEngineInstance(inst.id)
                  const updated = config.engineInstances.filter((i: EngineInstance) => i.id !== inst.id)
                  config.updateConfig('engineInstances', updated)
                } catch (e) {
                  console.error('Failed to remove engine instance:', e)
                }
              }}
              title={t('settings.removeEngine')}
            >
              ×
            </button>
          </div>
        ))}

        {/* Add engine instance form */}
        <EngineInstanceAdder onAdd={(inst) => {
          config.updateConfig('engineInstances', [...config.engineInstances, inst])
        }} />

        {/* Other agent hooks */}
        {config.agentHooks.filter(h => h.agentType !== 'claude-code').map((hook) => (
          <div key={hook.agentType} className="agent-hook-row">
            <span
              className={`agent-hook-row__status ${hook.connected ? 'agent-hook-row__status--connected' : 'agent-hook-row__status--disconnected'}`}
            />
            <span className="agent-hook-row__label">{hook.label}</span>
            <Toggle checked={hook.enabled} onChange={() => {
              config.toggleAgentHook(hook.agentType)
              const willEnable = !hook.enabled
              if (willEnable) {
                installHooks(hook.agentType)
              } else {
                removeHooks(hook.agentType)
              }
            }} />
          </div>
        ))}
      </SettingGroup>
    </SettingSection>
  )
}

function EngineInstanceAdder({ onAdd }: { onAdd: (inst: EngineInstance) => void }) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const [label, setLabel] = useState('')
  const [configRoot, setConfigRoot] = useState('')
  const [pathValid, setPathValid] = useState<boolean | null>(null)
  const [checking, setChecking] = useState(false)

  const checkPath = useCallback(async (path: string) => {
    if (!path.trim()) {
      setPathValid(null)
      return
    }
    setChecking(true)
    try {
      const valid = await verifyEnginePath(path)
      setPathValid(valid)
    } catch {
      setPathValid(false)
    }
    setChecking(false)
  }, [])

  useEffect(() => {
    const timer = setTimeout(() => checkPath(configRoot), 400)
    return () => clearTimeout(timer)
  }, [configRoot, checkPath])

  if (!open) {
    return (
      <button className="engine-add-btn" onClick={() => setOpen(true)}>
        + {t('settings.addEngineBranch')}
      </button>
    )
  }

  const canSubmit = label.trim() && configRoot.trim() && pathValid !== false

  return (
    <div className="engine-add-form">
      <div className="engine-add-form__row">
        <label>{t('settings.engineLabel')}</label>
        <input
          type="text"
          className="glass-input"
          value={label}
          placeholder="e.g. engine/cc"
          onChange={(e) => setLabel(e.target.value)}
        />
      </div>
      <div className="engine-add-form__row">
        <label>{t('settings.engineConfigRoot')}</label>
        <div className="engine-add-form__path-input">
          <input
            type="text"
            className="glass-input"
            value={configRoot}
            placeholder="~/.codefuse/engine/cc"
            onChange={(e) => setConfigRoot(e.target.value)}
          />
          {checking && <span className="engine-add-form__status">...</span>}
          {!checking && pathValid === true && (
            <span className="engine-add-form__status engine-add-form__status--valid">{t('settings.enginePathValid')}</span>
          )}
          {!checking && pathValid === false && (
            <span className="engine-add-form__status engine-add-form__status--invalid">{t('settings.enginePathInvalid')}</span>
          )}
        </div>
      </div>
      <div className="engine-add-form__actions">
        <button className="engine-add-form__cancel" onClick={() => { setOpen(false); setLabel(''); setConfigRoot(''); setPathValid(null) }}>
          {t('settings.cancel')}
        </button>
        <button
          className="engine-add-form__submit"
          disabled={!canSubmit}
          onClick={async () => {
            try {
              const inst = await addEngineInstance(label.trim(), configRoot.trim())
              onAdd(inst)
              setOpen(false)
              setLabel('')
              setConfigRoot('')
              setPathValid(null)
            } catch (e) {
              console.error('Failed to add engine instance:', e)
            }
          }}
        >
          {t('settings.add')}
        </button>
      </div>
    </div>
  )
}
