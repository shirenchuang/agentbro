import { useEffect, useRef, useState } from 'react'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { ask } from '@tauri-apps/plugin-dialog'
import { motion, AnimatePresence } from 'framer-motion'
import { SettingsSidebar } from './SettingsSidebar'
import { UpdateDialog } from './UpdateDialog'
import { FirstRunWelcome } from './FirstRunWelcome'
import { GeneralSection } from './sections/GeneralSection'
import { IslandSection } from './sections/IslandSection'
import { AgentsSection } from './sections/AgentsSection'
import { AgentMonitorSection } from './sections/AgentMonitorSection'
import { AboutSection } from './sections/AboutSection'
import { SwitchSection } from './sections/SwitchSection'
import { SkillManagerSection } from '../skills-v2/SkillManagerSection'
import { useUpdater } from '../../hooks/useUpdater'
import { useConfigStore } from '../../stores/configStore'
import { isTauri } from '../../services/tauriApi'
import type { CapabilityView, IslandSettingsView, MonitorSettingsView } from '../../types/capability'
import '../../styles/settings.css'

const sections: Record<string, () => ReactNode> = {
  'general': GeneralSection,
  'skill-manager-v2': SkillManagerSection,
}

interface SettingsAppProps {
  onClose: () => void
}

export function SettingsApp({ onClose }: SettingsAppProps) {
  const { t } = useTranslation()
  const updater = useUpdater()
  const autoInstallUpdate = useConfigStore((s) => s.autoInstallUpdate)
  const analyticsConsentPromptCompleted = useConfigStore((s) => s.analyticsConsentPromptCompleted)
  const [activeSection, setActiveSection] = useState('general')
  const [activeCapabilityView, setActiveCapabilityView] = useState<CapabilityView>('agent')
  const [activeIslandView, setActiveIslandView] = useState<IslandSettingsView>('overview')
  const [activeMonitorView, setActiveMonitorView] = useState<MonitorSettingsView>('overview')
  const [customAgentDialogOpen, setCustomAgentDialogOpen] = useState(false)
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)
  const [updateMinimized, setUpdateMinimized] = useState(false)
  const SectionComponent = sections[activeSection] ?? GeneralSection
  const isMarketSection = activeSection === 'island' && activeIslandView === 'market'
  const contentClassName = `settings-content settings-scroll${isMarketSection ? ' settings-content--market' : ''}`
  const openCustomAgentDialog = () => {
    setActiveSection('agents')
    setActiveCapabilityView('agent')
    setCustomAgentDialogOpen(true)
  }

  // Closing the settings window destroys it, which kills an in-flight update
  // download. Guard both the in-app close button and the macOS traffic-light.
  const downloadingRef = useRef(false)
  useEffect(() => {
    downloadingRef.current = updater.status === 'downloading'
  }, [updater.status])

  const confirmCloseWhileDownloading = async (): Promise<boolean> => {
    if (!downloadingRef.current) return true
    return ask(t('update.closeWhileDownloading'), {
      title: t('update.availableTitle'),
      kind: 'warning',
      okLabel: t('update.closeAnyway'),
      cancelLabel: t('update.keepDownloading'),
    })
  }

  const handleCloseRequest = async () => {
    if (await confirmCloseWhileDownloading()) onClose()
  }

  useEffect(() => {
    if (!isTauri()) return
    const unlistenPromise = (async () => {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      return getCurrentWindow().onCloseRequested(async (event) => {
        if (!downloadingRef.current) return
        event.preventDefault()
        if (await confirmCloseWhileDownloading()) onClose()
      })
    })()
    return () => {
      unlistenPromise.then((unlisten) => unlisten())
    }
    // onClose is stable for the window's lifetime; t is read at call time.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  return (
    <div className="settings-app">
      <SettingsSidebar
        activeSection={activeSection}
        activeCapabilityView={activeCapabilityView}
        activeIslandView={activeIslandView}
        activeMonitorView={activeMonitorView}
        collapsed={sidebarCollapsed}
        onCollapsedChange={setSidebarCollapsed}
        onSelect={setActiveSection}
        onCapabilityViewChange={setActiveCapabilityView}
        onIslandViewChange={setActiveIslandView}
        onMonitorViewChange={setActiveMonitorView}
        onAddCustomAgent={openCustomAgentDialog}
      />
      <div className={contentClassName}>
        <div className="settings-window-brand" aria-hidden="true">
          <span className="settings-window-brand__mark">
            <img src="/agentbro-app-icon.png" alt="" />
          </span>
          <span className="settings-window-brand__copy">
            <span className="settings-window-brand__name">AgentBro</span>
            <span className="settings-window-brand__slogan">{t('notch.slogan')}</span>
          </span>
        </div>
        <button
          className="settings-close-btn"
          onClick={handleCloseRequest}
        >
          {t('settings.close')}
        </button>
        {!autoInstallUpdate && (updater.status === 'available' || updater.status === 'ready') && updater.version && (
          <button
            className="settings-update-pill"
            type="button"
            title={t('settings.installNow', { defaultValue: 'Install Now' }) + ` v${updater.version}`}
            onClick={() => updater.installUpdate()}
          >
            <span aria-hidden="true">↑</span>
            <span>{t('settings.installNow', { defaultValue: 'Install' })}</span>
            <em>v{updater.version}</em>
          </button>
        )}
        <AnimatePresence mode="wait">
          <motion.div
            key={activeSection}
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -8 }}
            transition={{ duration: 0.15 }}
          >
            {activeSection === 'island' ? (
              <IslandSection activeView={activeIslandView} />
            ) : activeSection === 'monitor' ? (
              <AgentMonitorSection activeView={activeMonitorView} />
            ) : activeSection === 'switch' ? (
              <SwitchSection />
            ) : activeSection === 'agents' ? (
              <AgentsSection
                activeView={activeCapabilityView}
                onViewChange={setActiveCapabilityView}
                customAgentDialogOpen={customAgentDialogOpen}
                onCustomAgentDialogOpenChange={setCustomAgentDialogOpen}
              />
            ) : activeSection === 'about' ? (
              <AboutSection
                updateStatus={updater.status}
                updateInstallChannel={updater.installChannel}
                updateVersion={updater.version}
                updateError={updater.error}
                updateRestartPending={updater.restartPending}
                updateRestartBlockedByActivity={updater.restartBlockedByActivity}
                updateBlockingSessionCount={updater.blockingSessionCount}
                onCheckForUpdate={updater.checkForUpdate}
              />
            ) : (
              <SectionComponent />
            )}
          </motion.div>
        </AnimatePresence>
      </div>

      {(updater.status === 'available' || updater.status === 'downloading' || updater.status === 'ready') && updater.version && (
        <UpdateDialog
          version={updater.version}
          notes={updater.notes}
          date={updater.date}
          status={updater.status}
          installChannel={updater.installChannel}
          manualDownloadUrl={updater.manualDownloadUrl}
          downloadProgress={updater.downloadProgress}
          restartPending={updater.restartPending}
          restartBlockedByActivity={updater.restartBlockedByActivity}
          blockingSessionCount={updater.blockingSessionCount}
          minimized={updateMinimized}
          onMinimize={() => setUpdateMinimized(true)}
          onExpand={() => setUpdateMinimized(false)}
          onInstall={updater.installUpdate}
          onDismiss={() => {
            setUpdateMinimized(false)
            updater.dismissUpdate()
          }}
        />
      )}
      {!analyticsConsentPromptCompleted && <FirstRunWelcome />}
    </div>
  )
}
