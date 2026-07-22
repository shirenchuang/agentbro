import { useTranslation } from 'react-i18next'
import { useShallow } from 'zustand/react/shallow'
import { useConfigStore } from '../../../stores/configStore'
import { SettingSection } from '../SettingSection'
import { SettingGroup } from '../SettingGroup'
import { SettingRow } from '../SettingRow'
import { Toggle } from '../Toggle'
import { Dropdown } from '../Dropdown'
import { quitApp, setLanguage, setLaunchAtLogin } from '../../../services/tauriApi'
import { GlassButton } from '../../shared'

export function GeneralSection() {
  const { t, i18n } = useTranslation()
  const { language, launchAtLogin, updateConfig } = useConfigStore(useShallow((state) => ({
    language: state.language,
    launchAtLogin: state.launchAtLogin,
    updateConfig: state.updateConfig,
  })))

  const languageOptions = [
    { value: 'en', label: 'English' },
    { value: 'zh', label: '中文' },
    { value: 'ja', label: '日本語' },
    { value: 'ko', label: '한국어' },
    { value: 'tr', label: 'Türkçe' },
  ]

  return (
    <SettingSection title={t('settings.general')} description={t('settings.generalDesc')}>
      <SettingGroup>
        <SettingRow label={t('settings.language')} description={t('settings.languageDesc')}>
          <Dropdown
            value={(() => {
              const lang = i18n.language
              if (lang.startsWith('zh')) return 'zh'
              if (lang.startsWith('ja')) return 'ja'
              if (lang.startsWith('ko')) return 'ko'
              if (lang.startsWith('tr')) return 'tr'
              return 'en'
            })()}
            options={languageOptions}
            onChange={(v) => {
              const previousLanguage = language
              const nextLanguage = v as 'en' | 'zh' | 'ja' | 'ko' | 'tr'
              i18n.changeLanguage(v)
              updateConfig('language', nextLanguage)
              setLanguage(nextLanguage).catch((error) => {
                console.error('[settings] setLanguage:', error)
                i18n.changeLanguage(previousLanguage)
                updateConfig('language', previousLanguage)
              })
            }}
            minWidth={120}
          />
        </SettingRow>
        <SettingRow label={t('settings.launchAtLogin')} description={t('settings.launchAtLoginDesc')}>
          <Toggle checked={launchAtLogin} onChange={(v) => {
            const previous = launchAtLogin
            updateConfig('launchAtLogin', v)
            setLaunchAtLogin(v).catch((error) => {
              console.error('[settings] setLaunchAtLogin:', error)
              updateConfig('launchAtLogin', previous)
            })
          }} />
        </SettingRow>
        <SettingRow label={t('settings.quitApp')}>
          <GlassButton variant="danger" onClick={() => quitApp()}>
            {t('tray.quit')}
          </GlassButton>
        </SettingRow>
      </SettingGroup>
    </SettingSection>
  )
}
