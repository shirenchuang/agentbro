import { useSkillStoreV2 } from '../../stores/skillStoreV2'
import type { SkillManagerTab } from '../../stores/skillStoreV2'
import { SkillLibraryPage } from './SkillLibraryPage'
import { SkillPackPage } from './SkillPackPage'
import { AgentManagementPage } from './AgentManagementPage'
import { DiagnosisPage } from './DiagnosisPage'
import { SettingsPageV2 } from './SettingsPageV2'
import './SkillManagerV2.css'

const TABS: Array<{ id: SkillManagerTab; label: string }> = [
  { id: 'library', label: 'Skill 库' },
  { id: 'packs', label: '技能包' },
  { id: 'agents', label: 'Agent 管理' },
  { id: 'diagnostics', label: '诊断与修复' },
  { id: 'settings', label: '设置' },
]

export function SkillManagerShell() {
  const activeTab = useSkillStoreV2((s) => s.activeTab)
  const setTab = useSkillStoreV2((s) => s.setTab)

  return (
    <div className="sm2-shell">
      <div className="sm2__tab-strip">
        {TABS.map((t) => (
          <button
            key={t.id}
            className={`sm2__tab${activeTab === t.id ? ' sm2__tab--active' : ''}`}
            onClick={() => setTab(t.id)}
          >
            {t.label}
          </button>
        ))}
      </div>
      <div className="sm2-shell__page">
        {activeTab === 'library' && <SkillLibraryPage />}
        {activeTab === 'packs' && <SkillPackPage />}
        {activeTab === 'agents' && <AgentManagementPage />}
        {activeTab === 'diagnostics' && <DiagnosisPage />}
        {activeTab === 'settings' && <SettingsPageV2 />}
      </div>
    </div>
  )
}
