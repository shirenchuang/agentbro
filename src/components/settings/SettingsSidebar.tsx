import { useTranslation } from 'react-i18next'
import type { AgentProgramInfo } from '../../services/agentApi'
import { useAgentStore } from '../../stores/agentStore'
import { useSkillStore } from '../../stores/skillStore'
import type { CapabilityView, IslandSettingsView } from '../../types/capability'
import { isAgentProgramInstalled } from '../../utils/agentPrograms'
import { displayVersionValue } from '../../utils/versions'

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
      { id: 'island', labelKey: 'settings.island.title', icon: '🏝', iconBg: '#5856D6' },
      { id: 'agents', labelKey: 'settings.agents', icon: '🧩', iconBg: '#007AFF' },
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
  activeCapabilityView: CapabilityView
  activeIslandView: IslandSettingsView
  onSelect: (section: string) => void
  onCapabilityViewChange: (view: CapabilityView) => void
  onIslandViewChange: (view: IslandSettingsView) => void
  onAddCustomAgent: () => void
}

function isInstalled(agent: AgentProgramInfo) {
  return isAgentProgramInstalled(agent)
}

function displayVersion(agent: AgentProgramInfo) {
  const installed = displayVersionValue(agent.installedVersion)
  const latest = displayVersionValue(agent.latestVersion)
  if (agent.status === 'updateAvailable' && latest && installed) {
    return `${installed} → ${latest}`
  }
  if (agent.status === 'updateAvailable' && latest) {
    return `最新 ${latest}`
  }
  return installed
}

function agentStatusClass(agent: AgentProgramInfo) {
  if (agent.status === 'installed') return 'settings-capability-agent--installed'
  if (agent.status === 'updateAvailable') return 'settings-capability-agent--update'
  return 'settings-capability-agent--available'
}

export function SettingsSidebar({
  activeSection,
  activeCapabilityView,
  activeIslandView,
  onSelect,
  onCapabilityViewChange,
  onIslandViewChange,
  onAddCustomAgent,
}: SettingsSidebarProps) {
  const { t } = useTranslation()
  const {
    agents,
    selectedAgentId,
    focusAgent,
  } = useAgentStore()
  const { skills, packs } = useSkillStore()

  if (activeSection === 'island') {
    const navItems: Array<{ id: IslandSettingsView; label: string; icon: string; iconBg: string }> = [
      { id: 'overview', label: t('settings.island.tabs.overview', { defaultValue: 'Overview' }), icon: '✦', iconBg: '#5856D6' },
      { id: 'display', label: t('settings.island.tabs.display', { defaultValue: 'Display' }), icon: '◉', iconBg: '#007AFF' },
      { id: 'behavior', label: t('settings.island.tabs.behavior', { defaultValue: 'Behavior' }), icon: '⚡', iconBg: '#FF9500' },
      { id: 'integration', label: t('settings.island.tabs.integration', { defaultValue: 'Integration' }), icon: '⚙', iconBg: '#34C759' },
      { id: 'notify', label: t('settings.island.tabs.notify', { defaultValue: 'Notifications' }), icon: '🔔', iconBg: '#FF3B30' },
      { id: 'keys', label: t('settings.island.tabs.keys', { defaultValue: 'Shortcuts' }), icon: '⌘', iconBg: '#8E8E93' },
      { id: 'advanced', label: t('settings.island.tabs.advanced', { defaultValue: 'Advanced' }), icon: '⚒', iconBg: '#636366' },
    ]

    return (
      <nav className="settings-sidebar settings-sidebar--capability settings-scroll">
        <button
          type="button"
          className="settings-sidebar__back"
          onClick={() => onSelect('general')}
        >
          ‹ {t('settings.title')}
        </button>

        <div className="settings-sidebar__group-label">{t('settings.island.title')}</div>
        <div className="settings-capability-nav">
          {navItems.map((item) => (
            <button
              key={item.id}
              type="button"
              className={activeIslandView === item.id ? 'active' : ''}
              onClick={() => onIslandViewChange(item.id)}
            >
              <span
                className="settings-sidebar__icon settings-capability-nav__icon--colored"
                style={{ background: activeIslandView === item.id ? 'rgba(255,255,255,0.25)' : item.iconBg, color: '#fff' }}
              >
                {item.icon}
              </span>
              <span>{item.label}</span>
            </button>
          ))}
        </div>
      </nav>
    )
  }

  if (activeSection === 'agents') {
    const installedAgents = agents.filter(isInstalled)
    const availableAgents = agents.filter((agent) => !isInstalled(agent))
    const centralCount = skills.filter((skill) => skill.source === 'island' || skill.agents.some((agent) => agent.agent === 'central')).length
    const skillCount = skills.filter((skill) => skill.skillType === 'skill').length
    const pluginCount = packs.length + skills.filter((skill) => skill.skillType === 'plugin' || skill.skillType === 'mcp').length
    const profileCount = packs.length

    const navItems: Array<{ id: CapabilityView; label: string; icon: string; count?: number }> = [
      { id: 'agent', label: 'Agent 管理', icon: '🤖', count: agents.length },
      { id: 'central', label: '中央技能库', icon: '▣', count: centralCount },
      { id: 'skills', label: '全部 Skills', icon: '🧩', count: skillCount },
      { id: 'plugins', label: '插件与 MCP', icon: '🔌', count: pluginCount },
      { id: 'profiles', label: '技能包', icon: '📦', count: profileCount },
      { id: 'discover', label: '项目发现', icon: '🔎' },
      { id: 'market', label: '市场', icon: '🏪' },
      { id: 'sync', label: '同步', icon: '☁' },
    ]

    const chooseAgent = (agentId: string) => {
      focusAgent(agentId)
      onCapabilityViewChange('agent')
    }

    return (
      <nav className="settings-sidebar settings-sidebar--capability settings-scroll">
        <button
          type="button"
          className="settings-sidebar__back"
          onClick={() => onSelect('general')}
        >
          ‹ 设置
        </button>

        <div className="settings-capability-nav">
          {navItems.map((item) => (
            <button
              key={item.id}
              type="button"
              className={activeCapabilityView === item.id ? 'active' : ''}
              onClick={() => onCapabilityViewChange(item.id)}
            >
              <span className="settings-capability-nav__icon">{item.icon}</span>
              <span>{item.label}</span>
              {typeof item.count === 'number' && <em>{item.count}</em>}
            </button>
          ))}
        </div>

        <div className="settings-sidebar__separator" />

        <div className="settings-sidebar__group-label">已安装 AGENTS</div>
        <div className="settings-capability-agent-list">
          {installedAgents.map((agent) => (
            <button
              key={agent.id}
              type="button"
              className={`settings-capability-agent ${agentStatusClass(agent)} ${selectedAgentId === agent.id && activeCapabilityView === 'agent' ? 'active' : ''}`}
              onClick={() => chooseAgent(agent.id)}
            >
              <i />
              <span>{agent.displayName}</span>
              <em>{displayVersion(agent)}</em>
            </button>
          ))}
          {installedAgents.length === 0 && <div className="settings-capability-empty">暂无已安装 Agent</div>}
        </div>

        <div className="settings-sidebar__group-label">未安装</div>
        <div className="settings-capability-agent-list">
          {availableAgents.map((agent) => (
            <button
              key={agent.id}
              type="button"
              className={`settings-capability-agent ${agentStatusClass(agent)} ${selectedAgentId === agent.id && activeCapabilityView === 'agent' ? 'active' : ''}`}
              onClick={() => chooseAgent(agent.id)}
            >
              <i />
              <span>{agent.displayName}</span>
              <em>{agent.kind === 'cli' ? 'CLI' : 'APP'}</em>
            </button>
          ))}
          {availableAgents.length === 0 && <div className="settings-capability-empty">暂无可安装 Agent</div>}
          <button
            type="button"
            className="settings-capability-more"
            onClick={onAddCustomAgent}
          >
            + 添加自定义
          </button>
        </div>
      </nav>
    )
  }

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
