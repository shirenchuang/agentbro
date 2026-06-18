import { useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { filteredSkills, useSkillStoreV2 } from '../../stores/skillStoreV2'
import { skillApiV2 } from '../../services/skillApiV2'
import type { SkillSummary, DeleteCenterSkillPreview } from '../../services/skillApiV2'
import { AgentIconBadge } from './AgentIconBadge'
import { DistributeDialog } from './DistributeDialog'
import { SkillDetailSlider } from './SkillDetailSlider'
import { PreviewDialog } from './PreviewDialog'
import { skillSourceTypeLabel } from './skillLabels'

const STATUS_LABEL: Record<string, string> = {
  ok: '正常',
  conflict: '冲突',
  copyDiverged: '副本分叉',
  unmanaged: '未管理',
  updateAvailable: '可更新',
}

type FilterSelectOption = {
  value: string
  label: string
}

export function SkillLibraryPage() {
  const { t } = useTranslation()
  const state = useSkillStoreV2()
  const skills = filteredSkills(state)
  const overview = state.overview
  const startupScanInFlight = state.startupScanInFlight
  const [distributeFor, setDistributeFor] = useState<SkillSummary[] | null>(null)
  const [batchMode, setBatchMode] = useState(false)
  const [selectedSkillIds, setSelectedSkillIds] = useState<Set<string>>(new Set())
  const [sliderSkillId, setSliderSkillId] = useState<string | null>(null)
  const [deletePreview, setDeletePreview] = useState<DeleteCenterSkillPreview | null>(null)
  const [busy, setBusy] = useState(false)
  const sources = useMemo(
    () => Array.from(new Set(state.skills.map((s) => s.sourceType).filter(Boolean))).sort(),
    [state.skills],
  )
  const selectedSkills = useMemo(
    () => state.skills.filter((skill) => selectedSkillIds.has(skill.id)),
    [selectedSkillIds, state.skills],
  )
  const deleteLinkTargets = useMemo(
    () => deletePreview?.affectedTargets.filter((target) => target.mode === 'link') ?? [],
    [deletePreview],
  )
  const statusOptions = useMemo<FilterSelectOption[]>(() => [
    { value: '', label: '全部状态' },
    { value: 'ok', label: '正常' },
    { value: 'conflict', label: '冲突' },
    { value: 'copyDiverged', label: '副本分叉' },
  ], [])
  const sourceOptions = useMemo<FilterSelectOption[]>(() => [
    { value: '', label: '全部来源' },
    ...sources.map((source) => ({ value: source, label: skillSourceTypeLabel(t, source) })),
  ], [sources, t])

  useEffect(() => {
    state.init()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    setSelectedSkillIds((prev) => {
      const knownIds = new Set(state.skills.map((skill) => skill.id))
      const next = new Set(Array.from(prev).filter((id) => knownIds.has(id)))
      return next.size === prev.size ? prev : next
    })
  }, [state.skills])

  const refresh = () => state.refresh()
  const reload = async () => {
    await state.loadOverview(true)
  }

  const toggleBatchMode = () => {
    setBatchMode((prev) => {
      if (prev) setSelectedSkillIds(new Set())
      return !prev
    })
  }

  const toggleSkillSelection = (skillId: string) => {
    setSelectedSkillIds((prev) => {
      const next = new Set(prev)
      if (next.has(skillId)) next.delete(skillId)
      else next.add(skillId)
      return next
    })
  }

  const selectVisibleSkills = () => {
    setSelectedSkillIds((prev) => {
      const next = new Set(prev)
      for (const skill of skills) next.add(skill.id)
      return next
    })
  }

  const openBatchDistribute = () => {
    if (selectedSkills.length === 0) return
    setDistributeFor(selectedSkills)
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
          <button className={`sm2__btn${batchMode ? ' sm2__btn--active' : ''}`} onClick={toggleBatchMode}>
            {batchMode ? '完成选择' : '批量分发'}
          </button>
          <button className="sm2__btn" onClick={refresh} disabled={state.loading || startupScanInFlight}>
            {state.loading ? '刷新中…' : startupScanInFlight ? '后台同步中…' : '刷新'}
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
          <FilterSelect
            label="状态"
            value={state.filters.status}
            options={statusOptions}
            onChange={(value) => state.setFilter('status', value)}
          />
          <FilterSelect
            label="来源"
            value={state.filters.source}
            options={sourceOptions}
            onChange={(value) => state.setFilter('source', value)}
            wide
          />
          <div className="sm2__view-toggle sm2__view-toggle--soft">
            <button className={state.viewMode === 'cards' ? 'active' : ''} onClick={() => state.setViewMode('cards')}>卡片</button>
            <button className={state.viewMode === 'list' ? 'active' : ''} onClick={() => state.setViewMode('list')}>列表</button>
          </div>
        </div>
      </div>

      {batchMode && (
        <div className="sm2__batch-distribute-bar">
          <div className="sm2__batch-distribute-copy">
            <strong>已选择 {selectedSkillIds.size} 个 Skill</strong>
            <span>把多个 Skill 一次分发到同一组 Agent</span>
          </div>
          <div className="sm2__batch-distribute-actions">
            <button className="sm2__btn sm2__btn--ghost" onClick={selectVisibleSkills} disabled={skills.length === 0}>
              选择当前 {skills.length} 个
            </button>
            <button className="sm2__btn sm2__btn--ghost" onClick={() => setSelectedSkillIds(new Set())} disabled={selectedSkillIds.size === 0}>
              清空
            </button>
            <button className="sm2__btn sm2__btn--primary" onClick={openBatchDistribute} disabled={selectedSkillIds.size === 0}>
              分发 {selectedSkillIds.size} 个 Skill
            </button>
          </div>
        </div>
      )}

      {state.error && <div className="sm2__error">{state.error}</div>}

      <div className="sm2__main sm2__main--full">
        {state.loading && !overview ? (
          <div className="sm2__empty">加载 Skill 库…</div>
        ) : startupScanInFlight && skills.length === 0 ? (
          <div className="sm2__empty">正在后台同步 Skill 数据…</div>
        ) : skills.length === 0 ? (
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
                batchMode={batchMode}
                selected={selectedSkillIds.has(s.id)}
                onToggleSelection={() => toggleSkillSelection(s.id)}
                onClick={() => {
                  if (batchMode) toggleSkillSelection(s.id)
                  else setSliderSkillId(s.id)
                }}
              />
            ))}
          </div>
        ) : (
          <div className="sm2__list">
            {skills.map((s) => (
              <div
                key={s.id}
                className={`sm2__row${batchMode ? ' sm2__row--selectable' : ''}${selectedSkillIds.has(s.id) ? ' sm2__row--batch-selected' : ''}`}
                onClick={() => {
                  if (batchMode) toggleSkillSelection(s.id)
                  else setSliderSkillId(s.id)
                }}
              >
                {batchMode && (
                  <button
                    type="button"
                    className={`sm2__select-check${selectedSkillIds.has(s.id) ? ' sm2__select-check--active' : ''}`}
                    aria-label={`选择 ${s.name}`}
                    onClick={(event) => {
                      event.stopPropagation()
                      toggleSkillSelection(s.id)
                    }}
                  >
                    {selectedSkillIds.has(s.id) ? '✓' : ''}
                  </button>
                )}
                <div className="sm2__row-main">
                  <div className="sm2__row-title">{s.name}</div>
                  <div className="sm2__row-sub">{skillSourceTypeLabel(t, s.sourceType)} · {STATUS_LABEL[s.status] || s.status}</div>
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
        onDistribute={(s) => setDistributeFor([s])}
        onDelete={openDelete}
      />

      {distributeFor && state.settings && (
        <DistributeDialog
          skills={distributeFor}
          agents={state.agents}
          defaultMode={state.settings.defaultDistributeMode}
          onClose={() => setDistributeFor(null)}
          onDone={reload}
        />
      )}
      {deletePreview && (
        <PreviewDialog
          title={t('skills.deleteCenterSkill.title', { skillId: deletePreview.skillId })}
          confirmLabel={
            deletePreview.affectedTargets.length > 0
              ? t('skills.deleteCenterSkill.preserveCopies')
              : t('skills.deleteCenterSkill.deleteCenterOnly')
          }
          cancelLabel={t('skills.cancel')}
          busyLabel={t('skills.deleteCenterSkill.processing')}
          modalClassName="sm2__modal--delete-skill"
          destructive
          busy={busy}
          onCancel={() => setDeletePreview(null)}
          onConfirm={() => confirmDelete(false)}
          actions={(
            <>
              <button className="sm2__btn" onClick={() => setDeletePreview(null)} disabled={busy}>
                {t('skills.cancel')}
              </button>
              <button
                className="sm2__btn sm2-delete-skill__choice"
                onClick={() => confirmDelete(false)}
                disabled={busy}
              >
                {busy
                  ? t('skills.deleteCenterSkill.processing')
                  : deletePreview.affectedTargets.length > 0
                    ? t('skills.deleteCenterSkill.preserveCopies')
                    : t('skills.deleteCenterSkill.deleteCenterOnly')}
              </button>
              {deletePreview.affectedTargets.length > 0 && (
                <button
                  className="sm2__btn sm2-delete-skill__choice"
                  disabled={busy}
                  onClick={() => confirmDelete(true)}
                >
                  {busy ? t('skills.deleteCenterSkill.processing') : t('skills.deleteCenterSkill.removeAgentInstalls')}
                </button>
              )}
            </>
          )}
        >
          <div className="sm2-delete-skill">
            <div className="sm2-delete-skill__hero">
              <div className="sm2-delete-skill__mark" aria-hidden="true">!</div>
              <div className="sm2-delete-skill__hero-copy">
                <strong>{t('skills.deleteCenterSkill.confirmTitle')}</strong>
                <span>
                  {deletePreview.affectedTargets.length > 0
                    ? t('skills.deleteCenterSkill.impactWithTargets')
                    : t('skills.deleteCenterSkill.impactNoTargets')}
                </span>
              </div>
              <div className="sm2-delete-skill__impact">
                <strong>{deletePreview.affectedTargets.length}</strong>
                <span>{t('skills.deleteCenterSkill.agentInstalls')}</span>
              </div>
            </div>

            {deletePreview.affectedTargets.length > 0 ? (
              <div className="sm2-delete-skill__targets">
                {deletePreview.affectedTargets.map((target) => (
                  <div key={target.targetId} className="sm2-delete-skill__target">
                    <div>
                      <strong>{target.displayName || target.agentId}</strong>
                      <span>{t(`skills.mode.${target.mode}`, { defaultValue: target.mode })} · {t('skills.deleteCenterSkill.referenceCount', { count: target.claimCount })}</span>
                    </div>
                    <code>{target.targetPath}</code>
                  </div>
                ))}
              </div>
            ) : (
              <div className="sm2-delete-skill__zero">
                {t('skills.deleteCenterSkill.noTargets')}
              </div>
            )}

            {deleteLinkTargets.length > 0 && (
              <div className="sm2-delete-skill__note">
                {deleteLinkTargets.map((target) => (
                  <span key={target.targetId}>
                    {t('skills.deleteCenterSkill.linkWarning', { agent: target.displayName || target.agentId })}
                  </span>
                ))}
              </div>
            )}

          </div>
        </PreviewDialog>
      )}
    </div>
  )
}

function FilterSelect({
  label,
  value,
  options,
  onChange,
  wide = false,
}: {
  label: string
  value: string
  options: FilterSelectOption[]
  onChange: (value: string) => void
  wide?: boolean
}) {
  const [open, setOpen] = useState(false)
  const rootRef = useRef<HTMLDivElement>(null)
  const selected = options.find((option) => option.value === value) ?? options[0]

  useEffect(() => {
    if (!open) return
    const closeOnOutside = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false)
    }
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false)
    }
    window.addEventListener('pointerdown', closeOnOutside)
    window.addEventListener('keydown', closeOnEscape)
    return () => {
      window.removeEventListener('pointerdown', closeOnOutside)
      window.removeEventListener('keydown', closeOnEscape)
    }
  }, [open])

  return (
    <div ref={rootRef} className={`sm2__filter-select${wide ? ' sm2__filter-select--wide' : ''}${open ? ' sm2__filter-select--open' : ''}`}>
      <button
        type="button"
        className="sm2__filter-select-button"
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((current) => !current)}
      >
        <span className="sm2__filter-select-label">{label}</span>
        <span className="sm2__filter-select-value">{selected.label}</span>
        <span className="sm2__filter-select-caret" aria-hidden="true">⌄</span>
      </button>
      {open && (
        <div className="sm2__filter-select-menu" role="listbox" aria-label={label}>
          {options.map((option) => (
            <button
              key={option.value || '__all'}
              type="button"
              className={`sm2__filter-select-option${option.value === value ? ' sm2__filter-select-option--active' : ''}`}
              role="option"
              aria-selected={option.value === value}
              onClick={() => {
                onChange(option.value)
                setOpen(false)
              }}
            >
              <span>{option.label}</span>
              {option.value === value && <b aria-hidden="true">✓</b>}
            </button>
          ))}
        </div>
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
  batchMode,
  selected,
  onToggleSelection,
  onClick,
}: {
  skill: SkillSummary
  batchMode: boolean
  selected: boolean
  onToggleSelection: () => void
  onClick: () => void
}) {
  const { t } = useTranslation()
  return (
    <div className={`sm2__card${batchMode ? ' sm2__card--selectable' : ''}${selected ? ' sm2__card--batch-selected' : ''}`} onClick={onClick}>
      <div className="sm2__card-head">
        <div className="sm2__skill-avatar">{(skill.name || 'SK').slice(0, 2).toUpperCase()}</div>
        <div className="sm2__card-titleblock">
          <h3 className="sm2__card-title">{skill.name}</h3>
          <span>{skillSourceTypeLabel(t, skill.sourceType)}</span>
        </div>
        {batchMode ? (
          <button
            type="button"
            className={`sm2__select-check${selected ? ' sm2__select-check--active' : ''}`}
            aria-label={`选择 ${skill.name}`}
            onClick={(event) => {
              event.stopPropagation()
              onToggleSelection()
            }}
          >
            {selected ? '✓' : ''}
          </button>
        ) : (
          <span className={`sm2__status-dot sm2__status-dot--${skill.status}`} />
        )}
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
