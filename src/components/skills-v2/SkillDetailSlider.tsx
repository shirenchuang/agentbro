import { useEffect, useState } from 'react'
import { skillApiV2 } from '../../services/skillApiV2'
import { skillApi } from '../../services/skillApi'
import type { SkillDetail, SkillSummary, FileTreeNode } from '../../services/skillApiV2'
import { SlideOver } from './SlideOver'
import { AgentIconBadge } from './AgentIconBadge'

const STATUS_LABEL: Record<string, string> = {
  ok: '正常',
  conflict: '冲突',
  copy_outdated: '可更新',
  copy_modified: '已修改',
  copy_diverged: '已分叉',
  broken_link: '坏链接',
  missing: '失效',
}

export function SkillDetailSlider({
  skillId,
  open,
  onClose,
  onDistribute,
  onDelete,
}: {
  skillId: string | null
  open: boolean
  onClose: () => void
  onDistribute: (s: SkillSummary) => void
  onDelete: (id: string) => void
}) {
  const [detail, setDetail] = useState<SkillDetail | null>(null)
  const [loading, setLoading] = useState(false)
  const [activeFile, setActiveFile] = useState<string | null>(null)
  const [fileContent, setFileContent] = useState('')
  const [fileLoading, setFileLoading] = useState(false)
  const [syncing, setSyncing] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!open || !skillId) return
    setLoading(true)
    setError(null)
    skillApiV2
      .getSkillDetail(skillId)
      .then((d) => {
        setDetail(d)
        // default to SKILL.md preview
        const skillMd = findFile(d.files, 'SKILL.md')
        if (skillMd) loadFile(skillMd)
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, skillId])

  const loadFile = async (path: string) => {
    setActiveFile(path)
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

  return (
    <SlideOver
      open={open}
      onClose={onClose}
      width={720}
      title={summary?.name || skillId || ''}
      subtitle={summary?.description}
      actions={
        summary && (
          <>
            <button className="sm2__btn sm2__btn--primary" onClick={() => onDistribute(summary)}>
              分发
            </button>
            <button className="sm2__btn sm2__btn--ghost" onClick={() => skillApiV2.openPath(summary.centerPath)}>
              打开目录
            </button>
            <button className="sm2__btn sm2__btn--danger" onClick={() => onDelete(summary.id)}>
              删除
            </button>
          </>
        )
      }
    >
      {loading && <div className="sm2__empty">加载中…</div>}
      {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
      {summary && (
        <>
          <div className="sm2__detail-meta" style={{ marginBottom: 12 }}>
            <span className={`sm2__tag sm2__tag--${summary.status}`}>{STATUS_LABEL[summary.status] || summary.status}</span>{' '}
            <span className="sm2__tag">{summary.sourceType}</span>{' '}
            <span className="sm2__agents">
              {summary.installedAgents.map((a) => (
                <AgentIconBadge key={a.agentId} iconKey={a.iconKey} mode={a.mode} title={`${a.displayName} · ${a.mode}`} />
              ))}
            </span>
          </div>

          {/* File browser */}
          <div className="sm2__detail-section">
            <h4>目录与文件</h4>
            <div className="sm2__filebrowser">
              <div className="sm2__filetree settings-scroll">
                {detail?.files ? (
                  <FileTree node={detail.files} depth={0} active={activeFile} onSelect={loadFile} />
                ) : (
                  <div className="sm2__empty" style={{ padding: 12 }}>无文件</div>
                )}
              </div>
              <div className="sm2__fileview settings-scroll">
                <div className="sm2__fileview-path">{activeFile || '选择左侧文件查看内容'}</div>
                <pre className="sm2__fileview-content">
                  {fileLoading ? '加载中…' : fileContent || '（空）'}
                </pre>
              </div>
            </div>
          </div>

          {/* Installed targets */}
          <div className="sm2__detail-section">
            <h4>已安装 Agent（{detail?.targets.length || 0}）</h4>
            {detail?.targets.length === 0 ? (
              <div className="sm2__empty" style={{ padding: 8 }}>尚未分发到任何 Agent</div>
            ) : (
              detail?.targets.map((t) => (
                <div key={t.id} className="sm2__target-card">
                  <div className="sm2__target-card-head">
                    <AgentIconBadge iconKey={t.agentId} mode={t.actualMode as 'link' | 'copy'} size={24} />
                    <div className="sm2__target-card-info">
                      <strong>{t.agentId}</strong>
                      <span className={`sm2__tag sm2__tag--${t.status}`}>{STATUS_LABEL[t.status] || t.status}</span>
                      <span className="sm2__tag">{t.actualMode}</span>
                    </div>
                    <div className="sm2__claims">
                      {t.claims.map((c) => (
                        <span key={c.id} className="sm2__tag">
                          {c.claimType === 'pack' ? `pack:${c.packName}` : 'direct'}
                        </span>
                      ))}
                    </div>
                  </div>
                  <div className="sm2__detail-meta">{t.targetPath}</div>
                  {t.actualMode === 'copy' && t.status !== 'ok' && (
                    <div className="sm2__btn-row">
                      {t.status === 'copy_outdated' && (
                        <button className="sm2__btn" disabled={syncing === t.id} onClick={() => doSync(t.id, 'center_over_agent')}>更新副本</button>
                      )}
                      {t.status === 'copy_modified' && (
                        <button className="sm2__btn" disabled={syncing === t.id} onClick={() => doSync(t.id, 'agent_over_center')}>推送到中心库</button>
                      )}
                      {t.status === 'copy_diverged' && (
                        <>
                          <button className="sm2__btn" disabled={syncing === t.id} onClick={() => doSync(t.id, 'center_over_agent')}>用中心库覆盖</button>
                          <button className="sm2__btn" disabled={syncing === t.id} onClick={() => doSync(t.id, 'agent_over_center')}>用副本覆盖</button>
                          <button className="sm2__btn" disabled={syncing === t.id} onClick={() => doSync(t.id, 'keep_diverged')}>保留分叉</button>
                        </>
                      )}
                    </div>
                  )}
                </div>
              ))
            )}
          </div>

          {/* Source */}
          {detail?.source && (
            <div className="sm2__detail-section">
              <h4>来源</h4>
              <div className="sm2__detail-meta">
                <div>类型：{detail.source.sourceType}</div>
                {detail.source.importedFromAgent && <div>来自 Agent：{detail.source.importedFromAgent}</div>}
                {detail.source.importedFromPath && <div>原路径：{detail.source.importedFromPath}</div>}
                <div>Hash：{summary.currentHash.slice(0, 16)}…</div>
              </div>
            </div>
          )}
          <div style={{ height: 24 }} />
        </>
      )}
    </SlideOver>
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
  return (
    <div>
      <div
        className={`sm2__filetree-row${active === node.path ? ' sm2__filetree-row--active' : ''}`}
        style={{ paddingLeft: 8 + depth * 12 }}
        onClick={() => (isDir ? setExpanded(!expanded) : onSelect(node.path))}
      >
        <span className="sm2__filetree-icon">{isDir ? (expanded ? '▾' : '▸') : '📄'}</span>
        <span className="sm2__filetree-name">{node.name}</span>
      </div>
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
