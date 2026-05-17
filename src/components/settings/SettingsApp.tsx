import { useState } from 'react'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { motion, AnimatePresence } from 'framer-motion'
import { SettingsSidebar } from './SettingsSidebar'
import { GeneralSection } from './sections/GeneralSection'
import { IslandSection } from './sections/IslandSection'
import { AgentsSection } from './sections/AgentsSection'
import { AgentMonitorSection } from './sections/AgentMonitorSection'
import { LicenseSection } from './sections/LicenseSection'
import { AboutSection } from './sections/AboutSection'
import type { CapabilityView, IslandSettingsView } from '../../types/capability'
import '../../styles/settings.css'

const sections: Record<string, () => ReactNode> = {
  'general': GeneralSection,
  'monitor': AgentMonitorSection,
  'license': LicenseSection,
  'about': AboutSection,
}

interface SettingsAppProps {
  onClose: () => void
}

export function SettingsApp({ onClose }: SettingsAppProps) {
  const { t } = useTranslation()
  const [activeSection, setActiveSection] = useState('general')
  const [activeCapabilityView, setActiveCapabilityView] = useState<CapabilityView>('agent')
  const [activeIslandView, setActiveIslandView] = useState<IslandSettingsView>('overview')
  const [customAgentDialogOpen, setCustomAgentDialogOpen] = useState(false)
  const SectionComponent = sections[activeSection] ?? GeneralSection
  const openCustomAgentDialog = () => {
    setActiveSection('agents')
    setActiveCapabilityView('agent')
    setCustomAgentDialogOpen(true)
  }

  return (
    <div className="settings-app">
      <SettingsSidebar
        activeSection={activeSection}
        activeCapabilityView={activeCapabilityView}
        activeIslandView={activeIslandView}
        onSelect={setActiveSection}
        onCapabilityViewChange={setActiveCapabilityView}
        onIslandViewChange={setActiveIslandView}
        onAddCustomAgent={openCustomAgentDialog}
      />
      <div className="settings-content settings-scroll">
        <button
          className="settings-close-btn"
          onClick={onClose}
        >
          {t('settings.close')}
        </button>
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
            ) : activeSection === 'agents' ? (
              <AgentsSection
                activeView={activeCapabilityView}
                onViewChange={setActiveCapabilityView}
                customAgentDialogOpen={customAgentDialogOpen}
                onCustomAgentDialogOpenChange={setCustomAgentDialogOpen}
              />
            ) : (
              <SectionComponent />
            )}
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
  )
}
