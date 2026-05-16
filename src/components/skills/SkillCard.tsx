import { useCallback } from 'react'
import type { ScannedSkill } from '../../services/skillApi'
import { skillApi } from '../../services/skillApi'
import { useSkillStore } from '../../stores/skillStore'
import { useAgentStore } from '../../stores/agentStore'
import { agentColor, agentMatchesId, detectedAgentOptions, displayAgentName, shortAgentName } from '../../utils/agentPrograms'
import { displayVersionValue } from '../../utils/versions'

interface SkillCardProps {
  skill: ScannedSkill
  onRefresh: () => void
}

const TYPE_ICONS: Record<string, string> = {
  skill: '📝',
  plugin: '🔌',
  mcp: '🔌',
}

export function SkillCard({ skill, onRefresh }: SkillCardProps) {
  const { selectSkill, batchMode, batchSelected, toggleBatchItem } = useSkillStore()
  const { agents } = useAgentStore()
  const detectedAgents = detectedAgentOptions(agents)
  const toggleableAgents = skill.agents.filter(agent => agent.agent !== 'central')
  const allEnabled = toggleableAgents.length > 0 && toggleableAgents.every(a => a.enabled)
  const installedAgents = new Set(skill.agents.map(a => a.agent))
  const isAgentInstalled = (agent: string) => {
    return Array.from(installedAgents).some(id => agentMatchesId(id, agent))
  }
  const version = displayVersionValue(skill.frontmatter.version || skill.frontmatter.versionName)
  const visibleAgentStates = skill.agents.slice(0, 5)
  const hiddenAgentCount = Math.max(0, skill.agents.length - visibleAgentStates.length)
  const activePlatformAgents = detectedAgents.filter(agent => isAgentInstalled(agent.id)).slice(0, 6)

  const handleToggle = useCallback(async (e: React.MouseEvent) => {
    e.stopPropagation()
    for (const agent of toggleableAgents) {
      await skillApi.toggle(skill.id, agent.agent, !allEnabled)
    }
    onRefresh()
  }, [skill.id, toggleableAgents, allEnabled, onRefresh])

  const handleClick = useCallback(() => {
    if (batchMode) {
      toggleBatchItem(skill.id)
    } else {
      selectSkill(skill.id)
    }
  }, [batchMode, skill.id, selectSkill, toggleBatchItem])

  return (
    <div className="skill-card" onClick={handleClick}>
      {batchMode && (
        <input
          type="checkbox"
          checked={batchSelected.has(skill.id)}
          onChange={() => toggleBatchItem(skill.id)}
          onClick={e => e.stopPropagation()}
        />
      )}
      <div className={`skill-card__icon skill-card__icon--${skill.skillType}`}>
        <span>{TYPE_ICONS[skill.skillType] || '📦'}</span>
      </div>
      <div className="skill-card__info">
        <div className="skill-card__name-row">
          <div className="skill-card__name">{skill.name}</div>
          {version && <span className="skill-card__version">{version}</span>}
          {skill.hasUpdate && <span className="skill-card__update">可更新</span>}
        </div>
        {skill.description && (
          <div className="skill-card__desc">{skill.description}</div>
        )}
        <div className="skill-card__agent-tags">
          {visibleAgentStates.map((agent) => {
            const color = agentColor(agent.agent)
            return (
              <span
                key={`${skill.id}-${agent.agent}-${agent.installPath}-${agent.linkTarget ?? ''}`}
                style={{ '--platform-color': color } as React.CSSProperties}
              >
                <i />
                {displayAgentName(agent.agent, agents)}
              </span>
            )
          })}
          {hiddenAgentCount > 0 && (
            <span className="skill-card__agent-more">+{hiddenAgentCount}</span>
          )}
        </div>
      </div>
      {activePlatformAgents.length > 0 && (
        <div className="skill-card__platforms">
          {activePlatformAgents.map(agent => {
            const color = agentColor(agent.id)
            return (
              <span
                key={agent.id}
                className="skill-card__platform-icon skill-card__platform-icon--active"
                style={{ '--platform-color': color } as React.CSSProperties}
                title={agent.displayName}
              >
                {shortAgentName(agent.id, agents)}
              </span>
            )
          })}
        </div>
      )}
      <button
        className={`skill-card__toggle ${allEnabled ? 'skill-card__toggle--on' : ''}`}
        onClick={handleToggle}
        disabled={toggleableAgents.length === 0}
      />
      <div className="skill-card__bottom">
        <span>{skill.source} · {skill.skillType.toUpperCase()}</span>
        <span>{skill.fileSize > 0 ? `${Math.max(1, Math.round(skill.fileSize / 1024))} KB` : '本地扫描'}</span>
      </div>
    </div>
  )
}
