import { useEffect, useState } from 'react'
import { filteredSkills, useSkillStoreV2 } from '../../stores/skillStoreV2'
import { skillApiV2 } from '../../services/skillApiV2'
import type { SkillSummary, DeleteCenterSkillPreview } from '../../services/skillApiV2'
import { AgentIconBadge } from './AgentIconBadge'
import { AddSkillDialog } from './AddSkillDialog'
import { DistributeDialog } from './DistributeDialog'
import { PreviewDialog } from './PreviewDialog'

const STATUS_LABEL: Record<string, string> = {
  ok: '正常',
  conflict: '冲突',
  copyDiverged: '副本分叉',
  unmanaged: '未管理',
  updateAvailable: '可更新',
}

export function SkillLibraryPage() {
  const state = useSkillStoreV2()
  const skills = filteredSkills(state)
  const overview = state.overview
  const [addOpen, setAddOpen] = useState(false)
  const [distributeFor, setDistributeFor] = useState<SkillSummary | null>(null)
  const [deletePreview, setDeletePreview] = useState<DeleteCenterSkillPreview | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    state.init()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const refresh = () => state.refresh()
  const reloadDetail = async () => {
    if (state.selectedSkillId) await state.selectSkill(state.selectedSkillId)
    await state.loadOverview()
  }

  const openDelete = async (skillId: string) => {
    setBusy(true)
    try {
      const p = await skillApiV2.previewDeleteCenterSkill(skillId)
      setDeletePreview(p)
    } catch (e) {
      state.setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  const confirmDelete = async (removeLinked: boolean) => {
    if (!deletePreview) return
    setBusy(true)
    try {
      await skillApiV2.executeDeleteCenterSkill(deletePreview.skillId, removeLinked)
      setDeletePreview(null)
      await state.selectSkill(null)
      await state.loadOverview()
    } catch (e) {
      state.setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="sm2">
      <div className="sm2__header">
        <h2 className="sm2__title">Skill 库</h2>
        <div className="sm2__tabs">
          <button className="sm2__btn" onClick={() => setAddOpen(true)}>+ 添加到中心库</button>
          <button className="sm2__btn" onClick={refresh} disabled={state.loading}>
            {state.loading ? '刷新中…' : '刷新'}
          </button>
        </div>
      </div>

      {overview && (
        <div className="sm2__metrics">
          <Metric value={overview.metrics.centerSkillCount} label="中心库 Skill" />
          <Metric value={overview.metrics.targetCount} label="Agent 安装" />
          <Metric value={overview.metrics.unmanagedCount} label="未管理" />
          <Metric value={overview.metrics.issueCount} label="诊断问题" />
        </div>
      )}

      <div className="sm2__toolbar">
        <input
          className="sm2__search"
          placeholder="搜索名称 / 描述 / 来源 / Agent"
          value={state.filters.query}
          onChange={(e) => state.setFilter('query', e.target.value)}
        />
        <select className="sm2__select" value={state.filters.status} onChange={(e) => state.setFilter('status', e.target.value)}>
          <option value="">全部状态</option>
          <option value="ok">正常</option>
          <option value="conflict">冲突</option>
          <option value="copyDiverged">副本分叉</option>
          <option value="unmanaged">未管理</option>
        </select>
        <select className="sm2__select" value={state.filters.source} onChange={(e) => state.setFilter('source', e.target.value)}>
          <option value="">全部来源</option>
          <option value="local_folder">本地</option>
          <option value="github">GitHub</option>
          <option value="agent_import">Agent 接管</option>
          <option value="manual_center">手动</option>
        </select>
        <div className="sm2__view-toggle">
          <button className={state.viewMode === 'cards' ? 'active' : ''} onClick={() => state.setViewMode('cards')}>卡片</button>
          <button className={state.viewMode === 'list' ? 'active' : ''} onClick={() => state.setViewMode('list')}>列表</button>
        </div>
      </div>

      {state.error && <div className="sm2__error">{state.error}</div>}

      <div className="sm2__body">
        <div className="sm2__main">
          {skills.length === 0 ? (
            <div className="sm2__empty">
              中心库为空。点击「添加到中心库」导入第一个 Skill，或把 Skill 文件夹放入
              <code style={{ margin: '0 4px' }}>{state.settings?.centerPath}</code> 后刷新。
            </div>
          ) : state.viewMode === 'cards' ? (
            <div className="sm2__grid">
              {skills.map((s) => (
                <SkillCard
                  key={s.id}
                  skill={s}
                  selected={state.selectedSkillId === s.id}
                  onClick={() => state.selectSkill(s.id)}
                />
              ))}
            </div>
          ) : (
            <div className="sm2__list">
              {skills.map((s) => (
                <div
                  key={s.id}
                  className={`sm2__row${state.selectedSkillId === s.id ? ' sm2__row--selected' : ''}`}
                  onClick={() => state.selectSkill(s.id)}
                >
                  <div className="sm2__row-main">
                    <div className="sm2__row-title">{s.name}</div>
                    <div className="sm2__row-sub">{s.sourceType} · {STATUS_LABEL[s.status] || s.status}</div>
                  </div>
                  <AgentBadges skill={s} />
                </div>
              ))}
            </div>
          )}
        </div>
        <SkillDetailPanel
          onDistribute={(s) => setDistributeFor(s)}
          onDelete={openDelete}
          onAdoptRefresh={reloadDetail}
          busy={busy}
        />
      </div>

      {addOpen && (
        <AddSkillDialog onClose={() => setAddOpen(false)} onDone={reloadDetail} />
      )}
      {distributeFor && state.settings && (
        <DistributeDialog
          skill={distributeFor}
          agents={state.agents}
          defaultMode={state.settings.defaultDistributeMode}
          onClose={() => setDistributeFor(null)}
          onDone={reloadDetail}
        />
      )}
      {deletePreview && (
        <PreviewDialog
          title={`删除中心库 Skill「${deletePreview.skillId}」`}
          confirmLabel="仅删除中心库"
          destructive
          busy={busy}
          onCancel={() => setDeletePreview(null)}
          onConfirm={() => confirmDelete(false)}
        >
          {deletePreview.warnings.map((w, i) => (
            <div key={i} className="sm2__change sm2__change--blocked">{w}</div>
          ))}
          <p style={{ fontSize: 12, marginTop: 8 }}>
            受影响 Agent 安装：{deletePreview.affectedTargets.length}
          </p>
          {deletePreview.affectedTargets.map((t) => (
            <div key={t.targetId} className="sm2__target-row">
              <span>{t.displayName} · {t.mode}</span>
              <span>{t.claimCount} claim(s)</span>
            </div>
          ))}
          {deletePreview.affectedTargets.length > 0 && (
            <div className="sm2__btn-row">
              <button
                className="sm2__btn sm2__btn--danger"
                disabled={busy}
                onClick={() => confirmDelete(true)}
              >
                同时移除所有 Agent 安装
              </button>
            </div>
          )}
        </PreviewDialog>
      )}
    </div>
  )
}

function Metric({ value, label }: { value: number; label: string }) {
  return (
    <div className="sm2__metric">
      <div className="sm2__metric-value">{value}</div>
      <div className="sm2__metric-label">{label}</div>
    </div>
  )
}

function AgentBadges({ skill }: { skill: SkillSummary }) {
  return (
    <div className="sm2__agents">
      {skill.installedAgents.map((a) => (
        <AgentIconBadge key={a.agentId} iconKey={a.iconKey} mode={a.mode} title={`${a.displayName} · ${a.mode} · ${a.status}`} />
      ))}
    </div>
  )
}

function SkillCard({
  skill,
  selected,
  onClick,
}: {
  skill: SkillSummary
  selected: boolean
  onClick: () => void
}) {
  return (
    <div className={`sm2__card${selected ? ' sm2__card--selected' : ''}`} onClick={onClick}>
      <h3 className="sm2__card-title">{skill.name}</h3>
      <p className="sm2__card-desc">{skill.description || '（无描述）'}</p>
      <div className="sm2__card-tags">
        <span className={`sm2__tag sm2__tag--${skill.status}`}>{STATUS_LABEL[skill.status] || skill.status}</span>
        <span className="sm2__tag">{skill.sourceType}</span>
        {skill.installedAgents.length > 0 && (
          <div className="sm2__agents">
            {skill.installedAgents.map((a) => (
              <AgentIconBadge key={a.agentId} iconKey={a.iconKey} mode={a.mode} title={`${a.displayName} · ${a.mode}`} />
            ))}
          </div>
        )}
      </div>
    </div>
  )
}

function SkillDetailPanel({
  onDistribute,
  onDelete,
  onAdoptRefresh,
  busy,
}: {
  onDistribute: (s: SkillSummary) => void
  onDelete: (id: string) => void
  onAdoptRefresh: () => void
  busy: boolean
}) {
  const { selectedSkillDetail: detail } = useSkillStoreV2()
  const [syncing, setSyncing] = useState<string | null>(null)

  if (!detail) {
    return (
      <div className="sm2__detail">
        <div className="sm2__empty" style={{ padding: 20 }}>选择一个 Skill 查看详情</div>
      </div>
    )
  }

  const summary = detail
  const doSync = async (targetId: string, action: string) => {
    setSyncing(targetId)
    try {
      await skillApiV2.executeSyncCopy(targetId, action)
      await onAdoptRefresh()
    } catch (e) {
      // eslint-disable-next-line no-alert
      alert(String(e))
    } finally {
      setSyncing(null)
    }
  }

  return (
    <div className="sm2__detail">
      <h3>{summary.name}</h3>
      <p style={{ fontSize: 12, color: 'var(--text-secondary)', margin: '0 0 8px' }}>{summary.description}</p>
      <div className="sm2__detail-meta">
        <div>来源：{summary.sourceType}{summary.sourceUri ? ` · ${summary.sourceUri}` : ''}</div>
        <div>路径：{summary.centerPath}</div>
        <div>Hash：{summary.currentHash.slice(0, 12)}…</div>
      </div>

      <div className="sm2__btn-row">
        <button className="sm2__btn sm2__btn--primary" onClick={() => onDistribute(summary)}>分发到 Agent</button>
        <button className="sm2__btn sm2__btn--ghost" onClick={() => skillApiV2.openPath(summary.centerPath)}>打开目录</button>
        <button className="sm2__btn sm2__btn--danger" disabled={busy} onClick={() => onDelete(summary.id)}>删除</button>
      </div>

      <div className="sm2__detail-section">
        <h4>已安装 Agent</h4>
        {detail.targets.length === 0 ? (
          <div style={{ fontSize: 12, color: 'var(--text-secondary)' }}>尚未分发到任何 Agent</div>
        ) : (
          detail.targets.map((t) => (
            <div key={t.id} className="sm2__target-row">
              <div>
                <strong>{t.agentId}</strong>
                <div style={{ fontSize: 11, color: 'var(--text-secondary)' }}>
                  {t.actualMode} · {t.status} · {t.claims.length} claim(s):{' '}
                  {t.claims.map((c) => c.claimType === 'pack' ? `pack:${c.packName}` : 'direct').join(', ')}
                </div>
                <div style={{ fontSize: 11, color: 'var(--text-secondary)' }}>{t.targetPath}</div>
              </div>
              {t.actualMode === 'copy' && t.status !== 'ok' && (
                <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                  {t.status === 'copy_outdated' && (
                    <button className="sm2__btn" disabled={syncing === t.id} onClick={() => doSync(t.id, 'center_over_agent')}>更新副本</button>
                  )}
                  {t.status === 'copy_modified' && (
                    <button className="sm2__btn" disabled={syncing === t.id} onClick={() => doSync(t.id, 'agent_over_center')}>推送到中心库</button>
                  )}
                  {t.status === 'copy_diverged' && (
                    <>
                      <button className="sm2__btn" disabled={syncing === t.id} onClick={() => doSync(t.id, 'center_over_agent')}>用中心库覆盖</button>
                      <button className="sm2__btn" disabled={syncing === t.id} onClick={() => doSync(t.id, 'agent_over_center')}>用副本覆盖</button>
                      <button className="sm2__btn" disabled={syncing === t.id} onClick={() => doSync(t.id, 'keep_diverged')}>保留分叉</button>
                    </>
                  )}
                </div>
              )}
            </div>
          ))
        )}
      </div>

      {detail.source && (
        <div className="sm2__detail-section">
          <h4>来源</h4>
          <div className="sm2__detail-meta">
            <div>类型：{detail.source.sourceType}</div>
            {detail.source.importedFromAgent && <div>来自 Agent：{detail.source.importedFromAgent}</div>}
            {detail.source.importedFromPath && <div>原路径：{detail.source.importedFromPath}</div>}
          </div>
        </div>
      )}
    </div>
  )
}
