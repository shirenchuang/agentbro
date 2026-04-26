import { useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { useConfigStore } from '../../../stores/configStore'
import { installHooks, removeHooks, listDisplays, isTauri } from '../../../services/tauriApi'
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
      </SettingGroup>

      <SettingGroup label={t('settings.interface')}>
        <SettingRow label={t('settings.showUsageQuota')} description={t('settings.showUsageQuotaDesc')}>
          <Toggle checked={config.showUsageQuota} onChange={(v) => config.updateConfig('showUsageQuota', v)} />
        </SettingRow>
      </SettingGroup>

      <SettingGroup label={t('settings.cliHooks')}>
        {config.agentHooks.map((hook) => (
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
