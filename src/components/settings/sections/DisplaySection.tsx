import { useTranslation } from 'react-i18next'
import { useConfigStore } from '../../../stores/configStore'
import { useThemeStore } from '../../../stores/themeStore'
import { MODEL_PRICING } from '../../../utils/tokens'
import { SettingSection } from '../SettingSection'
import { SettingGroup } from '../SettingGroup'
import { SettingRow } from '../SettingRow'
import { Toggle } from '../Toggle'
import { Dropdown } from '../Dropdown'
import { Slider } from '../Slider'

const costModelOptions = Object.entries(MODEL_PRICING).map(([id, m]) => ({
  value: id,
  label: m.label,
}))

const fontSizeOptions = [
  { value: '11px', label: '11px — Small' },
  { value: '12px', label: '12px — Compact' },
  { value: '13px', label: '13px — Default' },
  { value: '14px', label: '14px — Medium' },
  { value: '16px', label: '16px — Large' },
]

export function DisplaySection() {
  const { t } = useTranslation()
  const config = useConfigStore()
  const { themes, activeThemeName, setActiveTheme } = useThemeStore()

  const themeOptions = themes.map((th) => ({
    value: th.name,
    label: th.name.charAt(0).toUpperCase() + th.name.slice(1).replace(/-/g, ' '),
  }))

  const tokenDisplayOptions = [
    { value: 'both', label: t('settings.tokensBoth') },
    { value: 'tokens', label: t('settings.tokensOnly') },
    { value: 'cost', label: t('settings.costOnly') },
    { value: 'hidden', label: t('settings.hidden') },
  ]

  return (
    <SettingSection title={t('settings.display')} description={t('settings.displayDesc')}>
      <SettingGroup label={t('settings.notchStyle')}>
        <div className="notch-style-cards">
          <div
            className={`notch-style-card ${config.notchStyle === 'compact' ? 'notch-style-card--active' : ''}`}
            onClick={() => config.updateConfig('notchStyle', 'compact')}
          >
            <div className="notch-style-card__preview">
              <div style={{
                width: 120, height: 28, borderRadius: 14,
                background: '#1d1d1f', border: '1px solid #3a3a3c',
              }} />
            </div>
            <div className="notch-style-card__label">{t('settings.compact')}</div>
            <div className="notch-style-card__description">{t('settings.compactDesc')}</div>
          </div>
          <div
            className={`notch-style-card ${config.notchStyle === 'detailed' ? 'notch-style-card--active' : ''}`}
            onClick={() => config.updateConfig('notchStyle', 'detailed')}
          >
            <div className="notch-style-card__preview">
              <div style={{
                width: 180, height: 36, borderRadius: 18,
                background: '#1d1d1f', border: '1px solid #3a3a3c',
              }} />
            </div>
            <div className="notch-style-card__label">{t('settings.detailed')}</div>
            <div className="notch-style-card__description">{t('settings.detailedDesc')}</div>
          </div>
        </div>
      </SettingGroup>

      <SettingGroup label={t('settings.theme', { defaultValue: 'Theme' })}>
        <SettingRow label={t('settings.activeTheme', { defaultValue: 'Active Theme' })} description={t('settings.activeThemeDesc', { defaultValue: 'Visual style for pixel indicator and colors' })}>
          <Dropdown
            value={activeThemeName}
            options={themeOptions}
            onChange={(v) => setActiveTheme(v)}
            minWidth={160}
          />
        </SettingRow>
      </SettingGroup>

      <SettingGroup label={t('settings.panelSize')}>
        <SettingRow label={t('settings.contentFontSize')}>
          <Dropdown
            value={config.contentFontSize}
            options={fontSizeOptions}
            onChange={(v) => config.updateConfig('contentFontSize', v)}
            minWidth={160}
          />
        </SettingRow>
        <SettingRow label={t('settings.completionCardHeight')} description={`${config.completionCardHeight}px`}>
          <Slider
            value={config.completionCardHeight}
            min={80}
            max={200}
            step={10}
            onChange={(v) => config.updateConfig('completionCardHeight', v)}
            unit="px"
          />
        </SettingRow>
        <SettingRow label={t('settings.maxPanelHeight')} description={`${config.maxPanelHeight}px`}>
          <Slider
            value={config.maxPanelHeight}
            min={300}
            max={800}
            step={20}
            onChange={(v) => config.updateConfig('maxPanelHeight', v)}
            unit="px"
          />
        </SettingRow>
      </SettingGroup>

      <SettingGroup label={t('settings.tokenCostDisplay')}>
        <SettingRow label={t('settings.tokenDisplayMode')} description={t('settings.tokenDisplayModeDesc')}>
          <Dropdown
            value={config.tokenDisplayMode}
            options={tokenDisplayOptions}
            onChange={(v) => config.updateConfig('tokenDisplayMode', v as 'tokens' | 'cost' | 'both' | 'hidden')}
            minWidth={150}
          />
        </SettingRow>
        <SettingRow label={t('settings.costModel')} description={t('settings.costModelDesc')}>
          <Dropdown
            value={config.costModel}
            options={costModelOptions}
            onChange={(v) => config.updateConfig('costModel', v)}
            minWidth={170}
          />
        </SettingRow>
      </SettingGroup>

      <SettingGroup>
        <SettingRow label={t('settings.agentActivity')} description={t('settings.agentActivityDesc')}>
          <Toggle checked={config.showAgentActivityDetails} onChange={(v) => config.updateConfig('showAgentActivityDetails', v)} />
        </SettingRow>
      </SettingGroup>
    </SettingSection>
  )
}
