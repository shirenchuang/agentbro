import { useCallback } from 'react'
import type { ScannedSkill } from '../../services/skillApi'
import { skillApi } from '../../services/skillApi'
import { useSkillStore } from '../../stores/skillStore'

interface SkillCardProps {
  skill: ScannedSkill
  onRefresh: () => void
}

const TYPE_ICONS: Record<string, string> = {
  skill: '📝',
  mcp: '🔌',
}

export function SkillCard({ skill, onRefresh }: SkillCardProps) {
  const { selectSkill, batchMode, batchSelected, toggleBatchItem } = useSkillStore()
  const allEnabled = skill.agents.every(a => a.enabled)

  const handleToggle = useCallback(async (e: React.MouseEvent) => {
    e.stopPropagation()
    for (const agent of skill.agents) {
      await skillApi.toggle(skill.id, agent.agent, !allEnabled)
    }
    onRefresh()
  }, [skill, allEnabled, onRefresh])

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
      <div className="skill-card__icon">
        {TYPE_ICONS[skill.skillType] || '📦'}
      </div>
      <div className="skill-card__info">
        <div className="skill-card__name">{skill.name}</div>
        {skill.description && (
          <div className="skill-card__desc">{skill.description}</div>
        )}
      </div>
      <div className="skill-card__agents">
        {skill.agents.map(a => (
          <span key={a.agent} className="skill-card__agent-tag">{a.agent}</span>
        ))}
      </div>
      <button
        className={`skill-card__toggle ${allEnabled ? 'skill-card__toggle--on' : ''}`}
        onClick={handleToggle}
      />
    </div>
  )
}
