import { useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { useSkillStoreV2 } from '../../stores/skillStoreV2'
import { SkillLibraryPage } from './SkillLibraryPage'
import { InstallPage } from './InstallPage'
import { SkillPackPage } from './SkillPackPage'
import { ProjectManagementPage } from './ProjectManagementPage'
import { AgentManagementPage } from './AgentManagementPage'
import { DiagnosisPage } from './DiagnosisPage'
import { SettingsPageV2 } from './SettingsPageV2'
import { RuntimeEnvironmentBadge } from '../settings/RuntimeEnvironmentSwitcher'
import { useSelectedRuntimeEnvironment } from '../../hooks/useRuntimeEnvironment'
import './SkillManagerV2.css'

export function SkillManagerShell() {
  const { t } = useTranslation()
  const activeTab = useSkillStoreV2((s) => s.activeTab)
  const runtimeEnvironmentId = useSkillStoreV2((s) => s.runtimeEnvironmentId)
  const switchRuntimeEnvironment = useSkillStoreV2((s) => s.switchRuntimeEnvironment)
  const { selectedEnvironmentId } = useSelectedRuntimeEnvironment()

  useEffect(() => {
    void switchRuntimeEnvironment(selectedEnvironmentId)
  }, [selectedEnvironmentId, switchRuntimeEnvironment])

  return (
    <div className="sm2-shell__page">
      <div className="sm2-shell__runtime-toolbar">
        <RuntimeEnvironmentBadge />
      </div>
      {runtimeEnvironmentId !== selectedEnvironmentId ? (
        <div className="sm2">
          <div className="sm2__empty">{t('skills.runtimeEnvironment.loading')}</div>
        </div>
      ) : (
        <>
          {activeTab === 'library' && <SkillLibraryPage />}
          {activeTab === 'install' && <InstallPage />}
          {activeTab === 'packs' && <SkillPackPage />}
          {activeTab === 'projects' && <ProjectManagementPage />}
          {activeTab === 'agents' && <AgentManagementPage />}
          {activeTab === 'diagnostics' && <DiagnosisPage />}
          {activeTab === 'settings' && <SettingsPageV2 />}
        </>
      )}
    </div>
  )
}
