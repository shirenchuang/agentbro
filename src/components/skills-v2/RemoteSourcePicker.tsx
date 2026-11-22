import { useCallback, useEffect, useRef, useState } from 'react'
import { skillApiV2, type RemoteSkillSourceEntry, type RemoteSkillSourceListing } from '../../services/skillApiV2'
import { PreviewDialog } from './PreviewDialog'

export type RemoteSourcePickerKind = 'directory' | 'archive'

export function RemoteSourcePicker({
  kind,
  environmentId,
  environmentName,
  onSelect,
  onCancel,
}: {
  kind: RemoteSourcePickerKind
  environmentId: string
  environmentName: string
  onSelect: (path: string) => void
  onCancel: () => void
}) {
  const [listing, setListing] = useState<RemoteSkillSourceListing | null>(null)
  const [pathInput, setPathInput] = useState('~')
  const [selectedArchive, setSelectedArchive] = useState<RemoteSkillSourceEntry | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const latestRequest = useRef(0)

  const loadDirectory = useCallback(async (path: string) => {
    const request = ++latestRequest.current
    setLoading(true)
    setError(null)
    try {
      const next = await skillApiV2.browseRemoteSkillSources(path, environmentId)
      if (request !== latestRequest.current) return
      setListing(next)
      setPathInput(next.path)
      setSelectedArchive(null)
    } catch (loadError) {
      if (request === latestRequest.current) setError(String(loadError))
    } finally {
      if (request === latestRequest.current) setLoading(false)
    }
  }, [environmentId])

  useEffect(() => {
    void loadDirectory('~')
    return () => { latestRequest.current += 1 }
  }, [loadDirectory])

  const visibleEntries = (listing?.entries ?? []).filter((entry) => (
    entry.entryType === 'directory' || kind === 'archive'
  ))
  const selectedPath = kind === 'directory' ? listing?.path : selectedArchive?.path
  const canSelect = Boolean(selectedPath && !loading && !error)
  const title = kind === 'directory'
    ? `选择 ${environmentName} 上的文件夹`
    : `选择 ${environmentName} 上的压缩包`

  return (
    <PreviewDialog
      title={title}
      modalClassName="sm2__modal--remote-source"
      onCancel={onCancel}
      onConfirm={() => {
        if (canSelect && selectedPath) onSelect(selectedPath)
      }}
      actions={(
        <>
          <button type="button" className="sm2__btn" onClick={onCancel}>取消</button>
          <button
            type="button"
            className="sm2__btn sm2__btn--primary"
            disabled={!canSelect}
            onClick={() => {
              if (canSelect && selectedPath) onSelect(selectedPath)
            }}
          >
            {kind === 'directory' ? '选择当前文件夹' : '选择压缩包'}
          </button>
        </>
      )}
    >
      <div className="sm2__remote-source-context">
        <span aria-hidden="true">&gt;_</span>
        <div>
          <strong>{environmentName}</strong>
          <small>
            {kind === 'directory'
              ? '这里只显示远程服务器上的文件夹'
              : '这里只显示远程服务器上的文件夹和 .zip 文件'}
          </small>
        </div>
      </div>

      <form
        className="sm2__remote-source-toolbar"
        onSubmit={(event) => {
          event.preventDefault()
          void loadDirectory(pathInput)
        }}
      >
        <button
          type="button"
          className="sm2__remote-source-nav"
          aria-label="远程主目录"
          title="远程主目录"
          disabled={loading}
          onClick={() => void loadDirectory('~')}
        >
          ⌂
        </button>
        <button
          type="button"
          className="sm2__remote-source-nav"
          aria-label="上一级目录"
          title="上一级目录"
          disabled={!listing?.parentPath || loading}
          onClick={() => {
            if (listing?.parentPath) void loadDirectory(listing.parentPath)
          }}
        >
          ↑
        </button>
        <input
          aria-label="远程目录路径"
          value={pathInput}
          onChange={(event) => setPathInput(event.target.value)}
        />
        <button type="submit" className="sm2__remote-source-go" disabled={loading || !pathInput.trim()}>
          前往
        </button>
      </form>

      <div className="sm2__remote-source-list" aria-busy={loading}>
        {loading ? (
          <div className="sm2__remote-source-empty">
            <span className="sm2__spinner" aria-hidden="true" />
            正在读取远程目录…
          </div>
        ) : error ? (
          <div className="sm2__remote-source-error" role="alert">{error}</div>
        ) : visibleEntries.length === 0 ? (
          <div className="sm2__remote-source-empty">
            {kind === 'directory' ? '这个目录下没有子文件夹' : '这个目录下没有文件夹或 .zip 文件'}
          </div>
        ) : (
          visibleEntries.map((entry) => {
            const selected = selectedArchive?.path === entry.path
            return (
              <button
                key={entry.path}
                type="button"
                className={`sm2__remote-source-entry${selected ? ' sm2__remote-source-entry--selected' : ''}`}
                aria-pressed={entry.entryType === 'archive' ? selected : undefined}
                onClick={() => {
                  if (entry.entryType === 'directory') {
                    void loadDirectory(entry.path)
                  } else {
                    setSelectedArchive(entry)
                  }
                }}
                onDoubleClick={() => {
                  if (entry.entryType === 'archive') onSelect(entry.path)
                }}
              >
                <span className={`sm2__remote-source-icon sm2__remote-source-icon--${entry.entryType}`} aria-hidden="true">
                  {entry.entryType === 'directory' ? '▸' : 'ZIP'}
                </span>
                <span className="sm2__remote-source-entry-copy">
                  <strong>{entry.name}</strong>
                  <small>
                    {entry.entryType === 'directory'
                      ? entry.hasSkillManifest ? 'Skill 文件夹' : '文件夹'
                      : '压缩包'}
                  </small>
                </span>
                {entry.hasSkillManifest && <em>SKILL.md</em>}
              </button>
            )
          })
        )}
      </div>
    </PreviewDialog>
  )
}
