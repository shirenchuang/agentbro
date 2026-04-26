import { useTranslation } from 'react-i18next'
import { useConfigStore } from '../../../stores/configStore'
import { SettingSection } from '../SettingSection'
import { SettingGroup } from '../SettingGroup'
import { SettingRow } from '../SettingRow'
import { Toggle } from '../Toggle'

export function LabsSection() {
  const { t } = useTranslation()
  const config = useConfigStore()

  return (
    <SettingSection title={t('settings.labs')} description={t('settings.labsDesc')}>
      <SettingGroup>
        <SettingRow label={t('settings.betaUpdates')} description={t('settings.betaUpdatesDesc')}>
          <Toggle checked={config.betaUpdates} onChange={(v) => config.updateConfig('betaUpdates', v)} />
        </SettingRow>
      </SettingGroup>

      <SettingGroup label={t('settings.experimentalFeatures')}>
        {config.labFeatures.map((feature) => (
          <SettingRow key={feature.id} label={feature.label} description={feature.description}>
            {/* TODO: 'streaming-diff' — wire to real-time diff rendering when streaming support lands */}
            {/* TODO: 'ai-summary' — wire to session summary generation when LLM summarization is implemented */}
            <Toggle checked={feature.enabled} onChange={() => config.toggleLabFeature(feature.id)} />
          </SettingRow>
        ))}
      </SettingGroup>
    </SettingSection>
  )
}
