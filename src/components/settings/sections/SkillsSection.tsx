import { useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { SettingSection } from '../SettingSection'
import { useSkillStore } from '../../../stores/skillStore'
import { SkillListView } from '../../skills/SkillListView'
import { CentralSkillListView } from '../../skills/CentralSkillListView'
import { PluginListView } from '../../skills/PluginListView'
import { PackListView } from '../../skills/PackListView'
import { DiscoverView } from '../../skills/DiscoverView'
import { MarketplaceView } from '../../skills/MarketplaceView'
import { SyncView } from '../../skills/SyncView'
import { SkillDetailSlider } from '../../skills/SkillDetailSlider'
import './SkillsSection.css'

const tabs = [
  { id: 'skills', label: '全部 Skills' },
  { id: 'central', label: '中央技能库' },
  { id: 'plugins', label: 'Plugins / MCP' },
  { id: 'packs', label: 'Profiles' },
  { id: 'discover', label: '项目发现' },
  { id: 'market', label: '市场' },
  { id: 'sync', label: '同步' },
] as const

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
          {tabs.map(tab => (
            <button
              key={tab.id}
              className={`skills-tab ${activeTab === tab.id ? 'skills-tab--active' : ''}`}
              onClick={() => setTab(tab.id)}
            >
              {tab.label}
            </button>
          ))}
        </div>

        {activeTab === 'skills' && <SkillListView />}
        {activeTab === 'central' && <CentralSkillListView />}
        {activeTab === 'plugins' && <PluginListView />}
        {activeTab === 'packs' && <PackListView />}
        {activeTab === 'discover' && <DiscoverView />}
        {activeTab === 'market' && <MarketplaceView />}
        {activeTab === 'sync' && <SyncView />}
      </div>

      <SkillDetailSlider />
    </SettingSection>
  )
}
