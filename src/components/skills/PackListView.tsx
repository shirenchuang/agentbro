import { useState, useCallback } from 'react'
import { useTranslation } from 'react-i18next'
import { useSkillStore } from '../../stores/skillStore'
import type { SkillPack } from '../../services/skillApi'
import { PackCard } from './PackCard'
import { PackDialog } from './PackDialog'

export function PackListView() {
  const { t } = useTranslation()
  const { packs, loadAll } = useSkillStore()
  const [dialogOpen, setDialogOpen] = useState(false)
  const [editingPack, setEditingPack] = useState<SkillPack | undefined>()

  const handleEdit = useCallback((pack: SkillPack) => {
    setEditingPack(pack)
    setDialogOpen(true)
  }, [])

  const handleCreate = useCallback(() => {
    setEditingPack(undefined)
    setDialogOpen(true)
  }, [])

  const handleClose = useCallback(() => {
    setDialogOpen(false)
    setEditingPack(undefined)
  }, [])

  return (
    <div className="capability-page">
      <div className="capability-page-head">
        <h1>📦 技能包</h1>
        <p>按场景组织一组 Skills，并快速应用到目标 Agent。</p>
      </div>

      <div className="capability-page-body">
        <div className="plugin-manager-stats">
          <div><strong>{packs.length}</strong><span>技能包</span></div>
          <div><strong>{packs.reduce((total, pack) => total + pack.skills.length, 0)}</strong><span>Skills</span></div>
          <div><strong>{new Set(packs.flatMap((pack) => pack.targetAgents)).size}</strong><span>目标 Agent</span></div>
          <div><strong>{packs.filter((pack) => pack.targetAgents.length > 0).length}</strong><span>可应用</span></div>
        </div>

        <div className="plugin-manager-toolbar">
          <div className="capability-toolbar-spacer" />
          <button className="skills-btn skills-btn--primary" onClick={handleCreate}>
            + 新建技能包
          </button>
        </div>

        <div className="pack-list">
          {packs.map(p => (
            <PackCard key={p.id} pack={p} onEdit={handleEdit} onRefresh={loadAll} />
          ))}
          <button type="button" className="pack-create-card" onClick={handleCreate}>
            + {t('skills.createPack')}
          </button>
        </div>

        {dialogOpen && (
          <PackDialog pack={editingPack} onClose={handleClose} />
        )}
      </div>
    </div>
  )
}
