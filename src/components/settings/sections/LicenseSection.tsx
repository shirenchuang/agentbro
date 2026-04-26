import { useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { useLicenseStore } from '../../../stores/licenseStore'
import { SettingSection } from '../SettingSection'
import { SettingGroup } from '../SettingGroup'
import { GlassButton } from '../../shared'

const statusDotClass: Record<string, string> = {
  active: 'license-status__dot--active',
  trial: 'license-status__dot--trial',
  trial_expired: 'license-status__dot--expired',
  invalid: 'license-status__dot--expired',
}

export function LicenseSection() {
  const { t } = useTranslation()
  const license = useLicenseStore()
  const [keyInput, setKeyInput] = useState(license.licenseKey)

  useEffect(() => {
    license.checkLicense()
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const statusLabels: Record<string, string> = {
    active: t('settings.licenseActive'),
    trial: t('settings.licenseTrial'),
    trial_expired: t('settings.licenseTrialExpired'),
    invalid: t('settings.licenseInvalid'),
  }

  function activate() {
    if (!keyInput.trim()) return
    license.activateLicense(keyInput.trim())
  }

  return (
    <SettingSection title={t('settings.license')} description={t('settings.licenseDesc')}>
      <SettingGroup>
        <div className="license-status">
          <span className={`license-status__dot ${statusDotClass[license.status] ?? ''}`} />
          <div className="license-status__info">
            <div className="license-status__label">{statusLabels[license.status]}</div>
            <div className="license-status__detail">
              {license.status === 'active' && t('settings.licenseActiveDetail')}
              {license.status === 'trial' && t('settings.licenseTrialDetail', { days: license.daysRemaining })}
              {license.status === 'trial_expired' && t('settings.licenseExpiredDetail')}
              {license.status === 'invalid' && t('settings.licenseInvalidDetail')}
            </div>
          </div>
        </div>
      </SettingGroup>

      {license.status === 'trial_expired' && (
        <div className="warning-card" style={{ marginBottom: 'var(--space-lg)' }}>
          <div className="warning-card__title">{t('settings.upgradeRequired')}</div>
          <div className="warning-card__text">
            {t('settings.upgradeText')}
          </div>
        </div>
      )}

      <SettingGroup label={t('settings.licenseKey')}>
        <div className="license-input-row">
          <input
            className="glass-input"
            type="text"
            placeholder="XXXX-XXXX-XXXX-XXXX"
            value={keyInput}
            onChange={(e) => setKeyInput(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter') activate() }}
            style={{ fontFamily: 'var(--font-mono)', fontSize: 'var(--font-size-sm)', letterSpacing: '0.5px' }}
          />
          <GlassButton variant="primary" onClick={activate}>{t('settings.activate')}</GlassButton>
        </div>
      </SettingGroup>

      {license.status === 'active' && (
        <SettingGroup>
          <div style={{ padding: 'var(--space-sm) 0' }}>
            <GlassButton variant="ghost" onClick={() => license.deactivateLicense()}>
              {t('settings.deactivateLicense')}
            </GlassButton>
          </div>
        </SettingGroup>
      )}
    </SettingSection>
  )
}
