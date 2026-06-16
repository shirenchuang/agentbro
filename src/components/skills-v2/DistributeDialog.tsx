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
  const installedAgents = agents.filter((agent) => agent.installed)
  const agentNameById = new Map(agents.map((agent) => [agent.id, agent.displayName]))

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
        modalClassName="sm2__modal--distribute"
        busy={busy}
        disabled={selected.size === 0}
        onConfirm={runPreview}
        onCancel={onClose}
      >
        <div className="sm2-distribute">
          <div className="sm2-distribute__summary">
            <div className="sm2-distribute__flow" aria-hidden="true">
              <span>SK</span>
              <b>→</b>
              <span>AG</span>
            </div>
            <div className="sm2-distribute__summary-copy">
              <strong>{skill.name}</strong>
              <span>从中心库分发到已安装 Hook 的 Agent</span>
            </div>
            <div className="sm2-distribute__count">
              {selected.size}/{installedAgents.length}
            </div>
          </div>

          <section className="sm2-distribute__section">
            <div className="sm2-distribute__section-head">
              <span>分发方式</span>
              <em>{mode === 'link' ? '随中心库更新' : '生成独立副本'}</em>
            </div>
            <div className="sm2-distribute__mode-grid">
              <label className={`sm2-distribute__mode${mode === 'link' ? ' sm2-distribute__mode--active' : ''}`}>
                <input
                  type="radio"
                  name="distribute-mode"
                  value="link"
                  checked={mode === 'link'}
                  onChange={() => setMode('link')}
                />
                <span className="sm2-distribute__mode-key">link</span>
                <span className="sm2-distribute__mode-title">软链接</span>
                <span className="sm2-distribute__mode-note">中心库更新后自动生效</span>
              </label>
              <label className={`sm2-distribute__mode${mode === 'copy' ? ' sm2-distribute__mode--active' : ''}`}>
                <input
                  type="radio"
                  name="distribute-mode"
                  value="copy"
                  checked={mode === 'copy'}
                  onChange={() => setMode('copy')}
                />
                <span className="sm2-distribute__mode-key">copy</span>
                <span className="sm2-distribute__mode-title">复制副本</span>
                <span className="sm2-distribute__mode-note">适合需要单独修改的 Agent</span>
              </label>
            </div>
          </section>

          <section className="sm2-distribute__section">
            <div className="sm2-distribute__section-head">
              <span>目标 Agent</span>
              <em>{installedAgents.length} 个可选</em>
            </div>
            <div className="sm2-distribute__agent-list">
              {agents.length === 0 ? (
                <div className="sm2__empty sm2__empty--compact">没有可用的 Agent。请先在 Agent 管理中安装 Hook。</div>
              ) : (
                agents.map((a) => {
                  const checked = selected.has(a.id)
                  return (
                    <label
                      key={a.id}
                      className={`sm2-distribute__agent${checked ? ' sm2-distribute__agent--active' : ''}${!a.installed ? ' sm2-distribute__agent--disabled' : ''}`}
                    >
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={() => toggle(a.id)}
                        disabled={!a.installed}
                      />
                      <span className="sm2-distribute__check" aria-hidden="true">{checked ? '✓' : ''}</span>
                      <AgentIconBadge iconKey={a.iconKey} title={a.displayName} size={28} />
                      <span className="sm2-distribute__agent-main">
                        <strong>{a.displayName}</strong>
                        <span>{a.installed ? 'Hook 已安装' : '未安装 Hook'}</span>
                      </span>
                    </label>
                  )
                })
              )}
            </div>
          </section>

          {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
        </div>
      </PreviewDialog>
    )
  }

  return (
    <PreviewDialog
      title="确认分发"
      confirmLabel="执行分发"
      modalClassName="sm2__modal--distribute"
      busy={busy}
      disabled={preview.blockers.length > 0}
      onConfirm={execute}
      onCancel={() => setPreview(null)}
    >
      <div className="sm2-distribute sm2-distribute--preview">
        <div className="sm2-distribute__summary">
          <div className="sm2-distribute__flow" aria-hidden="true">
            <span>{preview.requestedMode}</span>
            <b>→</b>
            <span>{preview.targetAgents.length}</span>
          </div>
          <div className="sm2-distribute__summary-copy">
            <strong>将影响 {preview.changes.length} 个目标</strong>
            <span>{preview.blockers.length > 0 ? '存在阻止项，暂不能执行' : '检查无阻止项，可以执行分发'}</span>
          </div>
        </div>

        <div className="sm2-distribute__preview-list">
          {preview.changes.map((c, i) => (
            <div key={i} className="sm2-distribute__change">
              <div className="sm2-distribute__change-main">
                <span className="sm2-distribute__change-action">
                  {c.action === 'create' ? '新增' : c.action === 'reuse' ? '复用' : c.action}
                </span>
                <strong>{agentNameById.get(c.agentId) ?? c.agentId}</strong>
                {c.action === 'create' && <span className="sm2__tag sm2__tag--ok">{c.actualMode}</span>}
              </div>
              <code>{c.targetPath}</code>
              {c.action !== 'create' && c.reason && <span className="sm2-distribute__change-reason">{c.reason}</span>}
            </div>
          ))}
          {preview.blockers.map((b, i) => (
            <div key={i} className="sm2-distribute__change sm2-distribute__change--blocked">
              <div className="sm2-distribute__change-main">
                <span className="sm2-distribute__change-action">阻止</span>
                <strong>{agentNameById.get(b.agentId) ?? b.agentId}</strong>
              </div>
              <span className="sm2-distribute__change-reason">{b.reason}</span>
            </div>
          ))}
        </div>

        {preview.blockers.length > 0 && (
          <p className="sm2-distribute__blocked-note">
            请先在诊断或 Agent 管理中接管同名未管理 Skill。
          </p>
        )}
        {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
      </div>
    </PreviewDialog>
  )
}
