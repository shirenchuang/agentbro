import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { TFunction } from 'i18next'
import { skillApiV2 } from '../../services/skillApiV2'
import type { ConflictBlocker, DistributionPreview, AgentSummary, SkillSummary } from '../../services/skillApiV2'
import { PreviewDialog } from './PreviewDialog'
import { AgentIconBadge } from './AgentIconBadge'
import { skillModeLabel } from './skillLabels'

type BlockerDecision = 'overwrite' | 'agent_over_center' | 'skip'

function blockerKey(blocker: ConflictBlocker) {
  return `${blocker.skillId}\u0000${blocker.agentId}`
}

function blockerReason(t: TFunction, blocker: ConflictBlocker) {
  if (blocker.existingPath) {
    return t('skills.blocker.unmanagedExists', {
      skillId: blocker.skillId,
      defaultValue: blocker.reason,
    })
  }
  return blocker.reason
}

function distributionErrorMessage(t: TFunction, error: unknown) {
  const message = String(error)
  const directChild = message.match(/Target path '([^']+)' must be a direct child of (.+)\./)
  if (directChild) {
    return t('skills.errors.targetDirectChild', {
      targetPath: directChild[1],
      skillsDir: directChild[2],
      defaultValue: message,
    })
  }
  const noParent = message.match(/Target path '([^']+)' has no parent directory\./)
  if (noParent) {
    return t('skills.errors.targetNoParent', {
      targetPath: noParent[1],
      defaultValue: message,
    })
  }
  const nameMismatch = message.match(/Target path '([^']+)' does not match skill '([^']+)'\./)
  if (nameMismatch) {
    return t('skills.errors.targetNameMismatch', {
      targetPath: nameMismatch[1],
      skillId: nameMismatch[2],
      defaultValue: message,
    })
  }
  return message
}

function blockerPathKindLabel(t: TFunction, kind?: string | null) {
  if (!kind) return null
  return t(`skills.pathKind.${kind}`, { defaultValue: kind })
}

function isManagedCopyBlocker(blocker: ConflictBlocker) {
  return blocker.reason.startsWith('Managed copy ')
}

export function DistributeDialog({
  skill,
  skills,
  agents,
  defaultMode,
  onClose,
  onDone,
}: {
  skill?: SkillSummary
  skills?: SkillSummary[]
  agents: AgentSummary[]
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
  const selectedSkills = skills ?? (skill ? [skill] : [])
  const skillIds = selectedSkills.map((item) => item.id)
  const skillNameById = new Map(selectedSkills.map((item) => [item.id, item.name]))
  const isBatch = selectedSkills.length > 1
  const agentOrder = new Map(agents.map((agent, index) => [agent.id, index]))
  const installedRefsByAgent = new Map<string, SkillSummary['installedAgents']>()
  const installedRefByAgentSkill = new Map<string, SkillSummary['installedAgents'][number]>()
  for (const item of selectedSkills) {
    for (const installedAgent of item.installedAgents) {
      const refs = installedRefsByAgent.get(installedAgent.agentId) ?? []
      refs.push(installedAgent)
      installedRefsByAgent.set(installedAgent.agentId, refs)
      installedRefByAgentSkill.set(`${installedAgent.agentId}\u0000${item.id}`, installedAgent)
    }
  }
  const installedCountForAgent = (agentId: string) => installedRefsByAgent.get(agentId)?.length ?? 0
  const needsDistributionForAgent = (agentId: string) => selectedSkills.some((item) => {
    const ref = installedRefByAgentSkill.get(`${agentId}\u0000${item.id}`)
    return !ref || ref.mode !== mode
  })
  const visibleAgents = agents
    .filter((agent) => agent.installed)
    .sort((a, b) => {
      const installedDelta = installedCountForAgent(b.id) - installedCountForAgent(a.id)
      return installedDelta || (agentOrder.get(a.id) ?? 0) - (agentOrder.get(b.id) ?? 0)
    })
  const selectableAgents = visibleAgents.filter((agent) => needsDistributionForAgent(agent.id))
  const agentNameById = new Map(agents.map((agent) => [agent.id, agent.displayName]))

  useEffect(() => {
    setMode(defaultMode)
  }, [defaultMode])

  const toggle = (id: string) => {
    if (!needsDistributionForAgent(id)) return
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
      const p = await skillApiV2.previewDistribute(skillIds, Array.from(selected), mode)
      setPreview(p)
      setBlockerDecisions({})
    } catch (e) {
      setError(distributionErrorMessage(t, e))
    } finally {
      setBusy(false)
    }
  }

  const execute = async () => {
    if (!preview) return
    setBusy(true)
    setError(null)
    try {
      const blockerDecisionsPayload = preview.blockers
        .map((blocker) => {
          const action = blockerDecisions[blockerKey(blocker)]
          return action ? { skillId: blocker.skillId, agentId: blocker.agentId, action } : null
        })
        .filter((item): item is { skillId: string; agentId: string; action: BlockerDecision } => Boolean(item))
      await skillApiV2.executeDistribute({ ...preview, blockerDecisions: blockerDecisionsPayload })
      onDone()
      onClose()
    } catch (e) {
      setError(distributionErrorMessage(t, e))
    } finally {
      setBusy(false)
    }
  }

  if (!preview) {
    return (
      <PreviewDialog
        title={isBatch ? `批量分发 ${selectedSkills.length} 个 Skill` : `分发「${selectedSkills[0]?.name ?? ''}」`}
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
              <span>{isBatch ? selectedSkills.length : 'SK'}</span>
              <b>→</b>
              <span>AG</span>
            </div>
            <div className="sm2-distribute__summary-copy">
              <strong>{isBatch ? `${selectedSkills.length} 个 Skill` : selectedSkills[0]?.name}</strong>
              <span>{isBatch ? '从中心库批量分发到已安装 Hook 的 Agent' : '从中心库分发到已安装 Hook 的 Agent'}</span>
            </div>
            <div className="sm2-distribute__count">
              {selected.size}/{selectableAgents.length}
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
                <span className="sm2-distribute__mode-key">{skillModeLabel(t, 'link')}</span>
                <span className="sm2-distribute__mode-title">
                  {skillModeLabel(t, 'link')}
                  <span className="sm2-distribute__mode-badge">推荐</span>
                </span>
                <span className="sm2-distribute__mode-note">修改同步生效</span>
              </label>
              <label className={`sm2-distribute__mode${mode === 'copy' ? ' sm2-distribute__mode--active' : ''}`}>
                <input
                  type="radio"
                  name="distribute-mode"
                  value="copy"
                  checked={mode === 'copy'}
                  onChange={() => setMode('copy')}
                />
                <span className="sm2-distribute__mode-key">{skillModeLabel(t, 'copy')}</span>
                <span className="sm2-distribute__mode-title">{skillModeLabel(t, 'copy')}</span>
                <span className="sm2-distribute__mode-note">适合需要单独修改的 Agent</span>
              </label>
            </div>
          </section>

          <section className="sm2-distribute__section">
            <div className="sm2-distribute__section-head">
              <span>目标 Agent</span>
              <em>{selectableAgents.length} 个可选</em>
            </div>
            <div className="sm2-distribute__agent-list">
              {visibleAgents.length === 0 ? (
                <div className="sm2__empty sm2__empty--compact">没有可用的 Agent。请先在 Agent 管理中安装 Hook。</div>
              ) : (
                visibleAgents.map((a) => {
                  const installedRefs = installedRefsByAgent.get(a.id) ?? []
                  const checked = selected.has(a.id)
                  const disabled = !needsDistributionForAgent(a.id)
                  const installText = installedRefs.length === 0
                    ? 'Hook 已安装'
                    : isBatch
                      ? installedRefs.length >= selectedSkills.length
                        ? disabled
                          ? '已全部安装'
                          : `已全部安装 · 可转换为${skillModeLabel(t, mode)}`
                        : `${installedRefs.length}/${selectedSkills.length} 已安装`
                      : disabled
                        ? `已安装 · ${skillModeLabel(t, installedRefs[0].mode)}`
                        : `已安装 · 可转换为${skillModeLabel(t, mode)}`
                  return (
                    <label
                      key={a.id}
                      className={`sm2-distribute__agent${checked ? ' sm2-distribute__agent--active' : ''}${disabled ? ' sm2-distribute__agent--disabled sm2-distribute__agent--installed' : ''}`}
                    >
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={() => toggle(a.id)}
                        disabled={disabled}
                      />
                      <span className="sm2-distribute__check" aria-hidden="true">{checked ? '✓' : ''}</span>
                      <AgentIconBadge iconKey={a.iconKey} title={a.displayName} size={28} />
                      <span className="sm2-distribute__agent-main">
                        <strong>{a.displayName}</strong>
                        <span>{installText}</span>
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

  const unresolvedBlockers = preview.blockers.filter((blocker) => !blockerDecisions[blockerKey(blocker)]).length

  return (
    <PreviewDialog
      title="确认分发"
      confirmLabel={preview.blockers.length > 0 ? '按选择执行' : '执行分发'}
      modalClassName="sm2__modal--distribute"
      busy={busy}
      disabled={unresolvedBlockers > 0}
      onConfirm={execute}
      onCancel={() => setPreview(null)}
    >
      <div className="sm2-distribute sm2-distribute--preview">
        <div className="sm2-distribute__summary">
          <div className="sm2-distribute__flow" aria-hidden="true">
            <span>{skillModeLabel(t, preview.requestedMode)}</span>
            <b>→</b>
            <span>{preview.targetAgents.length}</span>
          </div>
          <div className="sm2-distribute__summary-copy">
            <strong>将影响 {preview.changes.length + preview.blockers.length} 个目标</strong>
            <span>
              {preview.blockers.length > 0
                ? unresolvedBlockers > 0 ? `还有 ${unresolvedBlockers} 个阻止项需要选择处理方式` : '阻止项已选择处理方式，可以继续'
                : '检查无阻止项，可以执行分发'}
            </span>
          </div>
        </div>

        <div className="sm2-distribute__preview-list">
          {preview.changes.map((c, i) => (
            <div key={i} className="sm2-distribute__change">
              <div className="sm2-distribute__change-main">
                <span className="sm2-distribute__change-action">
                  {c.action === 'create' ? '新增' : c.action === 'reuse' ? '复用' : c.action === 'convert' ? '转换' : c.action}
                </span>
                <strong>{skillNameById.get(c.skillId) ?? c.skillId} → {agentNameById.get(c.agentId) ?? c.agentId}</strong>
                {c.action === 'create' && <span className="sm2__tag sm2__tag--ok">{skillModeLabel(t, c.actualMode)}</span>}
              </div>
              <code>{c.targetPath}</code>
              {c.action !== 'create' && c.reason && <span className="sm2-distribute__change-reason">{c.reason}</span>}
            </div>
          ))}
          {preview.blockers.map((b) => {
            const key = blockerKey(b)
            const decision = blockerDecisions[key]
            const pathKind = blockerPathKindLabel(t, b.existingPathKind)
            const managedCopyBlocker = isManagedCopyBlocker(b)
            return (
            <div key={key} className="sm2-distribute__change sm2-distribute__change--blocked">
              <div className="sm2-distribute__change-main">
                <span className="sm2-distribute__change-action">阻止</span>
                <strong>{skillNameById.get(b.skillId) ?? b.skillId} → {agentNameById.get(b.agentId) ?? b.agentId}</strong>
                {decision && (
                  <span className="sm2__tag sm2__tag--unmanaged">
                    {decision === 'overwrite' ? managedCopyBlocker ? '中心库为准' : '将覆盖' : decision === 'agent_over_center' ? 'Agent 为准' : '将忽略'}
                  </span>
                )}
              </div>
              <span className="sm2-distribute__change-reason">{blockerReason(t, b)}</span>
              {b.existingPath && (
                <div className="sm2-distribute__path-row">
                  <code>{b.existingPath}</code>
                  {pathKind && <span className="sm2__tag sm2__tag--unmanaged">{pathKind}</span>}
                  <button type="button" className="sm2__btn sm2__btn--ghost" onClick={() => skillApiV2.openPath(b.existingPath!)}>
                    {t('skills.actions.openFolder', { defaultValue: 'Open Folder' })}
                  </button>
                </div>
              )}
              {b.resolvedExistingPath && (
                <div className="sm2__muted">{t('skills.labels.realPath', { defaultValue: 'Real path' })}：<code>{b.resolvedExistingPath}</code></div>
              )}
              <div className="sm2-distribute__decision-row" role="radiogroup" aria-label={`${agentNameById.get(b.agentId) ?? b.agentId} 阻止项处理方式`}>
                {b.existingPath && (
                  <button
                    type="button"
                    className={`sm2-distribute__decision${decision === 'overwrite' ? ' sm2-distribute__decision--active' : ''}`}
                    onClick={() => setBlockerDecisions((prev) => ({ ...prev, [key]: 'overwrite' }))}
                  >
                    {managedCopyBlocker ? '以中心库为准' : '覆盖安装'}
                  </button>
                )}
                {managedCopyBlocker && (
                  <button
                    type="button"
                    className={`sm2-distribute__decision${decision === 'agent_over_center' ? ' sm2-distribute__decision--active' : ''}`}
                    onClick={() => setBlockerDecisions((prev) => ({ ...prev, [key]: 'agent_over_center' }))}
                  >
                    以 Agent 为准
                  </button>
                )}
                <button
                  type="button"
                  className={`sm2-distribute__decision${decision === 'skip' ? ' sm2-distribute__decision--active' : ''}`}
                  onClick={() => setBlockerDecisions((prev) => ({ ...prev, [key]: 'skip' }))}
                >
                  忽略此目标
                </button>
              </div>
            </div>
            )
          })}
        </div>

        {preview.blockers.length > 0 && (
          <p className="sm2-distribute__blocked-note">
            未管理目标可覆盖后重新安装；已修改副本请先选择以中心库或 Agent 为准；忽略只跳过该目标。
          </p>
        )}
        {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
      </div>
    </PreviewDialog>
  )
}
