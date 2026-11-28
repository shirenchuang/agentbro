import { useEffect, useRef, useState } from 'react'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { open } from '@tauri-apps/plugin-dialog'
import { useTranslation } from 'react-i18next'
import { skillApiV2, type ProjectDiscovery, type ProjectDiscoverySource } from '../../services/skillApiV2'
import { isTauri } from '../../services/tauriApi'
import { LOCAL_RUNTIME_ENVIRONMENT_ID } from '../../stores/runtimeEnvironmentStore'
import { captureProjectView, useSkillStoreV2 } from '../../stores/skillStoreV2'
import { isWindowsPlatform } from '../../utils/platform'
import { PreviewDialog } from './PreviewDialog'

const sourceNames: Record<ProjectDiscoverySource, string> = {
  codex: 'Codex', 'claude-code': 'Claude Code', zcode: 'ZCode', antigravity: 'Antigravity',
}

export function ProjectImportPanel({ environmentId }: { environmentId: string }) {
  const { t } = useTranslation()
  const native = isTauri() && environmentId === LOCAL_RUNTIME_ENVIRONMENT_ID
  const addProjects = useSkillStoreV2((state) => state.addProjects)
  const [path, setPath] = useState('')
  const [discovering, setDiscovering] = useState(false)
  const [adding, setAdding] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [dragOver, setDragOver] = useState(false)
  const dropZone = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!native) return
    const view = captureProjectView(null)
    let disposed = false
    let unlisten: (() => void) | undefined
    const listen = async () => getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (disposed || !view.isRuntimeCurrent()) return
      const bounds = dropZone.current?.getBoundingClientRect()
      if (!bounds || payload.type === 'leave') {
        setDragOver(false)
        return
      }
      const scale = window.devicePixelRatio || 1
      const x = payload.position.x / scale
      const y = payload.position.y / scale
      const inside = x >= bounds.left && x < bounds.right && y >= bounds.top && y < bounds.bottom
      setDragOver(inside && payload.type !== 'drop')
      if (!inside || payload.type !== 'drop') return
      if (payload.paths.length === 1) {
        setPath(payload.paths[0])
        setError(null)
      } else {
        setError(t('skills.projectImport.oneFolder'))
      }
    })
    void listen().then((release) => {
      if (disposed) release()
      else unlisten = release
    }).catch(() => {})
    return () => {
      disposed = true
      unlisten?.()
    }
  }, [native, t])

  const submit = async (choose: boolean) => {
    const view = captureProjectView(null)
    setAdding(true)
    setError(null)
    try {
      const selected = choose ? await open({ directory: true, multiple: false }) : path.trim()
      if (typeof selected !== 'string' || !selected || !view.isRuntimeCurrent()) return
      if (choose) setPath(selected)
      const [result] = await addProjects([selected])
      if (!result || !view.isRuntimeCurrent()) return
      if (result.error) setError(result.error)
      else setPath((current) => current.trim() === selected ? '' : current)
    } catch (cause) {
      if (view.isRuntimeCurrent()) setError(String(cause))
    } finally {
      if (view.isRuntimeCurrent()) setAdding(false)
    }
  }

  return (
    <div className={`sm2__project-import${dragOver ? ' sm2__project-import--dragging' : ''}`} ref={dropZone}>
      {native && (
        <div className="sm2__btn-row sm2__project-import-actions">
          <button className="sm2__btn sm2__btn--primary" disabled={adding} onClick={() => void submit(true)}>
            {t('skills.projectImport.chooseFolder')}
          </button>
          <button className="sm2__btn" disabled={adding} onClick={() => setDiscovering(true)}>{t('skills.projectImport.discover')}</button>
        </div>
      )}
      <form onSubmit={(event) => { event.preventDefault(); if (path.trim() && !adding) void submit(false) }}>
        <label htmlFor="skill-project-root">{t(native ? 'skills.projectImport.localPath' : 'skills.projectImport.path')}</label>
        <div className="sm2__project-import-row">
          <input
            id="skill-project-root"
            className="sm2__search sm2__search--full"
            value={path}
            onChange={(event) => { setPath(event.target.value); setError(null) }}
            placeholder={isWindowsPlatform() ? 'C:\\Users\\me\\work\\repo' : '/Users/me/work/repo'}
          />
          <button className="sm2__btn" disabled={!path.trim() || adding} type="submit">
            {t(adding ? 'skills.projectImport.adding' : 'skills.projectImport.add')}
          </button>
        </div>
      </form>
      {error && <div className="sm2__error" role="alert">{error}</div>}
      {discovering && <ProjectDiscoveryDialog onClose={() => setDiscovering(false)} />}
    </div>
  )
}

function ProjectDiscoveryDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation()
  const addProjects = useSkillStoreV2((state) => state.addProjects)
  const selectProject = useSkillStoreV2((state) => state.selectProject)
  const [data, setData] = useState<ProjectDiscovery | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [selected, setSelected] = useState<string[]>([])
  const [results, setResults] = useState<Record<string, string | null>>({})
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    const view = captureProjectView(null)
    let disposed = false
    void skillApiV2.discoverProjects().then((report) => {
      if (!disposed && view.isRuntimeCurrent()) setData(report)
    }).catch((cause: unknown) => {
      if (!disposed && view.isRuntimeCurrent()) setError(String(cause))
    })
    return () => { disposed = true }
  }, [])

  const addSelected = async () => {
    const view = captureProjectView(null)
    setBusy(true)
    try {
      const added = await addProjects(selected)
      if (!view.isRuntimeCurrent()) return
      if (added.length && added.every((row) => !row.error)) {
        onClose()
        return
      }
      setResults((current) => ({ ...current, ...Object.fromEntries(added.map(({ path, error }) => [path, error])) }))
      setSelected(added.filter((row) => row.error).map((row) => row.path))
    } finally {
      if (view.isRuntimeCurrent()) setBusy(false)
    }
  }

  return (
    <PreviewDialog
      title={t('skills.projectImport.discover')}
      confirmLabel={t('skills.projectImport.addSelected', { count: selected.length })}
      cancelLabel={t('skills.cancel')}
      busyLabel={t('skills.projectImport.adding')}
      busy={busy}
      disabled={selected.length === 0}
      onConfirm={() => void addSelected()}
      onCancel={onClose}
    >
      {error && <div className="sm2__error" role="alert">{error}</div>}
      {!data && !error && <p role="status">{t('skills.projectImport.loading')}</p>}
      {data?.unavailableSources.length ? (
        <p className="sm2__notice sm2__notice--warn">{t('skills.projectImport.unavailableSources', { sources: data.unavailableSources.map((source) => sourceNames[source]).join(', ') })}</p>
      ) : null}
      {data?.candidates.length === 0 && <p>{t('skills.projectImport.noCandidates')}</p>}
      <ul className="sm2__project-candidates">
        {data?.candidates.map((candidate) => {
          const added = results[candidate.path] === null
          const available = candidate.directoryAvailable && !candidate.registeredProjectId && !added
          return (
            <li key={candidate.path}>
              <label>
                <input
                  type="checkbox"
                  checked={selected.includes(candidate.path)}
                  disabled={busy || !available}
                  onChange={(event) => setSelected((current) => event.target.checked ? [...current, candidate.path] : current.filter((path) => path !== candidate.path))}
                />
                <span>
                  <strong>{candidate.name}</strong><code>{candidate.path}</code>
                  <span>{candidate.sources.map((source) => sourceNames[source]).join(' · ')}</span>
                  {candidate.lastUsedAt && <time dateTime={candidate.lastUsedAt}>{new Date(candidate.lastUsedAt).toLocaleDateString()}</time>}
                </span>
              </label>
              {candidate.git && <p>{t('skills.projectImport.gitWorktree')} <code>{candidate.git.worktree}</code>{candidate.git.branch && <> · {candidate.git.branch}</>}<br />{t('skills.projectImport.gitCommon')} <code>{candidate.git.commonDirectory}</code></p>}
              {!candidate.directoryAvailable && <p>{t('skills.projectImport.unavailablePath')}</p>}
              {results[candidate.path] && <p className="sm2__error" role="alert">{results[candidate.path]}</p>}
              {(added || candidate.registeredProjectId) && <p>{t('skills.projectImport.alreadyAdded')}</p>}
              {candidate.registeredProjectId && <button className="sm2__btn" disabled={busy} onClick={() => { void selectProject(candidate.registeredProjectId); onClose() }}>{t('skills.projectImport.showProject')}</button>}
            </li>
          )
        })}
      </ul>
    </PreviewDialog>
  )
}
