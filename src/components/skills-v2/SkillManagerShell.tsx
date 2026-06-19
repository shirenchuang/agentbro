import { useSkillStoreV2 } from '../../stores/skillStoreV2'
import { SkillLibraryPage } from './SkillLibraryPage'
import { InstallPage } from './InstallPage'
import { SkillPackPage } from './SkillPackPage'
import { ProjectManagementPage } from './ProjectManagementPage'
import { AgentManagementPage } from './AgentManagementPage'
import { DiagnosisPage } from './DiagnosisPage'
import { SettingsPageV2 } from './SettingsPageV2'
import './SkillManagerV2.css'

export function SkillManagerShell() {
  const activeTab = useSkillStoreV2((s) => s.activeTab)

  return (
    <div className="sm2-shell__page">
      {activeTab === 'library' && <SkillLibraryPage />}
      {activeTab === 'install' && <InstallPage />}
      {activeTab === 'packs' && <SkillPackPage />}
      {activeTab === 'projects' && <ProjectManagementPage />}
      {activeTab === 'agents' && <AgentManagementPage />}
      {activeTab === 'diagnostics' && <DiagnosisPage />}
      {activeTab === 'settings' && <SettingsPageV2 />}
    </div>
  )
}
