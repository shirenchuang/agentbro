import { useTranslation } from 'react-i18next'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { MouseEvent as ReactMouseEvent } from 'react'
import { useSkillStoreV2 } from '../../stores/skillStoreV2'
import { useSessionStore } from '../../stores/sessionStore'
import type { SkillManagerTab } from '../../stores/skillStoreV2'
import { AgentIconBadge } from '../skills-v2/AgentIconBadge'
import type { IslandSettingsView, MonitorSettingsView } from '../../types/capability'
import { buildAgentUsageScores, readStoredAgentOrder, sortAgentSummaries, writeStoredAgentOrder } from '../../utils/agentOrdering'
import { RuntimeEnvironmentSwitcher } from './RuntimeEnvironmentSwitcher'

interface SidebarItem {
  id: string
  labelKey: string
  defaultLabel?: string
  icon: string
  iconBg: string
  hidden?: boolean
}

interface SidebarGroup {
  labelKey?: string
  items: SidebarItem[]
}

const SHARED_SKILLS_AGENT_ID = 'agents'

interface AgentDropTarget {
  agentId: string
  edge: 'before' | 'after'
}

const sidebarGroups: SidebarGroup[] = [
  {
    items: [
      { id: 'tasks', labelKey: 'settings.tasks', defaultLabel: 'Tasks', icon: '✓', iconBg: '#34C759' },
      { id: 'usage', labelKey: 'settings.usage', defaultLabel: 'Usage', icon: '▥', iconBg: '#007AFF' },
      { id: 'general', labelKey: 'settings.controlTowerSettings', defaultLabel: 'Settings', icon: '⚙', iconBg: '#8E8E93' },
      { id: 'island', labelKey: 'settings.island.title', icon: '🏝', iconBg: '#5856D6', hidden: true },
      { id: 'skill-manager-v2', labelKey: 'settings.skillManager', icon: '🧩', iconBg: '#34C759', hidden: true },
      { id: 'remote-servers', labelKey: 'settings.remoteServers.title', icon: '>_', iconBg: '#009C95', hidden: true },
    ],
  },
  {
    labelKey: 'settings.agentBro',
    items: [
      { id: 'about', labelKey: 'settings.about', icon: 'ℹ', iconBg: '#007AFF', hidden: true },
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
  const setSkillInstallTab = useSkillStoreV2((s) => s.setInstallTab)
  const marketplaceInstallTask = useSkillStoreV2((s) => s.marketplaceInstallTask)
  const skillAgents = useSkillStoreV2((s) => s.agents)
  const skillSelectedAgentId = useSkillStoreV2((s) => s.selectedAgentId)
  const selectAgent = useSkillStoreV2((s) => s.selectAgent)
  const requestCustomAgentDialog = useSkillStoreV2((s) => s.requestCustomAgentDialog)
  const sessionList = useSessionStore((s) => s.sessionList)
  const activeSessionId = useSessionStore((s) => s.activeSessionId)
  const [showUninstalledSkillAgents, setShowUninstalledSkillAgents] = useState(false)
  const [manualAgentOrder, setManualAgentOrder] = useState<string[]>(() => readStoredAgentOrder())
  const [draggedAgentId, setDraggedAgentId] = useState<string | null>(null)
  const [agentDropTarget, setAgentDropTarget] = useState<AgentDropTarget | null>(null)
  const agentMouseCleanupRef = useRef<(() => void) | null>(null)
  const suppressAgentClickRef = useRef(false)
  const marketplaceTaskItems = marketplaceInstallTask ? Object.values(marketplaceInstallTask.items) : []
  const marketplaceTaskCompleted = marketplaceTaskItems.filter((item) => ['success', 'failed', 'cancelled'].includes(item.status)).length
  const marketplaceTaskBadge = marketplaceInstallTask
    ? marketplaceInstallTask.busy
      ? `${marketplaceTaskCompleted}/${marketplaceTaskItems.length}`
      : marketplaceInstallTask.result?.cancelled
        ? '■'
        : marketplaceInstallTask.result?.failedCount
          ? '!'
          : '✓'
    : null
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
  const reorderInstalledAgent = useCallback((sourceAgentId: string, target: AgentDropTarget) => {
    const next = installedSkillAgents.map((agent) => agent.id).filter((agentId) => agentId !== sourceAgentId)
    const targetIndex = next.indexOf(target.agentId)
    if (targetIndex < 0) return
    next.splice(targetIndex + (target.edge === 'after' ? 1 : 0), 0, sourceAgentId)
    setManualAgentOrder(next)
    writeStoredAgentOrder(next)
  }, [installedSkillAgents])
  const finishAgentDrag = () => {
    setDraggedAgentId(null)
    setAgentDropTarget(null)
  }
  const startAgentMouseDrag = (event: ReactMouseEvent<HTMLDivElement>, agentId: string) => {
    if (event.button !== 0 || installedSkillAgents.length < 2) return
    agentMouseCleanupRef.current?.()

    const startX = event.clientX
    const startY = event.clientY
    let dragging = false
    let dropTarget: AgentDropTarget | null = null

    const handleMouseMove = (mouseEvent: MouseEvent) => {
      if (!dragging && Math.hypot(mouseEvent.clientX - startX, mouseEvent.clientY - startY) < 6) return
      if (!dragging) {
        dragging = true
        setDraggedAgentId(agentId)
      }
      mouseEvent.preventDefault()

      const targetRow = Array.from(document.querySelectorAll<HTMLElement>('.sm2-sidebar__subitem-row[data-agent-id]')).find((row) => {
        const bounds = row.getBoundingClientRect()
        return mouseEvent.clientX >= bounds.left
          && mouseEvent.clientX <= bounds.right
          && mouseEvent.clientY >= bounds.top
          && mouseEvent.clientY <= bounds.bottom
      })
      const targetAgentId = targetRow?.dataset.agentId
      if (!targetRow || !targetAgentId || targetAgentId === agentId) {
        dropTarget = null
        setAgentDropTarget(null)
        return
      }

      const bounds = targetRow.getBoundingClientRect()
      const nextTarget: AgentDropTarget = {
        agentId: targetAgentId,
        edge: mouseEvent.clientY < bounds.top + bounds.height / 2 ? 'before' : 'after',
      }
      if (dropTarget?.agentId === nextTarget.agentId && dropTarget.edge === nextTarget.edge) return
      dropTarget = nextTarget
      setAgentDropTarget(nextTarget)
    }

    const handleMouseUp = (mouseEvent: MouseEvent) => {
      if (dragging) mouseEvent.preventDefault()
      cleanup()
      if (dragging && dropTarget) reorderInstalledAgent(agentId, dropTarget)
      if (dragging) {
        suppressAgentClickRef.current = true
        window.setTimeout(() => { suppressAgentClickRef.current = false }, 0)
      }
      finishAgentDrag()
    }

    const cleanup = () => {
      window.removeEventListener('mousemove', handleMouseMove, true)
      window.removeEventListener('mouseup', handleMouseUp, true)
      agentMouseCleanupRef.current = null
    }

    window.addEventListener('mousemove', handleMouseMove, true)
    window.addEventListener('mouseup', handleMouseUp, true)
    agentMouseCleanupRef.current = cleanup
  }
  useEffect(() => () => agentMouseCleanupRef.current?.(), [])
  const sidebarClassName = `settings-sidebar settings-scroll${collapsed ? ' settings-sidebar--collapsed' : ''}`
  const capabilitySidebarClassName = `settings-sidebar settings-sidebar--capability settings-scroll${collapsed ? ' settings-sidebar--collapsed' : ''}`
  const toggleLabel = collapsed ? t('settings.expandSidebar', { defaultValue: 'Expand sidebar' }) : t('settings.collapseSidebar', { defaultValue: 'Collapse sidebar' })
  const sectionTitleById: Record<string, string> = {
    island: t('settings.island.title'),
    'remote-servers': t('settings.remoteServers.title', { defaultValue: 'Remote Servers' }),
    monitor: t('settings.agentMonitor'),
    agents: t('settings.agents'),
    switch: t('settings.switch'),
    'skill-manager-v2': t('settings.skillManager', { defaultValue: 'Agent管理' }),
  }
  const isCapabilitySection = activeSection === 'island' || activeSection === 'monitor' || activeSection === 'agents' || activeSection === 'switch' || activeSection === 'skill-manager-v2'
  const brandTitle = isCapabilitySection ? sectionTitleById[activeSection] : t('settings.title')
  const backToSettingsLabel = t('settings.backToSettings', { defaultValue: 'Back to Settings' })
  const openSkillTab = (tab: SkillManagerTab) => {
    setSkillTab(tab)
    if (tab === 'install' && marketplaceInstallTask) setSkillInstallTab('official')
  }
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
              onClick={() => openSkillTab(item.id)}
            >
              <span
                className="settings-sidebar__icon settings-capability-nav__icon--colored"
                style={{ background: skillActiveTab === item.id ? 'rgba(255,255,255,0.25)' : item.iconBg, color: '#fff' }}
              >
                {item.icon}
              </span>
              <span className="settings-sidebar__label-text">{item.label}</span>
              {item.id === 'install' && marketplaceTaskBadge && (
                <em className="settings-sidebar__task-count" aria-label={t('skills.marketInstall.globalTaskTitle')}>
                  {marketplaceTaskBadge}
                </em>
              )}
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
                    installedSkillAgents.map((a) => (
                      <div
                        key={a.id}
                        className={`sm2-sidebar__subitem-row${installedSkillAgents.length > 1 ? ' sm2-sidebar__subitem-row--draggable' : ''}${draggedAgentId === a.id ? ' sm2-sidebar__subitem-row--dragging' : ''}${agentDropTarget?.agentId === a.id ? ` sm2-sidebar__subitem-row--drop-${agentDropTarget.edge}` : ''}`}
                        data-agent-id={a.id}
                        title={`拖拽调整 ${a.displayName} 顺序`}
                        onMouseDown={(event) => startAgentMouseDrag(event, a.id)}
                      >
                        <button
                          type="button"
                          className={`sm2-sidebar__subitem${skillSelectedAgentId === a.id ? ' sm2-sidebar__subitem--active' : ''}`}
                          onClick={() => {
                            if (suppressAgentClickRef.current) return
                            selectAgent(a.id)
                          }}
                        >
                          <AgentIconBadge iconKey={a.iconKey} title={a.displayName} size={20} />
                          <span className="sm2-sidebar__subitem-label">{a.displayName}</span>
                          <span className="sm2-sidebar__subitem-dot" />
                        </button>
                        <span className="sm2-sidebar__drag-handle" aria-hidden="true" />
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
              <button
                type="button"
                className="sm2-sidebar__subgroup-add"
                aria-label={t('settings.addEngineBranch')}
                title={t('settings.addEngineBranch')}
                onClick={requestCustomAgentDialog}
              >
                <span aria-hidden="true">＋</span>
                <span>{t('settings.addEngineBranch')}</span>
              </button>
            </div>
          )}
        </div>
        <RuntimeEnvironmentSwitcher
          collapsed={collapsed}
          onManageRemote={() => onSelect('remote-servers')}
        />
      </nav>
    )
  }

  return (
    <nav className={sidebarClassName}>
      {toggleSidebar}
      {sidebarGroups.map((group, gi) => (
        <div key={gi} hidden={group.items.every((item) => item.hidden)}>
          {gi > 0 && <div className="settings-sidebar__separator" />}
          {group.labelKey && <div className="settings-sidebar__group-label">{t(group.labelKey)}</div>}
          <div className="settings-sidebar__group">
            {group.items.map((item) => {
              const isActive = activeSection === item.id
              const label = t(item.labelKey, { defaultValue: item.defaultLabel ?? item.labelKey })
              return (
                <div
                  key={item.id}
                  role="button"
                  tabIndex={0}
                  hidden={item.hidden}
                  className={`settings-sidebar__item ${isActive ? 'settings-sidebar__item--active' : ''}`}
                  aria-label={label}
                  title={label}
                  onClick={() => {
                    if (item.id === 'skill-manager-v2' && marketplaceInstallTask) openSkillTab('install')
                    onSelect(item.id)
                  }}
                  onKeyDown={(e) => {
                    if (e.key !== 'Enter' && e.key !== ' ') return
                    e.preventDefault()
                    if (item.id === 'skill-manager-v2' && marketplaceInstallTask) openSkillTab('install')
                    onSelect(item.id)
                  }}
                >
                  <span
                    className="settings-sidebar__icon"
                    style={{ background: isActive ? 'rgba(255,255,255,0.25)' : item.iconBg, color: '#ffffff' }}
                  >
                    {item.icon}
                  </span>
                  <span className="settings-sidebar__label-text">{label}</span>
                  {item.id === 'skill-manager-v2' && marketplaceTaskBadge && (
                    <span
                      className={`settings-sidebar__task-badge${marketplaceInstallTask?.busy ? ' settings-sidebar__task-badge--busy' : ''}`}
                      aria-label={t('skills.marketInstall.globalTaskTitle')}
                    >
                      {marketplaceTaskBadge}
                    </span>
                  )}
                </div>
              )
            })}
          </div>
        </div>
      ))}
    </nav>
  )
}
