import { useEffect, useState } from 'react'
import { useSkillStoreV2 } from '../../stores/skillStoreV2'
import { skillApiV2 } from '../../services/skillApiV2'
import type { DiagnosisIssue } from '../../services/skillApiV2'

const SEVERITY_LABEL: Record<string, string> = {
  info: '提示',
  warning: '警告',
  error: '错误',
}

export function DiagnosisPage() {
  const state = useSkillStoreV2()
  const [busy, setBusy] = useState(false)
  const [filter, setFilter] = useState<'all' | 'auto' | 'confirm' | 'info'>('all')
  const [notice, setNotice] = useState<string | null>(null)

  useEffect(() => {
    state.runDiagnosis()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const issues = state.issues.filter((i) => filter === 'all' || i.fixKind === filter)

  const fix = async (issue: DiagnosisIssue) => {
    if (issue.fixKind === 'confirm') {
      if (!confirm(`${issue.title}\n\n${issue.detail}\n\n确认执行？此操作可能不可逆。`)) return
    }
    setBusy(true)
    setNotice(null)
    try {
      await skillApiV2.executeFixIssue(issue.issueType, issue.entityId || '')
      await state.runDiagnosis()
      setNotice('修复成功')
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
      setNotice(`已修复 ${n} 个低风险项`)
    } catch (e) {
      state.setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="sm2">
      <div className="sm2__header">
        <h2 className="sm2__title">诊断与修复</h2>
        <div className="sm2__tabs">
          <button className="sm2__btn" onClick={() => state.runDiagnosis()} disabled={busy}>运行诊断</button>
          <button className="sm2__btn sm2__btn--primary" onClick={safeFix} disabled={busy}>一键修复安全项</button>
        </div>
      </div>

      {notice && <div className="sm2__notice sm2__notice--ok">{notice}</div>}
      {state.error && <div className="sm2__error">{state.error}</div>}

      <div className="sm2__toolbar">
        <select className="sm2__select" value={filter} onChange={(e) => setFilter(e.target.value as 'all' | 'auto' | 'confirm' | 'info')}>
          <option value="all">全部（{state.issues.length}）</option>
          <option value="auto">可一键修复</option>
          <option value="confirm">需确认</option>
          <option value="info">仅提示</option>
        </select>
        <button className="sm2__btn sm2__btn--ghost" onClick={() => skillApiV2.exportSnapshot()}>刷新 JSON 快照</button>
      </div>

      <div className="sm2__main">
        {issues.length === 0 ? (
          <div className="sm2__empty">没有发现问题。中心库与各 Agent 状态一致。</div>
        ) : (
          issues.map((issue) => (
            <div key={issue.id} className="sm2__issue">
              <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                <p className="sm2__issue-title">{issue.title}</p>
                <span className={`sm2__tag sm2__tag--${issue.severity === 'error' ? 'conflict' : issue.severity === 'warning' ? 'copyDiverged' : 'unmanaged'}`}>
                  {SEVERITY_LABEL[issue.severity]}
                </span>
              </div>
              <p className="sm2__issue-detail">{issue.detail}</p>
              {issue.fixKind !== 'info' && (
                <button
                  className={`sm2__btn ${issue.fixKind === 'confirm' ? 'sm2__btn--danger' : 'sm2__btn--primary'}`}
                  disabled={busy}
                  onClick={() => fix(issue)}
                >
                  {issue.actions[0]?.label || '修复'}
                </button>
              )}
            </div>
          ))
        )}
      </div>
    </div>
  )
}
