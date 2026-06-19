import { useEffect, useMemo, useState } from 'react'
import { useSkillStoreV2 } from '../../stores/skillStoreV2'
import { skillApiV2 } from '../../services/skillApiV2'
import type { DiagnosisIssue } from '../../services/skillApiV2'

const FILTER_LABEL: Record<DiagnosisFilter, string> = {
  all: '全部',
  auto: '可安全修复',
  confirm: '需要你确认',
  info: '仅提示',
}

const ISSUE_GROUPS: Array<{ id: IssueGroupId; title: string; description: string }> = [
  {
    id: 'unmanaged',
    title: '未接管的 Skill',
    description: '这些 Skill 已经在某个 Agent 目录里，但还没有交给 AgentBro 管理。AgentBro 会先提示，不会擅自覆盖。',
  },
  {
    id: 'sync',
    title: '同步与快照',
    description: '这些问题通常可以安全处理，用来让中心库、Agent 目录和 JSON 快照重新对齐。',
  },
  {
    id: 'confirm',
    title: '需要你决定',
    description: '这些项可能涉及本地修改或冲突，需要确认保留哪一份内容。',
  },
  {
    id: 'library',
    title: '中心库整理',
    description: '这些提示来自中心库或 Skill 包关系，通常用于提醒你补录或清理管理信息。',
  },
]

type DiagnosisFilter = 'all' | 'auto' | 'confirm' | 'info'
type IssueGroupId = 'unmanaged' | 'sync' | 'confirm' | 'library'

export function DiagnosisPage() {
  const state = useSkillStoreV2()
  const [busy, setBusy] = useState(false)
  const [filter, setFilter] = useState<DiagnosisFilter>('all')
  const [notice, setNotice] = useState<string | null>(null)

  useEffect(() => {
    state.loadDiagnosisIssues()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const stats = useMemo(() => summarizeIssues(state.issues), [state.issues])
  const visibleIssues = useMemo(
    () => state.issues.filter((issue) => filter === 'all' || issue.fixKind === filter),
    [filter, state.issues],
  )
  const groups = useMemo(() => groupIssues(visibleIssues), [visibleIssues])
  const busyNow = busy || state.busyAction === 'diagnosis'
  const statusTitle = state.issues.length === 0 ? 'Skill 状态正常' : 'Skill 状态需要整理'
  const statusDetail = state.issues.length === 0
    ? '中心库与 Agent 目录当前一致。若刚手动安装过 Skill，可以重新检查。'
    : `${state.issues.length} 项需要查看，其中 ${stats.auto} 项可以安全修复。`

  const runDiagnosis = async () => {
    setNotice(null)
    await state.runDiagnosis()
  }

  const fix = async (issue: DiagnosisIssue) => {
    if (issue.fixKind === 'confirm') {
      const text = `${friendlyTitle(issue)}\n\n${friendlyDetail(issue)}\n\n确认执行？此操作可能会改写 Skill 内容。`
      if (!confirm(text)) return
    }
    setBusy(true)
    setNotice(null)
    try {
      await skillApiV2.executeFixIssue(issue.issueType, issue.entityId || '')
      await state.runDiagnosis()
      setNotice('已处理 1 项问题。')
    } catch (e) {
      state.setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  const safeFix = async () => {
    setBusy(true)
    setNotice(null)
    try {
      const n = await skillApiV2.executeSafeFixes()
      await state.runDiagnosis()
      const remaining = useSkillStoreV2.getState().issues.length
      setNotice(n === 0 ? '没有可自动处理的安全问题。' : `已处理 ${n} 项安全问题，还剩 ${remaining} 项需要查看。`)
    } catch (e) {
      state.setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="sm2 sm2--diagnosis">
      <div className="sm2__diagnosis-hero">
        <div className="sm2__diagnosis-hero-main">
          <span className={`sm2__diagnosis-status sm2__diagnosis-status--${state.issues.length === 0 ? 'ok' : 'warn'}`}>
            {state.issues.length === 0 ? '状态正常' : '需要处理'}
          </span>
          <h2 className="sm2__title sm2__diagnosis-title">{statusTitle}</h2>
          <p>{statusDetail}</p>
          {stats.auto > 0 && (
            <p className="sm2__diagnosis-safe-note">
              安全修复只会清理失效记录、断开的链接或刷新快照，不会删除你的 Skill 内容。
            </p>
          )}
        </div>
        <div className="sm2__diagnosis-actions">
          <button className="sm2__btn" onClick={runDiagnosis} disabled={busyNow}>
            重新检查
          </button>
          <button className="sm2__btn sm2__btn--primary" onClick={safeFix} disabled={busyNow || stats.auto === 0}>
            修复安全项
          </button>
        </div>
      </div>

      <div className="sm2__diagnosis-metrics" aria-label="诊断摘要">
        <DiagnosisMetric value={stats.auto} label="可安全修复" tone={stats.auto > 0 ? 'warn' : 'ok'} />
        <DiagnosisMetric value={stats.confirm} label="需要你确认" tone={stats.confirm > 0 ? 'danger' : 'ok'} />
        <DiagnosisMetric value={stats.info} label="仅提示" tone={stats.info > 0 ? 'muted' : 'ok'} />
      </div>

      {notice && <div className="sm2__notice sm2__notice--ok">{notice}</div>}
      {state.error && <div className="sm2__error">{state.error}</div>}

      <div className="sm2__toolbar sm2__diagnosis-toolbar">
        <div className="sm2__diagnosis-filter" role="group" aria-label="问题筛选">
          {(['all', 'auto', 'confirm', 'info'] as DiagnosisFilter[]).map((item) => (
            <button
              key={item}
              type="button"
              className={`sm2__tab ${filter === item ? 'sm2__tab--active' : ''}`}
              onClick={() => setFilter(item)}
            >
              {FILTER_LABEL[item]}（{filterCount(item, state.issues.length, stats)}）
            </button>
          ))}
        </div>
        <button className="sm2__btn sm2__btn--ghost" onClick={() => skillApiV2.exportSnapshot()}>
          刷新 JSON 快照
        </button>
      </div>

      <div className="sm2__main sm2__main--full sm2__diagnosis-main">
        {visibleIssues.length === 0 ? (
          <div className="sm2__empty sm2__diagnosis-empty">
            <strong>{filter === 'all' ? '没有发现需要处理的问题' : `没有${FILTER_LABEL[filter]}项`}</strong>
            <span>中心库与 Agent 目录当前一致。若刚手动安装过 Skill，可以重新检查。</span>
          </div>
        ) : (
          ISSUE_GROUPS.map((group) => {
            const items = groups[group.id]
            if (items.length === 0) return null
            return (
              <section key={group.id} className="sm2__diagnosis-group">
                <div className="sm2__diagnosis-group-head">
                  <div>
                    <h3>{group.title}</h3>
                    <p>{group.description}</p>
                  </div>
                  <span className="sm2__tag">{items.length} 项</span>
                </div>
                <div className="sm2__diagnosis-list">
                  {items.map((issue) => (
                    <IssueCard key={issue.id} issue={issue} busy={busyNow} onFix={fix} />
                  ))}
                </div>
              </section>
            )
          })
        )}
      </div>
    </div>
  )
}

function DiagnosisMetric({ value, label, tone }: { value: number; label: string; tone: 'ok' | 'warn' | 'danger' | 'muted' }) {
  return (
    <div className={`sm2__diagnosis-metric sm2__diagnosis-metric--${tone}`}>
      <strong>{value} 项</strong>
      <span>{label}</span>
    </div>
  )
}

function IssueCard({ issue, busy, onFix }: { issue: DiagnosisIssue; busy: boolean; onFix: (issue: DiagnosisIssue) => void }) {
  return (
    <article className={`sm2__diagnosis-issue sm2__diagnosis-issue--${issue.fixKind}`}>
      <div className="sm2__diagnosis-issue-body">
        <div className="sm2__diagnosis-issue-title-row">
          <h4>{friendlyTitle(issue)}</h4>
          <span className={`sm2__tag sm2__tag--${issueTagTone(issue)}`}>{fixKindLabel(issue)}</span>
        </div>
        <p>{friendlyDetail(issue)}</p>
        {issue.entityId && <code>{issue.entityId}</code>}
      </div>
      <div className="sm2__diagnosis-issue-actions">
        {issue.fixKind === 'info' ? (
          <span className="sm2__diagnosis-hint">{infoActionHint(issue)}</span>
        ) : (
          <button
            className={`sm2__btn ${issue.fixKind === 'confirm' ? 'sm2__btn--danger' : 'sm2__btn--primary'}`}
            disabled={busy}
            onClick={() => onFix(issue)}
          >
            {friendlyActionLabel(issue)}
          </button>
        )}
      </div>
    </article>
  )
}

function summarizeIssues(issues: DiagnosisIssue[]) {
  return {
    auto: issues.filter((issue) => issue.fixKind === 'auto').length,
    confirm: issues.filter((issue) => issue.fixKind === 'confirm').length,
    info: issues.filter((issue) => issue.fixKind === 'info').length,
  }
}

function filterCount(filter: DiagnosisFilter, total: number, stats: ReturnType<typeof summarizeIssues>): number {
  return filter === 'all' ? total : stats[filter]
}

function groupIssues(issues: DiagnosisIssue[]): Record<IssueGroupId, DiagnosisIssue[]> {
  return issues.reduce<Record<IssueGroupId, DiagnosisIssue[]>>(
    (acc, issue) => {
      acc[groupForIssue(issue)].push(issue)
      return acc
    },
    { unmanaged: [], sync: [], confirm: [], library: [] },
  )
}

function groupForIssue(issue: DiagnosisIssue): IssueGroupId {
  if (issue.issueType === 'agent_unmanaged') return 'unmanaged'
  if (issue.fixKind === 'confirm') return 'confirm'
  if (['broken_link', 'target_missing', 'orphan_claim', 'snapshot_stale', 'agents_managed_duplicate'].includes(issue.issueType)) return 'sync'
  return 'library'
}

function friendlyTitle(issue: DiagnosisIssue): string {
  switch (issue.issueType) {
    case 'agent_unmanaged':
      return `发现未接管 Skill${agentNameFromTitle(issue.title) ? ` · ${agentNameFromTitle(issue.title)}` : ''}`
    case 'snapshot_stale':
      return 'JSON 快照需要刷新'
    case 'broken_link':
      return '发现断开的 Skill 链接'
    case 'target_missing':
      return '发现失效的安装记录'
    case 'agents_managed_duplicate':
      return '.agents 里有已管理的重复 Skill'
    case 'orphan_claim':
      return '发现失效的 Skill 包占用记录'
    case 'copy_diverged':
      return '中心库和 Agent 副本都发生了修改'
    case 'copy_outdated':
      return 'Agent 副本落后于中心库'
    case 'copy_modified':
      return 'Agent 副本有本地修改'
    case 'center_unmanaged':
      return '中心库里有未登记的 Skill'
    case 'pack_member_missing':
      return 'Skill 包引用了缺失的成员'
    default:
      return issue.title || '需要查看的问题'
  }
}

function friendlyDetail(issue: DiagnosisIssue): string {
  if (issue.issueType === 'agent_unmanaged') {
    const path = pathFromDetail(issue.detail)
    const reason = reasonFromDetail(issue.detail)
    return `${path ? `${path}。` : ''}${unmanagedReasonText(reason)}`
  }
  if (issue.issueType === 'snapshot_stale') {
    return '中心库已经变化，JSON 快照还停留在旧版本。刷新后，外部工具和人工排查会看到最新状态。'
  }
  if (issue.issueType === 'broken_link') {
    return `${quotedPath(issue.detail)} 指向的 Skill 已不存在，可以清理这条断开的链接。`
  }
  if (issue.issueType === 'target_missing') {
    return `${quotedPath(issue.detail)} 已不在磁盘上，可以移除这条过期记录。`
  }
  if (issue.issueType === 'agents_managed_duplicate') {
    return `${quotedPath(issue.detail)} 已由中心库管理。建议删除 .agents/skills 里的这份重复目标，避免多个 Agent 隐式加载旧副本。`
  }
  if (issue.issueType === 'orphan_claim') {
    return '某个 Skill 包还占用着已不存在的安装目标，可以安全移除这条占用记录。'
  }
  if (issue.issueType === 'copy_diverged') {
    return '中心库和 Agent 里的副本都改过，需要你决定以哪一份为准。'
  }
  if (issue.issueType === 'copy_outdated') {
    return `${quotedPath(issue.detail)} 可以从中心库更新，但会改写 Agent 目录里的副本。`
  }
  if (issue.issueType === 'copy_modified') {
    return `${quotedPath(issue.detail)} 和中心库快照不同，需要确认是否把这份本地修改推回中心库。`
  }
  if (issue.issueType === 'center_unmanaged') {
    return '这个目录看起来是 Skill，但还没有进入 AgentBro 的中心库索引。'
  }
  if (issue.issueType === 'pack_member_missing') {
    return '某个 Skill 包引用了中心库里不存在的 Skill，安装这个包时可能缺少内容。'
  }
  return stripInternalReason(issue.detail)
}

function unmanagedReasonText(reason: string): string {
  switch (reason) {
    case 'same_name_as_center_skill':
      return '本地已有同名 Skill，AgentBro 暂时不会接管，避免覆盖你的内容。'
    case 'not_in_center_library':
      return '这个 Skill 不在中心库里。你可以在 Agent 管理页把它导入中心库，之后再统一分发。'
    case 'path_conflict':
      return '这个路径和现有管理记录冲突，需要先确认保留哪一份。'
    case '':
      return 'AgentBro 还没有接管这个 Skill。需要统一管理时，可以去 Agent 管理页接管。'
    default:
      return 'AgentBro 还没有接管这个 Skill。需要统一管理时，可以去 Agent 管理页接管。'
  }
}

function friendlyActionLabel(issue: DiagnosisIssue): string {
  switch (issue.issueType) {
    case 'snapshot_stale':
      return '刷新快照'
    case 'broken_link':
      return '清理断开链接'
    case 'target_missing':
      return '移除失效记录'
    case 'agents_managed_duplicate':
      return '删除 .agents 重复项'
    case 'orphan_claim':
      return '移除占用记录'
    case 'copy_modified':
      return '推回中心库'
    case 'copy_outdated':
      return '更新副本'
    case 'copy_diverged':
      return '处理冲突'
    default:
      return issue.actions[0]?.label || '处理'
  }
}

function fixKindLabel(issue: DiagnosisIssue): string {
  if (issue.fixKind === 'auto') return '可安全修复'
  if (issue.fixKind === 'confirm') return '需要确认'
  if (issue.fixKind === 'info') return '仅提示'
  return '手动处理'
}

function issueTagTone(issue: DiagnosisIssue): string {
  if (issue.fixKind === 'auto') return 'copyDiverged'
  if (issue.fixKind === 'confirm') return 'conflict'
  if (issue.fixKind === 'info') return 'unmanaged'
  return 'ok'
}

function infoActionHint(issue: DiagnosisIssue): string {
  if (issue.issueType === 'agent_unmanaged') return '去 Agent 管理页接管'
  if (issue.issueType === 'center_unmanaged') return '导入中心库后即可管理'
  return '查看后按需处理'
}

function agentNameFromTitle(title: string): string {
  const prefix = 'Unmanaged skill in '
  return title.startsWith(prefix) ? title.slice(prefix.length).trim() : ''
}

function pathFromDetail(detail: string): string {
  const [path] = detail.split(' — reason:')
  return path?.trim() ?? ''
}

function reasonFromDetail(detail: string): string {
  const marker = 'reason:'
  const index = detail.indexOf(marker)
  return index >= 0 ? detail.slice(index + marker.length).trim() : ''
}

function quotedPath(detail: string): string {
  const match = detail.match(/'([^']+)'/)
  return match?.[1] ?? '目标路径'
}

function stripInternalReason(detail: string): string {
  return detail.replace(/\s+— reason:\s+[a-z0-9_/-]+/gi, '').trim()
}
