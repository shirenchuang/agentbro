import { useEffect, useRef, useState } from 'react'
import type { AnchorHTMLAttributes, MouseEvent, RefObject, UIEvent } from 'react'
import { createPortal } from 'react-dom'
import { useTranslation } from 'react-i18next'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { open as openShell } from '@tauri-apps/plugin-shell'
import { skillApiV2 } from '../../services/skillApiV2'
import { isTauri } from '../../services/tauriApi'
import type { CopyTargetDiffPreview, SkillDetail, SkillSummary, FileTreeNode } from '../../services/skillApiV2'
import { SlideOver } from './SlideOver'
import { AgentIconBadge } from './AgentIconBadge'
import { skillModeLabel, skillSourceTypeLabel, targetClaimLabel } from './skillLabels'
import { PreviewDialog } from './PreviewDialog'
import { extractSkillDescription as extractFrontmatterDescription, stripSkillFrontmatter as stripFrontmatter } from './frontmatter'

type DetailTab = 'overview' | 'files' | 'agents' | 'source'
type FileViewMode = 'preview' | 'source'
type CopyMenuState = { x: number; y: number; text: string }

export interface SkillDetailFallback {
  id: string
  name: string
  centerPath: string
  description?: string
  sourceType?: string
  sourceUri?: string | null
}

const STATUS_LABEL: Record<string, string> = {
  ok: '正常',
  conflict: '冲突',
  copy_outdated: '可更新',
  copy_modified: '已修改',
  copy_diverged: '已分叉',
  broken_link: '坏链接',
  missing: '失效',
  copyDiverged: '副本分叉',
}

const markdownComponents = {
  a: MarkdownLink,
}

export function SkillDetailSlider({
  skillId,
  open,
  onClose,
  onDistribute,
  onDelete,
  fallbackSkill,
}: {
  skillId: string | null
  open: boolean
  onClose: () => void
  onDistribute?: (s: SkillSummary) => void
  onDelete?: (id: string) => void
  fallbackSkill?: SkillDetailFallback | null
}) {
  const { t } = useTranslation()
  const [detail, setDetail] = useState<SkillDetail | null>(null)
  const [loading, setLoading] = useState(false)
  const [activeFile, setActiveFile] = useState<string | null>(null)
  const [fileContent, setFileContent] = useState('')
  const [fileLoading, setFileLoading] = useState(false)
  const [syncing, setSyncing] = useState<string | null>(null)
  const [diffPreview, setDiffPreview] = useState<CopyTargetDiffPreview | null>(null)
  const [diffLoadingTarget, setDiffLoadingTarget] = useState<string | null>(null)
  const [diffError, setDiffError] = useState<string | null>(null)
  const [deleteTargetId, setDeleteTargetId] = useState<string | null>(null)
  const [batchDeleteMode, setBatchDeleteMode] = useState(false)
  const [selectedDeleteTargetIds, setSelectedDeleteTargetIds] = useState<Set<string>>(new Set())
  const [batchDeleteTargetIds, setBatchDeleteTargetIds] = useState<string[] | null>(null)
  const [deletingTarget, setDeletingTarget] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [tab, setTab] = useState<DetailTab>('overview')
  const [fileViewMode, setFileViewMode] = useState<FileViewMode>('preview')
  const [skillDocPath, setSkillDocPath] = useState<string | null>(null)
  const [skillDocContent, setSkillDocContent] = useState('')
  const [copyMenu, setCopyMenu] = useState<CopyMenuState | null>(null)

  useEffect(() => {
    if (!open || !skillId) return
    setLoading(true)
    setError(null)
    setTab('overview')
    setDetail(null)
    setActiveFile(null)
    setSkillDocPath(null)
    setSkillDocContent('')
    setFileContent('')
    setDiffPreview(null)
    setDiffError(null)
    setDeleteTargetId(null)
    setBatchDeleteMode(false)
    setCopyMenu(null)
    setSelectedDeleteTargetIds(new Set())
    setBatchDeleteTargetIds(null)
    skillApiV2
      .getSkillDetail(skillId)
      .then((d) => {
        setDetail(d)
        const skillMd = findFile(d.files, 'SKILL.md')
        setSkillDocPath(skillMd)
        if (skillMd) {
          loadFile(skillMd)
          skillApiV2.readFileContent(skillMd).then(setSkillDocContent).catch(() => setSkillDocContent(''))
        } else {
          setSkillDocContent('')
        }
      })
      .catch((e) => {
        if (!fallbackSkill) {
          setError(String(e))
          return
        }
        const d = fallbackToSkillDetail(fallbackSkill)
        setDetail(d)
        const fallbackDocPath = `${fallbackSkill.centerPath}/SKILL.md`
        setSkillDocPath(fallbackDocPath)
        return skillApiV2.readFileTree(fallbackSkill.centerPath)
          .then(async (files) => {
            const skillMd = findFile(files, 'SKILL.md') || fallbackDocPath
            setDetail({ ...d, files })
            setSkillDocPath(skillMd)
            setActiveFile(skillMd)
            const content = await skillApiV2.readFileContent(skillMd)
            setSkillDocContent(content)
            setFileContent(content)
          })
          .catch(() => skillApiV2
            .readFileContent(fallbackDocPath)
            .then((content) => {
              setActiveFile(fallbackDocPath)
              setSkillDocContent(content)
              setFileContent(content)
            })
            .catch(() => {
              setSkillDocContent('')
              setFileContent('')
            }))
      })
      .finally(() => setLoading(false))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, skillId, fallbackSkill?.centerPath])

  useEffect(() => {
    if (!copyMenu) return
    const closeMenu = () => setCopyMenu(null)
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') closeMenu()
    }
    window.addEventListener('pointerdown', closeMenu)
    window.addEventListener('scroll', closeMenu, true)
    window.addEventListener('keydown', closeOnEscape)
    return () => {
      window.removeEventListener('pointerdown', closeMenu)
      window.removeEventListener('scroll', closeMenu, true)
      window.removeEventListener('keydown', closeOnEscape)
    }
  }, [copyMenu])

  const loadFile = async (path: string) => {
    setActiveFile(path)
    setFileViewMode(isMarkdownPath(path) ? 'preview' : 'source')
    setFileLoading(true)
    try {
      const content = await skillApiV2.readFileContent(path)
      setFileContent(content)
    } catch (e) {
      setFileContent(`无法读取文件：${e}`)
    } finally {
      setFileLoading(false)
    }
  }

  const doSync = async (targetId: string, action: string) => {
    setSyncing(targetId)
    try {
      await skillApiV2.executeSyncCopy(targetId, action)
      if (skillId) {
        const d = await skillApiV2.getSkillDetail(skillId)
        setDetail(d)
      }
      setDiffPreview(null)
    } catch (e) {
      setError(String(e))
    } finally {
      setSyncing(null)
    }
  }

  const openDiff = async (targetId: string) => {
    setDiffLoadingTarget(targetId)
    setDiffError(null)
    try {
      const preview = await skillApiV2.previewCopyTargetDiff(targetId)
      setDiffPreview(preview)
    } catch (e) {
      setDiffError(String(e))
    } finally {
      setDiffLoadingTarget(null)
    }
  }

  const confirmDeleteTarget = async () => {
    if (!deleteTargetId) return
    setDeletingTarget(true)
    try {
      await skillApiV2.deleteSkillTargetDistribution(deleteTargetId)
      if (skillId) {
        const d = await skillApiV2.getSkillDetail(skillId)
        setDetail(d)
      }
      if (diffPreview?.targetId === deleteTargetId) {
        setDiffPreview(null)
      }
      setDeleteTargetId(null)
    } catch (e) {
      setError(String(e))
    } finally {
      setDeletingTarget(false)
    }
  }

  const toggleBatchDeleteTarget = (targetId: string) => {
    setSelectedDeleteTargetIds((current) => {
      const next = new Set(current)
      if (next.has(targetId)) {
        next.delete(targetId)
      } else {
        next.add(targetId)
      }
      return next
    })
  }

  const setAllBatchDeleteTargets = (checked: boolean) => {
    setSelectedDeleteTargetIds(checked ? new Set(detail?.targets.map((target) => target.id) ?? []) : new Set())
  }

  const cancelBatchDelete = () => {
    setBatchDeleteMode(false)
    setSelectedDeleteTargetIds(new Set())
  }

  const confirmBatchDeleteTargets = async () => {
    const targetIds = batchDeleteTargetIds ?? []
    if (targetIds.length === 0) return
    setDeletingTarget(true)
    try {
      const failed: string[] = []
      const failedIds: string[] = []
      for (const targetId of targetIds) {
        const target = detail?.targets.find((item) => item.id === targetId)
        try {
          await skillApiV2.deleteSkillTargetDistribution(targetId)
        } catch (e) {
          failedIds.push(targetId)
          failed.push(`${target ? pathBasename(target.targetPath) || targetId : targetId}: ${String(e)}`)
        }
      }
      if (skillId) {
        const d = await skillApiV2.getSkillDetail(skillId)
        setDetail(d)
      }
      if (diffPreview && targetIds.includes(diffPreview.targetId)) {
        setDiffPreview(null)
      }
      if (failed.length > 0) {
        setError(failed.slice(0, 3).join('\n'))
        setSelectedDeleteTargetIds(new Set(failedIds))
      } else {
        setError(null)
        setSelectedDeleteTargetIds(new Set())
        setBatchDeleteMode(false)
      }
      setBatchDeleteTargetIds(null)
    } catch (e) {
      setError(String(e))
    } finally {
      setDeletingTarget(false)
    }
  }

  const summary: SkillSummary | null = detail
  const tabs: Array<{ id: DetailTab; label: string }> = [
    { id: 'overview', label: '概览' },
    { id: 'files', label: '文件' },
    { id: 'agents', label: `Agent (${detail?.targets.length || 0})` },
    { id: 'source', label: '来源' },
  ]

  const deleteTarget = detail?.targets.find((target) => target.id === deleteTargetId) ?? null
  const deleteAgent = deleteTarget ? detail?.installedAgents.find((agent) => agent.agentId === deleteTarget.agentId) : null
  const selectedBatchDeleteTargets = detail?.targets.filter((target) => selectedDeleteTargetIds.has(target.id)) ?? []
  const batchDeleteTargets = detail?.targets.filter((target) => batchDeleteTargetIds?.includes(target.id)) ?? []

  const openCopyMenu = (event: MouseEvent<HTMLElement>) => {
    const target = event.target instanceof Element ? event.target : null
    if (!target?.closest('.selectable')) return
    const text = window.getSelection()?.toString() || ''
    if (!text.trim()) return
    event.preventDefault()
    event.stopPropagation()
    setCopyMenu({
      x: Math.max(8, Math.min(event.clientX, window.innerWidth - 104)),
      y: Math.max(8, Math.min(event.clientY, window.innerHeight - 48)),
      text,
    })
  }

  const copySelectedText = async () => {
    if (!copyMenu) return
    try {
      await writeClipboardText(copyMenu.text)
      setCopyMenu(null)
    } catch (e) {
      setError(`复制失败：${e}`)
    }
  }

  return (
    <>
    <SlideOver
      open={open}
      onClose={onClose}
      width={1040}
      className="sm2__slideover--skill-detail"
      title={<span className="selectable" onContextMenu={openCopyMenu}>{summary?.name || skillId || ''}</span>}
      actions={
        summary && (
          <>
            {onDistribute && (
              <button className="sm2__btn sm2__btn--primary" onClick={() => onDistribute(summary)}>
                分发
              </button>
            )}
            <button className="sm2__btn sm2__btn--ghost" onClick={() => skillApiV2.openPath(summary.centerPath)}>
              打开目录
            </button>
            {onDelete && (
              <button className="sm2__btn sm2__btn--danger" onClick={() => onDelete(summary.id)}>
                删除
              </button>
            )}
          </>
        )
      }
    >
      {loading && <div className="sm2__empty">加载中…</div>}
      {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
      {detail && (
        <div className="sm2__skill-detail" onContextMenu={openCopyMenu}>
          <div className="sm2__detail-pills">
            <span className={`sm2__tag sm2__tag--${detail.status}${isCopyDiffStatus(detail.status) ? ' sm2__detail-status--copy-diff' : ''}`}>
              {STATUS_LABEL[detail.status] || detail.status}
            </span>
            <span className="sm2__tag">{skillSourceTypeLabel(t, detail.sourceType)}</span>
            {isLinkedCenterSkill(detail) && (
              <span className="sm2__tag sm2__tag--center-link" title={linkedCenterSkillTitle(detail)}>
                本地软链
              </span>
            )}
            <span className="sm2__tag">{detail.skillType}</span>
            {detail.installedAgents.length > 0 && (
              <span className="sm2__agents">
                {detail.installedAgents.map((a) => (
                  <AgentIconBadge key={a.agentId} iconKey={a.iconKey} mode={a.mode} title={`${a.displayName} · ${skillModeLabel(t, a.mode)}`} />
                ))}
              </span>
            )}
          </div>

          <div className="sm2__subtabs">
            {tabs.map((t) => (
              <button key={t.id} className={`sm2__subtab${tab === t.id ? ' sm2__subtab--active' : ''}`} onClick={() => setTab(t.id)}>
                {t.label}
              </button>
            ))}
          </div>

          {tab === 'overview' && (
            <OverviewTab
              detail={detail}
              docPath={skillDocPath}
              docContent={skillDocContent || fileContent}
              fileLoading={fileLoading}
            />
          )}
          {tab === 'files' && (
            <FilesTab
              detail={detail}
              activeFile={activeFile}
              fileContent={fileContent}
              fileLoading={fileLoading}
              viewMode={fileViewMode}
              onViewModeChange={setFileViewMode}
              onSelect={loadFile}
            />
          )}
          {tab === 'agents' && (
            <AgentsTab
              detail={detail}
              syncing={syncing}
              diffLoadingTarget={diffLoadingTarget}
              diffError={diffError}
              onSync={doSync}
              onOpenDiff={openDiff}
              onDeleteTarget={setDeleteTargetId}
              batchDeleteMode={batchDeleteMode}
              selectedTargetIds={selectedDeleteTargetIds}
              onEnterBatchDelete={() => {
                setDeleteTargetId(null)
                setBatchDeleteMode(true)
              }}
              onCancelBatchDelete={cancelBatchDelete}
              onToggleBatchDeleteTarget={toggleBatchDeleteTarget}
              onToggleAllBatchDeleteTargets={setAllBatchDeleteTargets}
              onConfirmBatchDelete={() => setBatchDeleteTargetIds(selectedBatchDeleteTargets.map((target) => target.id))}
            />
          )}
          {tab === 'source' && <SourceTab detail={detail} />}
        </div>
      )}
    </SlideOver>
    {diffPreview && (
      <CopyDiffDialog key={diffPreview.targetId} preview={diffPreview} onClose={() => setDiffPreview(null)} />
    )}
    {deleteTarget && (
      <PreviewDialog
        title="删除 Agent 分发"
        confirmLabel="确认删除"
        busyLabel="删除中…"
        destructive
        busy={deletingTarget}
        onCancel={() => setDeleteTargetId(null)}
        onConfirm={confirmDeleteTarget}
      >
        <div className="sm2__delete-target-preview">
          <p>
            将从 <strong>{deleteAgent?.displayName || deleteTarget.agentId}</strong> 移除这个 Skill 分发，并删除对应的本地目标。
          </p>
          <code>{deleteTarget.targetPath}</code>
        </div>
      </PreviewDialog>
    )}
    {batchDeleteTargetIds && (
      <PreviewDialog
        title="确认批量删除 Agent 分发"
        confirmLabel="确认删除"
        busyLabel="删除中…"
        destructive
        busy={deletingTarget}
        disabled={batchDeleteTargets.length === 0}
        onCancel={() => setBatchDeleteTargetIds(null)}
        onConfirm={confirmBatchDeleteTargets}
      >
        <div className="sm2__delete-target-preview">
          <p>
            将从 <strong>{batchDeleteTargets.length}</strong> 个 Agent 移除这个 Skill 分发，并删除对应的本地目标。
          </p>
          <div className="sm2__delete-target-list">
            {batchDeleteTargets.map((target) => {
              const agent = detail?.installedAgents.find((item) => item.agentId === target.agentId)
              return (
                <code key={target.id}>
                  {(agent?.displayName || target.agentId)} · {target.targetPath}
                </code>
              )
            })}
          </div>
        </div>
      </PreviewDialog>
    )}
    {copyMenu && createPortal(
      <div
        className="sm2__selection-menu"
        role="menu"
        style={{ left: copyMenu.x, top: copyMenu.y }}
        onPointerDown={(event) => event.stopPropagation()}
      >
        <button
          type="button"
          role="menuitem"
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => void copySelectedText()}
        >
          复制
        </button>
      </div>,
      document.body,
    )}
    </>
  )
}

async function writeClipboardText(text: string): Promise<void> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text)
      return
    }
  } catch {
    // Fall through to the WebView-compatible copy command.
  }
  const textarea = document.createElement('textarea')
  textarea.value = text
  textarea.style.position = 'fixed'
  textarea.style.opacity = '0'
  document.body.appendChild(textarea)
  textarea.select()
  const copied = document.execCommand('copy')
  textarea.remove()
  if (!copied) throw new Error('剪贴板不可用')
}

function OverviewTab({
  detail,
  docPath,
  docContent,
  fileLoading,
}: {
  detail: SkillDetail
  docPath: string | null
  docContent: string
  fileLoading: boolean
}) {
  const { t } = useTranslation()
  const frontmatter = Object.entries(detail.frontmatter || {})
  const sourceLabel = skillSourceTypeLabel(t, detail.source?.sourceType || detail.sourceType)
  const sourceValue = detail.source?.sourceUri || detail.sourceUri || sourceLabel
  const linkedCenter = isLinkedCenterSkill(detail)
  return (
    <div className="sm2__detail-overview sm2__detail-overview--reader">
      <section className="sm2__skill-doc">
        <div className="sm2__skill-doc-head">
          <div>
            <span>{docPath ? pathBasename(docPath) : 'SKILL.md'}</span>
            <strong>说明文档</strong>
          </div>
          <small>{STATUS_LABEL[detail.status] || detail.status}</small>
        </div>
        <div className="sm2__markdown sm2__markdown--document sm2__markdown--skilldoc selectable">
          {fileLoading ? (
            <div className="sm2__empty sm2__empty--compact">读取说明文档…</div>
          ) : docContent ? (
            <>
              <SkillFrontmatterIntro description={detail.frontmatter.description || detail.description} />
              <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents}>{stripFrontmatter(docContent)}</ReactMarkdown>
            </>
          ) : (
            <div className="sm2__empty sm2__empty--compact">未找到说明文档</div>
          )}
        </div>
      </section>

      <aside className="sm2__skill-aside">
        <section className="sm2__aside-panel">
          <div className="sm2__aside-head">
            <h3>Agent 安装</h3>
            <span>{detail.targets.length}</span>
          </div>
          {detail.targets.length === 0 ? (
            <div className="sm2__aside-empty">尚未分发到 Agent</div>
          ) : (
            <div className="sm2__install-mini-list">
              {detail.targets.map((target) => {
                const agent = detail.installedAgents.find((item) => item.agentId === target.agentId)
                const changedCopy = target.actualMode === 'copy' && isCopyDiffStatus(target.status)
                return (
                  <div key={target.id} className={`sm2__install-mini${changedCopy ? ' sm2__install-mini--copy-diff' : ''}`}>
                    <AgentIconBadge iconKey={agent?.iconKey || target.agentId} mode={target.actualMode} size={26} />
                    <div>
                      <strong>{agent?.displayName || target.agentId}</strong>
                      <span className={changedCopy ? 'sm2__install-mini-status--copy-diff' : undefined}>
                        {skillModeLabel(t, target.actualMode)} · {STATUS_LABEL[target.status] || target.status}
                      </span>
                    </div>
                  </div>
                )
              })}
            </div>
          )}
        </section>

        <section className="sm2__aside-panel">
          <div className="sm2__aside-head">
            <h3>信息</h3>
          </div>
          <div className="sm2__compact-info">
            <CompactInfo label="来源" value={sourceValue} />
            <CompactInfo label={linkedCenter ? '软链中心目录' : '中心目录'} value={detail.centerPath} mono />
            {linkedCenter && detail.centerResolvedPath && (
              <CompactInfo label="真实源目录" value={detail.centerResolvedPath} mono />
            )}
            <CompactInfo label="Hash" value={detail.currentHash} mono short />
          </div>
        </section>

        {frontmatter.length > 0 && (
          <details className="sm2__aside-panel sm2__metadata-disclosure">
            <summary>
              <span>元数据</span>
              <strong>{frontmatter.length}</strong>
            </summary>
            <div className="sm2__compact-info sm2__compact-info--meta">
              {frontmatter.map(([key, value]) => (
                <CompactInfo key={key} label={key} value={formatMetaValue(value)} mono={isCodeLikeMeta(key, value)} />
              ))}
            </div>
          </details>
        )}
      </aside>
    </div>
  )
}

function FilesTab({
  detail,
  activeFile,
  fileContent,
  fileLoading,
  viewMode,
  onViewModeChange,
  onSelect,
}: {
  detail: SkillDetail
  activeFile: string | null
  fileContent: string
  fileLoading: boolean
  viewMode: FileViewMode
  onViewModeChange: (mode: FileViewMode) => void
  onSelect: (path: string) => void
}) {
  const canPreview = Boolean(activeFile && isMarkdownPath(activeFile))
  const effectiveMode = canPreview ? viewMode : 'source'
  const activeName = activeFile ? pathBasename(activeFile) || activeFile : '未选择文件'
  const activeDisplayPath = activeFile ? relativeFilePath(activeFile, detail.centerPath) : '选择左侧文件查看内容'
  const fileCount = countFiles(detail.files)
  return (
    <section className="sm2__panel sm2__panel--flush">
      <div className="sm2__panel-head sm2__panel-head--filebrowser">
        <div>
          <h3>目录与文件</h3>
          <span>{activeFile ? activeDisplayPath : '选择文件后查看内容'}</span>
        </div>
        <strong>{fileCount} 个文件</strong>
      </div>
      <div className="sm2__filebrowser sm2__filebrowser--expansive">
        <div className="sm2__filetree-pane">
          <div className="sm2__filetree-head">
            <span>目录</span>
            <strong>{fileCount}</strong>
          </div>
          <div className="sm2__filetree settings-scroll">
            {detail.files ? (
              <FileTree node={detail.files} depth={0} active={activeFile} onSelect={onSelect} />
            ) : (
              <div className="sm2__empty sm2__empty--compact">无文件</div>
            )}
          </div>
        </div>
        <div className="sm2__fileview">
          <div className="sm2__fileview-header">
            <div className="sm2__fileview-title">
              <strong>{activeName}</strong>
              <span>{activeDisplayPath}</span>
            </div>
            <div className="sm2__filemode-toggle">
              <button
                className={effectiveMode === 'preview' ? 'active' : ''}
                disabled={!canPreview}
                onClick={() => onViewModeChange('preview')}
              >
                预览
              </button>
              <button
                className={effectiveMode === 'source' ? 'active' : ''}
                onClick={() => onViewModeChange('source')}
              >
                源码
              </button>
            </div>
          </div>
          <div className="sm2__filecontent settings-scroll">
            {fileLoading ? (
              <div className="sm2__empty sm2__empty--compact">加载中…</div>
            ) : effectiveMode === 'preview' && canPreview ? (
              <div className={`sm2__markdown sm2__markdown--file selectable${isSkillMarkdownPath(activeFile) ? ' sm2__markdown--file-skill' : ''}`}>
                {isSkillMarkdownPath(activeFile) && (
                  <SkillFrontmatterIntro description={extractFrontmatterDescription(fileContent)} compact />
                )}
                <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents}>{stripFrontmatter(fileContent || '（空）')}</ReactMarkdown>
              </div>
            ) : (
              <pre className="sm2__fileview-content selectable">{fileContent || '（空）'}</pre>
            )}
          </div>
        </div>
      </div>
    </section>
  )
}

function AgentsTab({
  detail,
  syncing,
  diffLoadingTarget,
  diffError,
  onSync,
  onOpenDiff,
  onDeleteTarget,
  batchDeleteMode,
  selectedTargetIds,
  onEnterBatchDelete,
  onCancelBatchDelete,
  onToggleBatchDeleteTarget,
  onToggleAllBatchDeleteTargets,
  onConfirmBatchDelete,
}: {
  detail: SkillDetail
  syncing: string | null
  diffLoadingTarget: string | null
  diffError: string | null
  onSync: (targetId: string, action: string) => void
  onOpenDiff: (targetId: string) => void
  onDeleteTarget: (targetId: string) => void
  batchDeleteMode: boolean
  selectedTargetIds: Set<string>
  onEnterBatchDelete: () => void
  onCancelBatchDelete: () => void
  onToggleBatchDeleteTarget: (targetId: string) => void
  onToggleAllBatchDeleteTargets: (checked: boolean) => void
  onConfirmBatchDelete: () => void
}) {
  const { t } = useTranslation()
  if (detail.targets.length === 0) {
    return <div className="sm2__empty sm2__empty--compact">尚未分发到任何 Agent</div>
  }
  const allSelected = detail.targets.length > 0 && selectedTargetIds.size === detail.targets.length
  return (
    <section className="sm2__panel sm2__agent-targets">
      <div className="sm2__agent-target-toolbar">
        {batchDeleteMode ? (
          <>
            <label className="sm2__agent-target-select-all">
              <input
                type="checkbox"
                checked={allSelected}
                onChange={(event) => onToggleAllBatchDeleteTargets(event.currentTarget.checked)}
              />
              <span>全选</span>
            </label>
            <span>{selectedTargetIds.size} / {detail.targets.length} 已选择</span>
            <div className="sm2__agent-target-toolbar-actions">
              <button className="sm2__btn sm2__btn--ghost" onClick={onCancelBatchDelete}>
                取消
              </button>
              <button className="sm2__btn sm2__btn--danger" disabled={selectedTargetIds.size === 0} onClick={onConfirmBatchDelete}>
                删除 {selectedTargetIds.size} 个分发
              </button>
            </div>
          </>
        ) : (
          <>
            <span>管理已分发到 Agent 的本地目标</span>
            <button className="sm2__btn sm2__btn--danger-ghost" onClick={onEnterBatchDelete}>
              批量删除分发
            </button>
          </>
        )}
      </div>
      <div className="sm2__agent-target-grid">
        {detail.targets.map((target) => {
          const agent = detail.installedAgents.find((item) => item.agentId === target.agentId)
          const agentName = agent?.displayName || target.agentId
          const statusLabel = STATUS_LABEL[target.status] || target.status
          const modeLabel = skillModeLabel(t, target.actualMode)
          const hasCopyDiff = target.actualMode === 'copy' && isCopyDiffStatus(target.status)
          const selected = selectedTargetIds.has(target.id)
          return (
            <article key={target.id} className={`sm2__agent-target-card sm2__agent-target-card--${target.actualMode}${hasCopyDiff ? ' sm2__agent-target-card--copy-diff' : ''}${selected ? ' sm2__agent-target-card--selected' : ''}`}>
              <div className="sm2__agent-target-head">
                <div className="sm2__agent-target-title">
                  {batchDeleteMode && (
                    <label className="sm2__agent-target-check">
                      <input
                        type="checkbox"
                        checked={selected}
                        aria-label={`选择 ${agentName} 的 Skill 分发`}
                        onChange={() => onToggleBatchDeleteTarget(target.id)}
                      />
                    </label>
                  )}
                  <AgentIconBadge iconKey={agent?.iconKey || target.agentId} mode={target.actualMode} size={38} />
                  <div>
                    <strong>{agentName}</strong>
                    <span>{modeLabel} · {statusLabel}</span>
                  </div>
                </div>
                <span className={`sm2__agent-target-status sm2__agent-target-status--${target.status}`}>
                  {statusLabel}
                </span>
              </div>

              <div className="sm2__agent-target-body">
                <div className="sm2__agent-target-tags" aria-label={`${agentName} 安装标记`}>
                  <span className={`sm2__target-chip sm2__target-chip--${target.actualMode}`}>
                    {modeLabel}
                  </span>
                  {target.claims.length === 0 && (
                    <span className="sm2__target-chip sm2__target-chip--claim-direct">
                      {targetClaimLabel(t, null)}
                    </span>
                  )}
                  {target.claims.map((c) => (
                    <span key={c.id} className={`sm2__target-chip sm2__target-chip--claim-${c.claimType}`}>
                      {targetClaimLabel(t, c)}
                    </span>
                  ))}
                </div>

                <div className="sm2__agent-target-paths">
                  <span>目标目录</span>
                  <code>{target.targetPath}</code>
                  {target.resolvedTargetPath && target.resolvedTargetPath !== target.targetPath && (
                    <div className="sm2__agent-target-resolved">
                      <span>真实路径</span>
                      <small>打开将跳转到真实路径</small>
                      <code>{target.resolvedTargetPath}</code>
                    </div>
                  )}
                </div>
              </div>

              <div className="sm2__agent-target-actions">
                <div className="sm2__agent-target-actions-main">
                  {target.actualMode === 'copy' && target.status !== 'ok' && (
                    <>
                    {hasCopyDiff && (
                      <button className="sm2__btn sm2__btn--copy-diff" disabled={diffLoadingTarget === target.id} onClick={() => onOpenDiff(target.id)}>
                        {diffLoadingTarget === target.id ? '读取 diff…' : '查看 diff'}
                      </button>
                    )}
                    {target.status === 'copy_outdated' && (
                      <button className="sm2__btn" disabled={syncing === target.id} onClick={() => onSync(target.id, 'center_over_agent')}>同步中心库</button>
                    )}
                    {target.status === 'copy_modified' && (
                      <>
                        <button className="sm2__btn" disabled={syncing === target.id} onClick={() => onSync(target.id, 'center_over_agent')}>同步中心库</button>
                        <button className="sm2__btn" disabled={syncing === target.id} onClick={() => onSync(target.id, 'agent_over_center')}>推回中心库</button>
                      </>
                    )}
                    {target.status === 'copy_diverged' && (
                      <>
                        <button className="sm2__btn" disabled={syncing === target.id} onClick={() => onSync(target.id, 'center_over_agent')}>中心库覆盖</button>
                        <button className="sm2__btn" disabled={syncing === target.id} onClick={() => onSync(target.id, 'agent_over_center')}>副本覆盖</button>
                        <button className="sm2__btn" disabled={syncing === target.id} onClick={() => onSync(target.id, 'keep_diverged')}>保留分叉</button>
                      </>
                    )}
                    </>
                  )}
                </div>
                <div className="sm2__agent-target-actions-tail">
                  <button className="sm2__btn sm2__btn--ghost" onClick={() => skillApiV2.openPath(target.targetPath)}>
                    {t('skills.actions.open', { defaultValue: 'Open' })}
                  </button>
                  <button className="sm2__btn sm2__btn--danger-ghost" disabled={batchDeleteMode} onClick={() => onDeleteTarget(target.id)}>
                    删除分发
                  </button>
                </div>
              </div>
            </article>
          )
        })}
      </div>
      {diffError && <div className="sm2__error sm2__copy-diff-error">{diffError}</div>}
    </section>
  )
}

function CopyDiffDialog({ preview, onClose }: { preview: CopyTargetDiffPreview; onClose: () => void }) {
  const [activePath, setActivePath] = useState(preview.files[0]?.path ?? null)
  const oldScrollRef = useRef<HTMLDivElement | null>(null)
  const newScrollRef = useRef<HTMLDivElement | null>(null)
  const syncingScrollRef = useRef(false)
  const activeFile = preview.files.find((file) => file.path === activePath) ?? preview.files[0] ?? null
  const tree = buildDiffTree(preview.files)
  const rows = activeFile ? buildSplitDiff(activeFile) : []

  const syncVerticalScroll = (side: 'old' | 'new', event: UIEvent<HTMLDivElement>) => {
    if (syncingScrollRef.current) return
    const next = side === 'old' ? newScrollRef.current : oldScrollRef.current
    if (!next) return
    syncingScrollRef.current = true
    next.scrollTop = event.currentTarget.scrollTop
    requestAnimationFrame(() => {
      syncingScrollRef.current = false
    })
  }

  if (typeof document === 'undefined') return null

  return createPortal(
    <div className="sm2__copy-diff-overlay" onClick={onClose}>
      <section className="sm2__copy-diff-dialog" role="dialog" aria-modal="true" aria-label="Agent 副本 diff" onClick={(e) => e.stopPropagation()}>
        <header className="sm2__copy-diff-dialog-head">
          <div>
            <span>Agent 副本 diff</span>
            <h3>中心库对比 Agent 副本</h3>
          </div>
          <div className="sm2__copy-diff-dialog-meta">
            <strong>{preview.files.length} 个文件有差异</strong>
            <button className="sm2__slideover-close" aria-label="关闭 diff" onClick={onClose}>✕</button>
          </div>
        </header>

        <div className="sm2__copy-diff-pathbar">
          <code>{preview.centerPath}</code>
          <span>→</span>
          <code>{preview.targetPath}</code>
        </div>

        {preview.files.length === 0 ? (
          <div className="sm2__empty sm2__empty--compact">这个副本当前与中心库一致</div>
        ) : (
          <div className="sm2__copy-diff-workspace">
            <aside className="sm2__copy-diff-tree-pane">
              <div className="sm2__copy-diff-tree-head">
                <span>目录树</span>
                <strong>{preview.files.length}</strong>
              </div>
              <div className="sm2__copy-diff-tree settings-scroll">
                {tree.children.map((node) => (
                  <DiffTreeNodeView key={node.path} node={node} activePath={activeFile?.path ?? null} onSelect={setActivePath} />
                ))}
              </div>
            </aside>

            <main className="sm2__copy-diff-code-pane">
              {activeFile && (
                <>
                  <div className="sm2__copy-diff-code-head">
                    <div>
                      <strong>{activeFile.path}</strong>
                      <span>{copyDiffChangeLabel(activeFile.changeType)}</span>
                    </div>
                  </div>
                  <div className="sm2__copy-diff-code settings-scroll selectable">
                    <div className="sm2__copy-diff-split">
                      <DiffSide title="中心库" side="old" rows={rows} scrollRef={oldScrollRef} onScroll={syncVerticalScroll} />
                      <DiffSide title="Agent 副本" side="new" rows={rows} scrollRef={newScrollRef} onScroll={syncVerticalScroll} />
                    </div>
                  </div>
                </>
              )}
            </main>
          </div>
        )}
      </section>
    </div>,
    document.body,
  )
}

function DiffSide({
  title,
  side,
  rows,
  scrollRef,
  onScroll,
}: {
  title: string
  side: 'old' | 'new'
  rows: SplitDiffRow[]
  scrollRef: RefObject<HTMLDivElement | null>
  onScroll: (side: 'old' | 'new', event: UIEvent<HTMLDivElement>) => void
}) {
  return (
    <section className={`sm2__copy-diff-side sm2__copy-diff-side--${side}`} aria-label={`${title}文件`}>
      <div className="sm2__copy-diff-side-title">{title}</div>
      <div
        ref={scrollRef}
        className="sm2__copy-diff-side-scroll"
        data-scroll-mode="independent-x-fixed-y-sync"
        onScroll={(event) => onScroll(side, event)}
      >
        {rows.map((row, index) => (
          row.type === 'hunk' ? (
            <div key={`hunk-${side}-${index}`} className="sm2__copy-diff-side-hunk">
              {row.hunk}
            </div>
          ) : (
            <div key={`${side}-${index}-${row.oldLine ?? ''}-${row.newLine ?? ''}`} className="sm2__copy-diff-side-row">
              <span className="sm2__copy-diff-ln">{side === 'old' ? row.oldLine ?? '' : row.newLine ?? ''}</span>
              <code className={`sm2__copy-diff-cell sm2__copy-diff-cell--${side === 'old' ? row.oldKind : row.newKind}`}>
                {side === 'old' ? row.oldText ?? '' : row.newText ?? ''}
              </code>
            </div>
          )
        ))}
      </div>
    </section>
  )
}

function isCopyDiffStatus(status: string): boolean {
  return status === 'copy_modified' || status === 'copy_diverged' || status === 'copy_outdated' || status === 'copyDiverged'
}

function copyDiffChangeLabel(changeType: string): string {
  if (changeType === 'copy_added') return '副本新增'
  if (changeType === 'copy_removed') return '副本缺失'
  if (changeType === 'modified') return '内容不同'
  return changeType
}

type CopyDiffFile = CopyTargetDiffPreview['files'][number]

interface DiffTreeNode {
  name: string
  path: string
  children: DiffTreeNode[]
  file?: CopyDiffFile
}

type DiffCellKind = 'context' | 'add' | 'remove' | 'empty'

interface DiffOp {
  type: 'context' | 'add' | 'remove'
  text: string
  oldLine?: number
  newLine?: number
}

type SplitDiffRow = {
  type: 'hunk'
  hunk: string
} | {
  type: 'line'
  oldLine?: number
  oldText?: string
  oldKind: DiffCellKind
  newLine?: number
  newText?: string
  newKind: DiffCellKind
}

function buildDiffTree(files: CopyDiffFile[]): DiffTreeNode {
  const root: DiffTreeNode = { name: '', path: '', children: [] }
  for (const file of files) {
    const parts = file.path.split(/[\\/]+/).filter(Boolean)
    let node = root
    parts.forEach((part, index) => {
      const path = parts.slice(0, index + 1).join('/')
      let child = node.children.find((item) => item.name === part)
      if (!child) {
        child = { name: part, path, children: [] }
        node.children.push(child)
        node.children.sort((a, b) => {
          if (Boolean(a.file) !== Boolean(b.file)) return a.file ? 1 : -1
          return a.name.localeCompare(b.name)
        })
      }
      node = child
    })
    node.file = file
  }
  return root
}

function DiffTreeNodeView({
  node,
  activePath,
  onSelect,
  depth = 0,
}: {
  node: DiffTreeNode
  activePath: string | null
  onSelect: (path: string) => void
  depth?: number
}) {
  if (node.file) {
    return (
      <button
        className={`sm2__copy-diff-tree-file${node.path === activePath ? ' sm2__copy-diff-tree-file--active' : ''}`}
        style={{ paddingLeft: 10 + depth * 14 }}
        onClick={() => onSelect(node.path)}
      >
        <span>{node.name}</span>
        <em>{copyDiffChangeLabel(node.file.changeType)}</em>
      </button>
    )
  }
  return (
    <div className="sm2__copy-diff-tree-dir">
      <div className="sm2__copy-diff-tree-dir-name" style={{ paddingLeft: 10 + depth * 14 }}>
        {node.name}
      </div>
      {node.children.map((child) => (
        <DiffTreeNodeView key={child.path} node={child} activePath={activePath} onSelect={onSelect} depth={depth + 1} />
      ))}
    </div>
  )
}

function buildSplitDiff(file: CopyDiffFile): SplitDiffRow[] {
  const oldLines = splitDiffLines(file.centerContent)
  const newLines = splitDiffLines(file.copyContent)
  const rows: SplitDiffRow[] = [{
    type: 'hunk',
    hunk: `@@ -1,${Math.max(oldLines.length, 1)} +1,${Math.max(newLines.length, 1)} @@`,
  }]

  if (file.changeType === 'copy_added') {
    return rows.concat(newLines.map((text, index) => ({
      type: 'line',
      oldKind: 'empty',
      newLine: index + 1,
      newText: text,
      newKind: 'add',
    })))
  }
  if (file.changeType === 'copy_removed') {
    return rows.concat(oldLines.map((text, index) => ({
      type: 'line',
      oldLine: index + 1,
      oldText: text,
      oldKind: 'remove',
      newKind: 'empty',
    })))
  }
  return rows.concat(opsToSplitRows(buildDiffOps(oldLines, newLines)))
}

function buildDiffOps(oldLines: string[], newLines: string[]): DiffOp[] {
  if (oldLines.length * newLines.length > 1000000) {
    return [
      ...oldLines.map((text, index) => ({ type: 'remove' as const, text, oldLine: index + 1 })),
      ...newLines.map((text, index) => ({ type: 'add' as const, text, newLine: index + 1 })),
    ]
  }
  const table = longestCommonSubsequenceTable(oldLines, newLines)
  const ops: DiffOp[] = []
  let oldIndex = 0
  let newIndex = 0
  let oldLine = 1
  let newLine = 1
  while (oldIndex < oldLines.length || newIndex < newLines.length) {
    if (oldIndex < oldLines.length && newIndex < newLines.length && oldLines[oldIndex] === newLines[newIndex]) {
      ops.push({ type: 'context', text: oldLines[oldIndex], oldLine, newLine })
      oldIndex += 1
      newIndex += 1
      oldLine += 1
      newLine += 1
    } else if (newIndex < newLines.length && (oldIndex === oldLines.length || table[oldIndex][newIndex + 1] > table[oldIndex + 1][newIndex])) {
      ops.push({ type: 'add', text: newLines[newIndex], newLine })
      newIndex += 1
      newLine += 1
    } else if (oldIndex < oldLines.length) {
      ops.push({ type: 'remove', text: oldLines[oldIndex], oldLine })
      oldIndex += 1
      oldLine += 1
    }
  }
  return ops
}

function opsToSplitRows(ops: DiffOp[]): SplitDiffRow[] {
  const rows: SplitDiffRow[] = []
  let index = 0
  while (index < ops.length) {
    const op = ops[index]
    if (op.type === 'context') {
      rows.push({
        type: 'line',
        oldLine: op.oldLine,
        oldText: op.text,
        oldKind: 'context',
        newLine: op.newLine,
        newText: op.text,
        newKind: 'context',
      })
      index += 1
      continue
    }

    const removes: DiffOp[] = []
    const adds: DiffOp[] = []
    while (index < ops.length && ops[index].type !== 'context') {
      const changed = ops[index]
      if (changed.type === 'remove') removes.push(changed)
      if (changed.type === 'add') adds.push(changed)
      index += 1
    }

    const rowCount = Math.max(removes.length, adds.length)
    for (let rowIndex = 0; rowIndex < rowCount; rowIndex += 1) {
      const removed = removes[rowIndex]
      const added = adds[rowIndex]
      rows.push({
        type: 'line',
        oldLine: removed?.oldLine,
        oldText: removed?.text,
        oldKind: removed ? 'remove' : 'empty',
        newLine: added?.newLine,
        newText: added?.text,
        newKind: added ? 'add' : 'empty',
      })
    }
  }
  return rows
}

function splitDiffLines(content?: string | null): string[] {
  if (content == null) return []
  const lines = content.split(/\r?\n/)
  if (lines.length > 1 && lines[lines.length - 1] === '') return lines.slice(0, -1)
  return lines
}

function longestCommonSubsequenceTable(oldLines: string[], newLines: string[]): number[][] {
  const table = Array.from({ length: oldLines.length + 1 }, () => Array(newLines.length + 1).fill(0))
  for (let oldIndex = oldLines.length - 1; oldIndex >= 0; oldIndex -= 1) {
    for (let newIndex = newLines.length - 1; newIndex >= 0; newIndex -= 1) {
      table[oldIndex][newIndex] = oldLines[oldIndex] === newLines[newIndex]
        ? table[oldIndex + 1][newIndex + 1] + 1
        : Math.max(table[oldIndex + 1][newIndex], table[oldIndex][newIndex + 1])
    }
  }
  return table
}

function SkillFrontmatterIntro({ description, compact = false }: { description?: string; compact?: boolean }) {
  const text = description?.trim()
  if (!text) return null
  return (
    <div className={`sm2__skill-frontmatter${compact ? ' sm2__skill-frontmatter--compact' : ''}`}>
      <span>说明</span>
      <p>{text}</p>
    </div>
  )
}

function SourceTab({ detail }: { detail: SkillDetail }) {
  const { t } = useTranslation()
  const sourceType = detail.source?.sourceType || detail.sourceType
  const sourceUri = detail.source?.sourceUri || detail.sourceUri
  const linkedCenter = isLinkedCenterSkill(detail)
  const summaryCards = [
    { label: '类型', value: skillSourceTypeLabel(t, sourceType) },
    { label: '中心类型', value: linkedCenter ? '软链中心目录' : null },
    { label: '导入 Agent', value: detail.source?.importedFromAgent },
    { label: '安装方式', value: detail.source?.installedVia },
    { label: '来源 Ref', value: detail.source?.sourceRef },
  ].filter(hasSourceValue)
  const pathCards = [
    { label: '真实源目录', value: linkedCenter ? detail.centerResolvedPath : null },
    { label: '导入路径', value: detail.source?.importedFromPath },
    { label: '中心目录', value: detail.centerPath },
    { label: '来源 URI', value: sourceUri },
    { label: 'Hash', value: detail.currentHash },
  ].filter(hasSourceValue)
  return (
    <section className="sm2__skill-source">
      {summaryCards.length > 0 && (
        <div className="sm2__skill-source-grid">
          {summaryCards.map((item) => (
            <div key={item.label} className="sm2__skill-source-card">
              <span>{item.label}</span>
              <strong title={item.value}>{item.value}</strong>
            </div>
          ))}
        </div>
      )}
      {pathCards.length > 0 && (
        <div className="sm2__skill-source-stack">
          {pathCards.map((item) => (
            <div key={item.label} className="sm2__skill-source-card sm2__skill-source-card--wide">
              <span>{item.label}</span>
              <code className="selectable" title={item.value}>{item.value}</code>
            </div>
          ))}
        </div>
      )}
    </section>
  )
}

function hasSourceValue(item: { label: string; value?: string | null }): item is { label: string; value: string } {
  return typeof item.value === 'string' && item.value.length > 0
}

function isLinkedCenterSkill(detail: SkillDetail): boolean {
  return Boolean(detail.centerResolvedPath && detail.centerResolvedPath !== detail.centerPath)
}

function linkedCenterSkillTitle(detail: SkillDetail): string {
  const realPath = detail.centerResolvedPath || detail.source?.sourceUri || detail.sourceUri || detail.centerPath
  return `本地文件夹导入的 Skill，真实地址是：${realPath}`
}

function CompactInfo({
  label,
  value,
  mono = false,
  short = false,
}: {
  label: string
  value: string
  mono?: boolean
  short?: boolean
}) {
  const display = short && value.length > 12 ? value.slice(0, 12) : value
  return (
    <div className="sm2__compact-row">
      <span>{label}</span>
      {mono ? <code className="selectable" title={value}>{display}</code> : <strong title={value}>{display}</strong>}
    </div>
  )
}

function MarkdownLink({ href, children, onClick, ...props }: AnchorHTMLAttributes<HTMLAnchorElement>) {
  const handleClick = (event: MouseEvent<HTMLAnchorElement>) => {
    onClick?.(event)
    if (event.defaultPrevented || !href || href.startsWith('#')) return
    event.preventDefault()
    openMarkdownHref(href)
  }

  return (
    <a {...props} href={href} target="_blank" rel="noreferrer" onClick={handleClick}>
      {children}
    </a>
  )
}

function openMarkdownHref(href: string) {
  const target = href.trim()
  if (!target || /^javascript:/i.test(target)) return
  if (isTauri()) {
    openShell(target).catch((err) => console.warn('[skills] open markdown link:', err))
  } else {
    window.open(target, '_blank', 'noopener,noreferrer')
  }
}

function findFile(node: FileTreeNode | null, name: string): string | null {
  if (!node) return null
  if (node.name === name && node.nodeType === 'file') return node.path
  if (node.children) {
    for (const c of node.children) {
      const f = findFile(c, name)
      if (f) return f
    }
  }
  return null
}

function countFiles(node: FileTreeNode | null): number {
  if (!node) return 0
  if (node.nodeType === 'file') return 1
  return node.children?.reduce((sum, child) => sum + countFiles(child), 0) || 0
}

function pathBasename(path: string): string {
  return path.split(/[\\/]+/).filter(Boolean).pop() || ''
}

function relativeFilePath(path: string, root: string): string {
  const normalizedPath = path.replace(/\\/g, '/')
  const normalizedRoot = root.replace(/\\/g, '/').replace(/\/+$/, '')
  if (normalizedPath === normalizedRoot) return pathBasename(path) || path
  const prefix = `${normalizedRoot}/`
  if (normalizedPath.startsWith(prefix)) return normalizedPath.slice(prefix.length)
  return normalizedPath
}

function isMarkdownPath(path: string | null): boolean {
  return Boolean(path && /\.(md|mdx|markdown)$/i.test(path))
}

function isSkillMarkdownPath(path: string | null): boolean {
  return Boolean(path && /(^|\/)SKILL\.md$/i.test(path))
}

function formatMetaValue(value: string): string {
  const trimmed = value.trim()
  if (!trimmed) return '空'
  if ((trimmed.startsWith('[') && trimmed.endsWith(']')) || (trimmed.startsWith('{') && trimmed.endsWith('}'))) {
    try {
      const parsed = JSON.parse(trimmed)
      if (Array.isArray(parsed)) return parsed.join(', ')
      if (typeof parsed === 'object' && parsed) return JSON.stringify(parsed, null, 2)
    } catch {
      return value
    }
  }
  return value
}

function isCodeLikeMeta(key: string, value: string): boolean {
  const normalizedKey = key.toLowerCase()
  return normalizedKey.includes('path')
    || normalizedKey.includes('bin')
    || normalizedKey.includes('command')
    || normalizedKey.includes('help')
    || value.includes('/')
    || value.includes('--')
    || value.startsWith('[')
    || value.startsWith('{')
}

function fallbackToSkillDetail(fallback: SkillDetailFallback): SkillDetail {
  return {
    id: fallback.id,
    name: fallback.name,
    description: fallback.description || '这个 Skill 尚未接管到中心库，只能预览本地说明文档。',
    skillType: 'skill',
    sourceType: fallback.sourceType || 'unmanaged_agent',
    sourceUri: fallback.sourceUri || fallback.centerPath,
    centerPath: fallback.centerPath,
    currentHash: 'unmanaged',
    status: 'unmanaged',
    installedAgents: [],
    frontmatter: {},
    files: null,
    targets: [],
    source: null,
  }
}

function FileTree({
  node,
  depth,
  active,
  onSelect,
}: {
  node: FileTreeNode
  depth: number
  active: string | null
  onSelect: (path: string) => void
}) {
  const [expanded, setExpanded] = useState(depth < 1)
  const isDir = node.nodeType === 'dir'
  const activeDescendant = Boolean(active && isDir && node.children?.some((child) => nodeContainsPath(child, active)))
  const isActive = active === node.path
  return (
    <div className={`sm2__filetree-node${isDir ? ' sm2__filetree-node--dir' : ' sm2__filetree-node--file'}`}>
      <button
        type="button"
        className={[
          'sm2__filetree-row',
          isActive ? 'sm2__filetree-row--active' : '',
          activeDescendant ? 'sm2__filetree-row--branch' : '',
        ].filter(Boolean).join(' ')}
        style={{ paddingLeft: 10 + depth * 14 }}
        aria-expanded={isDir ? expanded : undefined}
        onClick={() => (isDir ? setExpanded(!expanded) : onSelect(node.path))}
      >
        <span className="sm2__filetree-twist">{isDir ? (expanded ? '▾' : '▸') : ''}</span>
        <span className={`sm2__filetree-kind${isDir ? ' sm2__filetree-kind--dir' : ' sm2__filetree-kind--file'}`} />
        <span className="sm2__filetree-name">{node.name}</span>
      </button>
      {isDir && expanded && node.children && (
        <div>
          {node.children.map((c) => (
            <FileTree key={c.path} node={c} depth={depth + 1} active={active} onSelect={onSelect} />
          ))}
        </div>
      )}
    </div>
  )
}

function nodeContainsPath(node: FileTreeNode, path: string): boolean {
  if (node.path === path) return true
  return node.children?.some((child) => nodeContainsPath(child, path)) || false
}
