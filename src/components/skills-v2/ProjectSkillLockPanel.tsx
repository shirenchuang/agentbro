import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { skillApiV2, type ProjectLockedSkill, type ProjectSkillLock, type ProjectSkillRestoreResult } from '../../services/skillApiV2'
import { captureProjectView, useSkillStoreV2 } from '../../stores/skillStoreV2'

export function ProjectSkillLockPanel({ projectId, lock }: { projectId: string; lock: ProjectSkillLock }) {
  const { t } = useTranslation()
  const [selection, setSelection] = useState<string[]>([])
  const [results, setResults] = useState<ProjectSkillRestoreResult[]>([])
  const [verification, setVerification] = useState<{
    lock: ProjectSkillLock
    statuses: Map<string, ProjectLockedSkill['status']>
  } | null>(null)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState<'verify' | 'restore' | null>(null)
  const verifiedStatuses = verification?.lock === lock ? verification.statuses : null
  const ready = lock.skills.filter((skill) => skill.status === 'ready' && !verifiedStatuses?.has(skill.name))
  const selected = ready.filter((skill) => selection.includes(skill.name)).map((skill) => skill.name)
  const resultsByName = new Map(results.map((result) => [result.name, result]))
  const canVerify = lock.version === 2 && lock.skills.some((skill) => skill.status === 'existing')

  async function restore() {
    if (busy || !selected.length) return
    const view = captureProjectView(projectId)
    if (!view.isSelected()) return
    setBusy('restore')
    setError('')
    try {
      const outcomes = await skillApiV2.restoreProjectSkillLock(projectId, lock.revision, selected)
      if (view.isSelected()) setResults(outcomes)
      if (view.isRuntimeCurrent()) await useSkillStoreV2.getState().refreshProject(projectId)
    } catch (error) {
      if (view.isSelected()) setError(String(error))
    } finally {
      setBusy(null)
    }
  }

  async function verify() {
    if (busy || !canVerify) return
    const view = captureProjectView(projectId)
    if (!view.isSelected()) return
    setBusy('verify')
    setError('')
    setVerification(null)
    try {
      const statuses = await skillApiV2.verifyProjectSkillLock(projectId, lock.revision)
      if (view.isSelected()) setVerification({ lock, statuses: new Map(Object.entries(statuses)) })
    } catch (error) {
      if (view.isSelected()) setError(String(error))
    } finally {
      setBusy(null)
    }
  }

  return (
    <section className="sm2__project-skill-lock sm2__project-install-panel" aria-label="skills-lock.json">
      <header className="sm2__project-lock-actions">
        <strong>skills-lock.json {lock.version != null && <small>v{lock.version}</small>}</strong>
        <div className="sm2__btn-row">
          {canVerify && (
            <button type="button" className="sm2__btn" disabled={busy !== null} onClick={verify}>
              {busy === 'verify' ? t('skills.projectLock.verifying') : t('skills.projectLock.verify')}
            </button>
          )}
          {ready.length > 0 && (
            <>
              <label>
                <input
                  type="checkbox"
                  disabled={busy !== null}
                  checked={selected.length === ready.length}
                  onChange={(event) => setSelection(event.target.checked ? ready.map((skill) => skill.name) : [])}
                />
                {t('skills.projectLock.selectAll')}
              </label>
              <button type="button" className="sm2__btn sm2__btn--primary" disabled={busy !== null || !selected.length} onClick={restore}>
                {busy === 'restore' ? t('skills.projectLock.restoring') : t('skills.projectLock.restore', { count: selected.length })}
              </button>
            </>
          )}
        </div>
      </header>
      <p>{t('skills.projectLock.destination', { path: '.agents/skills' })}</p>
      {(error || lock.error) && <div className="sm2__error" role="alert">{error || lock.error}</div>}
      {!lock.error && !lock.skills.length && <p>{t('skills.projectLock.empty')}</p>}
      {lock.skills.length > 0 && (
        <ul className="sm2__project-lock-list" aria-live="polite">
          {lock.skills.map((skill) => {
            const result = resultsByName.get(skill.name)
            const status = verifiedStatuses?.get(skill.name)
              ?? (verifiedStatuses && skill.status === 'existing' ? 'unverified' : undefined)
            const statusLabel = !status && result && !result.error
              ? t('skills.projectLock.restored')
              : t('skills.projectLock.status.' + (status ?? skill.status))
            return (
              <li key={skill.name} className="sm2__project-lock-row">
                <label>
                  {skill.status === 'ready' && !status && (
                    <input
                      type="checkbox"
                      disabled={busy !== null}
                      checked={selection.includes(skill.name)}
                      onChange={(event) => {
                        const checked = event.target.checked
                        setSelection((previous) => checked
                          ? [...previous, skill.name]
                          : previous.filter((name) => name !== skill.name))
                      }}
                    />
                  )}
                  <strong>{skill.name}</strong>
                </label>
                <code>
                  {skill.source}
                  {skill.reference && <> · {skill.reference}</>}
                </code>
                <span>{statusLabel}</span>
                {result?.error && <p className="sm2__project-lock-error">{result.error}</p>}
              </li>
            )
          })}
        </ul>
      )}
    </section>
  )
}
