import { useState, useCallback } from 'react'
import { useTranslation } from 'react-i18next'
import type { SkillPack } from '../../services/skillApi'
import { skillApi } from '../../services/skillApi'
import { useSkillStore } from '../../stores/skillStore'

const AGENTS = ['claude-code', 'codex', 'gemini-cli', 'cursor', 'hermes']

interface PackDialogProps {
  pack?: SkillPack
  onClose: () => void
}

export function PackDialog({ pack, onClose }: PackDialogProps) {
  const { t } = useTranslation()
  const { skills, loadAll } = useSkillStore()
  const [name, setName] = useState(pack?.name || '')
  const [description, setDescription] = useState(pack?.description || '')
  const [selectedSkills, setSelectedSkills] = useState<Set<string>>(new Set(pack?.skills || []))
  const [selectedAgents, setSelectedAgents] = useState<Set<string>>(new Set(pack?.targetAgents || []))

  const toggleSkill = useCallback((id: string) => {
    setSelectedSkills(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id); else next.add(id)
      return next
    })
  }, [])

  const toggleAgent = useCallback((id: string) => {
    setSelectedAgents(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id); else next.add(id)
      return next
    })
  }, [])

  const handleSave = useCallback(async () => {
    if (!name.trim()) return

    const data: SkillPack = {
      id: pack?.id || `pack-${Date.now()}`,
      name: name.trim(),
      description: description.trim(),
      skills: Array.from(selectedSkills),
      targetAgents: Array.from(selectedAgents),
    }

    if (pack) {
      await skillApi.updatePack(data)
    } else {
      await skillApi.createPack(data)
    }
    loadAll()
    onClose()
  }, [pack, name, description, selectedSkills, selectedAgents, loadAll, onClose])

  return (
    <div className="skills-dialog-overlay" onClick={onClose}>
      <div className="skills-dialog" onClick={e => e.stopPropagation()}>
        <div className="skills-dialog__header">
          <div className="skills-dialog__title">
            {pack ? t('skills.editPack') : t('skills.createPack')}
          </div>
        </div>
        <div className="skills-dialog__body">
          <div className="install-form-row">
            <label className="install-form-label">{t('skills.packName')}</label>
            <input
              className="install-form-input"
              value={name}
              onChange={e => setName(e.target.value)}
              placeholder={t('skills.packNamePlaceholder')}
            />
          </div>
          <div className="install-form-row">
            <label className="install-form-label">{t('skills.packDesc')}</label>
            <input
              className="install-form-input"
              value={description}
              onChange={e => setDescription(e.target.value)}
              placeholder={t('skills.packDescPlaceholder')}
            />
          </div>
          <div className="install-form-row">
            <label className="install-form-label">{t('skills.selectSkills')}</label>
            <div className="install-targets">
              {skills.map(s => (
                <div
                  key={s.id}
                  className={`install-target-chip ${selectedSkills.has(s.id) ? 'install-target-chip--selected' : ''}`}
                  onClick={() => toggleSkill(s.id)}
                >
                  {s.name}
                </div>
              ))}
            </div>
          </div>
          <div className="install-form-row">
            <label className="install-form-label">{t('skills.targetAgents')}</label>
            <div className="install-targets">
              {AGENTS.map(a => (
                <div
                  key={a}
                  className={`install-target-chip ${selectedAgents.has(a) ? 'install-target-chip--selected' : ''}`}
                  onClick={() => toggleAgent(a)}
                >
                  {a}
                </div>
              ))}
            </div>
          </div>
        </div>
        <div className="skills-dialog__footer">
          <button className="skills-btn" onClick={onClose}>{t('skills.cancel')}</button>
          <button className="skills-btn skills-btn--primary" onClick={handleSave} disabled={!name.trim()}>
            {pack ? t('skills.save') : t('skills.create')}
          </button>
        </div>
      </div>
    </div>
  )
}
