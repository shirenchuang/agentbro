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

const ALL_AGENTS = ['claude-code', 'codex', 'gemini-cli', 'cursor', 'hermes']

const AGENT_LABELS: Record<string, { short: string; color: string }> = {
  'claude-code': { short: 'C', color: '#d97706' },
  'codex':       { short: 'X', color: '#10b981' },
  'gemini-cli':  { short: 'G', color: '#3b82f6' },
  'cursor':      { short: 'U', color: '#8b5cf6' },
  'hermes':      { short: 'H', color: '#ef4444' },
}

export function SkillCard({ skill, onRefresh }: SkillCardProps) {
  const { selectSkill, batchMode, batchSelected, toggleBatchItem } = useSkillStore()
  const allEnabled = skill.agents.every(a => a.enabled)
  const installedAgents = new Set(skill.agents.map(a => a.agent))

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
      <div className="skill-card__platforms">
        {ALL_AGENTS.map(agent => {
          const installed = installedAgents.has(agent)
          const cfg = AGENT_LABELS[agent] ?? { short: agent[0].toUpperCase(), color: '#888' }
          return (
            <span
              key={agent}
              className={`skill-card__platform-icon ${installed ? 'skill-card__platform-icon--active' : ''}`}
              style={{ '--platform-color': cfg.color } as React.CSSProperties}
              title={agent}
            >
              {cfg.short}
            </span>
          )
        })}
      </div>
      <button
        className={`skill-card__toggle ${allEnabled ? 'skill-card__toggle--on' : ''}`}
        onClick={handleToggle}
      />
    </div>
  )
}
