import { useEffect, useState } from 'react'
import { useSkillStoreV2 } from '../../stores/skillStoreV2'
import { skillApiV2 } from '../../services/skillApiV2'
import type { SkillPackDetail, DistributionPreview } from '../../services/skillApiV2'
import { PreviewDialog } from './PreviewDialog'
import { AgentIconBadge } from './AgentIconBadge'

export function SkillPackPage() {
  const state = useSkillStoreV2()
  const [editing, setEditing] = useState(false)
  const [applyFor, setApplyFor] = useState<SkillPackDetail | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    state.loadOverview()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const detail = state.selectedPackDetail

  return (
    <div className="sm2 sm2--masterdetail">
      <div className="sm2__header">
        <h2 className="sm2__title">技能包</h2>
        <div className="sm2__tabs">
          <button className="sm2__btn sm2__btn--primary" onClick={() => setEditing(true)}>+ 新建技能包</button>
          <button className="sm2__btn" onClick={() => state.loadOverview()}>刷新</button>
        </div>
      </div>

      <div className="sm2__body sm2__body--masterdetail">
        <div className="sm2__rail settings-scroll">
          {state.packs.length === 0 ? (
            <div className="sm2__empty" style={{ padding: 12 }}>还没有技能包</div>
          ) : (
            state.packs.map((p) => (
              <div
                key={p.id}
                className={`sm2__rail-item${state.selectedPackId === p.id ? ' sm2__rail-item--active' : ''}`}
                onClick={() => state.selectPack(p.id)}
              >
                <div className="sm2__rail-item-main">
                  <div className="sm2__rail-item-title">{p.name}</div>
                  <div className="sm2__rail-item-sub">
                    {p.memberCount} 成员 · {p.appliedAgentCount} Agent
                  </div>
                </div>
                {p.healthy ? <span className="sm2__dot sm2__dot--ok" /> : <span className="sm2__dot sm2__dot--conflict" />}
              </div>
            ))
          )}
        </div>

        <div className="sm2__detailpane settings-scroll">
          {!detail ? (
            <div className="sm2__empty" style={{ padding: 40 }}>← 选择左侧一个技能包查看详情</div>
          ) : (
            <PackDetail
              detail={detail}
              onApply={() => setApplyFor(detail)}
              onEdit={() => setEditing(true)}
              onDelete={async () => {
                if (!confirm('删除该技能包？')) return
                setBusy(true)
                try {
                  await skillApiV2.deletePack(detail.id)
                  await state.selectPack(null)
                  await state.loadOverview()
                } catch (e) {
                  alert(String(e))
                } finally {
                  setBusy(false)
                }
              }}
              busy={busy}
            />
          )}
        </div>
      </div>

      {editing && (
        <PackBuilderDialog
          existing={state.selectedPackDetail}
          onClose={() => setEditing(false)}
          onSaved={async () => {
            setEditing(false)
            await state.loadOverview()
          }}
        />
      )}
      {applyFor && (
        <ApplyPackDialog
          pack={applyFor}
          agents={state.agents}
          defaultMode={state.settings?.defaultDistributeMode || 'link'}
          onClose={() => setApplyFor(null)}
          onDone={async () => {
            setApplyFor(null)
            await state.loadOverview()
          }}
        />
      )}
    </div>
  )
}

function PackDetail({
  detail,
  onApply,
  onEdit,
  onDelete,
  busy,
}: {
  detail: SkillPackDetail
  onApply: () => void
  onEdit: () => void
  onDelete: () => void
  busy: boolean
}) {
  return (
    <div className="sm2__agentdetail">
      <div className="sm2__packdetail-header">
        <div className="sm2__agentdetail-titles">
          <h3>{detail.name}</h3>
          <div className="sm2__detail-meta">{detail.description || '（无描述）'}</div>
        </div>
        <div className="sm2__btn-row" style={{ margin: 0 }}>
          <button className="sm2__btn sm2__btn--primary" onClick={onApply}>应用到 Agent</button>
          <button className="sm2__btn" onClick={onEdit}>编辑</button>
          <button className="sm2__btn sm2__btn--danger" disabled={busy} onClick={onDelete}>删除</button>
        </div>
      </div>

      <div className="sm2__packdetail-grid">
        <div className="sm2__detail-section">
          <h4>成员 Skills（{detail.members.length}）</h4>
          {detail.members.length === 0 ? (
            <div className="sm2__empty" style={{ padding: 8 }}>无成员</div>
          ) : (
            detail.members.map((m) => (
              <div key={m.skillId} className="sm2__pack-member">
                <span>{m.skillName}</span>
                {m.missing ? (
                  <span className="sm2__tag sm2__tag--conflict">缺失</span>
                ) : (
                  <span className="sm2__tag sm2__tag--ok">就绪</span>
                )}
              </div>
            ))
          )}
        </div>

        <div className="sm2__detail-section">
          <h4>已应用 Agent（{detail.appliedAgents.length}）</h4>
          {detail.appliedAgents.length === 0 ? (
            <div className="sm2__empty" style={{ padding: 8 }}>尚未应用</div>
          ) : (
            <div className="sm2__agents">
              {detail.appliedAgents.map((a, i) => (
                <AgentIconBadge key={i} iconKey={a.packName} title={a.packName} size={28} />
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

function PackBuilderDialog({
  existing,
  onClose,
  onSaved,
}: {
  existing: SkillPackDetail | null
  onClose: () => void
  onSaved: () => void
}) {
  const skills = useSkillStoreV2((s) => s.skills)
  const [name, setName] = useState(existing?.name || '')
  const [description, setDescription] = useState(existing?.description || '')
  const [selected, setSelected] = useState<Set<string>>(
    new Set(existing?.members.map((m) => m.skillId) || []),
  )
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const toggle = (id: string) =>
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })

  const save = async () => {
    if (!name.trim()) {
      setError('技能包名称必填')
      return
    }
    setBusy(true)
    setError(null)
    try {
      await skillApiV2.upsertPack({
        id: existing?.id || '',
        name,
        description,
        tags: [],
        skillIds: Array.from(selected),
      })
      onSaved()
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <PreviewDialog
      title={existing ? '编辑技能包' : '创建技能包'}
      confirmLabel="保存"
      busy={busy}
      onConfirm={save}
      onCancel={onClose}
    >
      <div className="sm2__field">
        <label>名称 *</label>
        <input value={name} onChange={(e) => setName(e.target.value)} placeholder="例如 Code Review 工具集" />
      </div>
      <div className="sm2__field">
        <label>描述</label>
        <textarea value={description} onChange={(e) => setDescription(e.target.value)} rows={2} />
      </div>
      <div className="sm2__field">
        <label>选择中心库 Skills</label>
        <div className="sm2__scroll">
          {skills.length === 0 ? (
            <div style={{ fontSize: 12, color: 'var(--text-secondary)' }}>中心库还没有 Skill</div>
          ) : (
            skills.map((s) => (
              <label key={s.id} className="sm2__checkbox-row">
                <input type="checkbox" checked={selected.has(s.id)} onChange={() => toggle(s.id)} />
                <span>{s.name}</span>
              </label>
            ))
          )}
        </div>
      </div>
      {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
    </PreviewDialog>
  )
}

function ApplyPackDialog({
  pack,
  agents,
  defaultMode,
  onClose,
  onDone,
}: {
  pack: SkillPackDetail
  agents: { id: string; displayName: string; iconKey: string; installed: boolean }[]
  defaultMode: 'link' | 'copy'
  onClose: () => void
  onDone: () => void
}) {
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [mode, setMode] = useState<'link' | 'copy'>(defaultMode)
  const [preview, setPreview] = useState<DistributionPreview | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const toggle = (id: string) =>
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })

  const runPreview = async () => {
    setBusy(true)
    setError(null)
    try {
      const p = await skillApiV2.previewApplyPack(pack.id, Array.from(selected), mode)
      setPreview(p)
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  const execute = async () => {
    setBusy(true)
    setError(null)
    try {
      await skillApiV2.executeApplyPack(pack.id, Array.from(selected), mode)
      onDone()
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  if (!preview) {
    return (
      <PreviewDialog
        title={`应用「${pack.name}」`}
        confirmLabel="预览影响"
        busy={busy}
        disabled={selected.size === 0}
        onConfirm={runPreview}
        onCancel={onClose}
      >
        <div className="sm2__field">
          <label>分发方式</label>
          <select value={mode} onChange={(e) => setMode(e.target.value as 'link' | 'copy')}>
            <option value="link">link</option>
            <option value="copy">copy</option>
          </select>
        </div>
        <label style={{ fontSize: 12, fontWeight: 600 }}>目标 Agent</label>
        <div className="sm2__scroll" style={{ marginTop: 6 }}>
          {agents.map((a) => (
            <label key={a.id} className="sm2__checkbox-row">
              <input type="checkbox" checked={selected.has(a.id)} onChange={() => toggle(a.id)} disabled={!a.installed} />
              <AgentIconBadge iconKey={a.iconKey} title={a.displayName} />
              <span>{a.displayName}</span>
            </label>
          ))}
        </div>
        {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
      </PreviewDialog>
    )
  }

  return (
    <PreviewDialog
      title="确认应用"
      confirmLabel="执行应用"
      destructive
      busy={busy}
      disabled={preview.blockers.length > 0}
      onConfirm={execute}
      onCancel={() => setPreview(null)}
    >
      {preview.changes.map((c, i) => (
        <div key={i} className="sm2__change">
          {c.action === 'create' ? `新增 ${c.agentId} (${c.actualMode})` : `复用 ${c.agentId}`}
        </div>
      ))}
      {preview.blockers.map((b, i) => (
        <div key={i} className="sm2__change sm2__change--blocked">
          阻止 {b.skillId}/{b.agentId}：{b.reason}
        </div>
      ))}
      {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
    </PreviewDialog>
  )
}
