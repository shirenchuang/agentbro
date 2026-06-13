import { useEffect, useState } from 'react'
import { filteredSkills, useSkillStoreV2 } from '../../stores/skillStoreV2'
import { skillApiV2 } from '../../services/skillApiV2'
import type { SkillSummary, DeleteCenterSkillPreview } from '../../services/skillApiV2'
import { AgentIconBadge } from './AgentIconBadge'
import { AddSkillDialog } from './AddSkillDialog'
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
  const [addOpen, setAddOpen] = useState(false)
  const [distributeFor, setDistributeFor] = useState<SkillSummary | null>(null)
  const [sliderSkillId, setSliderSkillId] = useState<string | null>(null)
  const [deletePreview, setDeletePreview] = useState<DeleteCenterSkillPreview | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    state.init()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const refresh = () => state.refresh()
  const reload = async () => {
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
      setSliderSkillId(null)
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
          <button className="sm2__btn sm2__btn--primary" onClick={() => setAddOpen(true)}>+ 添加到中心库</button>
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

      {addOpen && <AddSkillDialog onClose={() => setAddOpen(false)} onDone={reload} />}
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
          {deletePreview.affectedTargets.length > 0 && (
            <div className="sm2__btn-row">
              <button className="sm2__btn sm2__btn--danger" disabled={busy} onClick={() => confirmDelete(true)}>
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

function SkillCard({ skill, onClick }: { skill: SkillSummary; onClick: () => void }) {
  return (
    <div className="sm2__card" onClick={onClick}>
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
