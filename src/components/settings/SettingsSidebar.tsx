import { useTranslation } from 'react-i18next'

interface SidebarItem {
  id: string
  labelKey: string
  icon: string
  iconBg: string
}

interface SidebarGroup {
  labelKey?: string
  items: SidebarItem[]
}

const sidebarGroups: SidebarGroup[] = [
  {
    items: [
      { id: 'general', labelKey: 'settings.general', icon: '⚙', iconBg: '#8E8E93' },
      { id: 'display', labelKey: 'settings.display', icon: '🖥', iconBg: '#007AFF' },
      { id: 'sound', labelKey: 'settings.sound', icon: '🔊', iconBg: '#FF2D55' },
    ],
  },
  {
    items: [
      { id: 'shortcuts', labelKey: 'settings.shortcuts', icon: '⌨', iconBg: '#5856D6' },
      { id: 'ssh-remote', labelKey: 'settings.sshRemote', icon: '🔗', iconBg: '#FF9500' },
      { id: 'skills', labelKey: 'settings.skills', icon: '🧩', iconBg: '#AF52DE' },
      { id: 'hooks', labelKey: 'settings.hooks', icon: '🔌', iconBg: '#34C759' },
      { id: 'webhooks', labelKey: 'settings.webhooks', icon: '📣', iconBg: '#FF2D55' },
      { id: 'labs', labelKey: 'settings.labs', icon: '🧪', iconBg: '#8E8E93' },
    ],
  },
  {
    labelKey: 'settings.agentIsland',
    items: [
      { id: 'license', labelKey: 'settings.license', icon: '🔑', iconBg: '#FFCC00' },
      { id: 'about', labelKey: 'settings.about', icon: 'ℹ', iconBg: '#007AFF' },
    ],
  },
]

interface SettingsSidebarProps {
  activeSection: string
  onSelect: (section: string) => void
}

export function SettingsSidebar({ activeSection, onSelect }: SettingsSidebarProps) {
  const { t } = useTranslation()

  return (
    <nav className="settings-sidebar settings-scroll">
      {sidebarGroups.map((group, gi) => (
        <div key={gi}>
          {gi > 0 && <div className="settings-sidebar__separator" />}
          {group.labelKey && <div className="settings-sidebar__group-label">{t(group.labelKey)}</div>}
          <div className="settings-sidebar__group">
            {group.items.map((item) => {
              const isActive = activeSection === item.id
              return (
                <div
                  key={item.id}
                  role="button"
                  tabIndex={0}
                  className={`settings-sidebar__item ${isActive ? 'settings-sidebar__item--active' : ''}`}
                  onClick={() => onSelect(item.id)}
                  onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onSelect(item.id) } }}
                >
                  <span
                    className="settings-sidebar__icon"
                    style={{ background: isActive ? 'rgba(255,255,255,0.25)' : item.iconBg, color: '#ffffff' }}
                  >
                    {item.icon}
                  </span>
                  <span>{t(item.labelKey)}</span>
                </div>
              )
            })}
          </div>
        </div>
      ))}
    </nav>
  )
}
