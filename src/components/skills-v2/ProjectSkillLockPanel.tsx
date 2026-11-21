import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { skillApiV2, type ProjectSkillLock, type ProjectSkillRestoreResult } from '../../services/skillApiV2'
import { captureProjectView, useSkillStoreV2 } from '../../stores/skillStoreV2'

export function ProjectSkillLockPanel({ projectId, lock }: { projectId: string; lock: ProjectSkillLock }) {
  const { t } = useTranslation()
  const [selection, setSelection] = useState<string[]>([])
  const [results, setResults] = useState<ProjectSkillRestoreResult[]>([])
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const ready = lock.skills.filter((skill) => skill.status === 'ready')
  const selected = ready.filter((skill) => selection.includes(skill.name)).map((skill) => skill.name)
  const resultsByName = new Map(results.map((result) => [result.name, result]))

  async function restore() {
    if (busy || !selected.length) return
    const view = captureProjectView(projectId)
    if (!view.isSelected()) return
    setBusy(true)
    setError('')
    try {
      const outcomes = await skillApiV2.restoreProjectSkillLock(projectId, lock.revision, selected)
      if (view.isSelected()) setResults(outcomes)
      if (view.isRuntimeCurrent()) await useSkillStoreV2.getState().refreshProject(projectId)
    } catch (error) {
      if (view.isSelected()) setError(String(error))
    } finally {
      setBusy(false)
    }
  }

  return (
    <section className="sm2__project-skill-lock sm2__project-install-panel" aria-label="skills-lock.json">
      <header className="sm2__project-lock-actions">
        <strong>skills-lock.json {lock.version != null && <small>v{lock.version}</small>}</strong>
        {ready.length > 0 && (
          <div className="sm2__btn-row">
            <label>
              <input
                type="checkbox"
                disabled={busy}
                checked={selected.length === ready.length}
                onChange={(event) => setSelection(event.target.checked ? ready.map((skill) => skill.name) : [])}
              />
              {t('skills.projectLock.selectAll')}
            </label>
            <button type="button" className="sm2__btn sm2__btn--primary" disabled={busy || !selected.length} onClick={restore}>
              {busy ? t('skills.projectLock.restoring') : t('skills.projectLock.restore', { count: selected.length })}
            </button>
          </div>
        )}
      </header>
      <p>{t('skills.projectLock.destination', { path: '.agents/skills' })}</p>
      {(error || lock.error) && <div className="sm2__error" role="alert">{error || lock.error}</div>}
      {!lock.error && !lock.skills.length && <p>{t('skills.projectLock.empty')}</p>}
      {lock.skills.length > 0 && (
        <ul className="sm2__project-lock-list" aria-live="polite">
          {lock.skills.map((skill) => {
            const result = resultsByName.get(skill.name)
            return (
              <li key={skill.name} className="sm2__project-lock-row">
                <label>
                  {skill.status === 'ready' && (
                    <input
                      type="checkbox"
                      disabled={busy}
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
                <span>{result && !result.error ? t('skills.projectLock.restored') : t('skills.projectLock.status.' + skill.status)}</span>
                {result?.error && <p className="sm2__project-lock-error">{result.error}</p>}
              </li>
            )
          })}
        </ul>
      )}
    </section>
  )
}
