import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { open, save } from '@tauri-apps/plugin-dialog'
import { skillApiV2, type ConversationMigrationDirection, type ConversationMigrationPreview, type ConversationMigrationSelection, type MigratedConversation, type ProjectConversationSummary } from '../../services/skillApiV2'
import { agentApi } from '../../services/agentApi'
import { captureProjectView } from '../../stores/skillStoreV2'
import { PreviewDialog } from './PreviewDialog'
import { writeClipboardText } from './clipboard'
import './ProjectConversationMigration.css'

const agents = { codex: 'Codex', zcode: 'ZCode', 'claude-code': 'Claude Code' }

const directions = [
  { id: 'zcodeToCodex', source: 'zcode', target: 'codex' },
  { id: 'codexToZcode', source: 'codex', target: 'zcode' },
  { id: 'claudeCodeToCodex', source: 'claude-code', target: 'codex' },
  { id: 'codexToClaudeCode', source: 'codex', target: 'claude-code' },
  { id: 'claudeCodeToZcode', source: 'claude-code', target: 'zcode' },
  { id: 'zcodeToClaudeCode', source: 'zcode', target: 'claude-code' },
] as const

export function ProjectConversationMigration({ projectId, rootPath }: { projectId: string; rootPath: string }) {
  const { t } = useTranslation()
  const [direction, setDirection] = useState<ConversationMigrationDirection>('zcodeToCodex')
  return (
    <>
      <label className="sm2-conversation-migration__direction">
        <span>{t('skills.conversationMigration.directionLabel')}</span>
        <select className="sm2__search" value={direction} onChange={(event) => {
          const option = directions.find((item) => item.id === event.currentTarget.value)
          if (option) setDirection(option.id)
        }}>
          {directions.map(({ id, source, target }) => (
            <option key={id} value={id}>{agents[source]} → {agents[target]}</option>
          ))}
        </select>
      </label>
      <ConversationSelection key={direction} direction={direction} projectId={projectId} rootPath={rootPath} />
    </>
  )
}

function ConversationSelection({ direction, projectId, rootPath }: {
  direction: ConversationMigrationDirection
  projectId: string
  rootPath: string
}) {
  const { t } = useTranslation()
  const [sessions, setSessions] = useState<ProjectConversationSummary[] | null>(null)
  const [selected, setSelected] = useState<string[]>([])
  const [targetDirectory, setTargetDirectory] = useState(rootPath)
  const [preview, setPreview] = useState<{ selection: ConversationMigrationSelection; value: ConversationMigrationPreview } | null>(null)
  const [result, setResult] = useState<MigratedConversation[]>([])
  const [resultDirectory, setResultDirectory] = useState('')
  const [copiedResume, setCopiedResume] = useState(false)
  const [saved, setSaved] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    void skillApiV2.listProjectConversations(projectId, direction)
      .then((items) => { if (!cancelled) setSessions(items) })
      .catch((cause: unknown) => { if (!cancelled) setError(String(cause)) })
    return () => { cancelled = true }
  }, [direction, projectId])

  const chooseFolder = async () => {
    const view = captureProjectView(projectId)
    try {
      const path = await open({ directory: true, multiple: false, defaultPath: targetDirectory })
      if (typeof path === 'string' && view.isSelected()) setTargetDirectory(path)
    } catch (cause) {
      if (view.isSelected()) setError(String(cause))
    }
  }

  const review = async () => {
    const view = captureProjectView(projectId)
    const selection = { projectId, sessionIds: selected, targetDirectory, direction }
    setBusy(true)
    setError(null)
    setSaved(false)
    setCopiedResume(false)
    setResult([])
    try {
      const value = await skillApiV2.previewProjectConversationMigration(selection)
      if (view.isSelected()) setPreview({ selection, value })
    } catch (cause) {
      if (view.isSelected()) setError(String(cause))
    } finally {
      if (view.isSelected()) setBusy(false)
    }
  }

  const apply = async (action: 'migrate' | 'export') => {
    if (!preview) return
    const view = captureProjectView(projectId)
    setBusy(true)
    setError(null)
    try {
      const { selection, value } = preview
      if (action === 'export') {
        const path = await save({
          defaultPath: 'conversation-handoff.zip',
          filters: [{ name: t('skills.conversationMigration.archive'), extensions: ['zip'] }],
        })
        if (!path || !view.isSelected()) return
        await skillApiV2.exportProjectConversationHandoff(selection, value.revision, path)
        if (view.isSelected()) setSaved(true)
      } else {
        const migrated = await skillApiV2.migrateProjectConversations(selection, value.revision)
        if (view.isSelected()) {
          setResult(migrated)
          setResultDirectory(value.bundle.targetDirectory)
        }
      }
      if (view.isSelected()) setPreview(null)
    } catch (cause) {
      if (view.isSelected()) setError(String(cause))
    } finally {
      if (view.isSelected()) setBusy(false)
    }
  }

  const title = (row: ProjectConversationSummary) => row.title || row.id
  const text = (key: string) => t('skills.conversationMigration.' + key)
  const target = directions.find((item) => item.id === direction)?.target ?? 'codex'
  const openTarget = async (id: string) => {
    if (target === 'codex') await skillApiV2.openMigratedCodexThread(id)
    else if (target === 'zcode') await agentApi.openApp('zcode')
    else {
      await writeClipboardText(`claude --resume ${id}`)
      setCopiedResume(true)
    }
  }
  const openLabel = { codex: 'openCodex', zcode: 'openZcode', 'claude-code': 'copyResume' }[target]
  const migrateLabel = { codex: 'migrate', zcode: 'migrateZcode', 'claude-code': 'migrateClaude' }[target]

  return (
    <section className="sm2-conversation-migration">
      {error && !preview && <div className="sm2__error" role="alert">{error}</div>}
      {sessions === null && !error && <p role="status">{text('loading')}</p>}
      {sessions?.length === 0 && <p>{text('empty')}</p>}
      {sessions && sessions.length > 0 && (
        <>
          <div className="sm2__btn-row">
            <button className="sm2__btn" disabled={busy || selected.length === sessions.length} onClick={() => setSelected(sessions.map((row) => row.id))}>{text('selectAll')}</button>
            <button className="sm2__btn" disabled={busy || selected.length === 0} onClick={() => setSelected([])}>{text('clear')}</button>
          </div>
          <div className="sm2-conversation-migration__sessions">
            {sessions.map((row) => (
              <label className="sm2-conversation-migration__session" key={row.id}>
                <input type="checkbox" disabled={busy} checked={selected.includes(row.id)} onChange={(event) => {
                  const checked = event.currentTarget.checked
                  setSelected((current) => checked ? [...current, row.id] : current.filter((id) => id !== row.id))
                }} />
                <span>
                  <strong>{title(row)}</strong>
                  {row.archived && <span> · {text('archived')}</span>}
                  {!row.directoryExists && <span> · {text('missingFolder')}</span>}
                  <code>{row.cwd}</code>
                </span>
              </label>
            ))}
          </div>
          <div className="sm2__btn-row sm2-conversation-migration__target">
            <span>{text('targetFolder')}</span>
            <code>{targetDirectory}</code>
            <button className="sm2__btn" disabled={busy} onClick={chooseFolder}>{text('chooseFolder')}</button>
          </div>
          <button className="sm2__btn sm2__btn--primary" disabled={busy || selected.length === 0} onClick={review}>{text('preview')}</button>
        </>
      )}
      {saved && <p role="status">{text('saved')}</p>}
      {result.length > 0 && target === 'claude-code' && (
        <p>{t('skills.conversationMigration.claudeResumeCwd', { directory: resultDirectory })}</p>
      )}
      {copiedResume && <p role="status">{text('resumeCopied')}</p>}
      {result.length > 0 && (
        <ul className="sm2-conversation-migration__results" aria-label={text('results')}>
          {result.map((row) => (
            <li key={row.sourceId}>
              <span>{sessions?.find((session) => session.id === row.sourceId)?.title || row.sourceId}</span>
              <span>{row.reused ? text('alreadyImported') : text('imported')}</span>
              <code>{row.targetId}</code>
              <button className="sm2__btn" onClick={() => void openTarget(row.targetId).catch((cause: unknown) => setError(String(cause)))}>{text(openLabel)}</button>
            </li>
          ))}
        </ul>
      )}
      {preview && (
        <PreviewDialog
          title={text('preview')}
          busy={busy}
          onCancel={() => setPreview(null)}
          onConfirm={() => void apply('migrate')}
          actions={(
            <>
              <button className="sm2__btn" disabled={busy} onClick={() => setPreview(null)}>{t('skills.cancel')}</button>
              <button className="sm2__btn" disabled={busy} onClick={() => void apply('export')}>{text('export')}</button>
              <button className="sm2__btn sm2__btn--primary" disabled={busy || !preview.value.bundle.conversations.some((row) => row.messages.length > 0)} onClick={() => void apply('migrate')}>{busy ? text('working') : text(migrateLabel)}</button>
            </>
          )}
        >
          {error && <div className="sm2__error" role="alert">{error}</div>}
          <dl className="sm2-conversation-migration__mapping">
            <dt>{text('sourceProject')}</dt><dd><code>{preview.value.bundle.projectRoot}</code></dd>
            <dt>{text('targetFolder')}</dt><dd><code>{preview.value.bundle.targetDirectory}</code></dd>
          </dl>
          {preview.value.bundle.conversations.map((row) => (
            <details className="sm2-conversation-migration__preview" key={row.id}>
              <summary>{title(row)} · {t('skills.conversationMigration.messageCount', { count: row.messages.length })}</summary>
              <code>{row.cwd}</code>
              {row.git?.branch && <p>{row.git.branch} · <code>{row.git.commit}</code></p>}
              {row.readIssue && <p>{text(row.messages.length ? 'partiallyReadable' : 'unreadable')}</p>}
              {row.returnContext && <p>{t('skills.conversationMigration.returnedHistory', { agent: agents[row.returnContext.originalAgent], id: row.returnContext.originalSessionId, count: row.returnContext.matchingMessages })}</p>}
              {row.messages.map((message, index) => (
                <article key={message.id}>
                  {row.returnContext?.matchingMessages === index && <p><strong>{text('continuedMessages')}</strong></p>}
                  <strong>{text(message.role)}</strong>
                  {message.timestamp && <time dateTime={message.timestamp}>{new Date(message.timestamp).toLocaleString()}</time>}
                  <p>{message.text}</p>
                </article>
              ))}
            </details>
          ))}
        </PreviewDialog>
      )}
    </section>
  )
}
