import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useConfigStore } from '../../../stores/configStore'
import { SettingSection } from '../SettingSection'
import { SettingGroup } from '../SettingGroup'
import { SettingRow } from '../SettingRow'
import { GlassButton, GlassInput } from '../../shared'

export function SSHRemoteSection() {
  const { t } = useTranslation()
  const config = useConfigStore()
  const [newHostName, setNewHostName] = useState('')
  const [newHostAddr, setNewHostAddr] = useState('')

  function addHost() {
    if (!newHostName.trim() || !newHostAddr.trim()) return
    config.addSSHHost({
      id: `ssh-${Date.now()}`,
      name: newHostName.trim(),
      host: newHostAddr.trim(),
      enabled: true,
    })
    setNewHostName('')
    setNewHostAddr('')
  }

  return (
    <SettingSection title={t('settings.sshRemote')} description={t('settings.sshRemoteDesc')}>
      <div className="description-card">
        {t('settings.sshDescription')}
      </div>

      <div className="warning-card">
        <div className="warning-card__title">{t('settings.sshPrerequisites')}</div>
        <div className="warning-card__text">
          {t('settings.sshPrerequisitesText')}
        </div>
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
        {config.sshHosts.length === 0 && (
          <div style={{ padding: 'var(--space-md) 0', color: '#aeaeb2', fontSize: 'var(--font-size-sm)' }}>
            {t('settings.noRemoteHosts')}
          </div>
        )}
        {config.sshHosts.map((host) => (
          <div key={host.id} className="ssh-host-card">
            <div className="ssh-host-card__info">
              <div className="ssh-host-card__name">{host.name}</div>
              <div className="ssh-host-card__host">{host.host}</div>
            </div>
            <button className="ssh-host-card__remove" onClick={() => config.removeSSHHost(host.id)} title={t('settings.removeHost')}>
              ✕
            </button>
          </div>
        ))}
        <div style={{ display: 'flex', gap: 'var(--space-sm)', paddingTop: 'var(--space-md)' }}>
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
          <GlassButton variant="primary" onClick={addHost}>{t('settings.add')}</GlassButton>
        </div>
      </SettingGroup>
    </SettingSection>
  )
}
