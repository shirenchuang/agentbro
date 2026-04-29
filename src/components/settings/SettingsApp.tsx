import { useState } from 'react'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { motion, AnimatePresence } from 'framer-motion'
import { SettingsSidebar } from './SettingsSidebar'
import { GeneralSection } from './sections/GeneralSection'
import { DisplaySection } from './sections/DisplaySection'
import { SoundSection } from './sections/SoundSection'
import { ShortcutsSection } from './sections/ShortcutsSection'
import { SSHRemoteSection } from './sections/SSHRemoteSection'
import { HookSection } from './sections/HookSection'
import { WebhookSection } from './sections/WebhookSection'
import { LabsSection } from './sections/LabsSection'
import { LicenseSection } from './sections/LicenseSection'
import { AboutSection } from './sections/AboutSection'
import '../../styles/settings.css'

const sections: Record<string, () => ReactNode> = {
  'general': GeneralSection,
  'display': DisplaySection,
  'sound': SoundSection,
  'shortcuts': ShortcutsSection,
  'ssh-remote': SSHRemoteSection,
  'hooks': HookSection,
  'webhooks': WebhookSection,
  'labs': LabsSection,
  'license': LicenseSection,
  'about': AboutSection,
}

interface SettingsAppProps {
  onClose: () => void
}

export function SettingsApp({ onClose }: SettingsAppProps) {
  const { t } = useTranslation()
  const [activeSection, setActiveSection] = useState('general')
  const SectionComponent = sections[activeSection] ?? GeneralSection

  return (
    <div className="settings-app">
      <SettingsSidebar activeSection={activeSection} onSelect={setActiveSection} />
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
            <SectionComponent />
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
  )
}
