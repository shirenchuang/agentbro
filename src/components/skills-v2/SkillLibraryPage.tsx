import { useEffect, useMemo, useState } from 'react'
import { filteredSkills, useSkillStoreV2 } from '../../stores/skillStoreV2'
import { skillApiV2 } from '../../services/skillApiV2'
import type { SkillSummary, DeleteCenterSkillPreview } from '../../services/skillApiV2'
import { AgentIconBadge } from './AgentIconBadge'
import { DistributeDialog } from './DistributeDialog'
import { SkillDetailSlider } from './SkillDetailSlider'
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
  const [distributeFor, setDistributeFor] = useState<SkillSummary | null>(null)
  const [sliderSkillId, setSliderSkillId] = useState<string | null>(null)
  const [deletePreview, setDeletePreview] = useState<DeleteCenterSkillPreview | null>(null)
  const [busy, setBusy] = useState(false)
  const sources = useMemo(
    () => Array.from(new Set(state.skills.map((s) => s.sourceType).filter(Boolean))).sort(),
    [state.skills],
  )

  useEffect(() => {
    state.init()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const refresh = () => state.refresh()
  const reload = async () => {
    await state.loadOverview(true)
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
      setSliderSkillId(null)
      await state.loadOverview(true)
    } catch (e) {
      state.setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="sm2 sm2--library">
      <div className="sm2__header sm2__header--stacked">
        <div>
          <h2 className="sm2__title">Skill 库</h2>
          <p className="sm2__header-subtitle">统一管理中心库 Skills，查看安装到哪些 Agent，并处理分发、更新和删除。</p>
        </div>
        <div className="sm2__tabs">
          <button className="sm2__btn sm2__btn--primary" onClick={() => state.setTab('install')}>安装 Skill</button>
          <button className="sm2__btn" onClick={refresh} disabled={state.loading}>
            {state.loading ? '刷新中…' : '刷新'}
          </button>
        </div>
      </div>

      {overview && (
        <div className="sm2__metrics sm2__library-metrics">
          <Metric value={overview.metrics.centerSkillCount} label="中心库 Skill" />
          <Metric value={overview.metrics.targetCount} label="Agent 安装" />
          <Metric value={overview.metrics.unmanagedCount} label="未管理" />
          <Metric value={overview.metrics.issueCount} label="诊断问题" />
        </div>
      )}

      <div className="sm2__library-filterbar">
        <div className="sm2__filter-search">
          <span className="sm2__filter-icon">⌕</span>
          <input
            className="sm2__search sm2__search--quiet"
            placeholder="搜索名称、描述、来源或 Agent"
            value={state.filters.query}
            onChange={(e) => state.setFilter('query', e.target.value)}
          />
        </div>
        <div className="sm2__filter-controls">
          <label className="sm2__select-field">
            <span>状态</span>
            <select className="sm2__select" value={state.filters.status} onChange={(e) => state.setFilter('status', e.target.value)}>
              <option value="">全部状态</option>
              <option value="ok">正常</option>
              <option value="conflict">冲突</option>
              <option value="copyDiverged">副本分叉</option>
            </select>
          </label>
          <label className="sm2__select-field">
            <span>来源</span>
            <select className="sm2__select" value={state.filters.source} onChange={(e) => state.setFilter('source', e.target.value)}>
              <option value="">全部来源</option>
              {sources.map((source) => (
                <option key={source} value={source}>{source}</option>
              ))}
            </select>
          </label>
          <div className="sm2__view-toggle sm2__view-toggle--soft">
            <button className={state.viewMode === 'cards' ? 'active' : ''} onClick={() => state.setViewMode('cards')}>卡片</button>
            <button className={state.viewMode === 'list' ? 'active' : ''} onClick={() => state.setViewMode('list')}>列表</button>
          </div>
        </div>
      </div>

      {state.error && <div className="sm2__error">{state.error}</div>}

      <div className="sm2__main sm2__main--full">
        {skills.length === 0 ? (
          <div className="sm2__empty">
            中心库为空。点击「添加到中心库」导入第一个 Skill，或把 Skill 文件夹放入
            <code style={{ margin: '0 4px' }}>{state.settings?.centerPath}</code> 后刷新。
          </div>
        ) : state.viewMode === 'cards' ? (
          <div className="sm2__grid">
            {skills.map((s) => (
              <SkillCard key={s.id} skill={s} onClick={() => setSliderSkillId(s.id)} />
            ))}
          </div>
        ) : (
          <div className="sm2__list">
            {skills.map((s) => (
              <div key={s.id} className="sm2__row" onClick={() => setSliderSkillId(s.id)}>
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

      <SkillDetailSlider
        skillId={sliderSkillId}
        open={!!sliderSkillId}
        onClose={() => setSliderSkillId(null)}
        onDistribute={(s) => setDistributeFor(s)}
        onDelete={openDelete}
      />

      {distributeFor && state.settings && (
        <DistributeDialog
          skill={distributeFor}
          agents={state.agents}
          defaultMode={state.settings.defaultDistributeMode}
          onClose={() => setDistributeFor(null)}
          onDone={reload}
        />
      )}
      {deletePreview && (
        <PreviewDialog
          title={`删除中心库 Skill「${deletePreview.skillId}」`}
          confirmLabel="仅删除中心库"
          modalClassName="sm2__modal--delete-skill"
          destructive
          busy={busy}
          disabled={!deletePreview.removable}
          onCancel={() => setDeletePreview(null)}
          onConfirm={() => confirmDelete(false)}
        >
          <div className="sm2-delete-skill">
            <div className="sm2-delete-skill__hero">
              <div className="sm2-delete-skill__mark" aria-hidden="true">!</div>
              <div className="sm2-delete-skill__hero-copy">
                <strong>删除前确认</strong>
                <span>
                  {deletePreview.affectedTargets.length > 0
                    ? '中心库记录会被删除，已安装到 Agent 的目标可选择一并移除。'
                    : '当前没有 Agent 安装会被影响，只会移除中心库记录。'}
                </span>
              </div>
              <div className="sm2-delete-skill__impact">
                <strong>{deletePreview.affectedTargets.length}</strong>
                <span>Agent 安装</span>
              </div>
            </div>

            {deletePreview.affectedTargets.length > 0 ? (
              <div className="sm2-delete-skill__targets">
                {deletePreview.affectedTargets.map((target) => (
                  <div key={target.targetId} className="sm2-delete-skill__target">
                    <div>
                      <strong>{target.displayName || target.agentId}</strong>
                      <span>{target.mode} · {target.claimCount} 个引用</span>
                    </div>
                    <code>{target.targetPath}</code>
                  </div>
                ))}
              </div>
            ) : (
              <div className="sm2-delete-skill__zero">
                这次删除不会触碰任何 Agent 目录。
              </div>
            )}

            {deletePreview.warnings.length > 0 && (
              <div className="sm2-delete-skill__warnings">
                {deletePreview.warnings.map((w, i) => (
                  <div key={i} className="sm2-delete-skill__warning">{w}</div>
                ))}
              </div>
            )}

            {deletePreview.affectedTargets.length > 0 && (
              <div className="sm2-delete-skill__inline-action">
                <button className="sm2__btn sm2-delete-skill__secondary-danger" disabled={busy} onClick={() => confirmDelete(true)}>
                  同时移除所有 Agent 安装
                </button>
              </div>
            )}
          </div>
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

function SkillCard({ skill, onClick }: { skill: SkillSummary; onClick: () => void }) {
  return (
    <div className="sm2__card" onClick={onClick}>
      <div className="sm2__card-head">
        <div className="sm2__skill-avatar">{(skill.name || 'SK').slice(0, 2).toUpperCase()}</div>
        <div className="sm2__card-titleblock">
          <h3 className="sm2__card-title">{skill.name}</h3>
          <span>{skill.sourceType}</span>
        </div>
        <span className={`sm2__status-dot sm2__status-dot--${skill.status}`} />
      </div>
      <p className="sm2__card-desc">{skill.description || '（无描述）'}</p>
      <div className="sm2__card-tags">
        <span className={`sm2__tag sm2__tag--${skill.status}`}>{STATUS_LABEL[skill.status] || skill.status}</span>
        <span className="sm2__tag">{skill.skillType}</span>
      </div>
      <div className="sm2__card-foot">
        {skill.installedAgents.length > 0 && (
          <div className="sm2__agents">
            {skill.installedAgents.map((a) => (
              <AgentIconBadge key={a.agentId} iconKey={a.iconKey} mode={a.mode} title={`${a.displayName} · ${a.mode}`} />
            ))}
          </div>
        )}
        {skill.installedAgents.length === 0 && <span className="sm2__row-sub">尚未分发到 Agent</span>}
      </div>
    </div>
  )
}
