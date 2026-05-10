import { useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { SettingSection } from '../SettingSection'
import { useSkillStore } from '../../../stores/skillStore'
import { SkillListView } from '../../skills/SkillListView'
import { PackListView } from '../../skills/PackListView'
import { SyncView } from '../../skills/SyncView'
import { SkillDetailSlider } from '../../skills/SkillDetailSlider'
import './SkillsSection.css'

export function SkillsSection() {
  const { t } = useTranslation()
  const { activeTab, setTab, loadAll } = useSkillStore()

  useEffect(() => {
    loadAll()
  }, [loadAll])

  return (
    <SettingSection
      title={t('skills.title')}
      description={t('skills.description')}
    >
      <div className="skills-section">
        <div className="skills-tabs">
          {(['skills', 'packs', 'sync'] as const).map(tab => (
            <button
              key={tab}
              className={`skills-tab ${activeTab === tab ? 'skills-tab--active' : ''}`}
              onClick={() => setTab(tab)}
            >
              {t(`skills.tab_${tab}`)}
            </button>
          ))}
        </div>

        {activeTab === 'skills' && <SkillListView />}
        {activeTab === 'packs' && <PackListView />}
        {activeTab === 'sync' && <SyncView />}
      </div>

      <SkillDetailSlider />
    </SettingSection>
  )
}
