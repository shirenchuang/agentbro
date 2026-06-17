import { useEffect, useMemo, useState } from 'react'
import { createPortal } from 'react-dom'
import { useTranslation } from 'react-i18next'
import { useSkillStoreV2 } from '../../stores/skillStoreV2'
import { skillApiV2 } from '../../services/skillApiV2'
import type {
  DeleteSkillPackPreview,
  DistributionPreview,
  RemovePackFromAgentPreview,
  RemoveSkillFromPackPreview,
  SkillPackDetail,
  SkillPackSummary,
} from '../../services/skillApiV2'
import { PreviewDialog } from './PreviewDialog'
import { AgentIconBadge } from './AgentIconBadge'
import { skillModeLabel, skillSourceTypeLabel } from './skillLabels'

type BuilderMode = 'create' | 'edit' | 'duplicate'

export function SkillPackPage() {
  const state = useSkillStoreV2()
  const [packQuery, setPackQuery] = useState('')
  const [builderMode, setBuilderMode] = useState<BuilderMode | null>(null)
  const [applyFor, setApplyFor] = useState<SkillPackDetail | null>(null)
  const [deletePreview, setDeletePreview] = useState<DeleteSkillPackPreview | null>(null)
  const [removeSkillPreview, setRemoveSkillPreview] = useState<RemoveSkillFromPackPreview | null>(null)
  const [revokePreview, setRevokePreview] = useState<RemovePackFromAgentPreview | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    state.loadOverview()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    if (state.selectedPackId || builderMode || state.packs.length === 0) return
    state.selectPack(state.packs[0].id)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state.packs, state.selectedPackId, builderMode])

  const filteredPacks = useMemo(() => {
    const q = packQuery.trim().toLowerCase()
    if (!q) return state.packs
    return state.packs.filter((pack) =>
      [pack.name, pack.description, pack.tags.join(' ')]
        .join(' ')
        .toLowerCase()
        .includes(q),
    )
  }, [packQuery, state.packs])

  const detail = state.selectedPackDetail
  const missingCount = detail?.members.filter((member) => member.missing).length || 0
  const totalMembers = state.packs.reduce((sum, pack) => sum + pack.memberCount, 0)
  const totalApplied = state.packs.reduce((sum, pack) => sum + pack.appliedAgentCount, 0)
  const unhealthyPacks = state.packs.filter((pack) => !pack.healthy).length

  const startCreate = () => {
    state.selectPack(null)
    setBuilderMode('create')
  }

  const startEdit = () => setBuilderMode('edit')
  const startDuplicate = () => setBuilderMode('duplicate')

  const openDelete = async (packId: string) => {
    setBusy(true)
    try {
      setDeletePreview(await skillApiV2.previewDeletePack(packId))
    } catch (e) {
      state.setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  const openRemoveSkill = async (packId: string, skillId: string) => {
    setBusy(true)
    try {
      setRemoveSkillPreview(await skillApiV2.previewRemoveSkillFromPack(packId, skillId))
    } catch (e) {
      state.setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  const openRevoke = async (packId: string, agentId: string) => {
    setBusy(true)
    try {
      setRevokePreview(await skillApiV2.previewRemovePackFromAgent(packId, agentId))
    } catch (e) {
      state.setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="sm2 sm2--packs sm2--packs-redesign">
      <div className="sm2__header sm2__header--stacked">
        <div>
          <h2 className="sm2__title">技能包</h2>
          <p className="sm2__header-subtitle">管理一组中心库 Skill ID，并通过 claims 安全应用或撤销到 Agent。</p>
        </div>
        <div className="sm2__tabs">
          <button className="sm2__btn sm2__btn--primary" onClick={startCreate}>新建技能包</button>
          <button className="sm2__btn" onClick={() => state.loadOverview(true)} disabled={state.loading || busy}>
            {state.loading ? '刷新中…' : '刷新'}
          </button>
        </div>
      </div>

      {state.error && <div className="sm2__error">{state.error}</div>}

      <div className="sm2__pack-dashboard">
        <PackMetric label="技能包" value={state.packs.length} />
        <PackMetric label="成员引用" value={totalMembers} />
        <PackMetric label="Agent 应用" value={totalApplied} />
        <PackMetric label="需要处理" value={unhealthyPacks + missingCount} tone={unhealthyPacks + missingCount > 0 ? 'warn' : 'ok'} />
      </div>

      <div className="sm2__pack-layout">
        <aside className="sm2__pack-sidebar settings-scroll">
          <div className="sm2__pack-sidebar-head">
            <strong>全部技能包</strong>
            <span>{filteredPacks.length}</span>
          </div>
          <input
            className="sm2__search sm2__search--full"
            value={packQuery}
            onChange={(e) => setPackQuery(e.target.value)}
            placeholder="搜索名称、描述或标签"
          />

          {state.packs.length === 0 ? (
            <div className="sm2__pack-empty-state">
              <strong>还没有技能包</strong>
              <span>从中心库挑选 Skills，保存成可复用组合。</span>
              <button className="sm2__btn sm2__btn--primary" onClick={startCreate}>创建第一个</button>
            </div>
          ) : filteredPacks.length === 0 ? (
            <div className="sm2__empty sm2__empty--compact">没有匹配的技能包</div>
          ) : (
            <div className="sm2__pack-list">
              {filteredPacks.map((pack) => (
                <PackListItem
                  key={pack.id}
                  pack={pack}
                  active={state.selectedPackId === pack.id && !builderMode}
                  onClick={() => {
                    setBuilderMode(null)
                    state.selectPack(pack.id)
                  }}
                />
              ))}
            </div>
          )}
        </aside>

        <main className="sm2__pack-canvas settings-scroll">
          {!detail ? (
            <PackLanding onCreate={startCreate} />
          ) : (
            <PackDetail
              detail={detail}
              busy={busy}
              onApply={() => setApplyFor(detail)}
              onEdit={startEdit}
              onDuplicate={startDuplicate}
              onDelete={() => openDelete(detail.id)}
              onRemoveSkill={(skillId) => openRemoveSkill(detail.id, skillId)}
              onRevoke={(agentId) => openRevoke(detail.id, agentId)}
            />
          )}
        </main>
      </div>

      {builderMode && (
        <PackBuilderDialog
          mode={builderMode}
          existing={builderMode === 'create' ? null : detail}
          onCancel={() => setBuilderMode(null)}
          onSaved={async (packId) => {
            setBuilderMode(null)
            await state.loadOverview(true)
            await state.selectPack(packId)
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
            const packId = applyFor.id
            setApplyFor(null)
            await state.loadOverview(true)
            await state.selectPack(packId)
          }}
        />
      )}
      {deletePreview && (
        <DeletePackDialog
          preview={deletePreview}
          busy={busy}
          onClose={() => setDeletePreview(null)}
          onDelete={async () => {
            setBusy(true)
            try {
              await skillApiV2.deletePack(deletePreview.packId)
              setDeletePreview(null)
              await state.selectPack(null)
              await state.loadOverview(true)
            } catch (e) {
              state.setError(String(e))
            } finally {
              setBusy(false)
            }
          }}
        />
      )}
      {removeSkillPreview && (
        <RemoveSkillFromPackDialog
          preview={removeSkillPreview}
          busy={busy}
          onClose={() => setRemoveSkillPreview(null)}
          onExecute={async (alsoRemoveTargets) => {
            setBusy(true)
            try {
              await skillApiV2.removeSkillFromPack(removeSkillPreview.packId, removeSkillPreview.skillId, alsoRemoveTargets)
              const packId = removeSkillPreview.packId
              setRemoveSkillPreview(null)
              await state.selectPack(packId)
              await state.loadOverview(true)
            } catch (e) {
              state.setError(String(e))
            } finally {
              setBusy(false)
            }
          }}
        />
      )}
      {revokePreview && (
        <RevokePackDialog
          preview={revokePreview}
          busy={busy}
          onClose={() => setRevokePreview(null)}
          onExecute={async () => {
            setBusy(true)
            try {
              await skillApiV2.removePackFromAgent(revokePreview.packId, revokePreview.agentId)
              const packId = revokePreview.packId
              setRevokePreview(null)
              await state.selectPack(packId)
              await state.loadOverview(true)
            } catch (e) {
              state.setError(String(e))
            } finally {
              setBusy(false)
            }
          }}
        />
      )}
    </div>
  )
}

function PackBuilderDialog({
  mode,
  existing,
  onCancel,
  onSaved,
}: {
  mode: BuilderMode
  existing: SkillPackDetail | null
  onCancel: () => void
  onSaved: (packId: string) => void
}) {
  if (typeof document === 'undefined') return null

  const title = mode === 'edit' ? '编辑技能包' : mode === 'duplicate' ? '复制技能包' : '创建技能包'

  return createPortal(
    <div className="sm2__overlay sm2__pack-builder-overlay" onClick={onCancel}>
      <div
        className="sm2__modal sm2__pack-builder-modal"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onClick={(e) => e.stopPropagation()}
      >
        <PackBuilderPanel mode={mode} existing={existing} onCancel={onCancel} onSaved={onSaved} />
      </div>
    </div>,
    document.body,
  )
}

function PackMetric({ label, value, tone }: { label: string; value: number; tone?: 'ok' | 'warn' }) {
  return (
    <div className={`sm2__pack-metric${tone ? ` sm2__pack-metric--${tone}` : ''}`}>
      <strong>{value}</strong>
      <span>{label}</span>
    </div>
  )
}

function PackListItem({
  pack,
  active,
  onClick,
}: {
  pack: SkillPackSummary
  active: boolean
  onClick: () => void
}) {
  return (
    <button className={`sm2__pack-list-item${active ? ' sm2__pack-list-item--active' : ''}`} onClick={onClick}>
      <span className="sm2__pack-list-top">
        <strong>{pack.name}</strong>
        <span className={`sm2__pack-health ${pack.healthy ? 'sm2__pack-health--ok' : 'sm2__pack-health--warn'}`}>
          {pack.healthy ? 'OK' : '缺失'}
        </span>
      </span>
      <span className="sm2__pack-list-desc">{pack.description || '无描述'}</span>
      <span className="sm2__pack-list-meta">
        <span>{pack.memberCount} Skills</span>
        <span>{pack.appliedAgentCount} Agents</span>
      </span>
    </button>
  )
}

function PackLanding({ onCreate }: { onCreate: () => void }) {
  return (
    <div className="sm2__pack-landing">
      <div className="sm2__pack-landing-mark">PACK</div>
      <strong>选择一个技能包</strong>
      <span>右侧会显示成员、应用 Agent、claim 影响和可执行操作。</span>
      <button className="sm2__btn sm2__btn--primary" onClick={onCreate}>新建技能包</button>
    </div>
  )
}

function PackDetail({
  detail,
  busy,
  onApply,
  onEdit,
  onDuplicate,
  onDelete,
  onRemoveSkill,
  onRevoke,
}: {
  detail: SkillPackDetail
  busy: boolean
  onApply: () => void
  onEdit: () => void
  onDuplicate: () => void
  onDelete: () => void
  onRemoveSkill: (skillId: string) => void
  onRevoke: (agentId: string) => void
}) {
  const missing = detail.members.filter((member) => member.missing)
  return (
    <div className="sm2__pack-workbench">
      <header className="sm2__pack-detail-hero">
        <div className="sm2__pack-detail-main">
          <div className="sm2__pack-kicker">Skill Pack</div>
          <h3>{detail.name}</h3>
          <p>{detail.description || '这个技能包还没有描述。'}</p>
          <div className="sm2__pack-tagline">
            {detail.tags.length > 0 ? detail.tags.map((tag) => <span key={tag}>{tag}</span>) : <span>未设置标签</span>}
          </div>
        </div>
        <div className="sm2__pack-actions">
          <button className="sm2__btn sm2__btn--primary" onClick={onApply} disabled={busy || detail.members.length === 0 || missing.length > 0}>
            应用
          </button>
          <button className="sm2__btn" onClick={onEdit} disabled={busy}>编辑</button>
          <button className="sm2__btn" onClick={onDuplicate} disabled={busy}>复制</button>
          <button className="sm2__btn sm2__btn--danger" onClick={onDelete} disabled={busy}>删除</button>
        </div>
      </header>

      {missing.length > 0 && (
        <div className="sm2__notice sm2__notice--warn">
          {missing.length} 个成员已不在中心库中。应用前需要先恢复或从技能包移除。
        </div>
      )}

      <div className="sm2__pack-summary-grid">
        <PackMetric label="成员 Skills" value={detail.members.length} />
        <PackMetric label="已应用 Agent" value={detail.appliedAgents.length} />
        <PackMetric label="缺失成员" value={missing.length} tone={missing.length > 0 ? 'warn' : 'ok'} />
      </div>

      <div className="sm2__pack-sections">
        <section className="sm2__pack-section sm2__pack-section--members">
          <div className="sm2__pack-section-head">
            <div>
              <h3>成员 Skills</h3>
              <span>技能包只保存这些中心库 Skill ID。</span>
            </div>
            <span>{detail.members.length}</span>
          </div>
          {detail.members.length === 0 ? (
            <div className="sm2__empty sm2__empty--compact">无成员</div>
          ) : (
            <div className="sm2__pack-member-list">
              {detail.members.map((member, index) => (
                <div key={member.skillId} className={`sm2__pack-member-row${member.missing ? ' sm2__pack-member-row--missing' : ''}`}>
                  <div className="sm2__pack-member-index">{index + 1}</div>
                  <div className="sm2__pack-member-body">
                    <strong>{member.skillName}</strong>
                    <span>{member.skillId}</span>
                  </div>
                  <div className="sm2__pack-member-state">
                    <span className={`sm2__tag sm2__tag--${member.missing ? 'conflict' : 'ok'}`}>
                      {member.missing ? '缺失' : '就绪'}
                    </span>
                    <button className="sm2__btn sm2__btn--ghost" disabled={busy} onClick={() => onRemoveSkill(member.skillId)}>
                      移除
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </section>

        <section className="sm2__pack-section">
          <div className="sm2__pack-section-head">
            <div>
              <h3>已应用 Agent</h3>
              <span>撤销时只移除该技能包 claim。</span>
            </div>
            <span>{detail.appliedAgents.length}</span>
          </div>
          {detail.appliedAgents.length === 0 ? (
            <div className="sm2__empty sm2__empty--compact">尚未应用到任何 Agent</div>
          ) : (
            <div className="sm2__pack-agent-list">
              {detail.appliedAgents.map((agent, index) => (
                <div key={agent.agentId || index} className="sm2__pack-agent-row">
                  <AgentIconBadge iconKey={agent.iconKey || agent.agentId || agent.packName} title={agent.displayName || agent.agentId || agent.packName} size={30} />
                  <div>
                    <strong>{agent.displayName || agent.agentId || agent.packName}</strong>
                    <span>{agent.memberCount} member claims</span>
                  </div>
                  {agent.agentId && (
                    <button className="sm2__btn sm2__btn--danger" disabled={busy} onClick={() => onRevoke(agent.agentId!)}>
                      撤销
                    </button>
                  )}
                </div>
              ))}
            </div>
          )}
        </section>
      </div>
    </div>
  )
}

function PackBuilderPanel({
  mode,
  existing,
  onCancel,
  onSaved,
}: {
  mode: BuilderMode
  existing: SkillPackDetail | null
  onCancel: () => void
  onSaved: (packId: string) => void
}) {
  const skills = useSkillStoreV2((s) => s.skills)
  const { t } = useTranslation()
  const [name, setName] = useState(mode === 'duplicate' && existing ? `${existing.name} Copy` : existing?.name || '')
  const [description, setDescription] = useState(existing?.description || '')
  const [tagsText, setTagsText] = useState(existing?.tags.join(', ') || '')
  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState<Set<string>>(new Set(existing?.members.map((member) => member.skillId) || []))
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const existingMemberIds = useMemo(() => new Set(existing?.members.map((member) => member.skillId) || []), [existing])
  const existingApplied = mode === 'edit' && (existing?.appliedAgents.length || 0) > 0
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return skills
    return skills.filter((skill) =>
      [skill.name, skill.id, skill.description, skill.sourceType, skillSourceTypeLabel(t, skill.sourceType)]
        .join(' ')
        .toLowerCase()
        .includes(q),
    )
  }, [query, skills, t])
  const selectedSkills = skills.filter((skill) => selected.has(skill.id))

  const toggle = (id: string) => {
    if (existingApplied && existingMemberIds.has(id) && selected.has(id)) {
      setError('已应用技能包中的成员需要通过详情页「移除」预览影响范围。')
      return
    }
    setError(null)
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const save = async () => {
    if (!name.trim()) {
      setError('技能包名称必填。')
      return
    }
    if (existingApplied) {
      const removed = Array.from(existingMemberIds).filter((id) => !selected.has(id))
      if (removed.length > 0) {
        setError('已应用技能包不能直接移除成员，请回到详情页使用「移除」。')
        return
      }
    }
    const skillIds = Array.from(selected)
    const tags = tagsText
      .split(',')
      .map((tag) => tag.trim())
      .filter(Boolean)
    setBusy(true)
    setError(null)
    try {
      const saved = await skillApiV2.upsertPack({
        id: mode === 'edit' ? existing?.id || '' : '',
        name: name.trim(),
        description: description.trim(),
        tags,
        skillIds,
      })
      onSaved(saved.id)
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="sm2__pack-builder2">
      <header className="sm2__pack-builder-head">
        <div>
          <div className="sm2__pack-kicker">{mode === 'edit' ? 'Edit Pack' : mode === 'duplicate' ? 'Duplicate Pack' : 'New Pack'}</div>
          <h3>{mode === 'edit' ? '编辑技能包' : mode === 'duplicate' ? '复制技能包' : '创建技能包'}</h3>
          <p>选择中心库 Skills。保存的只是 Skill ID，不绑定 Agent 或分发方式。</p>
        </div>
        <div className="sm2__pack-actions">
          <button className="sm2__btn" onClick={onCancel} disabled={busy}>取消</button>
          <button className="sm2__btn sm2__btn--primary" onClick={save} disabled={busy}>
            {busy ? '保存中…' : '保存'}
          </button>
        </div>
      </header>

      <div className="sm2__pack-builder2-grid">
        <section className="sm2__pack-section sm2__pack-section--selected">
          <div className="sm2__pack-section-head">
            <div>
              <h3>基本信息</h3>
              <span>名称、用途和标签。</span>
            </div>
          </div>
          <div className="sm2__builder-form2">
            <label className="sm2__field">
              <span>名称 *</span>
              <input value={name} onChange={(e) => setName(e.target.value)} placeholder="Code Review 工具集" />
            </label>
            <label className="sm2__field">
              <span>描述</span>
              <textarea value={description} onChange={(e) => setDescription(e.target.value)} rows={4} placeholder="这个技能包适合什么场景" />
            </label>
            <label className="sm2__field">
              <span>标签</span>
              <input value={tagsText} onChange={(e) => setTagsText(e.target.value)} placeholder="review, frontend, release" />
            </label>
          </div>
        </section>

        <section className="sm2__pack-section sm2__pack-section--picker">
          <div className="sm2__pack-section-head">
            <div>
              <h3>中心库 Skills</h3>
              <span>{filtered.length} 个可选。</span>
            </div>
            <span>{selected.size} 已选</span>
          </div>
          <input className="sm2__search sm2__search--full" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="搜索 Skill" />
          <div className="sm2__skill-picker2">
            {filtered.length === 0 ? (
              <div className="sm2__empty sm2__empty--compact">没有匹配的 Skill</div>
            ) : (
              filtered.map((skill) => (
                <button key={skill.id} className={`sm2__skill-pick${selected.has(skill.id) ? ' sm2__skill-pick--selected' : ''}`} onClick={() => toggle(skill.id)}>
                  <span className="sm2__skill-pick-check">{selected.has(skill.id) ? '✓' : ''}</span>
                  <span className="sm2__skill-pick-body">
                    <strong>{skill.name}</strong>
                    <span>{skill.description || skill.id}</span>
                  </span>
                  <span className="sm2__tag">{skillSourceTypeLabel(t, skill.sourceType)}</span>
                </button>
              ))
            )}
          </div>
        </section>

        <section className="sm2__pack-section">
          <div className="sm2__pack-section-head">
            <div>
              <h3>已选择</h3>
              <span>保存后的成员顺序。</span>
            </div>
            <span>{selectedSkills.length}</span>
          </div>
          {selectedSkills.length === 0 ? (
            <div className="sm2__empty sm2__empty--compact">还没有选择 Skill</div>
          ) : (
            <div className="sm2__pack-member-list">
              {selectedSkills.map((skill, index) => (
                <div key={skill.id} className="sm2__pack-member-row">
                  <div className="sm2__pack-member-index">{index + 1}</div>
                  <div className="sm2__pack-member-body">
                    <strong>{skill.name}</strong>
                    <span>{skill.id}</span>
                  </div>
                  <button className="sm2__btn sm2__btn--ghost" onClick={() => toggle(skill.id)}>移除</button>
                </div>
              ))}
            </div>
          )}
        </section>
      </div>
      {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
    </div>
  )
}

function DeletePackDialog({
  preview,
  busy,
  onClose,
  onDelete,
}: {
  preview: DeleteSkillPackPreview
  busy: boolean
  onClose: () => void
  onDelete: () => void
}) {
  return (
    <PreviewDialog
      title={`删除技能包「${preview.packName}」`}
      confirmLabel="删除"
      destructive
      busy={busy}
      disabled={!preview.removable}
      onConfirm={onDelete}
      onCancel={onClose}
    >
      {preview.warnings.map((warning, index) => (
        <div key={index} className="sm2__change sm2__change--blocked">{warning}</div>
      ))}
      {preview.appliedAgents.length > 0 && (
        <div className="sm2__detail-meta">已应用 Agent：{preview.appliedAgents.join('、')}</div>
      )}
      {preview.affectedTargets.length > 0 && (
        <div className="sm2__scroll" style={{ marginTop: 8 }}>
          {preview.affectedTargets.map((target) => (
            <div key={target.targetId} className="sm2__target-row">
              <span>{target.displayName}</span>
              <code>{target.targetPath}</code>
            </div>
          ))}
        </div>
      )}
    </PreviewDialog>
  )
}

function RemoveSkillFromPackDialog({
  preview,
  busy,
  onClose,
  onExecute,
}: {
  preview: RemoveSkillFromPackPreview
  busy: boolean
  onClose: () => void
  onExecute: (alsoRemoveTargets: boolean) => void
}) {
  return (
    <PreviewDialog
      title={`从「${preview.packName}」移除「${preview.skillName}」`}
      confirmLabel="保留为直接分发"
      busy={busy}
      onConfirm={() => onExecute(false)}
      onCancel={onClose}
    >
      <div className="sm2__detail-meta">
        影响 {preview.appliedAgentCount} 个 Agent，{preview.affectedTargets.length} 个安装目标。
      </div>
      {preview.affectedTargets.length > 0 && (
        <div className="sm2__scroll" style={{ marginTop: 8 }}>
          {preview.affectedTargets.map((target) => (
            <div key={target.targetId} className="sm2__target-row">
              <span>{target.displayName} · {target.mode} · {target.claimCount} claim(s)</span>
              <code>{target.targetPath}</code>
            </div>
          ))}
        </div>
      )}
      <div className="sm2__btn-row">
        <button className="sm2__btn sm2__btn--danger" disabled={busy} onClick={() => onExecute(true)}>
          同步从 Agent 移除
        </button>
      </div>
    </PreviewDialog>
  )
}

function RevokePackDialog({
  preview,
  busy,
  onClose,
  onExecute,
}: {
  preview: RemovePackFromAgentPreview
  busy: boolean
  onClose: () => void
  onExecute: () => void
}) {
  return (
    <PreviewDialog
      title={`从 ${preview.displayName} 撤销「${preview.packName}」`}
      confirmLabel="撤销技能包"
      destructive
      busy={busy}
      onConfirm={onExecute}
      onCancel={onClose}
    >
      <div className="sm2__detail-meta">
        将移除 {preview.willRemoveTargets} 个仅由该技能包安装的目标，保留 {preview.willPreserveTargets} 个仍有其他 claim 的目标。
      </div>
      {preview.affectedTargets.length > 0 && (
        <div className="sm2__scroll" style={{ marginTop: 8 }}>
          {preview.affectedTargets.map((target) => (
            <div key={target.targetId} className="sm2__target-row">
              <span>{target.displayName} · {target.mode} · {target.claimCount} claim(s)</span>
              <code>{target.targetPath}</code>
            </div>
          ))}
        </div>
      )}
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
  const { t } = useTranslation()
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [mode, setMode] = useState<'link' | 'copy'>(defaultMode)
  const [preview, setPreview] = useState<DistributionPreview | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const installedAgents = agents.filter((agent) => agent.installed)
  const changesByAction = useMemo(() => {
    const grouped = { create: 0, reuse: 0, other: 0 }
    if (!preview) return grouped
    for (const change of preview.changes) {
      if (change.action === 'create') grouped.create += 1
      else if (change.action === 'reuse') grouped.reuse += 1
      else grouped.other += 1
    }
    return grouped
  }, [preview])

  const toggle = (id: string) =>
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      setPreview(null)
      return next
    })

  const runPreview = async () => {
    setBusy(true)
    setError(null)
    try {
      setPreview(await skillApiV2.previewApplyPack(pack.id, Array.from(selected), mode))
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
        <div className="sm2__apply-grid">
          <div className="sm2__field">
            <label>分发方式</label>
            <select value={mode} onChange={(e) => setMode(e.target.value as 'link' | 'copy')}>
              <option value="link">{skillModeLabel(t, 'link')}</option>
              <option value="copy">{skillModeLabel(t, 'copy')}</option>
            </select>
          </div>
          <div className="sm2__field">
            <label>成员数量</label>
            <input value={`${pack.members.length} Skills`} readOnly />
          </div>
        </div>
        <label style={{ fontSize: 12, fontWeight: 700 }}>目标 Agent</label>
        <div className="sm2__agent-choice-grid">
          {installedAgents.map((agent) => (
            <button key={agent.id} className={`sm2__agent-choice${selected.has(agent.id) ? ' sm2__agent-choice--active' : ''}`} onClick={() => toggle(agent.id)}>
              <AgentIconBadge iconKey={agent.iconKey} title={agent.displayName} size={30} />
              <span>{agent.displayName}</span>
            </button>
          ))}
        </div>
        {installedAgents.length === 0 && <div className="sm2__empty sm2__empty--compact">没有可用 Agent</div>}
        {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
      </PreviewDialog>
    )
  }

  return (
    <PreviewDialog
      title="确认应用预览"
      confirmLabel="执行应用"
      destructive
      busy={busy}
      disabled={preview.blockers.length > 0}
      onConfirm={execute}
      onCancel={() => setPreview(null)}
    >
      <div className="sm2__preview-stats">
        <PackMetric label="新增 target" value={changesByAction.create} />
        <PackMetric label="复用 target" value={changesByAction.reuse} />
        <PackMetric label="阻止项" value={preview.blockers.length} tone={preview.blockers.length > 0 ? 'warn' : 'ok'} />
      </div>
      {preview.changes.map((change, index) => (
        <div key={index} className="sm2__change">
          {change.action === 'create'
            ? `新增 ${change.agentId} / ${change.skillId} (${skillModeLabel(t, change.actualMode)})`
            : `复用 ${change.agentId} / ${change.skillId}`}
        </div>
      ))}
      {preview.blockers.map((blocker, index) => (
        <div key={index} className="sm2__change sm2__change--blocked">
          阻止 {blocker.skillId}/{blocker.agentId}：{blocker.reason}
        </div>
      ))}
      {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
    </PreviewDialog>
  )
}
