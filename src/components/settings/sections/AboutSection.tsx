import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useConfigStore } from '../../../stores/configStore'
import { quitApp, exportDiagnostics } from '../../../services/tauriApi'
import { useUpdater } from '../../../hooks/useUpdater'
import { SettingSection } from '../SettingSection'
import { SettingGroup } from '../SettingGroup'
import { SettingRow } from '../SettingRow'
import { Toggle } from '../Toggle'
import { GlassButton } from '../../shared'

export function AboutSection() {
  const { t } = useTranslation()
  const config = useConfigStore()
  const updater = useUpdater()
  const [diagStatus, setDiagStatus] = useState<'idle' | 'copying' | 'copied' | 'error'>('idle')

  const handleExportDiagnostics = async () => {
    setDiagStatus('copying')
    try {
      const json = await exportDiagnostics()
      await navigator.clipboard.writeText(json)
      setDiagStatus('copied')
      setTimeout(() => setDiagStatus('idle'), 2000)
    } catch {
      setDiagStatus('error')
      setTimeout(() => setDiagStatus('idle'), 2000)
    }
  }

  const updateDescription = (() => {
    switch (updater.status) {
      case 'checking': return 'Checking for updates...'
      case 'available': return `Update ${updater.version} available`
      case 'downloading': return `Downloading ${updater.version}...`
      case 'ready': return `Update ${updater.version} ready — restart to apply`
      case 'error': return updater.error ?? 'Update check failed'
      case 'up-to-date': return t('settings.latestVersion')
      default: return t('settings.latestVersion')
    }
  })()

  return (
    <SettingSection title={t('settings.aboutTitle')}>
      <div className="about-header">
        <div className="about-header__icon">🏝</div>
        <div className="about-header__name">Agent Island</div>
        <div className="about-header__version">Version 0.1.0-alpha</div>
      </div>

      <SettingGroup>
        <SettingRow label={t('settings.checkForUpdates')} description={updateDescription}>
          <GlassButton
            variant="secondary"
            onClick={() => updater.checkForUpdate()}
            disabled={updater.status === 'checking' || updater.status === 'downloading'}
          >
            {updater.status === 'checking' || updater.status === 'downloading' ? '...' : t('settings.checkNow')}
          </GlassButton>
        </SettingRow>
        <SettingRow label={t('settings.exportDiagnostics')} description={t('settings.exportDiagnosticsDesc')}>
          <GlassButton
            variant="secondary"
            onClick={handleExportDiagnostics}
            disabled={diagStatus === 'copying'}
          >
            {diagStatus === 'copied' ? 'Copied!' : diagStatus === 'error' ? 'Failed' : t('settings.export')}
          </GlassButton>
        </SettingRow>
      </SettingGroup>

      <SettingGroup>
        <SettingRow label={t('settings.telemetry')} description={t('settings.telemetryDesc')}>
          <Toggle checked={config.telemetryEnabled} onChange={(v) => config.updateConfig('telemetryEnabled', v)} />
        </SettingRow>
      </SettingGroup>

      <SettingGroup label={t('settings.credits')}>
        <div className="credits-list" style={{ padding: 'var(--space-sm) 0' }}>
          <div>{t('settings.builtWith')}</div>
          <div>{t('settings.designSystem')}</div>
          <div style={{ marginTop: 'var(--space-sm)', color: '#aeaeb2' }}>
            {t('settings.copyright')}
          </div>
        </div>
      </SettingGroup>

      <SettingGroup>
        <div style={{ padding: 'var(--space-sm) 0' }}>
          <GlassButton variant="danger" onClick={() => quitApp()}>
            {t('settings.quitApp')}
          </GlassButton>
        </div>
      </SettingGroup>
    </SettingSection>
  )
}
