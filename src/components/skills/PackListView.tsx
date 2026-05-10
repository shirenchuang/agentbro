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
    <div>
      <div className="pack-list">
        {packs.map(p => (
          <PackCard key={p.id} pack={p} onEdit={handleEdit} onRefresh={loadAll} />
        ))}
        <div className="pack-create-card" onClick={handleCreate}>
          + {t('skills.createPack')}
        </div>
      </div>

      {dialogOpen && (
        <PackDialog pack={editingPack} onClose={handleClose} />
      )}
    </div>
  )
}
