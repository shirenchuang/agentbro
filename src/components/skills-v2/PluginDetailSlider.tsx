import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  skillApiV2,
  type PluginDetail,
  type PluginFileContent,
  type PluginFileNode,
  type PluginStatus,
} from '../../services/skillApiV2'
import { SlideOver } from './SlideOver'
import { isMarkdownPath } from './filePreview'
import { parseSkillFrontmatter, stripSkillFrontmatter } from './frontmatter'
import { MarkdownContent } from './MarkdownContent'

type PluginDetailTab = 'overview' | 'files'
type FileViewMode = 'preview' | 'source'

export function PluginDetailSlider({
  agentId,
  plugin,
  open,
  onClose,
}: {
  agentId: string
  plugin: PluginStatus | null
  open: boolean
  onClose: () => void
}) {
  const { t } = useTranslation()
  const source = plugin?.source?.replace(/^[^:]+:/, '') || t('skills.pluginManagement.local')

  return (
    <SlideOver
      open={open}
      onClose={onClose}
      width={920}
      className="sm2__slideover--skill-detail sm2__slideover--plugin-detail"
      title={plugin?.name || ''}
      subtitle={plugin ? `${plugin.id} · ${source}${plugin.version ? ` · v${plugin.version}` : ''}` : undefined}
    >
      {open && plugin && (
        <PluginDetailContent
          key={`${agentId}:${plugin.id}`}
          agentId={agentId}
          plugin={plugin}
          source={source}
        />
      )}
    </SlideOver>
  )
}

function PluginDetailContent({
  agentId,
  plugin,
  source,
}: {
  agentId: string
  plugin: PluginStatus
  source: string
}) {
  const { t } = useTranslation()
  const [detail, setDetail] = useState<PluginDetail | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [tab, setTab] = useState<PluginDetailTab>('overview')
  const [selectedNode, setSelectedNode] = useState<PluginFileNode | null>(null)

  useEffect(() => {
    let cancelled = false
    skillApiV2
      .getPluginDetail(agentId, plugin.id)
      .then((nextDetail) => {
        if (cancelled) return
        setDetail(nextDetail)
        setSelectedNode(nextDetail.files)
      })
      .catch((nextError) => {
        if (!cancelled) setError(String(nextError))
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [agentId, plugin.id])

  const components = useMemo(
    () => detail?.files ? summarizePluginComponents(detail.files, t) : [],
    [detail, t],
  )

  return (
    <>
      {loading && (
        <div className="sm2__plugin-drawer-loading" role="status">
          <span className="sm2__spinner" aria-hidden="true" />
          {t('skills.pluginManagement.loadingDetails')}
        </div>
      )}
      {error && <div className="sm2__error" role="alert">{error}</div>}
      {detail && (
        <div className="sm2__skill-detail sm2__plugin-drawer-content">
          <div className="sm2__detail-pills">
            <span className={`sm2__tag sm2__tag--${detail.enabled ? 'ok' : 'unmanaged'}`}>
              {t(detail.enabled
                ? 'skills.pluginManagement.enabledTag'
                : 'skills.pluginManagement.disabledTag')}
            </span>
            <span className="sm2__tag">{source}</span>
            {detail.version && <span className="sm2__tag">v{detail.version}</span>}
            <span className="sm2__tag sm2__plugin-readonly-pill">
              {t('skills.pluginManagement.structureReadOnly')}
            </span>
          </div>

          <div className="sm2__subtabs" role="tablist">
            <button
              type="button"
              role="tab"
              aria-selected={tab === 'overview'}
              className={`sm2__subtab${tab === 'overview' ? ' sm2__subtab--active' : ''}`}
              onClick={() => setTab('overview')}
            >
              {t('skills.pluginManagement.overviewTab')}
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={tab === 'files'}
              className={`sm2__subtab${tab === 'files' ? ' sm2__subtab--active' : ''}`}
              onClick={() => setTab('files')}
            >
              {t('skills.pluginManagement.filesTab', { count: detail.fileCount })}
            </button>
          </div>

          {tab === 'overview' && (
            <PluginOverview detail={detail} components={components} />
          )}
          {tab === 'files' && (
            <PluginFiles
              agentId={agentId}
              pluginId={plugin.id}
              detail={detail}
              selectedNode={selectedNode}
              onSelect={setSelectedNode}
            />
          )}
        </div>
      )}
    </>
  )
}

function PluginOverview({
  detail,
  components,
}: {
  detail: PluginDetail
  components: Array<{ id: string; label: string; count: number }>
}) {
  const { t } = useTranslation()
  return (
    <div className="sm2__plugin-overview">
      <section className="sm2__plugin-manifest-card">
        <div className="sm2__plugin-manifest-glyph" aria-hidden="true">
          {pluginInitials(detail.name)}
        </div>
        <div>
          <span>{t('skills.pluginManagement.about')}</span>
          <p>{detail.description || t('skills.pluginManagement.descriptionUnavailable')}</p>
        </div>
      </section>

      <section className="sm2__plugin-overview-section">
        <div className="sm2__plugin-section-head">
          <div>
            <span>{t('skills.pluginManagement.packageContents')}</span>
            <strong>{t('skills.pluginManagement.packageContentsDescription')}</strong>
          </div>
          <small>{t('skills.pluginManagement.itemsShown', { count: detail.fileCount })}</small>
        </div>
        {components.length > 0 ? (
          <div className="sm2__plugin-capability-grid">
            {components.map((component) => (
              <div key={component.id} className="sm2__plugin-capability">
                <span aria-hidden="true">{componentGlyph(component.id)}</span>
                <div>
                  <strong>{component.label}</strong>
                  <small>
                    {t('skills.pluginManagement.componentItems', { count: component.count })}
                  </small>
                </div>
              </div>
            ))}
          </div>
        ) : (
          <p className="sm2__plugin-overview-empty">
            {t('skills.pluginManagement.packageContentsUnavailable')}
          </p>
        )}
      </section>

      <section className="sm2__plugin-overview-section">
        <div className="sm2__plugin-section-head">
          <div>
            <span>{t('skills.pluginManagement.metadata')}</span>
            <strong>{t('skills.pluginManagement.metadataDescription')}</strong>
          </div>
        </div>
        <dl className="sm2__plugin-drawer-meta">
          <PluginMeta label={t('skills.pluginManagement.pluginId')} value={detail.id} code />
          <PluginMeta
            label={t('skills.pluginManagement.source')}
            value={detail.source || t('skills.pluginManagement.local')}
            code
          />
          {detail.version && (
            <PluginMeta label={t('skills.pluginManagement.version')} value={detail.version} />
          )}
          {detail.author && (
            <PluginMeta label={t('skills.pluginManagement.author')} value={detail.author} />
          )}
          {detail.license && (
            <PluginMeta label={t('skills.pluginManagement.license')} value={detail.license} />
          )}
          {detail.homepage && (
            <PluginMeta label={t('skills.pluginManagement.homepage')} value={detail.homepage} code />
          )}
          {detail.installPath && (
            <PluginMeta
              label={t('skills.pluginManagement.installPath')}
              value={detail.installPath}
              code
              wide
            />
          )}
          {detail.manifestPath && (
            <PluginMeta
              label={t('skills.pluginManagement.manifestPath')}
              value={detail.manifestPath}
              code
              wide
            />
          )}
        </dl>
      </section>
    </div>
  )
}

function PluginFiles({
  agentId,
  pluginId,
  detail,
  selectedNode,
  onSelect,
}: {
  agentId: string
  pluginId: string
  detail: PluginDetail
  selectedNode: PluginFileNode | null
  onSelect: (node: PluginFileNode) => void
}) {
  const { t } = useTranslation()
  const [viewPreference, setViewPreference] = useState<{
    path: string
    mode: FileViewMode
  } | null>(null)

  if (!detail.files) {
    return (
      <div className="sm2__empty sm2__empty--compact sm2__plugin-drawer-empty">
        <strong>{t('skills.pluginManagement.filesUnavailableTitle')}</strong>
        <span>{t('skills.pluginManagement.filesUnavailableDescription')}</span>
      </div>
    )
  }
  const canPreview = selectedNode?.nodeType === 'file' && isMarkdownPath(selectedNode.path)
  const effectiveMode = canPreview
    ? viewPreference?.path === selectedNode.path ? viewPreference.mode : 'preview'
    : 'source'
  const selectNode = (node: PluginFileNode) => {
    onSelect(node)
  }
  return (
    <section className="sm2__panel sm2__panel--flush sm2__plugin-file-panel">
      <div className="sm2__panel-head sm2__panel-head--filebrowser">
        <div>
          <h3>{t('skills.pluginManagement.fileStructure')}</h3>
          <span>{detail.installPath || detail.files.name}</span>
        </div>
        <strong>{t('skills.pluginManagement.visibleItems', { count: detail.fileCount })}</strong>
      </div>
      {detail.truncated && (
        <div className="sm2__plugin-tree-warning">
          {t('skills.pluginManagement.treeLimited')}
        </div>
      )}
      <div className="sm2__filebrowser sm2__filebrowser--expansive sm2__plugin-filebrowser">
        <div className="sm2__filetree-pane">
          <div className="sm2__filetree-head">
            <span>{t('skills.pluginManagement.directory')}</span>
            <strong>{detail.fileCount}</strong>
          </div>
          <div className="sm2__filetree settings-scroll">
            <PluginTreeNode
              node={detail.files}
              depth={0}
              selectedPath={selectedNode?.path || null}
              onSelect={selectNode}
            />
          </div>
        </div>
        <div className="sm2__fileview sm2__plugin-node-inspector">
          {selectedNode ? (
            <>
              <div className="sm2__fileview-header">
                <div className="sm2__fileview-title">
                  <strong>{selectedNode.name}</strong>
                  <span>{selectedNode.path}</span>
                </div>
                <div className="sm2__plugin-fileview-actions">
                  <span className="sm2__plugin-node-type">
                    {t(`skills.pluginManagement.nodeTypes.${selectedNode.nodeType}`)}
                  </span>
                  {canPreview && (
                    <div
                      className="sm2__filemode-toggle"
                      role="group"
                      aria-label={t('skills.pluginManagement.markdownViewMode')}
                    >
                      <button
                        type="button"
                        className={effectiveMode === 'preview' ? 'active' : ''}
                        aria-pressed={effectiveMode === 'preview'}
                        onClick={() => setViewPreference({
                          path: selectedNode.path,
                          mode: 'preview',
                        })}
                      >
                        {t('skills.pluginManagement.previewMode')}
                      </button>
                      <button
                        type="button"
                        className={effectiveMode === 'source' ? 'active' : ''}
                        aria-pressed={effectiveMode === 'source'}
                        onClick={() => setViewPreference({
                          path: selectedNode.path,
                          mode: 'source',
                        })}
                      >
                        {t('skills.pluginManagement.sourceMode')}
                      </button>
                    </div>
                  )}
                </div>
              </div>
              {selectedNode.nodeType === 'file' ? (
                <PluginFilePreview
                  key={`${pluginId}:${selectedNode.path}`}
                  agentId={agentId}
                  pluginId={pluginId}
                  node={selectedNode}
                  viewMode={effectiveMode}
                />
              ) : (
                <div className="sm2__plugin-node-preview">
                  <div className={`sm2__plugin-node-glyph sm2__plugin-node-glyph--${selectedNode.nodeType}`} aria-hidden="true">
                    {selectedNode.nodeType === 'directory' ? '⌑' : '↗'}
                  </div>
                  <strong>{selectedNode.name}</strong>
                  <code>{selectedNode.path}</code>
                  {selectedNode.nodeType === 'directory' && (
                    <span>
                      {selectedNode.omittedCount !== null
                        ? t('skills.pluginManagement.collapsedItems', { count: selectedNode.omittedCount })
                        : t('skills.pluginManagement.childrenCount', {
                            count: selectedNode.children?.length || 0,
                          })}
                    </span>
                  )}
                  <p>
                    {t(selectedNode.nodeType === 'symlink'
                      ? 'skills.pluginManagement.symlinkNotOpened'
                      : 'skills.pluginManagement.directoryHint')}
                  </p>
                </div>
              )}
            </>
          ) : (
            <div className="sm2__empty sm2__empty--compact">
              {t('skills.pluginManagement.selectNode')}
            </div>
          )}
        </div>
      </div>
    </section>
  )
}

function PluginFilePreview({
  agentId,
  pluginId,
  node,
  viewMode,
}: {
  agentId: string
  pluginId: string
  node: PluginFileNode
  viewMode: FileViewMode
}) {
  const { t } = useTranslation()
  const [file, setFile] = useState<PluginFileContent | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    let cancelled = false
    skillApiV2
      .readPluginFile(agentId, pluginId, node.path)
      .then((nextFile) => {
        if (!cancelled) setFile(nextFile)
      })
      .catch((nextError) => {
        if (!cancelled) setError(String(nextError))
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [agentId, node.path, pluginId])

  if (loading) {
    return (
      <div className="sm2__plugin-file-loading" role="status">
        <span className="sm2__spinner" aria-hidden="true" />
        {t('skills.pluginManagement.loadingFile')}
      </div>
    )
  }
  if (error) {
    return (
      <div className="sm2__plugin-file-message sm2__plugin-file-message--error" role="alert">
        <strong>{t('skills.pluginManagement.filePreviewFailed')}</strong>
        <span>{error}</span>
      </div>
    )
  }
  if (!file) return null

  return (
    <div className="sm2__plugin-file-preview">
      <div className="sm2__plugin-file-preview-meta">
        <span>{formatFileSize(file.size)}</span>
        {file.mimeType && <code>{file.mimeType}</code>}
        <small>{t('skills.pluginManagement.readOnlyPreview')}</small>
      </div>
      {file.kind === 'text' && (
        <>
          {file.truncated && (
            <div className="sm2__plugin-file-truncated">
              {t('skills.pluginManagement.textPreviewTruncated')}
            </div>
          )}
          {viewMode === 'preview' && isMarkdownPath(node.path) ? (
            <div className="sm2__plugin-markdown-preview settings-scroll">
              <div className="sm2__markdown sm2__markdown--file selectable">
                <PluginMarkdownFrontmatter content={file.content || ''} />
                <MarkdownContent content={stripSkillFrontmatter(file.content || '（空）')} />
              </div>
            </div>
          ) : (
            <pre className="sm2__plugin-source-preview" tabIndex={0}>
              <code>{file.content || ''}</code>
            </pre>
          )}
        </>
      )}
      {file.kind === 'image' && file.dataBase64 && file.mimeType && (
        <div className="sm2__plugin-image-preview">
          <img
            src={`data:${file.mimeType};base64,${file.dataBase64}`}
            alt={node.name}
          />
        </div>
      )}
      {file.kind === 'image' && !file.dataBase64 && (
        <div className="sm2__plugin-file-message">
          <div className="sm2__plugin-node-glyph sm2__plugin-node-glyph--file" aria-hidden="true">IMG</div>
          <strong>{t('skills.pluginManagement.imageTooLarge')}</strong>
          <span>{t('skills.pluginManagement.imageTooLargeDescription')}</span>
        </div>
      )}
      {file.kind === 'binary' && (
        <div className="sm2__plugin-file-message">
          <div className="sm2__plugin-node-glyph sm2__plugin-node-glyph--file" aria-hidden="true">BIN</div>
          <strong>{t('skills.pluginManagement.binaryPreviewUnavailable')}</strong>
          <span>{t('skills.pluginManagement.binaryPreviewDescription')}</span>
        </div>
      )}
    </div>
  )
}

function PluginMarkdownFrontmatter({ content }: { content: string }) {
  const { t } = useTranslation()
  const entries = Object.entries(parseSkillFrontmatter(content))
  if (entries.length === 0) return null

  return (
    <aside className="sm2__plugin-markdown-frontmatter">
      <div className="sm2__plugin-markdown-frontmatter-head">
        <span aria-hidden="true">YAML</span>
        <strong>{t('skills.pluginManagement.documentMetadata')}</strong>
      </div>
      <dl>
        {entries.map(([key, value]) => (
          <div key={key}>
            <dt>{key}</dt>
            <dd>{value}</dd>
          </div>
        ))}
      </dl>
    </aside>
  )
}

function PluginTreeNode({
  node,
  depth,
  selectedPath,
  onSelect,
}: {
  node: PluginFileNode
  depth: number
  selectedPath: string | null
  onSelect: (node: PluginFileNode) => void
}) {
  const { t } = useTranslation()
  const isDirectory = node.nodeType === 'directory'
  const expandable = isDirectory && node.omittedCount === null && Boolean(node.children?.length)
  const [expanded, setExpanded] = useState(depth < 2 && expandable)
  const selected = selectedPath === node.path
  const selectedDescendant = Boolean(
    selectedPath
    && isDirectory
    && node.children?.some((child) => nodeContainsPath(child, selectedPath)),
  )
  const activate = () => {
    onSelect(node)
    if (expandable) setExpanded((current) => !current)
  }
  return (
    <div className={`sm2__filetree-node${isDirectory ? ' sm2__filetree-node--dir' : ' sm2__filetree-node--file'}`}>
      <button
        type="button"
        className={[
          'sm2__filetree-row',
          selected ? 'sm2__filetree-row--active' : '',
          selectedDescendant ? 'sm2__filetree-row--branch' : '',
        ].filter(Boolean).join(' ')}
        style={{ paddingLeft: 10 + depth * 14 }}
        aria-expanded={expandable ? expanded : undefined}
        onClick={activate}
      >
        <span className="sm2__filetree-twist">
          {expandable ? (expanded ? '▾' : '▸') : ''}
        </span>
        <span className={`sm2__filetree-kind sm2__plugin-tree-kind--${node.nodeType}`} />
        <span className="sm2__filetree-name">{node.name}</span>
        {node.omittedCount !== null && (
          <span className="sm2__plugin-tree-count">
            {t('skills.pluginManagement.collapsedCount', { count: node.omittedCount })}
          </span>
        )}
      </button>
      {expandable && expanded && node.children && (
        <div>
          {node.children.map((child) => (
            <PluginTreeNode
              key={`${child.path}:${child.nodeType}`}
              node={child}
              depth={depth + 1}
              selectedPath={selectedPath}
              onSelect={onSelect}
            />
          ))}
        </div>
      )}
    </div>
  )
}

function PluginMeta({
  label,
  value,
  code = false,
  wide = false,
}: {
  label: string
  value: string
  code?: boolean
  wide?: boolean
}) {
  return (
    <div className={wide ? 'sm2__plugin-drawer-meta-item sm2__plugin-drawer-meta-item--wide' : 'sm2__plugin-drawer-meta-item'}>
      <dt>{label}</dt>
      <dd title={value}>{code ? <code>{value}</code> : value}</dd>
    </div>
  )
}

function summarizePluginComponents(
  root: PluginFileNode,
  t: ReturnType<typeof useTranslation>['t'],
) {
  const componentNames: Record<string, { id: string; label: string }> = {
    skills: { id: 'skills', label: t('skills.pluginManagement.components.skills') },
    commands: { id: 'commands', label: t('skills.pluginManagement.components.commands') },
    agents: { id: 'agents', label: t('skills.pluginManagement.components.agents') },
    hooks: { id: 'hooks', label: t('skills.pluginManagement.components.hooks') },
    scripts: { id: 'runtime', label: t('skills.pluginManagement.components.runtime') },
    docs: { id: 'docs', label: t('skills.pluginManagement.components.docs') },
    assets: { id: 'assets', label: t('skills.pluginManagement.components.assets') },
    '.mcp.json': { id: 'mcp', label: t('skills.pluginManagement.components.mcp') },
  }
  return (root.children || [])
    .map((node) => {
      const component = componentNames[node.name.toLowerCase()]
      if (!component) return null
      return {
        ...component,
        count: node.nodeType === 'file'
          ? 1
          : node.omittedCount ?? node.children?.length ?? 0,
      }
    })
    .filter((component): component is { id: string; label: string; count: number } => Boolean(component))
}

function componentGlyph(id: string) {
  return ({
    skills: 'S',
    commands: '›_',
    agents: 'A',
    hooks: '↻',
    runtime: '{}',
    docs: 'D',
    assets: '◇',
    mcp: 'M',
  } as Record<string, string>)[id] || '·'
}

function nodeContainsPath(node: PluginFileNode, path: string): boolean {
  if (node.path === path) return true
  return node.children?.some((child) => nodeContainsPath(child, path)) || false
}

function pluginInitials(value: string) {
  return value
    .split(/[-_\s]+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0])
    .join('')
    .toUpperCase()
    .slice(0, 2) || 'PL'
}

function formatFileSize(size: number) {
  if (size < 1024) return `${size} B`
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(size < 10 * 1024 ? 1 : 0)} KB`
  return `${(size / (1024 * 1024)).toFixed(1)} MB`
}
