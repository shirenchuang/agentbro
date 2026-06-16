import { useEffect, useState } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { skillApiV2 } from '../../services/skillApiV2'
import { skillApi } from '../../services/skillApi'
import type { SkillDetail, SkillSummary, FileTreeNode } from '../../services/skillApiV2'
import { SlideOver } from './SlideOver'
import { AgentIconBadge } from './AgentIconBadge'

type DetailTab = 'overview' | 'files' | 'agents' | 'source'
type FileViewMode = 'preview' | 'source'

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
  const [detail, setDetail] = useState<SkillDetail | null>(null)
  const [loading, setLoading] = useState(false)
  const [activeFile, setActiveFile] = useState<string | null>(null)
  const [fileContent, setFileContent] = useState('')
  const [fileLoading, setFileLoading] = useState(false)
  const [syncing, setSyncing] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [tab, setTab] = useState<DetailTab>('overview')
  const [fileViewMode, setFileViewMode] = useState<FileViewMode>('preview')
  const [skillDocPath, setSkillDocPath] = useState<string | null>(null)
  const [skillDocContent, setSkillDocContent] = useState('')

  useEffect(() => {
    if (!open || !skillId) return
    setLoading(true)
    setError(null)
    setTab('overview')
    setDetail(null)
    setSkillDocContent('')
    setFileContent('')
    skillApiV2
      .getSkillDetail(skillId)
      .then((d) => {
        setDetail(d)
        const skillMd = findFile(d.files, 'SKILL.md')
        setSkillDocPath(skillMd)
        if (skillMd) {
          loadFile(skillMd)
          skillApi.readFileContent(skillMd).then(setSkillDocContent).catch(() => setSkillDocContent(''))
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
        setSkillDocPath(`${fallbackSkill.centerPath}/SKILL.md`)
        skillApi
          .readFileContent(`${fallbackSkill.centerPath}/SKILL.md`)
          .then((content) => {
            setSkillDocContent(content)
            setFileContent(content)
          })
          .catch(() => {
            setSkillDocContent('')
            setFileContent('')
          })
      })
      .finally(() => setLoading(false))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, skillId, fallbackSkill?.centerPath])

  const loadFile = async (path: string) => {
    setActiveFile(path)
    setFileViewMode(isMarkdownPath(path) ? 'preview' : 'source')
    setFileLoading(true)
    try {
      const content = await skillApi.readFileContent(path)
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
    } catch (e) {
      setError(String(e))
    } finally {
      setSyncing(null)
    }
  }

  const summary: SkillSummary | null = detail
  const tabs: Array<{ id: DetailTab; label: string }> = [
    { id: 'overview', label: '概览' },
    { id: 'files', label: '文件' },
    { id: 'agents', label: `Agent (${detail?.targets.length || 0})` },
    { id: 'source', label: '来源' },
  ]

  return (
    <SlideOver
      open={open}
      onClose={onClose}
      width={1040}
      className="sm2__slideover--skill-detail"
      title={summary?.name || skillId || ''}
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
        <div className="sm2__skill-detail">
          <div className="sm2__detail-pills">
            <span className={`sm2__tag sm2__tag--${detail.status}`}>{STATUS_LABEL[detail.status] || detail.status}</span>
            <span className="sm2__tag">{detail.sourceType}</span>
            <span className="sm2__tag">{detail.skillType}</span>
            {detail.installedAgents.length > 0 && (
              <span className="sm2__agents">
                {detail.installedAgents.map((a) => (
                  <AgentIconBadge key={a.agentId} iconKey={a.iconKey} mode={a.mode} title={`${a.displayName} · ${a.mode}`} />
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
          {tab === 'agents' && <AgentsTab detail={detail} syncing={syncing} onSync={doSync} />}
          {tab === 'source' && <SourceTab detail={detail} />}
        </div>
      )}
    </SlideOver>
  )
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
  const frontmatter = Object.entries(detail.frontmatter || {})
  const sourceLabel = detail.source?.sourceType || detail.sourceType
  const sourceValue = detail.source?.sourceUri || detail.sourceUri || sourceLabel
  return (
    <div className="sm2__detail-overview sm2__detail-overview--reader">
      <section className="sm2__skill-doc">
        <div className="sm2__skill-doc-head">
          <div>
            <span>{docPath ? docPath.split('/').pop() : 'SKILL.md'}</span>
            <strong>说明文档</strong>
          </div>
          <small>{STATUS_LABEL[detail.status] || detail.status}</small>
        </div>
        <div className="sm2__markdown sm2__markdown--document sm2__markdown--skilldoc">
          {fileLoading ? (
            <div className="sm2__empty sm2__empty--compact">读取说明文档…</div>
          ) : docContent ? (
            <>
              <SkillFrontmatterIntro description={detail.frontmatter.description || detail.description} />
              <ReactMarkdown remarkPlugins={[remarkGfm]}>{stripFrontmatter(docContent)}</ReactMarkdown>
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
                return (
                  <div key={target.id} className="sm2__install-mini">
                    <AgentIconBadge iconKey={agent?.iconKey || target.agentId} mode={target.actualMode} size={26} />
                    <div>
                      <strong>{agent?.displayName || target.agentId}</strong>
                      <span>{target.actualMode} · {STATUS_LABEL[target.status] || target.status}</span>
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
            <CompactInfo label="中心目录" value={detail.centerPath} mono />
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
  const activeName = activeFile ? activeFile.split('/').pop() || activeFile : '未选择文件'
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
              <div className={`sm2__markdown sm2__markdown--file${isSkillMarkdownPath(activeFile) ? ' sm2__markdown--file-skill' : ''}`}>
                {isSkillMarkdownPath(activeFile) && (
                  <SkillFrontmatterIntro description={extractFrontmatterDescription(fileContent)} compact />
                )}
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{stripFrontmatter(fileContent || '（空）')}</ReactMarkdown>
              </div>
            ) : (
              <pre className="sm2__fileview-content">{fileContent || '（空）'}</pre>
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
  onSync,
}: {
  detail: SkillDetail
  syncing: string | null
  onSync: (targetId: string, action: string) => void
}) {
  if (detail.targets.length === 0) {
    return <div className="sm2__empty sm2__empty--compact">尚未分发到任何 Agent</div>
  }
  return (
    <section className="sm2__panel">
      {detail.targets.map((t) => (
        <div key={t.id} className="sm2__object-row sm2__object-row--path">
          <div className="sm2__object-row-titleline">
            <AgentIconBadge iconKey={t.agentId} mode={t.actualMode} size={26} />
            <div>
              <strong>{t.agentId}</strong>
              <span>{t.actualMode} · {STATUS_LABEL[t.status] || t.status}</span>
            </div>
          </div>
          <div className="sm2__object-row-body">
            <code>{t.targetPath}</code>
            <div className="sm2__claims">
              {t.claims.map((c) => (
                <span key={c.id} className="sm2__tag">
                  {c.claimType === 'pack' ? `技能包：${c.packName}` : '独立安装'}
                </span>
              ))}
            </div>
            {t.actualMode === 'copy' && t.status !== 'ok' && (
              <div className="sm2__btn-row">
                {t.status === 'copy_outdated' && (
                  <button className="sm2__btn" disabled={syncing === t.id} onClick={() => onSync(t.id, 'center_over_agent')}>用中心库更新</button>
                )}
                {t.status === 'copy_modified' && (
                  <button className="sm2__btn" disabled={syncing === t.id} onClick={() => onSync(t.id, 'agent_over_center')}>推回中心库</button>
                )}
                {t.status === 'copy_diverged' && (
                  <>
                    <button className="sm2__btn" disabled={syncing === t.id} onClick={() => onSync(t.id, 'center_over_agent')}>中心库覆盖</button>
                    <button className="sm2__btn" disabled={syncing === t.id} onClick={() => onSync(t.id, 'agent_over_center')}>副本覆盖</button>
                    <button className="sm2__btn" disabled={syncing === t.id} onClick={() => onSync(t.id, 'keep_diverged')}>保留分叉</button>
                  </>
                )}
              </div>
            )}
          </div>
        </div>
      ))}
    </section>
  )
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
  const sourceType = detail.source?.sourceType || detail.sourceType
  const sourceUri = detail.source?.sourceUri || detail.sourceUri
  const summaryCards = [
    { label: '类型', value: sourceType },
    { label: '导入 Agent', value: detail.source?.importedFromAgent },
    { label: '安装方式', value: detail.source?.installedVia },
    { label: '来源 Ref', value: detail.source?.sourceRef },
  ].filter(hasSourceValue)
  const pathCards = [
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
              <code title={item.value}>{item.value}</code>
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
      {mono ? <code title={value}>{display}</code> : <strong title={value}>{display}</strong>}
    </div>
  )
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

function relativeFilePath(path: string, root: string): string {
  if (path === root) return path.split('/').pop() || path
  const prefix = root.endsWith('/') ? root : `${root}/`
  if (path.startsWith(prefix)) return path.slice(prefix.length)
  return path
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

function stripFrontmatter(content: string): string {
  if (!content.startsWith('---')) return content
  const end = content.indexOf('\n---', 3)
  if (end === -1) return content
  return content.slice(end + 4).trim()
}

function extractFrontmatterDescription(content: string): string {
  if (!content.startsWith('---')) return ''
  const end = content.indexOf('\n---', 3)
  if (end === -1) return ''
  const frontmatter = content.slice(3, end)
  const lines = frontmatter.split(/\r?\n/)
  const descriptionLine = lines.find((line) => line.trim().startsWith('description:'))
  if (!descriptionLine) return ''
  return descriptionLine
    .split(/:(.*)/s)[1]
    ?.trim()
    .replace(/^['"]|['"]$/g, '') || ''
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
