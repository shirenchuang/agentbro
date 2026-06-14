import { useEffect, useMemo, useState } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import { open as openShell } from '@tauri-apps/plugin-shell'
import { skillApiV2 } from '../../services/skillApiV2'
import { skillApi } from '../../services/skillApi'
import type { AddCenterSkillPreview, AddCenterSkillDecision, AgentSkillInventoryAgent, AgentSkillInventoryItem } from '../../services/skillApiV2'
import type { MarketplaceSkill, SkillRegistry } from '../../services/skillApi'
import { AgentIconBadge } from './AgentIconBadge'

type Tab = 'market' | 'agent' | 'local' | 'git'
type MarketBoard = 'alltime' | 'trending' | 'hot'

const MARKET_CACHE_TTL_MS = 5 * 60 * 1000
const marketCache = new Map<string, { timestamp: number; data: MarketplaceSkill[] }>()

export function InstallView({ onBack, onDone }: { onBack: () => void; onDone: () => void }) {
  const [tab, setTab] = useState<Tab>('market')
  const [gitUrl, setGitUrl] = useState('')

  const installFromSource = (source?: string) => {
    if (source) setGitUrl(source)
    setTab('git')
  }

  return (
    <div className="sm2__install">
      <div className="sm2__install-header">
        <button className="sm2__btn sm2__btn--ghost" onClick={onBack}>← 返回 Skill 库</button>
        <h2 className="sm2__title">添加到中心库</h2>
      </div>

      <div className="sm2__install-tabs">
        <button className={`sm2__addtab${tab === 'market' ? ' sm2__addtab--active' : ''}`} onClick={() => setTab('market')}>
          浏览市场
        </button>
        <button className={`sm2__addtab${tab === 'agent' ? ' sm2__addtab--active' : ''}`} onClick={() => setTab('agent')}>
          本地 Agent 同步
        </button>
        <button className={`sm2__addtab${tab === 'local' ? ' sm2__addtab--active' : ''}`} onClick={() => setTab('local')}>
          本地安装
        </button>
        <button className={`sm2__addtab${tab === 'git' ? ' sm2__addtab--active' : ''}`} onClick={() => setTab('git')}>
          Git 安装
        </button>
      </div>

      <div className="sm2__install-body settings-scroll">
        {tab === 'market' && <MarketPanel onInstall={installFromSource} onDone={onDone} />}
        {tab === 'agent' && <AgentSyncPanel onDone={onDone} />}
        {tab === 'local' && <LocalPanel onDone={onDone} />}
        {tab === 'git' && <GitPanel initialUrl={gitUrl} onDone={onDone} />}
      </div>
    </div>
  )
}

// ── Marketplace ──────────────────────────────────────────────────

function MarketPanel({ onInstall, onDone }: { onInstall: (source?: string) => void; onDone: () => void }) {
  const [items, setItems] = useState<MarketplaceSkill[]>([])
  const [registries, setRegistries] = useState<SkillRegistry[]>([])
  const [registryId, setRegistryId] = useState('skills-sh')
  const [board, setBoard] = useState<MarketBoard>('alltime')
  const [sourceFilter, setSourceFilter] = useState('')
  const [query, setQuery] = useState('')
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [installing, setInstalling] = useState<string | null>(null)
  const [status, setStatus] = useState<string | null>(null)
  const isSkillsSh = ['skills-sh', 'skills.sh', 'skillssh'].includes(registryId)
  const boardTabs: Array<{ id: MarketBoard; label: string }> = [
    { id: 'alltime', label: '全部' },
    { id: 'trending', label: '趋势' },
    { id: 'hot', label: '热门' },
  ]

  useEffect(() => {
    skillApi.listRegistries().then(setRegistries).catch(() => setRegistries([]))
  }, [])

  useEffect(() => {
    setSourceFilter('')
  }, [registryId, board, query])

  useEffect(() => {
    let alive = true
    const timer = window.setTimeout(() => {
      const cacheKey = `${registryId || 'all'}|${board}|${query.trim().toLowerCase()}`
      const cached = marketCache.get(cacheKey)
      if (cached && Date.now() - cached.timestamp < MARKET_CACHE_TTL_MS) {
        setItems(cached.data)
        setError(null)
        setLoading(false)
        return
      }

      setLoading(true)
      setError(null)
      const request = skillApi.searchMarketplaceSkills(registryId || null, query.trim() || null, board)
      const timeout = new Promise<MarketplaceSkill[]>((_, reject) => {
        window.setTimeout(() => reject(new Error('市场加载超时，请稍后重试，或先使用本地 Agent 同步 / Git 安装。')), 18_000)
      })
      Promise.race([request, timeout])
        .then((next) => {
          if (!alive) return
          marketCache.set(cacheKey, { timestamp: Date.now(), data: next })
          setItems(next)
        })
        .catch((e) => {
          if (alive) setError(String(e))
        })
        .finally(() => {
          if (alive) setLoading(false)
        })
    }, 320)
    return () => {
      alive = false
      window.clearTimeout(timer)
    }
  }, [query, registryId, board])

  const sourceOptions = useMemo(() => {
    const counts = new Map<string, number>()
    for (const item of items) {
      const source = item.source || item.registryId
      counts.set(source, (counts.get(source) || 0) + 1)
    }
    return Array.from(counts.entries())
      .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
      .slice(0, 8)
  }, [items])

  const visibleItems = useMemo(
    () => sourceFilter ? items.filter((item) => (item.source || item.registryId) === sourceFilter) : items,
    [items, sourceFilter],
  )

  const install = async (it: MarketplaceSkill) => {
    setInstalling(it.id)
    setError(null)
    setStatus(null)
    const sourceType = it.registryId === 'skills-sh' ? 'skillssh' : it.downloadUrl.startsWith('github:') ? 'github' : 'url'
    const input = {
      sourcePath: it.downloadUrl,
      sourceType,
      sourceUri: it.downloadUrl,
      multi: false,
    }
    try {
      const preview = await skillApiV2.previewAddCenterSkill(input)
      if (preview.blockers.length > 0) {
        setError(`「${it.name}」与中心库已有 Skill 冲突，请转到 Git 安装确认覆盖/重命名。`)
        onInstall(it.downloadUrl)
        return
      }
      await skillApiV2.executeAddCenterSkill(input, [])
      setStatus(`已安装「${it.name}」到中心 Skill 库`)
      onDone()
    } catch (e) {
      setError(`${String(e)}。你也可以转到 Git 安装手动预览。`)
    } finally {
      setInstalling(null)
    }
  }

  return (
    <div className="sm2__install-market">
      <div className="sm2__market-boardbar">
        <div className="sm2__view-toggle sm2__market-boardtabs">
          {boardTabs.map((tab) => (
            <button
              key={tab.id}
              className={board === tab.id ? 'active' : ''}
              disabled={!isSkillsSh || Boolean(query.trim())}
              onClick={() => setBoard(tab.id)}
            >
              {tab.label}
            </button>
          ))}
        </div>
        <div className="sm2__market-count">
          {query.trim() ? '搜索结果' : boardTabs.find((tab) => tab.id === board)?.label || '全部'} · {visibleItems.length} 个
        </div>
      </div>
      <div className="sm2__install-searchrow sm2__market-toolbar">
        <input
          className="sm2__search"
          placeholder="搜索 skills.sh 市场，例如 code review、browser、docs"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <select className="sm2__select" value={registryId} onChange={(e) => setRegistryId(e.target.value)}>
          {(registries.length ? registries : [{ id: 'skills-sh', name: 'skills.sh' } as SkillRegistry]).map((r) => (
            <option key={r.id} value={r.id}>{r.name}</option>
          ))}
        </select>
      </div>
      {sourceOptions.length > 0 && (
        <div className="sm2__market-source-row">
          <span>来源</span>
          <button
            className={`sm2__source-chip${sourceFilter === '' ? ' sm2__source-chip--active' : ''}`}
            onClick={() => setSourceFilter('')}
          >
            全部来源
          </button>
          {sourceOptions.map(([source, count]) => (
            <button
              key={source}
              className={`sm2__source-chip${sourceFilter === source ? ' sm2__source-chip--active' : ''}`}
              onClick={() => setSourceFilter(source)}
            >
              {source} <small>{count}</small>
            </button>
          ))}
        </div>
      )}
      <div className="sm2__market-note">
        默认从 skills.sh 在线市场读取榜单；输入关键词后切换为搜索结果。安装会先进中心库，之后再分发给各个 Agent。
      </div>
      {status && <div className="sm2__notice sm2__notice--ok">{status}</div>}
      {loading ? (
        <div className="sm2__empty">加载市场…</div>
      ) : error ? (
        <div className="sm2__error" style={{ margin: 0 }}>{error}</div>
      ) : visibleItems.length === 0 ? (
        <div className="sm2__empty">
          没有匹配的技能。可换一个关键词，或使用本地 / Git 安装。
        </div>
      ) : (
        <div className="sm2__install-grid">
          {visibleItems.map((it) => (
            <div key={it.id} className="sm2__install-card">
              <div className="sm2__install-card-accent" />
              <div className="sm2__install-card-body">
                <div className="sm2__install-card-head">
                  <div className="sm2__market-skill-icon">{initials(it.name)}</div>
                  <div className="sm2__install-card-title">{it.name}</div>
                  {it.webUrl && (
                    <button className="sm2__icon-btn" title="在市场查看" onClick={() => openExternal(it.webUrl!)}>
                      ↗
                    </button>
                  )}
                </div>
                <div className="sm2__install-card-meta">
                  <span className="sm2__source-pill">{it.source || it.registryId}</span>
                  {typeof it.installCount === 'number' && (
                    <span className="sm2__install-count">↓ {formatInstallCount(it.installCount)}</span>
                  )}
                </div>
                <div className="sm2__install-card-desc">{it.description || it.downloadUrl.replace(/^github:/, '')}</div>
              </div>
              <div className="sm2__install-card-foot">
                <span className={`sm2__tag sm2__tag--${it.isInstalled ? 'ok' : 'unmanaged'}`}>
                  {it.isInstalled ? '已在中心库' : '在线'}
                </span>
                <button className="sm2__btn sm2__btn--primary" disabled={installing === it.id || it.isInstalled} onClick={() => install(it)}>
                  {installing === it.id ? '安装中…' : '安装到中心库'}
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

function openExternal(url: string) {
  if ('__TAURI_INTERNALS__' in window) {
    openShell(url).catch((err) => console.warn('[skills-market] open external:', err))
  } else {
    window.open(url, '_blank', 'noopener,noreferrer')
  }
}

function formatInstallCount(value: number) {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(value >= 10_000_000 ? 0 : 1)}M`
  if (value >= 1_000) return `${(value / 1_000).toFixed(value >= 10_000 ? 0 : 1)}K`
  return String(value)
}

function initials(value: string) {
  return value
    .split(/[-_\s]+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0])
    .join('')
    .toUpperCase()
    .slice(0, 2) || 'SK'
}

// ── Local Agent sync ─────────────────────────────────────────────

function AgentSyncPanel({ onDone }: { onDone: () => void }) {
  const [agents, setAgents] = useState<AgentSkillInventoryAgent[]>([])
  const [selectedAgent, setSelectedAgent] = useState('all')
  const [statusFilter, setStatusFilter] = useState('all')
  const [query, setQuery] = useState('')
  const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set())
  const [loading, setLoading] = useState(true)
  const [scanning, setScanning] = useState(false)
  const [importing, setImporting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)

  const load = async () => {
    setLoading(true)
    setError(null)
    try {
      const next = await skillApiV2.listAgentSkillInventory()
      setAgents(next)
      setSelectedIds((current) => {
        const valid = new Set(next.flatMap((agent) => agent.items.filter((item) => item.canImport).map((item) => importKey(item))))
        return new Set(Array.from(current).filter((id) => valid.has(id)))
      })
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const scan = async () => {
    setScanning(true)
    setError(null)
    setNotice(null)
    try {
      await skillApiV2.refresh()
      await load()
      setNotice('已重新扫描本地 Agent Skills')
    } catch (e) {
      setError(String(e))
    } finally {
      setScanning(false)
    }
  }

  const visibleAgents = useMemo(
    () => selectedAgent === 'all' ? agents : agents.filter((agent) => agent.agentId === selectedAgent),
    [agents, selectedAgent],
  )
  const q = query.trim().toLowerCase()
  const rows = useMemo(() => {
    return visibleAgents.flatMap((agent) => agent.items.map((item) => ({ agent, item })))
      .filter(({ item }) => {
        if (statusFilter === 'managed' && !item.managed) return false
        if (statusFilter === 'importable' && !item.canImport) return false
        if (statusFilter === 'unmanaged' && item.managed) return false
        if (statusFilter === 'conflict' && item.status !== 'conflict') return false
        if (!q) return true
        return [item.name, item.skillId, item.path, item.statusLabel, item.reason || '']
          .join(' ')
          .toLowerCase()
          .includes(q)
      })
  }, [visibleAgents, statusFilter, q])
  const importableRows = rows.filter(({ item }) => item.canImport)
  const totalManaged = agents.reduce((sum, agent) => sum + agent.managedCount, 0)
  const totalUnmanaged = agents.reduce((sum, agent) => sum + agent.unmanagedCount, 0)
  const totalImportable = agents.reduce((sum, agent) => sum + agent.importableCount, 0)

  const toggle = (item: AgentSkillInventoryItem) => {
    if (!item.canImport) return
    setSelectedIds((current) => {
      const next = new Set(current)
      const key = importKey(item)
      if (next.has(key)) next.delete(key)
      else next.add(key)
      return next
    })
  }

  const selectAllVisible = () => {
    setSelectedIds(new Set(importableRows.map(({ item }) => importKey(item))))
  }

  const importSelected = async () => {
    const selected = rows.filter(({ item }) => selectedIds.has(importKey(item)) && item.canImport)
    if (selected.length === 0) return
    setImporting(true)
    setError(null)
    setNotice(null)
    try {
      let ok = 0
      const failed: string[] = []
      for (const { item } of selected) {
        try {
          await skillApiV2.executeAdopt(item.agentId, item.id, 'import_keep')
          ok += 1
        } catch (e) {
          failed.push(`${item.name}: ${String(e)}`)
        }
      }
      await load()
      onDone()
      setSelectedIds(new Set())
      setNotice(`已接管 ${ok} 个 Skill${failed.length ? `，${failed.length} 个失败` : ''}`)
      if (failed.length > 0) setError(failed.slice(0, 3).join('\n'))
    } finally {
      setImporting(false)
    }
  }

  return (
    <div className="sm2__agent-sync">
      <div className="sm2__agent-sync-hero">
        <div>
          <h3>本地 Agent 同步</h3>
          <p>按 Agent 的本地 Skills 目录检查状态，只能把未管理且无冲突的 Skill 接管到中心库。</p>
        </div>
        <button className="sm2__btn" onClick={scan} disabled={scanning || importing}>
          {scanning ? '扫描中…' : '重新扫描 Agent'}
        </button>
      </div>

      <div className="sm2__agent-sync-stats">
        <MetricLite value={agents.length} label="Agent" />
        <MetricLite value={totalManaged} label="已管理" />
        <MetricLite value={totalUnmanaged} label="未管理" />
        <MetricLite value={totalImportable} label="可接管" tone={totalImportable > 0 ? 'ok' : undefined} />
      </div>

      <div className="sm2__agent-sync-toolbar">
        <input className="sm2__search" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="搜索 Skill 名称 / 路径 / 状态" />
        <select className="sm2__select" value={selectedAgent} onChange={(e) => setSelectedAgent(e.target.value)}>
          <option value="all">全部 Agent</option>
          {agents.map((agent) => (
            <option key={agent.agentId} value={agent.agentId}>{agent.displayName}</option>
          ))}
        </select>
        <select className="sm2__select" value={statusFilter} onChange={(e) => setStatusFilter(e.target.value)}>
          <option value="all">全部状态</option>
          <option value="importable">可接管</option>
          <option value="unmanaged">未管理</option>
          <option value="managed">已管理</option>
          <option value="conflict">同名冲突</option>
        </select>
      </div>

      <div className="sm2__agent-sync-actions">
        <span>已选择 {selectedIds.size} 个</span>
        <button className="sm2__btn" onClick={selectAllVisible} disabled={importableRows.length === 0 || importing}>选择当前可接管</button>
        <button className="sm2__btn" onClick={() => setSelectedIds(new Set())} disabled={selectedIds.size === 0 || importing}>清空</button>
        <button className="sm2__btn sm2__btn--primary" onClick={importSelected} disabled={selectedIds.size === 0 || importing}>
          {importing ? '接管中…' : '接管到中心库'}
        </button>
      </div>

      {notice && <div className="sm2__notice sm2__notice--ok">{notice}</div>}
      {error && <div className="sm2__error" style={{ margin: 0, whiteSpace: 'pre-wrap' }}>{error}</div>}

      {loading ? (
        <div className="sm2__empty">加载本地 Agent Skills…</div>
      ) : rows.length === 0 ? (
        <div className="sm2__empty">没有匹配的本地 Skill。可以先点击「重新扫描 Agent」。</div>
      ) : (
        <div className="sm2__agent-sync-list">
          {rows.map(({ agent, item }) => {
            const key = importKey(item)
            return (
              <div key={key} className={`sm2__agent-sync-row sm2__agent-sync-row--${item.status}`}>
                <label className="sm2__agent-sync-check">
                  <input
                    type="checkbox"
                    checked={selectedIds.has(key)}
                    disabled={!item.canImport || importing}
                    onChange={() => toggle(item)}
                  />
                </label>
                <AgentIconBadge iconKey={agent.iconKey} size={30} />
                <div className="sm2__agent-sync-main">
                  <div className="sm2__agent-sync-titleline">
                    <strong>{item.name}</strong>
                    <span>{agent.displayName}</span>
                  </div>
                  <code>{item.path}</code>
                  {item.reason && <small>{item.reason}</small>}
                </div>
                <span className={`sm2__tag sm2__tag--${item.status === 'conflict' ? 'conflict' : item.managed ? 'ok' : 'unmanaged'}`}>
                  {item.statusLabel}
                </span>
                {item.actualMode && <span className="sm2__tag">{item.actualMode}</span>}
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}

function importKey(item: AgentSkillInventoryItem) {
  return `${item.agentId}:${item.id}`
}

function MetricLite({ value, label, tone }: { value: number; label: string; tone?: 'ok' }) {
  return (
    <div className={`sm2__agent-sync-stat${tone ? ` sm2__agent-sync-stat--${tone}` : ''}`}>
      <strong>{value}</strong>
      <span>{label}</span>
    </div>
  )
}

// ── Local ────────────────────────────────────────────────────────

function LocalPanel({ onDone }: { onDone: () => void }) {
  const [sourcePath, setSourcePath] = useState('')
  const [multi, setMulti] = useState(false)
  const [preview, setPreview] = useState<AddCenterSkillPreview | null>(null)
  const [renames, setRenames] = useState<Record<string, string>>({})
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const chooseFolder = async () => {
    const dir = await open({ directory: true, multiple: false })
    if (typeof dir === 'string') setSourcePath(dir)
  }
  const chooseZip = async () => {
    const f = await open({ filters: [{ name: '压缩包', extensions: ['zip'] }], multiple: false })
    if (typeof f === 'string') setSourcePath(f)
  }

  const sourceType = sourcePath.trim().toLowerCase().endsWith('.zip') ? 'archive' : 'local_folder'

  const runPreview = async () => {
    if (!sourcePath) {
      setError('请先选择来源')
      return
    }
    setBusy(true)
    setError(null)
    try {
      const p = await skillApiV2.previewAddCenterSkill({ sourcePath, sourceType, multi })
      setPreview(p)
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  const execute = async () => {
    if (!preview) return
    setBusy(true)
    setError(null)
    try {
      const decisions: AddCenterSkillDecision[] = preview.blockers.map((b) => {
        const renamed = renames[b.skillId]?.trim()
        return renamed
          ? { skillId: b.skillId, proposedSkillId: renamed, resolution: 'create' }
          : { skillId: b.skillId, resolution: 'skip' }
      })
      const r = await skillApiV2.executeAddCenterSkill({ sourcePath, sourceType, multi }, decisions)
      alert(`导入完成：新增 ${r.skillIds.length}，更新 ${r.updated.length}，跳过 ${r.skipped.length}`)
      onDone()
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  if (preview) {
    return (
      <div className="sm2__install-form">
        <h3 className="sm2__install-h">确认导入预览</h3>
        {preview.candidates.map((c) => (
          <div key={c.skillId} className="sm2__change">
            <strong>{c.name}</strong> → <code>{c.proposedSkillId}</code>{' '}
            <span className={`sm2__tag sm2__tag--${c.action === 'update' ? 'ok' : 'unmanaged'}`}>
              {c.action === 'update' ? '更新' : '新增'}
            </span>
          </div>
        ))}
        {preview.blockers.map((b) => (
          <div key={b.skillId} className="sm2__change sm2__change--blocked">
            <strong>{b.skillId}</strong>：{b.reason}
            <div className="sm2__field" style={{ marginTop: 6 }}>
              <label>重命名为（留空则跳过）</label>
              <input value={renames[b.skillId] || ''} onChange={(e) => setRenames({ ...renames, [b.skillId]: e.target.value })} placeholder={`${b.skillId}-rename`} />
            </div>
          </div>
        ))}
        {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
        <div className="sm2__btn-row">
          <button className="sm2__btn" onClick={() => setPreview(null)} disabled={busy}>返回</button>
          <button className="sm2__btn sm2__btn--primary" onClick={execute} disabled={busy}>{busy ? '处理中…' : '执行导入'}</button>
        </div>
      </div>
    )
  }

  return (
    <div className="sm2__install-form">
      <h3 className="sm2__install-h">从本地导入</h3>
      <p className="sm2__install-sub">支持文件夹、压缩包,以及批量导入一个含多个 Skill 的目录。</p>

      <div className="sm2__install-options">
        <button className="sm2__install-option" onClick={chooseFolder}>
          <span className="sm2__install-option-icon">📁</span>
          <span className="sm2__install-option-label">选择文件夹</span>
        </button>
        <button className="sm2__install-option" onClick={chooseZip}>
          <span className="sm2__install-option-icon">🗜️</span>
          <span className="sm2__install-option-label">选择压缩包 (.zip)</span>
        </button>
      </div>

      <div className="sm2__field">
        <label>来源路径</label>
        <input value={sourcePath} onChange={(e) => setSourcePath(e.target.value)} placeholder="选择或粘贴包含 SKILL.md 的目录 / .zip" />
      </div>
      <label className="sm2__checkbox-row">
        <input type="checkbox" checked={multi} onChange={(e) => setMulti(e.target.checked)} />
        批量导入（该目录包含多个 Skill）
      </label>

      {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
      <p style={{ fontSize: 12, color: 'var(--text-secondary)' }}>
        每个 Skill 目录必须包含 SKILL.md。同名不同来源会被阻止并要求选择处理方式。
      </p>
      <div className="sm2__btn-row">
        <button className="sm2__btn sm2__btn--primary" onClick={runPreview} disabled={busy || !sourcePath}>
          {busy ? '处理中…' : '预览导入'}
        </button>
      </div>
    </div>
  )
}

// ── Git ──────────────────────────────────────────────────────────

function GitPanel({ initialUrl, onDone }: { initialUrl?: string; onDone: () => void }) {
  const [url, setUrl] = useState(initialUrl || '')
  const [branch, setBranch] = useState('')
  const [token, setToken] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [status, setStatus] = useState<string | null>(null)

  const run = async () => {
    if (!url.trim()) {
      setError('请输入 Git 仓库 URL')
      return
    }
    setBusy(true)
    setError(null)
    setStatus(null)
    try {
      const repo = await skillApi.previewGitHubRepoImport(url.trim())
      const selections = repo.skills.map((s) => ({ sourcePath: s.sourcePath, resolution: 'overwrite' as const }))
      if (selections.length === 0) {
        setError('该仓库未检测到任何 Skill（需含 SKILL.md）')
        return
      }
      await skillApi.importGitHubRepoSkills(url.trim(), selections)
      await skillApiV2.refresh()
      setStatus(`已导入 ${selections.length} 个 Skill 到中心库`)
      onDone()
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="sm2__install-form">
      <h3 className="sm2__install-h">从 Git 仓库克隆并导入</h3>
      <p className="sm2__install-sub">输入 GitHub / Git 仓库地址,自动检测并导入其中的 Skill。</p>
      <div className="sm2__field">
        <label>仓库 URL</label>
        <input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://github.com/username/repo" />
      </div>
      <div className="sm2__field">
        <label>分支（可选）</label>
        <input value={branch} onChange={(e) => setBranch(e.target.value)} placeholder="main" />
      </div>
      <div className="sm2__field">
        <label>访问令牌（私有仓库可选）</label>
        <input value={token} onChange={(e) => setToken(e.target.value)} type="password" placeholder="ghp_..." />
      </div>
      {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
      {status && <div className="sm2__change" style={{ background: 'rgba(52,199,89,0.12)' }}>{status}</div>}
      <div className="sm2__btn-row">
        <button className="sm2__btn sm2__btn--primary" onClick={run} disabled={busy}>
          {busy ? '克隆中…' : '安装'}
        </button>
      </div>
    </div>
  )
}
