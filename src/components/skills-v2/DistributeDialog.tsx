import { useEffect, useState } from 'react'
import { skillApiV2 } from '../../services/skillApiV2'
import type { DistributionPreview, AgentSummary, SkillSummary } from '../../services/skillApiV2'
import { PreviewDialog } from './PreviewDialog'
import { AgentIconBadge } from './AgentIconBadge'

export function DistributeDialog({
  skill,
  agents,
  defaultMode,
  onClose,
  onDone,
}: {
  skill: SkillSummary
  agents: AgentSummary[]
  defaultMode: 'link' | 'copy'
  onClose: () => void
  onDone: () => void
}) {
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [mode, setMode] = useState<'link' | 'copy'>(defaultMode)
  const [preview, setPreview] = useState<DistributionPreview | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    setMode(defaultMode)
  }, [defaultMode])

  const toggle = (id: string) => {
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const runPreview = async () => {
    if (selected.size === 0) {
      setError('请至少选择一个 Agent')
      return
    }
    setBusy(true)
    setError(null)
    try {
      const p = await skillApiV2.previewDistribute([skill.id], Array.from(selected), mode)
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
      await skillApiV2.executeDistribute(preview)
      onDone()
      onClose()
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  if (!preview) {
    return (
      <PreviewDialog
        title={`分发「${skill.name}」`}
        confirmLabel="预览影响"
        busy={busy}
        disabled={selected.size === 0}
        onConfirm={runPreview}
        onCancel={onClose}
      >
        <div className="sm2__field">
          <label>分发方式</label>
          <select value={mode} onChange={(e) => setMode(e.target.value as 'link' | 'copy')}>
            <option value="link">link（软链接，中心库更新即生效）</option>
            <option value="copy">copy（复制副本，需显式同步）</option>
          </select>
        </div>
        <label style={{ fontSize: 12, fontWeight: 600 }}>目标 Agent</label>
        <div className="sm2__scroll" style={{ marginTop: 6 }}>
          {agents.map((a) => (
            <label key={a.id} className="sm2__checkbox-row">
              <input
                type="checkbox"
                checked={selected.has(a.id)}
                onChange={() => toggle(a.id)}
                disabled={!a.installed}
              />
              <AgentIconBadge iconKey={a.iconKey} title={a.displayName} />
              <span>{a.displayName}</span>
              {!a.installed && <span style={{ fontSize: 11, color: '#8e8e93' }}>（未安装）</span>}
            </label>
          ))}
        </div>
        {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
      </PreviewDialog>
    )
  }

  return (
    <PreviewDialog
      title="确认分发"
      confirmLabel="执行分发"
      destructive
      busy={busy}
      disabled={preview.blockers.length > 0}
      onConfirm={execute}
      onCancel={() => setPreview(null)}
    >
      {preview.changes.map((c, i) => (
        <div key={i} className="sm2__change">
          {c.action === 'create' ? (
            <>
              <strong>新增</strong> {c.agentId} → 实际模式{' '}
              <span className="sm2__tag sm2__tag--ok">{c.actualMode}</span>
            </>
          ) : c.action === 'reuse' ? (
            <>
              <strong>复用</strong> {c.agentId}（追加 direct claim）
            </>
          ) : (
            <>
              {c.action}: {c.reason}
            </>
          )}
          <div style={{ fontSize: 11, color: 'var(--text-secondary)' }}>{c.targetPath}</div>
        </div>
      ))}
      {preview.blockers.map((b, i) => (
        <div key={i} className="sm2__change sm2__change--blocked">
          <strong>阻止</strong> {b.skillId} / {b.agentId}：{b.reason}
        </div>
      ))}
      {preview.blockers.length > 0 && (
        <p style={{ fontSize: 12, color: '#ff3b30', marginTop: 8 }}>
          存在阻止项，无法执行。请先在诊断或 Agent 管理中接管同名未管理 Skill。
        </p>
      )}
      {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
    </PreviewDialog>
  )
}
