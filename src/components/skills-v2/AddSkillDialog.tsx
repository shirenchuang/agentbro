import { useState } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import { skillApiV2 } from '../../services/skillApiV2'
import type { AddCenterSkillPreview, AddCenterSkillDecision } from '../../services/skillApiV2'
import { PreviewDialog } from './PreviewDialog'

export function AddSkillDialog({
  onClose,
  onDone,
}: {
  onClose: () => void
  onDone: () => void
}) {
  const [sourcePath, setSourcePath] = useState('')
  const [sourceType, setSourceType] = useState('local_folder')
  const [multi, setMulti] = useState(false)
  const [preview, setPreview] = useState<AddCenterSkillPreview | null>(null)
  const [renames, setRenames] = useState<Record<string, string>>({})
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const chooseFolder = async () => {
    const dir = await open({ directory: true, multiple: false })
    if (typeof dir === 'string') setSourcePath(dir)
  }

  const runPreview = async () => {
    if (!sourcePath) {
      setError('请先选择来源目录')
      return
    }
    setBusy(true)
    setError(null)
    try {
      const p = await skillApiV2.previewAddCenterSkill({
        sourcePath,
        sourceType,
        multi,
      })
      setPreview(p)
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  const execute = async () => {
    if (!preview) return
    setBusy(true)
    setError(null)
    try {
      const decisions: AddCenterSkillDecision[] = []
      for (const b of preview.blockers) {
        const renamed = renames[b.skillId]?.trim()
        if (renamed) {
          decisions.push({ skillId: b.skillId, proposedSkillId: renamed, resolution: 'create' })
        } else {
          decisions.push({ skillId: b.skillId, resolution: 'skip' })
        }
      }
      const result = await skillApiV2.executeAddCenterSkill(
        { sourcePath, sourceType, multi },
        decisions,
      )
      const msg =
        `导入完成：新增 ${result.skillIds.length}，更新 ${result.updated.length}，跳过 ${result.skipped.length}`
      onDone()
      onClose()
      // eslint-disable-next-line no-alert
      alert(msg)
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  if (!preview) {
    return (
      <PreviewDialog
        title="添加到中心库"
        confirmLabel="预览"
        busy={busy}
        disabled={!sourcePath}
        onConfirm={runPreview}
        onCancel={onClose}
      >
        <div className="sm2__field">
          <label>来源类型</label>
          <select value={sourceType} onChange={(e) => setSourceType(e.target.value)}>
            <option value="local_folder">本地文件夹</option>
            <option value="archive">压缩包</option>
            <option value="github">GitHub</option>
            <option value="agent_import">从 Agent 接管</option>
            <option value="manual_center">手动放入中心库</option>
          </select>
        </div>
        <div className="sm2__field">
          <label>来源目录</label>
          <div style={{ display: 'flex', gap: 8 }}>
            <input
              value={sourcePath}
              onChange={(e) => setSourcePath(e.target.value)}
              placeholder="选择包含 SKILL.md 的目录"
              style={{ flex: 1 }}
            />
            <button className="sm2__btn" type="button" onClick={chooseFolder}>
              选择
            </button>
          </div>
        </div>
        <label className="sm2__checkbox-row">
          <input type="checkbox" checked={multi} onChange={(e) => setMulti(e.target.checked)} />
          该目录包含多个 Skill（批量导入）
        </label>
        {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
        <p style={{ fontSize: 12, color: 'var(--text-secondary, #6e6e73)', marginTop: 8 }}>
          每个目录必须包含 SKILL.md。同名不同来源的 Skill 会被阻止并要求选择处理方式。
        </p>
      </PreviewDialog>
    )
  }

  return (
    <PreviewDialog
      title="确认导入预览"
      confirmLabel="执行导入"
      busy={busy}
      onConfirm={execute}
      onCancel={() => setPreview(null)}
    >
      {preview.candidates.length > 0 && (
        <>
          <h4 style={{ fontSize: 12, textTransform: 'uppercase', color: 'var(--text-secondary)' }}>将导入</h4>
          {preview.candidates.map((c) => (
            <div key={c.skillId} className="sm2__change">
              <strong>{c.name}</strong> → <code>{c.proposedSkillId}</code>{' '}
              <span className={`sm2__tag sm2__tag--${c.action === 'update' ? 'ok' : 'unmanaged'}`}>
                {c.action === 'update' ? '更新' : '新增'}
              </span>
            </div>
          ))}
        </>
      )}
      {preview.blockers.length > 0 && (
        <>
          <h4 style={{ fontSize: 12, textTransform: 'uppercase', color: '#ff3b30', marginTop: 12 }}>需要处理（同名不同来源）</h4>
          {preview.blockers.map((b) => (
            <div key={b.skillId} className="sm2__change sm2__change--blocked">
              <strong>{b.skillId}</strong>：{b.reason}
              <div className="sm2__field" style={{ marginTop: 6 }}>
                <label>重命名为（留空则跳过）</label>
                <input
                  value={renames[b.skillId] || ''}
                  onChange={(e) => setRenames({ ...renames, [b.skillId]: e.target.value })}
                  placeholder={`${b.skillId}-rename`}
                />
              </div>
            </div>
          ))}
        </>
      )}
      {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
    </PreviewDialog>
  )
}
