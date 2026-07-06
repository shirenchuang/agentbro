import { useTranslation } from 'react-i18next'
import { useCallback, useMemo, useState } from 'react'
import { useSkillStoreV2 } from '../../stores/skillStoreV2'
import { useSessionStore } from '../../stores/sessionStore'
import type { SkillManagerTab } from '../../stores/skillStoreV2'
import { AgentIconBadge } from '../skills-v2/AgentIconBadge'
import type { IslandSettingsView, MonitorSettingsView } from '../../types/capability'
import { buildAgentUsageScores, moveAgentInOrder, readStoredAgentOrder, sortAgentSummaries, writeStoredAgentOrder } from '../../utils/agentOrdering'

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

const SHARED_SKILLS_AGENT_ID = 'agents'

const sidebarGroups: SidebarGroup[] = [
  {
    items: [
      { id: 'general', labelKey: 'settings.general', icon: '⚙', iconBg: '#8E8E93' },
      { id: 'island', labelKey: 'settings.island.title', icon: '🏝', iconBg: '#5856D6' },
      { id: 'skill-manager-v2', labelKey: 'settings.skillManager', icon: '🧩', iconBg: '#34C759' },
    ],
  },
  {
    labelKey: 'settings.agentBro',
    items: [
      { id: 'about', labelKey: 'settings.about', icon: 'ℹ', iconBg: '#007AFF' },
    ],
  },
]

interface SettingsSidebarProps {
  activeSection: string
  activeIslandView: IslandSettingsView
  activeMonitorView: MonitorSettingsView
  collapsed: boolean
  onCollapsedChange: (collapsed: boolean) => void
  onSelect: (section: string) => void
  onIslandViewChange: (view: IslandSettingsView) => void
  onMonitorViewChange: (view: MonitorSettingsView) => void
}

export function SettingsSidebar({
  activeSection,
  activeIslandView,
  activeMonitorView,
  collapsed,
  onCollapsedChange,
  onSelect,
  onIslandViewChange,
  onMonitorViewChange,
}: SettingsSidebarProps) {
  const { t } = useTranslation()
  const skillActiveTab = useSkillStoreV2((s) => s.activeTab)
  const setSkillTab = useSkillStoreV2((s) => s.setTab)
  const skillAgents = useSkillStoreV2((s) => s.agents)
  const skillSelectedAgentId = useSkillStoreV2((s) => s.selectedAgentId)
  const selectAgent = useSkillStoreV2((s) => s.selectAgent)
  const sessionList = useSessionStore((s) => s.sessionList)
  const activeSessionId = useSessionStore((s) => s.activeSessionId)
  const [showUninstalledSkillAgents, setShowUninstalledSkillAgents] = useState(false)
  const [manualAgentOrder, setManualAgentOrder] = useState<string[]>(() => readStoredAgentOrder())
  const agentUsageScores = useMemo(() => buildAgentUsageScores(sessionList, activeSessionId), [sessionList, activeSessionId])
  const visibleSkillAgents = useMemo(() => skillAgents.filter((agent) => agent.id !== SHARED_SKILLS_AGENT_ID), [skillAgents])
  const installedSkillAgents = useMemo(
    () => sortAgentSummaries(
      visibleSkillAgents.filter((agent) => agent.installed),
      { manualOrder: manualAgentOrder, usageScores: agentUsageScores },
    ),
    [agentUsageScores, manualAgentOrder, visibleSkillAgents],
  )
  const uninstalledSkillAgents = useMemo(
    () => sortAgentSummaries(visibleSkillAgents.filter((agent) => !agent.installed), { usageScores: agentUsageScores }),
    [agentUsageScores, visibleSkillAgents],
  )
  const moveInstalledAgent = useCallback((agentId: string, direction: 'up' | 'down') => {
    const displayedAgentIds = installedSkillAgents.map((agent) => agent.id)
    const next = moveAgentInOrder(manualAgentOrder, displayedAgentIds, agentId, direction)
    setManualAgentOrder(next)
    writeStoredAgentOrder(next)
  }, [installedSkillAgents, manualAgentOrder])
  const sidebarClassName = `settings-sidebar settings-scroll${collapsed ? ' settings-sidebar--collapsed' : ''}`
  const capabilitySidebarClassName = `settings-sidebar settings-sidebar--capability settings-scroll${collapsed ? ' settings-sidebar--collapsed' : ''}`
  const toggleLabel = collapsed ? t('settings.expandSidebar', { defaultValue: 'Expand sidebar' }) : t('settings.collapseSidebar', { defaultValue: 'Collapse sidebar' })
  const sectionTitleById: Record<string, string> = {
    island: t('settings.island.title'),
    monitor: t('settings.agentMonitor'),
    agents: t('settings.agents'),
    switch: t('settings.switch'),
    'skill-manager-v2': t('settings.skillManager', { defaultValue: 'Agent管理' }),
  }
  const isCapabilitySection = activeSection === 'island' || activeSection === 'monitor' || activeSection === 'agents' || activeSection === 'switch' || activeSection === 'skill-manager-v2'
  const brandTitle = isCapabilitySection ? sectionTitleById[activeSection] : t('settings.title')
  const backToSettingsLabel = t('settings.backToSettings', { defaultValue: 'Back to Settings' })
  const toggleSidebar = (
    <div className={`settings-sidebar__brand${isCapabilitySection ? ' settings-sidebar__brand--contextual' : ''}`}>
      <button
        type="button"
        className="settings-sidebar__brand-home"
        aria-label={isCapabilitySection ? backToSettingsLabel : t('settings.title')}
        title={isCapabilitySection ? backToSettingsLabel : t('settings.title')}
        onClick={() => onSelect('general')}
      >
        <span className="settings-sidebar__brand-mark" aria-hidden="true">
          <img className="settings-sidebar__collapse-logo" src="/agentbro-logo.png" alt="" />
        </span>
        <span className="settings-sidebar__brand-copy">
          <span className="settings-sidebar__brand-title">{brandTitle}</span>
        </span>
      </button>
      <button
        type="button"
        className="settings-sidebar__collapse-toggle"
        aria-label={toggleLabel}
        title={toggleLabel}
        onClick={() => onCollapsedChange(!collapsed)}
      >
        <span className="settings-sidebar__brand-indicator" aria-hidden="true">
          {collapsed ? '›' : '‹'}
        </span>
      </button>
    </div>
  )

  if (activeSection === 'island') {
    const navItems: Array<{ id: IslandSettingsView; label: string; icon: string; iconBg: string }> = [
      { id: 'overview', label: t('settings.island.tabs.overview', { defaultValue: 'Overview' }), icon: '✦', iconBg: '#5856D6' },
      { id: 'display', label: t('settings.island.tabs.display', { defaultValue: 'Display' }), icon: '◉', iconBg: '#007AFF' },
      { id: 'market', label: t('settings.island.tabs.market', { defaultValue: 'Pet Market' }), icon: '🛒', iconBg: '#34C759' },
      { id: 'behavior', label: t('settings.island.tabs.behavior', { defaultValue: 'Behavior' }), icon: '⚡', iconBg: '#FF9500' },
      { id: 'integration', label: t('settings.island.tabs.integration', { defaultValue: 'Integration' }), icon: '⚙', iconBg: '#34C759' },
      { id: 'remote', label: t('settings.island.tabs.remote', { defaultValue: 'SSH Remote' }), icon: '⇄', iconBg: '#00A8A8' },
      { id: 'notify', label: t('settings.island.tabs.notify', { defaultValue: 'Notifications' }), icon: '🔔', iconBg: '#FF3B30' },
      { id: 'keys', label: t('settings.island.tabs.keys', { defaultValue: 'Shortcuts' }), icon: '⌨', iconBg: '#8E8E93' },
      { id: 'advanced', label: t('settings.island.tabs.advanced', { defaultValue: 'Advanced' }), icon: '⚒', iconBg: '#636366' },
    ]

    return (
      <nav className={capabilitySidebarClassName}>
        {toggleSidebar}
        <div className="settings-capability-nav">
          {navItems.map((item) => (
            <button
              key={item.id}
              type="button"
              className={activeIslandView === item.id ? 'active' : ''}
              aria-label={item.label}
              title={item.label}
              onClick={() => onIslandViewChange(item.id)}
            >
              <span
                className="settings-sidebar__icon settings-capability-nav__icon--colored"
                style={{ background: activeIslandView === item.id ? 'rgba(255,255,255,0.25)' : item.iconBg, color: '#fff' }}
              >
                {item.icon}
              </span>
              <span className="settings-sidebar__label-text">{item.label}</span>
            </button>
          ))}
        </div>
      </nav>
    )
  }

  if (activeSection === 'monitor') {
    const navItems: Array<{ id: MonitorSettingsView; label: string; icon: string; iconBg: string }> = [
      { id: 'overview', label: '监控总览', icon: '▦', iconBg: '#34C759' },
      { id: 'capture', label: '请求抓包', icon: '⇄', iconBg: '#007AFF' },
      { id: 'stats', label: '项目统计', icon: '▥', iconBg: '#FF9500' },
      { id: 'sessions', label: '会话追踪', icon: '◉', iconBg: '#5856D6' },
      { id: 'access', label: '接入设置', icon: '<>', iconBg: '#8E8E93' },
      { id: 'usage', label: '用量统计', icon: '$', iconBg: '#FF3B30' },
    ]

    return (
      <nav className={capabilitySidebarClassName}>
        {toggleSidebar}
        <div className="settings-capability-nav">
          {navItems.map((item) => (
            <button
              key={item.id}
              type="button"
              className={activeMonitorView === item.id ? 'active' : ''}
              aria-label={item.label}
              title={item.label}
              onClick={() => onMonitorViewChange(item.id)}
            >
              <span
                className="settings-sidebar__icon settings-capability-nav__icon--colored"
                style={{ background: activeMonitorView === item.id ? 'rgba(255,255,255,0.25)' : item.iconBg, color: '#fff' }}
              >
                {item.icon}
              </span>
              <span className="settings-sidebar__label-text">{item.label}</span>
            </button>
          ))}
        </div>
      </nav>
    )
  }

  if (activeSection === 'skill-manager-v2') {
    const skillTabs: Array<{ id: SkillManagerTab; label: string; icon: string; iconBg: string }> = [
      { id: 'library', label: 'Skill 库', icon: '🧩', iconBg: '#34C759' },
      { id: 'install', label: '安装 Skill', icon: '⬇', iconBg: '#FF9500' },
      { id: 'packs', label: '技能包', icon: '📦', iconBg: '#5856D6' },
      { id: 'agents', label: 'Agent 管理', icon: '🤖', iconBg: '#007AFF' },
      { id: 'diagnostics', label: '诊断与修复', icon: '🩺', iconBg: '#FF9500' },
      { id: 'settings', label: '设置', icon: '⚙', iconBg: '#8E8E93' },
    ]

    return (
      <nav className={capabilitySidebarClassName}>
        {toggleSidebar}
        <div className="settings-capability-nav">
          {skillTabs.map((item) => (
            <button
              key={item.id}
              type="button"
              className={skillActiveTab === item.id ? 'active' : ''}
              aria-label={item.label}
              title={item.label}
              onClick={() => setSkillTab(item.id)}
            >
              <span
                className="settings-sidebar__icon settings-capability-nav__icon--colored"
                style={{ background: skillActiveTab === item.id ? 'rgba(255,255,255,0.25)' : item.iconBg, color: '#fff' }}
              >
                {item.icon}
              </span>
              <span className="settings-sidebar__label-text">{item.label}</span>
            </button>
          ))}
          {skillActiveTab === 'agents' && (
            <div className="sm2-sidebar__subgroup">
              <div className="sm2-sidebar__subgroup-label">
                <span>已安装 Agent</span>
                <em>{installedSkillAgents.length}</em>
              </div>
              {visibleSkillAgents.length === 0 ? (
                <div className="sm2-sidebar__subgroup-empty">暂无</div>
              ) : (
                <>
                  {installedSkillAgents.length === 0 ? (
                    <div className="sm2-sidebar__subgroup-empty">暂无已安装 Agent</div>
                  ) : (
                    installedSkillAgents.map((a, index) => (
                      <div key={a.id} className="sm2-sidebar__subitem-row">
                        <button
                          type="button"
                          className={`sm2-sidebar__subitem${skillSelectedAgentId === a.id ? ' sm2-sidebar__subitem--active' : ''}`}
                          onClick={() => selectAgent(a.id)}
                        >
                          <AgentIconBadge iconKey={a.iconKey} title={a.displayName} size={20} />
                          <span className="sm2-sidebar__subitem-label">{a.displayName}</span>
                          <span className="sm2-sidebar__subitem-dot" />
                        </button>
                        <span className="sm2-sidebar__reorder" aria-label={`调整 ${a.displayName} 顺序`}>
                          <button
                            type="button"
                            className="sm2-sidebar__reorder-btn"
                            disabled={index === 0}
                            aria-label={`上移 ${a.displayName}`}
                            title={`上移 ${a.displayName}`}
                            onClick={() => moveInstalledAgent(a.id, 'up')}
                          >
                            ↑
                          </button>
                          <button
                            type="button"
                            className="sm2-sidebar__reorder-btn"
                            disabled={index === installedSkillAgents.length - 1}
                            aria-label={`下移 ${a.displayName}`}
                            title={`下移 ${a.displayName}`}
                            onClick={() => moveInstalledAgent(a.id, 'down')}
                          >
                            ↓
                          </button>
                        </span>
                      </div>
                    ))
                  )}
                  {uninstalledSkillAgents.length > 0 && (
                    <div className="sm2-sidebar__subgroup-section">
                      <button
                        type="button"
                        className="sm2-sidebar__fold-toggle"
                        aria-expanded={showUninstalledSkillAgents}
                        onClick={() => setShowUninstalledSkillAgents((open) => !open)}
                      >
                        <span className={`sm2-sidebar__fold-chevron${showUninstalledSkillAgents ? ' sm2-sidebar__fold-chevron--open' : ''}`}>
                          ›
                        </span>
                        <span>未安装 Agent</span>
                        <em>{uninstalledSkillAgents.length}</em>
                      </button>
                      {showUninstalledSkillAgents && uninstalledSkillAgents.map((a) => (
                        <button
                          key={a.id}
                          type="button"
                          className={`sm2-sidebar__subitem sm2-sidebar__subitem--muted${skillSelectedAgentId === a.id ? ' sm2-sidebar__subitem--active' : ''}`}
                          onClick={() => selectAgent(a.id)}
                        >
                          <AgentIconBadge iconKey={a.iconKey} title={a.displayName} size={20} />
                          <span className="sm2-sidebar__subitem-label">{a.displayName}</span>
                        </button>
                      ))}
                    </div>
                  )}
                </>
              )}
            </div>
          )}
        </div>
      </nav>
    )
  }

  return (
    <nav className={sidebarClassName}>
      {toggleSidebar}
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
                  aria-label={t(item.labelKey)}
                  title={t(item.labelKey)}
                  onClick={() => onSelect(item.id)}
                  onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onSelect(item.id) } }}
                >
                  <span
                    className="settings-sidebar__icon"
                    style={{ background: isActive ? 'rgba(255,255,255,0.25)' : item.iconBg, color: '#ffffff' }}
                  >
                    {item.icon}
                  </span>
                  <span className="settings-sidebar__label-text">{t(item.labelKey)}</span>
                </div>
              )
            })}
          </div>
        </div>
      ))}
    </nav>
  )
}
