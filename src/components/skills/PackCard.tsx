import { useCallback } from 'react'
import { useTranslation } from 'react-i18next'
import type { SkillPack } from '../../services/skillApi'
import { skillApi } from '../../services/skillApi'

interface PackCardProps {
  pack: SkillPack
  onEdit: (pack: SkillPack) => void
  onRefresh: () => void
}

export function PackCard({ pack, onEdit, onRefresh }: PackCardProps) {
  const { t } = useTranslation()

  const handleDelete = useCallback(async (e: React.MouseEvent) => {
    e.stopPropagation()
    await skillApi.deletePack(pack.id)
    onRefresh()
  }, [pack.id, onRefresh])

  const handleApply = useCallback(async (e: React.MouseEvent) => {
    e.stopPropagation()
    await skillApi.applyPack(pack)
    onRefresh()
  }, [pack, onRefresh])

  return (
    <div className="pack-card" onClick={() => onEdit(pack)}>
      <div className="pack-card__header">
        <div className="pack-card__name">{pack.name}</div>
        <div className="pack-card__actions">
          <button className="skills-btn skills-btn--primary skills-btn--small" onClick={handleApply}>
            {t('skills.applyPack')}
          </button>
          <button className="skills-btn skills-btn--small" onClick={(e) => { e.stopPropagation(); onEdit(pack) }}>
            {t('skills.edit')}
          </button>
          <button className="skills-btn skills-btn--small skills-btn--danger" onClick={handleDelete}>
            {t('skills.delete')}
          </button>
        </div>
      </div>
      {pack.description && (
        <div className="pack-card__desc">{pack.description}</div>
      )}
      <div className="pack-card__skills">
        {pack.skills.map(s => (
          <span key={s} className="pack-card__skill-chip">{s}</span>
        ))}
      </div>
      <div className="pack-card__agents">
        {pack.targetAgents.map(a => (
          <span key={a} className="skill-card__agent-tag">{a}</span>
        ))}
      </div>
    </div>
  )
}
