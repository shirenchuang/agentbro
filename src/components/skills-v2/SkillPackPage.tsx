import { useEffect, useMemo, useState } from 'react'
import { createPortal } from 'react-dom'
import { useTranslation } from 'react-i18next'
import { useSkillStoreV2 } from '../../stores/skillStoreV2'
import { skillApiV2 } from '../../services/skillApiV2'
import type {
  ConflictBlocker,
  DeleteSkillPackPreview,
  DistributionBlockerDecision,
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
type PackSyncConflictState = {
  pack: SkillPackDetail
  targetAgents: string[]
  requestedMode: 'link' | 'copy'
  preview: DistributionPreview
}

const SHARED_SKILLS_AGENT_ID = 'agents'
const DEFAULT_SKILL_PACK_ID = 'default'
type BlockerDecision = DistributionBlockerDecision['action']

function blockerKey(blocker: ConflictBlocker) {
  return `${blocker.skillId}\u0000${blocker.agentId}`
}

function isManagedCopyBlocker(blocker: ConflictBlocker) {
  return blocker.reason.startsWith('Managed copy ')
}

export function SkillPackPage() {
  const { t } = useTranslation()
  const state = useSkillStoreV2()
  const [packQuery, setPackQuery] = useState('')
  const [builderMode, setBuilderMode] = useState<BuilderMode | null>(null)
  const [applyFor, setApplyFor] = useState<SkillPackDetail | null>(null)
  const [deletePreview, setDeletePreview] = useState<DeleteSkillPackPreview | null>(null)
  const [removeSkillPreview, setRemoveSkillPreview] = useState<RemoveSkillFromPackPreview | null>(null)
  const [revokePreview, setRevokePreview] = useState<RemovePackFromAgentPreview | null>(null)
  const [syncConflict, setSyncConflict] = useState<PackSyncConflictState | null>(null)
  const [busy, setBusy] = useState(false)
  const [syncNotice, setSyncNotice] = useState<string | null>(null)

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
  const syncIssueCount = state.packs.reduce((sum, pack) => sum + (pack.pendingSyncCount || 0) + (pack.failedSyncCount || 0), 0)

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

  const syncPack = async (packId: string, agentIds: string[] = []) => {
    setBusy(true)
    setSyncNotice(null)
    try {
      const result = await skillApiV2.syncPackToAgents(packId, agentIds)
      const failedAgents = result.agents.filter((agent) => agent.status === 'failed')
      if (failedAgents.length > 0) {
        const targetAgents = failedAgents.map((agent) => agent.agentId)
        const requestedMode = state.settings?.defaultDistributeMode || 'link'
        const preview = await skillApiV2.previewApplyPack(packId, targetAgents, requestedMode)
        if (preview.blockers.length > 0) {
          const currentPack = useSkillStoreV2.getState().selectedPackDetail
          const pack = currentPack?.id === packId ? currentPack : await skillApiV2.getPackDetail(packId)
          setSyncConflict({ pack, targetAgents, requestedMode, preview })
          setSyncNotice('同步遇到需要处理的冲突，请选择处理方式。')
          await state.loadOverview(true)
          await state.selectPack(packId)
          return
        }
      }
      setSyncNotice(
        failedAgents.length > 0
          ? t('skills.packSyncFailedNotice', { count: failedAgents.length, defaultValue: '{{count}} 个 Agent 同步失败，可查看状态后重试。' })
          : t('skills.packSyncDoneNotice', { count: result.agents.length, defaultValue: '已同步 {{count}} 个 Agent。' }),
      )
      await state.loadOverview(true)
      await state.selectPack(packId)
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
      {syncNotice && <div className="sm2__notice sm2__notice--ok">{syncNotice}</div>}

      <div className="sm2__pack-dashboard">
        <PackMetric label="技能包" value={state.packs.length} />
        <PackMetric label="成员引用" value={totalMembers} />
        <PackMetric label="Agent 应用" value={totalApplied} />
        <PackMetric label="需要处理" value={unhealthyPacks + missingCount + syncIssueCount} tone={unhealthyPacks + missingCount + syncIssueCount > 0 ? 'warn' : 'ok'} />
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
              onSync={(agentIds) => syncPack(detail.id, agentIds)}
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
              const agentId = revokePreview.agentId
              setRevokePreview(null)
              await state.selectPack(packId)
              await state.loadOverview(true)
              if (useSkillStoreV2.getState().selectedAgentId === agentId) {
                await state.loadAgentDetail(agentId, true)
              }
            } catch (e) {
              state.setError(String(e))
            } finally {
              setBusy(false)
            }
          }}
        />
      )}
      {syncConflict && (
        <PackSyncConflictDialog
          key={`${syncConflict.pack.id}:${syncConflict.targetAgents.join(',')}`}
          conflict={syncConflict}
          onClose={() => setSyncConflict(null)}
          onDone={async () => {
            const packId = syncConflict.pack.id
            setSyncConflict(null)
            await state.loadOverview(true)
            await state.selectPack(packId)
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
  const isDefault = pack.id === DEFAULT_SKILL_PACK_ID
  const syncStatus = pack.syncStatus || 'synced'
  const hasSyncIssue = (pack.pendingSyncCount || 0) + (pack.failedSyncCount || 0) > 0
  return (
    <button className={`sm2__pack-list-item${active ? ' sm2__pack-list-item--active' : ''}${isDefault ? ' sm2__pack-list-item--default' : ''}`} onClick={onClick}>
      <span className="sm2__pack-list-top">
        <strong>{pack.name}</strong>
        <span className={`sm2__pack-health ${pack.healthy && !hasSyncIssue ? 'sm2__pack-health--ok' : 'sm2__pack-health--warn'}`}>
          {isDefault ? '系统' : hasSyncIssue ? packSyncStatusLabel(syncStatus) : pack.healthy ? 'OK' : '缺失'}
        </span>
      </span>
      <span className="sm2__pack-list-desc">{pack.description || '自定义 Skill 组合'}</span>
      <span className="sm2__pack-list-meta">
        <span>{pack.memberCount} Skills</span>
        <span>{pack.appliedAgentCount} Agents</span>
        {hasSyncIssue && <span>{(pack.pendingSyncCount || 0) + (pack.failedSyncCount || 0)} 待同步</span>}
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
  onSync,
}: {
  detail: SkillPackDetail
  busy: boolean
  onApply: () => void
  onEdit: () => void
  onDuplicate: () => void
  onDelete: () => void
  onRemoveSkill: (skillId: string) => void
  onRevoke: (agentId: string) => void
  onSync: (agentIds?: string[]) => void
}) {
  const missing = detail.members.filter((member) => member.missing)
  const isDefault = detail.id === DEFAULT_SKILL_PACK_ID
  const syncStatus = detail.syncStatus || 'synced'
  const needsSync = packNeedsSync(detail)
  return (
    <div className="sm2__pack-workbench">
      <header className="sm2__pack-detail-hero">
        <div className="sm2__pack-detail-main">
          <div className="sm2__pack-kicker">{isDefault ? 'Built-in Pack' : 'Skill Pack'}</div>
          <h3>{detail.name}</h3>
          <p>{detail.description || '保存一组中心库 Skill ID，方便应用到 Agent。'}</p>
          {isDefault ? (
            <div className="sm2__pack-tagline">
              <span>自动包含中心库全部 Skills</span>
              <span>不可编辑</span>
            </div>
          ) : detail.tags.length > 0 ? (
            <div className="sm2__pack-tagline">
              {detail.tags.map((tag) => <span key={tag}>{tag}</span>)}
            </div>
          ) : null}
        </div>
        <div className="sm2__pack-actions">
          <button className="sm2__btn sm2__btn--primary" onClick={onApply} disabled={busy || detail.members.length === 0 || missing.length > 0}>
            应用
          </button>
          {!isDefault && <button className="sm2__btn" onClick={onEdit} disabled={busy}>编辑</button>}
          {!isDefault && needsSync && (
            <button className="sm2__btn sm2__btn--primary" onClick={() => onSync()} disabled={busy}>
              {busy ? '同步中…' : '同步到 Agent'}
            </button>
          )}
          {!isDefault && <button className="sm2__btn" onClick={onDuplicate} disabled={busy}>复制</button>}
          {!isDefault && <button className="sm2__btn sm2__btn--danger" onClick={onDelete} disabled={busy}>删除</button>}
        </div>
      </header>

      {isDefault && (
        <div className="sm2__notice sm2__notice--ok">
          全量技能包是系统内置入口，成员始终等于当前中心库全量；这里只能应用或从 Agent 撤销应用。
        </div>
      )}

      {missing.length > 0 && (
        <div className="sm2__notice sm2__notice--warn">
          {missing.length} 个成员已不在中心库中。应用前需要先恢复或从技能包移除。
        </div>
      )}

      {!isDefault && needsSync && (
        <div className={`sm2__notice ${syncStatus === 'failed' || syncStatus === 'partial' ? 'sm2__notice--warn' : 'sm2__notice--ok'}`}>
          {packSyncStatusLabel(syncStatus)}：{detail.pendingSyncCount || 0} 个待同步，{detail.failedSyncCount || 0} 个失败。
        </div>
      )}

      <div className="sm2__pack-summary-grid">
        <PackMetric label="成员 Skills" value={detail.members.length} />
        <PackMetric label="已应用 Agent" value={detail.appliedAgents.length} />
        <PackMetric label="缺失成员" value={missing.length} tone={missing.length > 0 ? 'warn' : 'ok'} />
        <PackMetric label="同步状态" value={(detail.pendingSyncCount || 0) + (detail.failedSyncCount || 0)} tone={needsSync ? 'warn' : 'ok'} />
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
                    {!isDefault && (
                      <button className="sm2__btn sm2__btn--ghost" disabled={busy} onClick={() => onRemoveSkill(member.skillId)}>
                        移除
                      </button>
                    )}
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
                    <span>{agent.memberCount} member claims · {packSyncStatusLabel(agent.syncStatus || 'synced')}</span>
                    {agent.syncError && <span>{agent.syncError}</span>}
                  </div>
                  {agent.agentId && agent.syncStatus && agent.syncStatus !== 'synced' && (
                    <button className="sm2__btn" disabled={busy} onClick={() => onSync([agent.agentId!])}>
                      同步
                    </button>
                  )}
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

function packNeedsSync(pack: Pick<SkillPackDetail, 'pendingSyncCount' | 'failedSyncCount' | 'syncStatus'>) {
  return (pack.pendingSyncCount || 0) + (pack.failedSyncCount || 0) > 0 || ['pending', 'failed', 'partial', 'syncing'].includes(pack.syncStatus || '')
}

function packSyncStatusLabel(status: string) {
  if (status === 'pending') return '有变更未同步'
  if (status === 'syncing') return '同步中'
  if (status === 'failed') return '同步失败'
  if (status === 'partial') return '部分同步'
  return '已同步'
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
  const [query, setQuery] = useState('')
  const [sourceFilter, setSourceFilter] = useState('')
  const [selected, setSelected] = useState<Set<string>>(new Set(existing?.members.map((member) => member.skillId) || []))
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const existingMemberIds = useMemo(() => new Set(existing?.members.map((member) => member.skillId) || []), [existing])
  const existingApplied = mode === 'edit' && (existing?.appliedAgents.length || 0) > 0
  const sourceOptions = useMemo(() => {
    const counts = new Map<string, number>()
    skills.forEach((skill) => counts.set(skill.sourceType, (counts.get(skill.sourceType) || 0) + 1))
    return Array.from(counts.entries())
      .map(([value, count]) => ({
        value,
        count,
        label: skillSourceTypeLabel(t, value),
      }))
      .sort((a, b) => a.label.localeCompare(b.label))
  }, [skills, t])
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    return skills.filter((skill) =>
      (!sourceFilter || skill.sourceType === sourceFilter) &&
      (!q ||
        [skill.name, skill.id, skill.description, skill.sourceType, skillSourceTypeLabel(t, skill.sourceType)]
          .join(' ')
          .toLowerCase()
          .includes(q)),
    )
  }, [query, skills, sourceFilter, t])
  const selectedSkills = skills.filter((skill) => selected.has(skill.id))
  const selectedVisibleCount = filtered.filter((skill) => selected.has(skill.id)).length

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

  const selectVisible = () => {
    setError(null)
    setSelected((prev) => {
      const next = new Set(prev)
      filtered.forEach((skill) => next.add(skill.id))
      return next
    })
  }

  const clearSelected = () => {
    setError(null)
    setSelected(existingApplied ? new Set(existingMemberIds) : new Set())
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
    setBusy(true)
    setError(null)
    try {
      const saved = await skillApiV2.upsertPack({
        id: mode === 'edit' ? existing?.id || '' : '',
        name: name.trim(),
        description: '',
        tags: [],
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

      <div className="sm2__pack-builder-rail" aria-hidden="true">
        <span className="sm2__pack-builder-rail-item sm2__pack-builder-rail-item--active">命名</span>
        <span className={`sm2__pack-builder-rail-item${selected.size > 0 ? ' sm2__pack-builder-rail-item--active' : ''}`}>
          {selected.size > 0 ? `${selected.size} Skills` : '选择 Skills'}
        </span>
        <span className={`sm2__pack-builder-rail-item${name.trim() && selected.size > 0 ? ' sm2__pack-builder-rail-item--ready' : ''}`}>
          保存组合
        </span>
      </div>

      <div className="sm2__pack-builder2-grid">
        <section className="sm2__pack-section sm2__pack-builder-card sm2__pack-builder-card--identity">
          <div className="sm2__pack-section-head">
            <div>
              <h3>基本信息</h3>
              <span>只需要一个名称。</span>
            </div>
          </div>
          <div className="sm2__builder-form2">
            <label className="sm2__field">
              <span>技能包名称</span>
              <input value={name} onChange={(e) => setName(e.target.value)} placeholder="Code Review 工具集" />
            </label>
          </div>
        </section>

        <section className="sm2__pack-section sm2__pack-builder-card sm2__pack-section--picker">
          <div className="sm2__pack-section-head">
            <div>
              <h3>中心库 Skills</h3>
              <span>{filtered.length} 个可选。</span>
            </div>
            <span>{selectedVisibleCount}/{filtered.length}</span>
          </div>
          <div className="sm2__pack-picker-tools">
            <input className="sm2__search sm2__search--full" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="搜索 Skill" />
            <button className="sm2__btn sm2__btn--ghost" onClick={selectVisible} disabled={filtered.length === 0 || selectedVisibleCount === filtered.length}>
              全选当前
            </button>
          </div>
          <div className="sm2__pack-source-filter" aria-label="来源过滤">
            <span>来源过滤</span>
            <button
              type="button"
              className={!sourceFilter ? 'sm2__pack-source-filter-chip sm2__pack-source-filter-chip--active' : 'sm2__pack-source-filter-chip'}
              aria-pressed={!sourceFilter}
              onClick={() => setSourceFilter('')}
            >
              全部 <em>{skills.length}</em>
            </button>
            {sourceOptions.map((source) => (
              <button
                key={source.value}
                type="button"
                className={sourceFilter === source.value ? 'sm2__pack-source-filter-chip sm2__pack-source-filter-chip--active' : 'sm2__pack-source-filter-chip'}
                aria-pressed={sourceFilter === source.value}
                onClick={() => setSourceFilter(source.value)}
              >
                {source.label} <em>{source.count}</em>
              </button>
            ))}
          </div>
          <div className="sm2__skill-picker2">
            {filtered.length === 0 ? (
              <div className="sm2__empty sm2__empty--compact">没有匹配的 Skill</div>
            ) : (
              filtered.map((skill) => (
                <button
                  key={skill.id}
                  className={`sm2__skill-pick${selected.has(skill.id) ? ' sm2__skill-pick--selected' : ''}`}
                  aria-label={`选择 ${skill.name}`}
                  aria-pressed={selected.has(skill.id)}
                  onClick={() => toggle(skill.id)}
                >
                  <span className="sm2__skill-pick-check" aria-hidden="true">{selected.has(skill.id) ? '✓' : ''}</span>
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

        <section className="sm2__pack-section sm2__pack-builder-card sm2__pack-builder-card--selected">
          <div className="sm2__pack-section-head">
            <div>
              <h3>已选择</h3>
              <span>保存后的成员顺序。</span>
            </div>
            <span role="status" aria-label="已选择 Skill 数量">{selectedSkills.length}</span>
          </div>
          <div className="sm2__pack-selected-tools">
            <span>{selectedSkills.length === 0 ? '从右侧选择 Skill' : `将保存 ${selectedSkills.length} 个 Skill ID`}</span>
            <button className="sm2__btn sm2__btn--ghost" onClick={clearSelected} disabled={selectedSkills.length === 0 || (existingApplied && selectedSkills.length === existingMemberIds.size)}>
              清空
            </button>
          </div>
          {selectedSkills.length === 0 ? (
            <div className="sm2__pack-selected-empty">
              <strong>还没有成员</strong>
              <span>点击右侧 Skill，它会立即加入这个包。</span>
            </div>
          ) : (
            <div className="sm2__pack-member-list">
              {selectedSkills.map((skill, index) => (
                <div key={skill.id} className="sm2__pack-member-row">
                  <div className="sm2__pack-member-index">{index + 1}</div>
                  <div className="sm2__pack-member-body">
                    <strong>{skill.name}</strong>
                    <span>{skill.id}</span>
                  </div>
                  <button className="sm2__btn sm2__btn--ghost" aria-label={`移除 ${skill.name}`} onClick={() => toggle(skill.id)}>移除</button>
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

function PackSyncConflictDialog({
  conflict,
  onClose,
  onDone,
}: {
  conflict: PackSyncConflictState
  onClose: () => void
  onDone: () => void
}) {
  const { t } = useTranslation()
  const [preview, setPreview] = useState(conflict.preview)
  const [blockerDecisions, setBlockerDecisions] = useState<Record<string, BlockerDecision>>({})
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const changesByAction = useMemo(() => {
    const grouped = { create: 0, reuse: 0, other: 0 }
    for (const change of preview.changes) {
      if (change.action === 'create') grouped.create += 1
      else if (change.action === 'reuse') grouped.reuse += 1
      else grouped.other += 1
    }
    return grouped
  }, [preview])
  const unresolvedBlockers = preview.blockers.filter((blocker) => !blockerDecisions[blockerKey(blocker)]).length

  const execute = async () => {
    setBusy(true)
    setError(null)
    try {
      const blockerDecisionsPayload = preview.blockers
        .map((blocker) => {
          const action = blockerDecisions[blockerKey(blocker)]
          return action ? { skillId: blocker.skillId, agentId: blocker.agentId, action } : null
        })
        .filter((item): item is DistributionBlockerDecision => Boolean(item))
      const result = await skillApiV2.executeApplyPack(
        conflict.pack.id,
        conflict.targetAgents,
        conflict.requestedMode,
        blockerDecisionsPayload,
      )
      if (result.blockers.length > 0) {
        setPreview(result)
        setBlockerDecisions({})
        return
      }
      onDone()
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <PreviewDialog
      title="确认同步冲突"
      confirmLabel="按选择同步"
      destructive
      busy={busy}
      disabled={unresolvedBlockers > 0}
      onConfirm={execute}
      onCancel={onClose}
    >
      <div className="sm2__notice sm2__notice--warn" style={{ margin: 0 }}>
        当前同步的 Agent 里面有未接管的同名 Skill。请选择是否覆盖安装，或忽略该目标。
      </div>
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
          <div>
            阻止 {blocker.skillId}/{blocker.agentId}：{distributionBlockerMessage(blocker)}
          </div>
          {blocker.existingPath && (
            <div className="sm2-distribute__path-row" style={{ marginTop: 8 }}>
              <code>{blocker.existingPath}</code>
              <button type="button" className="sm2__btn sm2__btn--ghost" onClick={() => skillApiV2.openPath(blocker.existingPath!)}>
                打开
              </button>
            </div>
          )}
          <div className="sm2-distribute__decision-row" role="radiogroup" aria-label={`${blocker.agentId} 同步冲突处理方式`}>
            {blocker.existingPath && (
              <button
                type="button"
                className={`sm2-distribute__decision${blockerDecisions[blockerKey(blocker)] === 'overwrite' ? ' sm2-distribute__decision--active' : ''}`}
                onClick={() => setBlockerDecisions((prev) => ({ ...prev, [blockerKey(blocker)]: 'overwrite' }))}
              >
                {isManagedCopyBlocker(blocker) ? '以中心库为准' : '覆盖安装'}
              </button>
            )}
            {isManagedCopyBlocker(blocker) && (
              <button
                type="button"
                className={`sm2-distribute__decision${blockerDecisions[blockerKey(blocker)] === 'agent_over_center' ? ' sm2-distribute__decision--active' : ''}`}
                onClick={() => setBlockerDecisions((prev) => ({ ...prev, [blockerKey(blocker)]: 'agent_over_center' }))}
              >
                以 Agent 为准
              </button>
            )}
            <button
              type="button"
              className={`sm2-distribute__decision${blockerDecisions[blockerKey(blocker)] === 'skip' ? ' sm2-distribute__decision--active' : ''}`}
              onClick={() => setBlockerDecisions((prev) => ({ ...prev, [blockerKey(blocker)]: 'skip' }))}
            >
              忽略此目标
            </button>
          </div>
        </div>
      ))}
      {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
    </PreviewDialog>
  )
}

function distributionBlockerMessage(blocker: ConflictBlocker) {
  if (blocker.reason.startsWith('An unmanaged ')) {
    return '目标路径已存在未接管的同名 Skill，需要先选择覆盖安装或忽略。'
  }
  if (isManagedCopyBlocker(blocker)) {
    return '已管理的 copy 版本有本地修改，需要选择中心库或 Agent 版本为准。'
  }
  return blocker.reason
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
  const [blockerDecisions, setBlockerDecisions] = useState<Record<string, BlockerDecision>>({})
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const installedAgents = agents.filter((agent) => agent.installed && agent.id !== SHARED_SKILLS_AGENT_ID)
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
      setBlockerDecisions({})
      return next
    })

  const runPreview = async () => {
    setBusy(true)
    setError(null)
    try {
      setPreview(await skillApiV2.previewApplyPack(pack.id, Array.from(selected), mode))
      setBlockerDecisions({})
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
      const blockerDecisionsPayload = preview?.blockers
        .map((blocker) => {
          const action = blockerDecisions[blockerKey(blocker)]
          return action ? { skillId: blocker.skillId, agentId: blocker.agentId, action } : null
        })
        .filter((item): item is DistributionBlockerDecision => Boolean(item)) ?? []
      const result = await skillApiV2.executeApplyPack(pack.id, Array.from(selected), mode, blockerDecisionsPayload)
      if (result.blockers.length > 0) {
        setPreview(result)
        setBlockerDecisions({})
        return
      }
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
        modalClassName="sm2__modal--pack-apply"
        busy={busy}
        disabled={selected.size === 0}
        onConfirm={runPreview}
        onCancel={onClose}
      >
        <div className="sm2__pack-apply-form">
          <div className="sm2__apply-grid">
            <div className="sm2__field sm2__field--select">
              <label htmlFor="sm2-pack-apply-mode">分发方式</label>
              <select id="sm2-pack-apply-mode" value={mode} onChange={(e) => setMode(e.target.value as 'link' | 'copy')}>
                <option value="link">{skillModeLabel(t, 'link')}</option>
                <option value="copy">{skillModeLabel(t, 'copy')}</option>
              </select>
            </div>
            <div className="sm2__field sm2__field--readonly">
              <label htmlFor="sm2-pack-apply-members">成员数量</label>
              <input id="sm2-pack-apply-members" value={`${pack.members.length} Skills`} readOnly />
            </div>
          </div>
          <div className="sm2__pack-apply-section">
            <div className="sm2__pack-apply-label">目标 Agent</div>
            <div className="sm2__agent-choice-grid">
              {installedAgents.map((agent) => (
                <button
                  key={agent.id}
                  type="button"
                  className={`sm2__agent-choice${selected.has(agent.id) ? ' sm2__agent-choice--active' : ''}`}
                  onClick={() => toggle(agent.id)}
                >
                  <AgentIconBadge iconKey={agent.iconKey} title={agent.displayName} size={30} />
                  <span>{agent.displayName}</span>
                </button>
              ))}
            </div>
          </div>
          {installedAgents.length === 0 && <div className="sm2__empty sm2__empty--compact">没有可用 Agent</div>}
          {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
        </div>
      </PreviewDialog>
    )
  }

  const unresolvedBlockers = preview.blockers.filter((blocker) => !blockerDecisions[blockerKey(blocker)]).length

  return (
    <PreviewDialog
      title="确认应用预览"
      confirmLabel={preview.blockers.length > 0 ? '按选择执行' : '执行应用'}
      destructive
      busy={busy}
      disabled={unresolvedBlockers > 0}
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
          <div>
            阻止 {blocker.skillId}/{blocker.agentId}：{distributionBlockerMessage(blocker)}
          </div>
          {blocker.existingPath && (
            <div className="sm2-distribute__path-row" style={{ marginTop: 8 }}>
              <code>{blocker.existingPath}</code>
              <button type="button" className="sm2__btn sm2__btn--ghost" onClick={() => skillApiV2.openPath(blocker.existingPath!)}>
                打开
              </button>
            </div>
          )}
          <div className="sm2-distribute__decision-row" role="radiogroup" aria-label={`${blocker.agentId} 阻止项处理方式`}>
            {blocker.existingPath && (
              <button
                type="button"
                className={`sm2-distribute__decision${blockerDecisions[blockerKey(blocker)] === 'overwrite' ? ' sm2-distribute__decision--active' : ''}`}
                onClick={() => setBlockerDecisions((prev) => ({ ...prev, [blockerKey(blocker)]: 'overwrite' }))}
              >
                {isManagedCopyBlocker(blocker) ? '以中心库为准' : '覆盖安装'}
              </button>
            )}
            {isManagedCopyBlocker(blocker) && (
              <button
                type="button"
                className={`sm2-distribute__decision${blockerDecisions[blockerKey(blocker)] === 'agent_over_center' ? ' sm2-distribute__decision--active' : ''}`}
                onClick={() => setBlockerDecisions((prev) => ({ ...prev, [blockerKey(blocker)]: 'agent_over_center' }))}
              >
                以 Agent 为准
              </button>
            )}
            <button
              type="button"
              className={`sm2-distribute__decision${blockerDecisions[blockerKey(blocker)] === 'skip' ? ' sm2-distribute__decision--active' : ''}`}
              onClick={() => setBlockerDecisions((prev) => ({ ...prev, [blockerKey(blocker)]: 'skip' }))}
            >
              忽略此目标
            </button>
          </div>
        </div>
      ))}
      {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
    </PreviewDialog>
  )
}
