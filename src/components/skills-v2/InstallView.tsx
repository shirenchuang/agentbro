import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import { open as openShell } from '@tauri-apps/plugin-shell'
import { skillApiV2 } from '../../services/skillApiV2'
import type { AddCenterSkillPreview, AddCenterSkillDecision, AgentSkillInventoryAgent, AgentSkillInventoryItem } from '../../services/skillApiV2'
import type { MarketplaceSkill, MarketplaceSkillDetail, GitHubRepoPreview } from '../../services/skillApiV2'
import { AgentIconBadge } from './AgentIconBadge'
import { SlideOver } from './SlideOver'
import { useSkillStoreV2 } from '../../stores/skillStoreV2'
import { isMarketItemInstalled, marketSkillId } from './marketInstallState'

type Tab = 'market' | 'agent' | 'local' | 'git'
type MarketBoard = 'alltime' | 'trending' | 'hot'
type InstallDoneHandler = (skillId?: string) => void | Promise<void>

const MARKET_CACHE_TTL_MS = 5 * 60 * 1000
const marketCache = new Map<string, { timestamp: number; data: MarketplaceSkill[] }>()

export function InstallView({ onBack, onDone }: { onBack: () => void; onDone: InstallDoneHandler }) {
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

const MARKET_PAGE_SIZE = 24
const DEFAULT_RECOMMENDED_SOURCE_COUNT = 4
const RECOMMENDED_PUBLISHERS = [
  { id: 'anthropics', label: 'Anthropic' },
  { id: 'microsoft', label: 'Microsoft' },
  { id: 'openai', label: 'OpenAI' },
  { id: 'vercel-labs', label: 'Vercel' },
  { id: 'firebase', label: 'Firebase' },
  { id: 'supabase', label: 'Supabase' },
  { id: 'larksuite', label: 'LarkSuite' },
]
const RECOMMENDED_SOURCES = [
  'anthropics/claude-plugins-official',
  'anthropics/skills',
  'microsoft/azure-skills',
  'larksuite/cli',
  'vercel-labs/skills',
  'vercel-labs/agent-skills',
  'runcomfy-com/skills',
  'agentspace-so/runcomfy-agent-skills',
  'doany-ai/skills',
  'firebase/agent-skills',
  'supabase/agent-skills',
  'qu-skills/skills',
]

function SkillAvatar({ source, name }: { source: string | null; name: string }) {
  const [failed, setFailed] = useState(false)
  const owner = (source || '').split('/')[0]
  const avatarUrl = owner ? `https://github.com/${owner}.png?size=40` : ''

  if (!avatarUrl || failed) {
    return <div className="sm2__market-skill-icon">{initials(name)}</div>
  }
  return (
    <img
      src={avatarUrl}
      alt={owner}
      className="sm2__market-avatar"
      loading="lazy"
      onError={() => setFailed(true)}
    />
  )
}

type MarketViewMode = 'list' | 'cards'
type LocalPreviewViewMode = 'list' | 'cards'

export function MarketPanel({ onInstall, onDone }: { onInstall: (source?: string) => void; onDone: InstallDoneHandler }) {
  const [items, setItems] = useState<MarketplaceSkill[]>([])
  const [sourceItemsBySource, setSourceItemsBySource] = useState<Record<string, MarketplaceSkill[]>>({})
  const [board, setBoard] = useState<MarketBoard>('alltime')
  const [publisherFilter, setPublisherFilter] = useState('')
  const [sourceFilter, setSourceFilter] = useState('')
  const [query, setQuery] = useState('')
  const [page, setPage] = useState(1)
  const [loading, setLoading] = useState(true)
  const [sourceLoading, setSourceLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [sourceError, setSourceError] = useState<string | null>(null)
  const [installing, setInstalling] = useState<Set<string>>(() => new Set())
  const [installedIds, setInstalledIds] = useState<Set<string>>(() => new Set())
  const [status, setStatus] = useState<string | null>(null)
  const [viewMode, setViewMode] = useState<MarketViewMode>('cards')
  const [detailSkill, setDetailSkill] = useState<MarketplaceSkill | null>(null)
  const [sourcesExpanded, setSourcesExpanded] = useState(false)
  const gridRef = useRef<HTMLDivElement>(null)
  const centerSkills = useSkillStoreV2((s) => s.skills)
  const registryId = 'skills-sh'
  const isSkillsSh = ['skills-sh', 'skills.sh', 'skillssh'].includes(registryId)
  const boardTabs: Array<{ id: MarketBoard; label: string }> = [
    { id: 'alltime', label: '全部' },
    { id: 'trending', label: '趋势' },
    { id: 'hot', label: '热门' },
  ]

  useEffect(() => {
    setPublisherFilter('')
    setSourceFilter('')
    setSourcesExpanded(false)
    setPage(1)
  }, [registryId, board, query])

  useEffect(() => {
    setSourceFilter('')
    setPage(1)
  }, [publisherFilter])

  useEffect(() => {
    setPage(1)
  }, [sourceFilter])

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
      const request = skillApiV2.searchMarketplaceSkills(registryId || null, query.trim() || null, board)
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

  useEffect(() => {
    if (!sourceFilter) {
      setSourceLoading(false)
      setSourceError(null)
      return
    }
    if (Object.prototype.hasOwnProperty.call(sourceItemsBySource, sourceFilter)) {
      setSourceError(null)
      return
    }

    let alive = true
    setSourceLoading(true)
    setSourceError(null)
    skillApiV2.searchMarketplaceSkills(registryId, sourceFilter, null)
      .then((next) => {
        if (!alive) return
        const exactSourceItems = next.filter((item) => (item.source || item.registryId) === sourceFilter)
        setSourceItemsBySource((prev) => ({ ...prev, [sourceFilter]: exactSourceItems }))
      })
      .catch((e) => {
        if (alive) setSourceError(`加载「${sourceFilter}」失败：${String(e)}`)
      })
      .finally(() => {
        if (alive) setSourceLoading(false)
      })

    return () => {
      alive = false
    }
  }, [registryId, sourceFilter, sourceItemsBySource])

  const recommendedPublishers = useMemo(() => {
    const counts = new Map<string, { skills: number; sources: Set<string> }>()
    for (const item of items) {
      const source = item.source || item.registryId
      const publisher = publisherFromSource(source)
      const current = counts.get(publisher) || { skills: 0, sources: new Set<string>() }
      current.skills += 1
      current.sources.add(source)
      counts.set(publisher, current)
    }
    const options = RECOMMENDED_PUBLISHERS
      .map((entry) => {
        const value = counts.get(entry.id)
        if (!value) return null
        return {
          publisher: entry.id,
          label: entry.label,
          skillCount: value.skills,
          sourceCount: value.sources.size,
        }
      })
      .filter(Boolean) as Array<{ publisher: string; label: string; skillCount: number; sourceCount: number }>
    if (publisherFilter && !options.some((option) => option.publisher === publisherFilter)) {
      const value = counts.get(publisherFilter)
      if (value) {
        options.push({
          publisher: publisherFilter,
          label: publisherFilter,
          skillCount: value.skills,
          sourceCount: value.sources.size,
        })
      }
    }
    return options
  }, [items, publisherFilter])

  const recommendedSources = useMemo(() => {
    const counts = new Map<string, number>()
    for (const item of items) {
      const source = item.source || item.registryId
      counts.set(source, (counts.get(source) || 0) + 1)
    }
    const options = RECOMMENDED_SOURCES.map((source) => ({
      source,
      count: sourceItemsBySource[source]?.length ?? counts.get(source) ?? 0,
    }))
    if (sourceFilter && !options.some((option) => option.source === sourceFilter)) {
      const count = sourceItemsBySource[sourceFilter]?.length ?? counts.get(sourceFilter)
      if (count) {
        options.push({ source: sourceFilter, count })
      }
    }
    return options
  }, [items, sourceFilter, sourceItemsBySource])

  const visibleRecommendedSources = useMemo(() => {
    if (sourcesExpanded) return recommendedSources
    const collapsed = recommendedSources.slice(0, DEFAULT_RECOMMENDED_SOURCE_COUNT)
    if (sourceFilter && !collapsed.some((option) => option.source === sourceFilter)) {
      const selected = recommendedSources.find((option) => option.source === sourceFilter)
      if (selected) return [...collapsed, selected]
    }
    return collapsed
  }, [recommendedSources, sourceFilter, sourcesExpanded])

  const sourceScopeLoaded = sourceFilter
    ? Object.prototype.hasOwnProperty.call(sourceItemsBySource, sourceFilter)
    : false
  const sourceScopedItems = sourceFilter && sourceScopeLoaded ? sourceItemsBySource[sourceFilter] : null
  const marketItems = sourceScopedItems || items
  const visibleItems = useMemo(
    () => marketItems.filter((item) => {
      const source = item.source || item.registryId
      if (sourceFilter) return source === sourceFilter
      if (publisherFilter) return publisherFromSource(source) === publisherFilter
      return true
    }),
    [marketItems, publisherFilter, sourceFilter],
  )

  const publisherMarketCards = useMemo(() => {
    if (!publisherFilter || sourceFilter) return []
    const sourceOrder = new Map(RECOMMENDED_SOURCES.map((source, index) => [source, index]))
    const grouped = new Map<string, { source: string; skillCount: number; installCount: number; topSkills: string[] }>()
    const scopedSources = new Set(Object.keys(sourceItemsBySource))
    for (const source of RECOMMENDED_SOURCES) {
      if (publisherFromSource(source) !== publisherFilter) continue
      const scopedItems = sourceItemsBySource[source] || []
      grouped.set(source, {
        source,
        skillCount: scopedItems.length,
        installCount: scopedItems.reduce((sum, item) => sum + (item.installCount || 0), 0),
        topSkills: scopedItems.slice(0, 3).map((item) => item.name),
      })
    }
    for (const item of items) {
      const source = item.source || item.registryId
      if (publisherFromSource(source) !== publisherFilter) continue
      if (scopedSources.has(source)) continue
      const current = grouped.get(source) || { source, skillCount: 0, installCount: 0, topSkills: [] }
      current.skillCount += 1
      current.installCount += item.installCount || 0
      if (current.topSkills.length < 3) current.topSkills.push(item.name)
      grouped.set(source, current)
    }
    return Array.from(grouped.values()).sort((a, b) => {
      const aOrder = sourceOrder.get(a.source) ?? Number.MAX_SAFE_INTEGER
      const bOrder = sourceOrder.get(b.source) ?? Number.MAX_SAFE_INTEGER
      return aOrder - bOrder || b.skillCount - a.skillCount || a.source.localeCompare(b.source)
    })
  }, [items, publisherFilter, sourceFilter, sourceItemsBySource])

  const browsingPublisherMarkets = Boolean(publisherFilter && !sourceFilter)
  const selectedPublisherLabel =
    recommendedPublishers.find((option) => option.publisher === publisherFilter)?.label || publisherFilter
  const viewLoading = loading || Boolean(sourceFilter && sourceLoading && !sourceScopeLoaded)
  const viewError = error || sourceError

  const selectPublisher = useCallback((publisher: string) => {
    setPublisherFilter(publisher)
    setSourceFilter('')
  }, [])

  const selectSource = useCallback((source: string) => {
    setPublisherFilter(publisherFromSource(source))
    setSourceFilter(source)
  }, [])

  const clearMarketScope = useCallback(() => {
    setPublisherFilter('')
    setSourceFilter('')
  }, [])

  const togglePublisher = useCallback((publisher: string) => {
    if (publisherFilter === publisher && !sourceFilter) {
      clearMarketScope()
      return
    }
    selectPublisher(publisher)
  }, [clearMarketScope, publisherFilter, selectPublisher, sourceFilter])

  const toggleSource = useCallback((source: string) => {
    if (sourceFilter === source) {
      setSourceFilter('')
      return
    }
    selectSource(source)
  }, [selectSource, sourceFilter])

  const openPublisherMarkets = useCallback((source: string) => {
    selectPublisher(publisherFromSource(source))
  }, [selectPublisher])

  const totalPages = Math.max(1, Math.ceil(visibleItems.length / MARKET_PAGE_SIZE))
  const currentPage = Math.min(page, totalPages)
  const paginatedItems = visibleItems.slice(
    (currentPage - 1) * MARKET_PAGE_SIZE,
    currentPage * MARKET_PAGE_SIZE,
  )

  const visiblePages = useMemo(() => {
    return Array.from({ length: totalPages }, (_, i) => i + 1).filter((p) => {
      if (totalPages <= 7) return true
      if (p === 1 || p === totalPages) return true
      return Math.abs(p - currentPage) <= 1
    })
  }, [totalPages, currentPage])

  const changePage = useCallback((p: number) => {
    setPage(p)
    gridRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }, [])

  const install = async (it: MarketplaceSkill) => {
    if (installing.has(it.id)) return
    setInstalling((prev) => new Set(prev).add(it.id))
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
      // execute_add_center_skill previews internally; calling it directly
      // avoids a second remote download. A same-name/different-source
      // conflict surfaces as an error, which we redirect to the Git tab.
      const result = await skillApiV2.executeAddCenterSkill(input, [])
      const installedSkillId = result.skillIds[0] || result.updated[0] || marketSkillId(it)
      setInstalledIds((prev) => new Set([...prev, it.id]))
      setStatus(`已安装「${it.name}」到中心 Skill 库`)
      await onDone(installedSkillId)
    } catch (e) {
      const msg = String(e)
      if (/Blocked skill|requires an explicit decision|冲突/.test(msg)) {
        setError(`「${it.name}」与中心库已有 Skill 冲突，请转到 Git 安装确认覆盖/重命名。`)
        onInstall(it.downloadUrl)
      } else {
        setError(`${msg}。你也可以转到 Git 安装手动预览。`)
      }
    } finally {
      setInstalling((prev) => {
        const next = new Set(prev)
        next.delete(it.id)
        return next
      })
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
        <div className="sm2__search-wrapper">
          <span className="sm2__search-icon">⌕</span>
          <input
            className="sm2__search sm2__search--with-icon"
            placeholder="搜索 skills.sh 市场…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <div className="sm2__view-toggle sm2__view-toggle--soft">
          <button className={viewMode === 'list' ? 'active' : ''} onClick={() => setViewMode('list')}>列表</button>
          <button className={viewMode === 'cards' ? 'active' : ''} onClick={() => setViewMode('cards')}>卡片</button>
        </div>
      </div>
      {(recommendedPublishers.length > 0 || recommendedSources.length > 0) && (
        <div className="sm2__market-source-row">
          <div className="sm2__source-row-section">
            <div className="sm2__source-row-head">
              <span>推荐 Skill 创建者</span>
              <small>优先看大厂和高可信发布者</small>
            </div>
            <div className="sm2__source-chip-cloud">
              {recommendedPublishers.map(({ publisher, label, skillCount, sourceCount }) => (
                <button
                  key={publisher}
                  className={`sm2__source-chip${publisherFilter === publisher ? ' sm2__source-chip--active' : ''}`}
                  onClick={() => togglePublisher(publisher)}
                  title={`${sourceCount} 个来源，${skillCount} 个 Skill`}
                >
                  <span>{label}</span>
                </button>
              ))}
              {recommendedSources.length > DEFAULT_RECOMMENDED_SOURCE_COUNT && !sourcesExpanded && !sourceFilter && (
                <button
                  className="sm2__source-toggle"
                  onClick={() => setSourcesExpanded(true)}
                >
                  展开更多
                </button>
              )}
            </div>
          </div>

          {(sourcesExpanded || sourceFilter) && (
            <div className="sm2__source-row-section sm2__source-row-section--repos">
              <div className="sm2__source-row-head">
                <span>推荐市场</span>
                <small>常见、热门、维护活跃的来源仓库</small>
              </div>
              <div className="sm2__source-chip-cloud">
                {visibleRecommendedSources.map(({ source, count }) => (
                  <button
                    key={source}
                    className={`sm2__source-chip${sourceFilter === source ? ' sm2__source-chip--active' : ''}`}
                    onClick={() => toggleSource(source)}
                    title={`${count} 个 Skill`}
                  >
                    <span>{source}</span>
                  </button>
                ))}
                {recommendedSources.length > DEFAULT_RECOMMENDED_SOURCE_COUNT && (
                  <button
                    className="sm2__source-toggle"
                    onClick={() => setSourcesExpanded((value) => !value)}
                  >
                    {sourcesExpanded ? '收起' : '展开更多'}
                  </button>
                )}
              </div>
            </div>
          )}

          {(publisherFilter || sourceFilter) && (
            <button className="sm2__source-clear" onClick={clearMarketScope}>
              清除筛选
            </button>
          )}
        </div>
      )}
      <div className="sm2__market-note">
        默认从 skills.sh 在线市场读取榜单；输入关键词后切换为搜索结果。安装会先进中心库，之后再分发给各个 Agent。
      </div>
      {status && <div className="sm2__notice sm2__notice--ok">{status}</div>}
      {viewLoading ? (
        <div className="sm2__empty">{sourceFilter ? `加载「${sourceFilter}」…` : '加载市场…'}</div>
      ) : viewError ? (
        <div className="sm2__error" style={{ margin: 0 }}>{viewError}</div>
      ) : visibleItems.length === 0 ? (
        <div className="sm2__empty">
          没有匹配的技能。可换一个关键词，或使用本地 / Git 安装。
        </div>
      ) : browsingPublisherMarkets ? (
        <div className="sm2__source-market-stage" ref={gridRef}>
          <div className="sm2__source-market-head">
            <div>
              <span>选择市场</span>
              <strong>{selectedPublisherLabel}</strong>
            </div>
            <small>进入一个来源仓库后，再查看和安装里面的 Skill。</small>
          </div>
          <div className="sm2__source-market-grid">
            {publisherMarketCards.map((market) => (
              <button
                key={market.source}
                className="sm2__source-market-card"
                onClick={() => selectSource(market.source)}
              >
                <div className="sm2__source-market-card-head">
                  <SkillAvatar source={market.source} name={market.source} />
                  <div>
                    <span>skills.sh market</span>
                    <strong>{market.source}</strong>
                  </div>
                  <em>进入 ›</em>
                </div>
                <div className="sm2__source-market-stats">
                  <span>{market.skillCount > 0 ? `${market.skillCount} 个 Skill` : '点击后加载'}</span>
                  {market.installCount > 0 && <span>↓ {formatInstallCount(market.installCount)}</span>}
                </div>
                {market.topSkills.length > 0 && (
                  <div className="sm2__source-market-samples">
                    {market.topSkills.map((skillName) => (
                      <span key={skillName}>{skillName}</span>
                    ))}
                  </div>
                )}
              </button>
            ))}
          </div>
        </div>
      ) : (
        <>
          {sourceFilter && publisherFilter && (
            <div className="sm2__market-backbar">
              <button className="sm2__source-back" onClick={() => setSourceFilter('')}>
                ← 返回 {selectedPublisherLabel} 的市场
              </button>
            </div>
          )}
          {viewMode === 'list' ? (
            <div className="sm2__market-list" ref={gridRef}>
              {paginatedItems.map((it) => {
                const isJustInstalled = installedIds.has(it.id)
                const isCurrentInstalling = installing.has(it.id)
                const done = it.isInstalled || isJustInstalled || isMarketItemInstalled(it, centerSkills)
                return (
                  <div key={it.id} className={`sm2__market-item${isCurrentInstalling ? ' sm2__market-item--installing' : ''}`} onClick={() => setDetailSkill(it)} style={{ cursor: 'pointer' }}>
                    <SkillAvatar source={it.source} name={it.name} />
                    <div className="sm2__market-item-main">
                      <div className="sm2__market-item-title">
                        <strong>{it.name}</strong>
                        {it.webUrl && (
                          <button className="sm2__icon-btn sm2__icon-btn--sm" title="在市场查看" onClick={(e) => { e.stopPropagation(); openExternal(it.webUrl!) }}>↗</button>
                        )}
                      </div>
                      <div className="sm2__market-item-meta">
                        <button
                          className="sm2__source-pill"
                          title="查看这个创建者的所有市场"
                          onClick={(e) => { e.stopPropagation(); openPublisherMarkets(it.source || it.registryId) }}
                        >
                          {it.source || it.registryId}
                        </button>
                        {typeof it.installCount === 'number' && (
                          <span className="sm2__install-count">↓ {formatInstallCount(it.installCount)}</span>
                        )}
                        {done && <span className="sm2__tag sm2__tag--ok">✓ 已安装</span>}
                      </div>
                      {isCurrentInstalling && <div className="sm2__install-progress"><div className="sm2__install-progress-bar" /></div>}
                    </div>
                    <button
                      className={`sm2__icon-btn sm2__icon-btn--add${done ? ' sm2__icon-btn--installed' : ''}`}
                      title={done ? '已在中心库' : '安装到中心库'}
                      disabled={isCurrentInstalling || done}
                      onClick={(e) => { e.stopPropagation(); install(it) }}
                    >
                      {isCurrentInstalling ? '…' : done ? '✓' : '+'}
                    </button>
                  </div>
                )
              })}
            </div>
          ) : (
            <div className="sm2__install-grid" ref={gridRef}>
              {paginatedItems.map((it) => {
                const isJustInstalled = installedIds.has(it.id)
                const isCurrentInstalling = installing.has(it.id)
                const done = it.isInstalled || isJustInstalled || isMarketItemInstalled(it, centerSkills)
                return (
                  <div key={it.id} className={`sm2__install-card${isCurrentInstalling ? ' sm2__install-card--installing' : ''}${done ? ' sm2__install-card--installed' : ''}`} onClick={() => setDetailSkill(it)}>
                    <div className="sm2__install-card-head">
                      <SkillAvatar source={it.source} name={it.name} />
                      <div className="sm2__install-card-title">{it.name}</div>
                      {it.webUrl && (
                        <button className="sm2__icon-btn sm2__icon-btn--sm" title="在市场查看" onClick={(e) => { e.stopPropagation(); openExternal(it.webUrl!) }}>↗</button>
                      )}
                      <button
                        className={`sm2__icon-btn sm2__icon-btn--add${done ? ' sm2__icon-btn--installed' : ''}`}
                        title={done ? '已在中心库' : '安装到中心库'}
                        disabled={isCurrentInstalling || done}
                        onClick={(e) => { e.stopPropagation(); install(it) }}
                      >
                        {isCurrentInstalling ? '…' : done ? '✓' : '+'}
                      </button>
                    </div>
                    <div className="sm2__install-card-meta">
                      <button
                        className="sm2__source-pill"
                        title="查看这个创建者的所有市场"
                        onClick={(e) => { e.stopPropagation(); openPublisherMarkets(it.source || it.registryId) }}
                      >
                        {it.source || it.registryId}
                      </button>
                      {typeof it.installCount === 'number' && (
                        <span className="sm2__install-count">↓ {formatInstallCount(it.installCount)}</span>
                      )}
                      {done && <span className="sm2__tag sm2__tag--ok">✓ 已安装</span>}
                    </div>
                    {isCurrentInstalling && <div className="sm2__install-progress"><div className="sm2__install-progress-bar" /></div>}
                  </div>
                )
              })}
            </div>
          )}
          {!browsingPublisherMarkets && totalPages > 1 && (
            <div className="sm2__pagination">
              <button
                className="sm2__page-btn sm2__page-btn--nav"
                disabled={currentPage === 1}
                onClick={() => changePage(Math.max(1, currentPage - 1))}
              >
                ‹
              </button>
              {visiblePages.map((p, i) => {
                const prev = visiblePages[i - 1]
                const showGap = prev != null && p - prev > 1
                return (
                  <span key={p} className="sm2__page-group">
                    {showGap && <span className="sm2__page-gap">···</span>}
                    <button
                      className={`sm2__page-btn${p === currentPage ? ' sm2__page-btn--active' : ''}`}
                      onClick={() => changePage(p)}
                    >
                      {p}
                    </button>
                  </span>
                )
              })}
              <button
                className="sm2__page-btn sm2__page-btn--nav"
                disabled={currentPage === totalPages}
                onClick={() => changePage(Math.min(totalPages, currentPage + 1))}
              >
                ›
              </button>
            </div>
          )}
        </>
      )}
      <MarketSkillDetail
        skill={detailSkill}
        onClose={() => setDetailSkill(null)}
        installing={detailSkill ? installing.has(detailSkill.id) : false}
        installed={detailSkill ? installedIds.has(detailSkill.id) || isMarketItemInstalled(detailSkill, centerSkills) : false}
        onInstall={(it) => { setDetailSkill(null); install(it) }}
      />
    </div>
  )
}

function MarketSkillDetail({ skill, onClose, installing, installed, onInstall }: {
  skill: MarketplaceSkill | null
  onClose: () => void
  installing: boolean
  installed: boolean
  onInstall: (it: MarketplaceSkill) => void
}) {
  const [remoteDetailState, setRemoteDetailState] = useState<{ key: string; detail: MarketplaceSkillDetail | null } | null>(null)
  const source = skill?.source || skill?.registryId || ''
  const skillId = skill ? marketSkillId(skill) : ''
  const detailKey = skill && source && skillId && skill.registryId === 'skills-sh' ? `${source}/${skillId}` : ''
  const remoteDetail = remoteDetailState?.key === detailKey ? remoteDetailState.detail : null
  const detailLoading = Boolean(detailKey && remoteDetailState?.key !== detailKey)
  const pathParts = skill ? marketSkillPathParts(skill) : []
  const sourceUrl = source ? `https://www.skills.sh/${source}` : null
  const githubUrl = skill ? marketGithubUrl(skill, remoteDetail) : null
  const installCommand = skill ? (remoteDetail?.installCommand || marketInstallCommand(skill)) : ''
  const detailDescription = skill ? marketDescription(skill, remoteDetail) : null

  useEffect(() => {
    if (!detailKey) return
    let alive = true
    skillApiV2.fetchMarketplaceSkillDetail(source, skillId)
      .then((detail) => {
        if (alive) setRemoteDetailState({ key: detailKey, detail })
      })
      .catch(() => {
        if (alive) setRemoteDetailState({ key: detailKey, detail: null })
      })
    return () => {
      alive = false
    }
  }, [detailKey, skillId, source])

  return (
    <SlideOver
      open={!!skill}
      onClose={onClose}
      width={520}
      className="sm2__slideover--market-detail"
      title={skill?.name || ''}
      subtitle={source || undefined}
      actions={
        githubUrl ? (
          <button className="sm2__btn sm2__btn--ghost" onClick={() => openExternal(githubUrl)}>GitHub ↗</button>
        ) : undefined
      }
    >
      {skill && (
        <div className="sm2__market-detail">
          <div className="sm2__market-detail-hero">
            <SkillAvatar source={skill.source} name={skill.name} />
            <div>
              <div className="sm2__market-detail-eyebrow">skills.sh skill</div>
              <h3 className="sm2__market-detail-name">{skill.name}</h3>
              <div className="sm2__market-detail-meta">
                <span className="sm2__source-pill">{source}</span>
                {typeof skill.installCount === 'number' && (
                  <span className="sm2__install-count">↓ {formatInstallCount(skill.installCount)}</span>
                )}
                {(skill.isInstalled || installed) && <span className="sm2__tag sm2__tag--ok">✓ 已安装</span>}
              </div>
            </div>
          </div>

          <div className="sm2__market-detail-stats">
            <div className="sm2__market-detail-stat">
              <span>路径</span>
              <strong>{pathParts.length} 段</strong>
            </div>
            <div className="sm2__market-detail-stat">
              <span>来源</span>
              <strong>{source || 'skills.sh'}</strong>
            </div>
            <div className="sm2__market-detail-stat">
              <span>安装量</span>
              <strong>{typeof skill.installCount === 'number' ? formatInstallCount(skill.installCount) : '-'}</strong>
            </div>
          </div>

          {(detailDescription || detailLoading) && (
            <div className="sm2__market-detail-section">
              <h4>描述</h4>
              <p>{detailDescription || '正在读取 skills.sh 详情描述...'}</p>
            </div>
          )}

          <div className="sm2__market-detail-section">
            <h4>skills.sh 路径</h4>
            <div className="sm2__market-breadcrumb">
              {pathParts.map((part, index) => {
                const href = `https://www.skills.sh/${pathParts.slice(0, index + 1).join('/')}`
                return (
                  <span className="sm2__market-breadcrumb-part" key={`${part}-${index}`}>
                    {index > 0 && <span className="sm2__market-breadcrumb-separator">/</span>}
                    <button onClick={() => openExternal(href)}>{part}</button>
                  </span>
                )
              })}
            </div>
          </div>

          {installCommand && (
            <div className="sm2__market-install-command">
              <span>安装命令</span>
              <code>{installCommand}</code>
            </div>
          )}

          <div className="sm2__market-detail-section">
            <h4>信息</h4>
            <div className="sm2__compact-info">
              <div className="sm2__compact-row">
                <span>下载地址</span>
                <code>{skill.downloadUrl}</code>
              </div>
              <div className="sm2__compact-row">
                <span>来源市场</span>
                <strong>{skill.registryId}</strong>
              </div>
              {skill.syncedAt && (
                <div className="sm2__compact-row">
                  <span>同步时间</span>
                  <strong>{new Date(skill.syncedAt).toLocaleString()}</strong>
                </div>
              )}
            </div>
          </div>

          <div className="sm2__btn-row" style={{ marginTop: 20 }}>
            <button
              className="sm2__btn sm2__btn--primary"
              disabled={installing || skill.isInstalled || installed}
              onClick={() => onInstall(skill)}
            >
              {installing ? '安装中…' : skill.isInstalled || installed ? '已安装' : '安装到中心库'}
            </button>
            {sourceUrl && (
              <button className="sm2__btn" onClick={() => openExternal(sourceUrl)}>
                来源页 ↗
              </button>
            )}
            {githubUrl && (
              <button className="sm2__btn" onClick={() => openExternal(githubUrl)}>
                GitHub ↗
              </button>
            )}
            {skill.webUrl && (
              <button className="sm2__btn" onClick={() => openExternal(skill.webUrl!)}>
                Skill 页 ↗
              </button>
            )}
          </div>
        </div>
      )}
    </SlideOver>
  )
}

function marketSkillPathParts(skill: MarketplaceSkill) {
  const sourceParts = (skill.source || '').split('/').filter(Boolean)
  const skillId = marketSkillId(skill)
  return [...sourceParts, skillId].filter(Boolean)
}

function marketGithubUrl(skill: MarketplaceSkill, detail?: MarketplaceSkillDetail | null) {
  if (detail?.githubUrl) return detail.githubUrl
  const skillId = marketSkillId(skill)
  const repoUrl = marketGithubRepoUrl(skill)
  if (repoUrl && (skill.source || '').endsWith('/skills') && skillId && !skillId.includes('/')) {
    return `${repoUrl}/tree/main/skills/${skillId}`
  }
  return repoUrl
}

function marketGithubRepoUrl(skill: MarketplaceSkill) {
  const rawPath = skill.downloadUrl.replace(/^(github|skillssh):/, '')
  const parts = rawPath.split('/').filter(Boolean)
  if (parts.length >= 2) return `https://github.com/${parts[0]}/${parts[1]}`
  const sourceParts = (skill.source || '').split('/').filter(Boolean)
  if (sourceParts.length >= 2) return `https://github.com/${sourceParts[0]}/${sourceParts[1]}`
  return null
}

function marketInstallCommand(skill: MarketplaceSkill) {
  const skillId = marketSkillId(skill)
  const repoUrl = marketGithubRepoUrl(skill)
  if (repoUrl && skillId) return `npx skills add ${repoUrl} --skill ${skillId}`
  if (skill.source) return `npx skills add ${skill.source}${skillId ? `/${skillId}` : ''}`
  return ''
}

function marketDescription(skill: MarketplaceSkill, detail?: MarketplaceSkillDetail | null) {
  if (detail?.description) return detail.description
  const description = skill.description?.trim()
  if (!description) return null
  if (description.startsWith('来自 ') && description.endsWith(' 的在线 Skill')) return null
  if (description.startsWith('skills.sh ·')) return null
  return description
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

function publisherFromSource(source: string) {
  return source.split('/').filter(Boolean)[0] || source || 'skills.sh'
}

// ── Local Agent sync ─────────────────────────────────────────────

type AgentSyncRow = { agent: AgentSkillInventoryAgent; item: AgentSkillInventoryItem }
type AgentSyncViewMode = 'list' | 'cards'

const AGENT_STATUS_TABS: Array<{ id: string; label: string }> = [
  { id: 'all', label: '全部' },
  { id: 'importable', label: '可接管' },
  { id: 'unmanaged', label: '未管理' },
  { id: 'managed', label: '已管理' },
  { id: 'conflict', label: '同名冲突' },
]
const DEFAULT_AGENT_CHIP_COUNT = 6

function statusTone(item: AgentSkillInventoryItem) {
  if (item.status === 'conflict') return 'conflict'
  return item.managed ? 'ok' : 'unmanaged'
}

function installedAgentInventory(agents: AgentSkillInventoryAgent[]) {
  return agents
    .filter((agent) => agent.installed)
    .sort((a, b) => {
      const skillCountDiff = localSkillCount(b) - localSkillCount(a)
      if (skillCountDiff !== 0) return skillCountDiff
      const importableDiff = b.importableCount - a.importableCount
      if (importableDiff !== 0) return importableDiff
      return a.displayName.localeCompare(b.displayName)
    })
}

function localSkillCount(agent: AgentSkillInventoryAgent) {
  return agent.managedCount + agent.unmanagedCount
}

export function AgentSyncPanel({ onDone }: { onDone: InstallDoneHandler }) {
  const [agents, setAgents] = useState<AgentSkillInventoryAgent[]>([])
  const [selectedAgent, setSelectedAgent] = useState('all')
  const [statusFilter, setStatusFilter] = useState('all')
  const [query, setQuery] = useState('')
  const [viewMode, setViewMode] = useState<AgentSyncViewMode>('cards')
  const [agentsExpanded, setAgentsExpanded] = useState(false)
  const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set())
  const [detailRow, setDetailRow] = useState<AgentSyncRow | null>(null)
  const [loading, setLoading] = useState(true)
  const [scanning, setScanning] = useState(false)
  const [importing, setImporting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)

  const load = async () => {
    setLoading(true)
    setError(null)
    try {
      const next = installedAgentInventory(await skillApiV2.listAgentSkillInventory())
      setAgents(next)
      setSelectedAgent((current) => {
        if (current === 'all') return current
        return next.some((agent) => agent.agentId === current) ? current : 'all'
      })
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

  const visibleAgentChips = useMemo(() => {
    if (agentsExpanded) return agents
    const collapsed = agents.slice(0, DEFAULT_AGENT_CHIP_COUNT)
    if (selectedAgent !== 'all' && !collapsed.some((agent) => agent.agentId === selectedAgent)) {
      const selected = agents.find((agent) => agent.agentId === selectedAgent)
      if (selected) return [...collapsed, selected]
    }
    return collapsed
  }, [agents, agentsExpanded, selectedAgent])

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

  const adoptItems = async (targets: AgentSkillInventoryItem[]) => {
    const selected = targets.filter((item) => item.canImport)
    if (selected.length === 0) return
    setImporting(true)
    setError(null)
    setNotice(null)
    try {
      let ok = 0
      const failed: string[] = []
      for (const item of selected) {
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
      setDetailRow(null)
      setNotice(`已接管 ${ok} 个 Skill${failed.length ? `，${failed.length} 个失败` : ''}`)
      if (failed.length > 0) setError(failed.slice(0, 3).join('\n'))
    } finally {
      setImporting(false)
    }
  }

  const importSelected = () => {
    const selected = rows.filter(({ item }) => selectedIds.has(importKey(item))).map(({ item }) => item)
    void adoptItems(selected)
  }

  return (
    <div className="sm2__agent-sync sm2__install-market">
      <div className="sm2__market-boardbar">
        <div className="sm2__view-toggle sm2__market-boardtabs">
          {AGENT_STATUS_TABS.map((tab) => (
            <button
              key={tab.id}
              className={statusFilter === tab.id ? 'active' : ''}
              onClick={() => setStatusFilter(tab.id)}
            >
              {tab.label}
            </button>
          ))}
        </div>
        <div className="sm2__search-wrapper">
          <span className="sm2__search-icon">⌕</span>
          <input
            className="sm2__search sm2__search--with-icon"
            placeholder="搜索 Skill 名称 / 路径 / 状态…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <div className="sm2__view-toggle sm2__view-toggle--soft">
          <button className={viewMode === 'list' ? 'active' : ''} onClick={() => setViewMode('list')}>列表</button>
          <button className={viewMode === 'cards' ? 'active' : ''} onClick={() => setViewMode('cards')}>卡片</button>
        </div>
      </div>

      <div className="sm2__market-source-row">
        <div className="sm2__source-row-section">
          <div className="sm2__source-row-head">
            <span>本地 Agent</span>
            <small>按 Agent 过滤它的本地 Skills 目录</small>
          </div>
          <div className="sm2__source-chip-cloud">
            <button
              className={`sm2__source-chip sm2__source-chip--all${selectedAgent === 'all' ? ' sm2__source-chip--active' : ''}`}
              onClick={() => setSelectedAgent('all')}
            >
              <span>全部 Agent</span>
            </button>
            {visibleAgentChips.map((agent) => (
              <button
                key={agent.agentId}
                className={`sm2__source-chip${selectedAgent === agent.agentId ? ' sm2__source-chip--active' : ''}`}
                onClick={() => setSelectedAgent((current) => (current === agent.agentId ? 'all' : agent.agentId))}
                title={`${agent.managedCount} 已管理 · ${agent.importableCount} 可接管`}
              >
                <span>{agent.displayName}</span>
                {agent.importableCount > 0 && <em>{agent.importableCount}</em>}
              </button>
            ))}
            {agents.length > DEFAULT_AGENT_CHIP_COUNT && (
              <button className="sm2__source-toggle" onClick={() => setAgentsExpanded((value) => !value)}>
                {agentsExpanded ? '收起' : '展开更多'}
              </button>
            )}
          </div>
        </div>

        <button className="sm2__source-clear" onClick={scan} disabled={scanning || importing}>
          {scanning ? '扫描中…' : '重新扫描 Agent'}
        </button>
      </div>

      <div className="sm2__agent-sync-statline">
        <span><strong>{agents.length}</strong> Agent</span>
        <span><strong>{totalManaged}</strong> 已管理</span>
        <span><strong>{totalUnmanaged}</strong> 未管理</span>
        <span className="sm2__agent-sync-statline-ok"><strong>{totalImportable}</strong> 可接管</span>
      </div>

      <div className="sm2__market-note">
        按 Agent 的本地 Skills 目录检查状态，只能把未管理且无冲突的 Skill 接管到中心库。点击任意 Skill 查看详情。
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
      ) : viewMode === 'cards' ? (
        <div className="sm2__install-grid">
          {rows.map(({ agent, item }) => {
            const key = importKey(item)
            const checked = selectedIds.has(key)
            const tone = statusTone(item)
            return (
              <div
                key={key}
                className={`sm2__install-card sm2__agent-sync-card sm2__agent-sync-card--${tone}${checked ? ' sm2__agent-sync-card--selected' : ''}`}
                onClick={() => setDetailRow({ agent, item })}
              >
                <div className="sm2__agent-sync-card-head">
                  <input
                    type="checkbox"
                    className="sm2__agent-sync-checkbox"
                    checked={checked}
                    disabled={!item.canImport || importing}
                    onClick={(e) => e.stopPropagation()}
                    onChange={() => toggle(item)}
                  />
                  <AgentIconBadge iconKey={agent.iconKey} size={34} title={agent.displayName} />
                  <div className="sm2__agent-sync-card-title">{item.name}</div>
                  {item.canImport && (
                    <button
                      className="sm2__icon-btn sm2__icon-btn--add"
                      title="接管到中心库"
                      disabled={importing}
                      onClick={(e) => { e.stopPropagation(); void adoptItems([item]) }}
                    >
                      +
                    </button>
                  )}
                </div>
                <div className="sm2__agent-sync-card-meta">
                  <span className="sm2__source-pill">{agent.displayName}</span>
                  <span className={`sm2__tag sm2__tag--${tone}`}>{item.statusLabel}</span>
                  {item.actualMode && <span className="sm2__tag">{item.actualMode}</span>}
                </div>
                <code className="sm2__agent-sync-card-path">{item.path}</code>
              </div>
            )
          })}
        </div>
      ) : (
        <div className="sm2__market-list sm2__agent-sync-listview">
          {rows.map(({ agent, item }) => {
            const key = importKey(item)
            const checked = selectedIds.has(key)
            const tone = statusTone(item)
            return (
              <div
                key={key}
                className="sm2__market-item sm2__agent-sync-item"
                onClick={() => setDetailRow({ agent, item })}
                style={{ cursor: 'pointer' }}
              >
                <input
                  type="checkbox"
                  className="sm2__agent-sync-checkbox"
                  checked={checked}
                  disabled={!item.canImport || importing}
                  onClick={(e) => e.stopPropagation()}
                  onChange={() => toggle(item)}
                />
                <AgentIconBadge iconKey={agent.iconKey} size={34} title={agent.displayName} />
                <div className="sm2__market-item-main">
                  <div className="sm2__market-item-title">
                    <strong>{item.name}</strong>
                  </div>
                  <div className="sm2__market-item-meta">
                    <span className="sm2__source-pill">{agent.displayName}</span>
                    <span className={`sm2__tag sm2__tag--${tone}`}>{item.statusLabel}</span>
                    {item.actualMode && <span className="sm2__tag">{item.actualMode}</span>}
                    <code className="sm2__agent-sync-item-path">{item.path}</code>
                  </div>
                </div>
                {item.canImport && (
                  <button
                    className="sm2__icon-btn sm2__icon-btn--add"
                    title="接管到中心库"
                    disabled={importing}
                    onClick={(e) => { e.stopPropagation(); void adoptItems([item]) }}
                  >
                    +
                  </button>
                )}
              </div>
            )
          })}
        </div>
      )}

      <AgentSkillDetail
        row={detailRow}
        importing={importing}
        onClose={() => setDetailRow(null)}
        onAdopt={(item) => void adoptItems([item])}
      />
    </div>
  )
}

function AgentSkillDetail({ row, importing, onClose, onAdopt }: {
  row: AgentSyncRow | null
  importing: boolean
  onClose: () => void
  onAdopt: (item: AgentSkillInventoryItem) => void
}) {
  const agent = row?.agent
  const item = row?.item
  const tone = item ? statusTone(item) : 'unmanaged'
  const pathSegments = (item?.path || '').split('/').filter(Boolean)

  return (
    <SlideOver
      open={!!row}
      onClose={onClose}
      width={520}
      className="sm2__slideover--market-detail"
      title={item?.name || ''}
      subtitle={agent?.displayName || undefined}
      actions={
        item ? (
          <button className="sm2__btn sm2__btn--ghost" onClick={() => openExternal(item.path)}>在 Finder 打开 ↗</button>
        ) : undefined
      }
    >
      {agent && item && (
        <div className="sm2__market-detail">
          <div className="sm2__market-detail-hero">
            <AgentIconBadge iconKey={agent.iconKey} size={40} title={agent.displayName} />
            <div>
              <div className="sm2__market-detail-eyebrow">本地 Agent Skill</div>
              <h3 className="sm2__market-detail-name">{item.name}</h3>
              <div className="sm2__market-detail-meta">
                <span className="sm2__source-pill">{agent.displayName}</span>
                <span className={`sm2__tag sm2__tag--${tone}`}>{item.statusLabel}</span>
                {item.actualMode && <span className="sm2__tag">{item.actualMode}</span>}
              </div>
            </div>
          </div>

          <div className="sm2__market-detail-stats">
            <div className="sm2__market-detail-stat">
              <span>状态</span>
              <strong>{item.statusLabel}</strong>
            </div>
            <div className="sm2__market-detail-stat">
              <span>来源 Agent</span>
              <strong>{agent.displayName}</strong>
            </div>
            <div className="sm2__market-detail-stat">
              <span>接管</span>
              <strong>{item.canImport ? '可接管' : '不可接管'}</strong>
            </div>
          </div>

          {item.reason && (
            <div className="sm2__market-detail-section">
              <h4>说明</h4>
              <p>{item.reason}</p>
            </div>
          )}

          <div className="sm2__market-detail-section">
            <h4>本地路径</h4>
            <div className="sm2__market-breadcrumb">
              {pathSegments.map((part, index) => (
                <span className="sm2__market-breadcrumb-part" key={`${part}-${index}`}>
                  {index > 0 && <span className="sm2__market-breadcrumb-separator">/</span>}
                  <span>{part}</span>
                </span>
              ))}
            </div>
          </div>

          <div className="sm2__market-detail-section">
            <h4>信息</h4>
            <div className="sm2__compact-info">
              <div className="sm2__compact-row">
                <span>完整路径</span>
                <code>{item.path}</code>
              </div>
              <div className="sm2__compact-row">
                <span>Skill ID</span>
                <strong>{item.skillId}</strong>
              </div>
              {item.actualMode && (
                <div className="sm2__compact-row">
                  <span>安装模式</span>
                  <strong>{item.actualMode}</strong>
                </div>
              )}
              {item.hash && (
                <div className="sm2__compact-row">
                  <span>内容哈希</span>
                  <code>{item.hash.slice(0, 16)}</code>
                </div>
              )}
            </div>
          </div>

          <div className="sm2__btn-row" style={{ marginTop: 20 }}>
            <button
              className="sm2__btn sm2__btn--primary"
              disabled={!item.canImport || importing}
              onClick={() => onAdopt(item)}
            >
              {importing ? '接管中…' : item.canImport ? '接管到中心库' : item.managed ? '已被管理' : '无法接管'}
            </button>
            <button className="sm2__btn" onClick={() => openExternal(item.path)}>
              在 Finder 打开 ↗
            </button>
          </div>
        </div>
      )}
    </SlideOver>
  )
}

function importKey(item: AgentSkillInventoryItem) {
  return `${item.agentId}:${item.id}`
}

// ── Local ────────────────────────────────────────────────────────

export function LocalPanel({ onDone }: { onDone: InstallDoneHandler }) {
  const [sourcePath, setSourcePath] = useState('')
  const [multi, setMulti] = useState(false)
  const [preview, setPreview] = useState<AddCenterSkillPreview | null>(null)
  const [localViewMode, setLocalViewMode] = useState<LocalPreviewViewMode>('cards')
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
    const totalChanges = preview.candidates.length + preview.blockers.length
    return (
      <div className="sm2__local-preview">
        <div className="sm2__local-preview-head">
          <div>
            <h3 className="sm2__install-h">确认导入预览</h3>
            <p className="sm2__install-sub">
              将导入到中心库：{preview.centerPath || '中心 Skill 库'}
            </p>
          </div>
          <div className="sm2__local-preview-actions">
            <div className="sm2__local-preview-stats">
              <span><strong>{preview.candidates.length}</strong> 可导入</span>
              <span><strong>{preview.blockers.length}</strong> 需处理</span>
            </div>
            <div className="sm2__view-toggle sm2__view-toggle--soft">
              <button className={localViewMode === 'list' ? 'active' : ''} onClick={() => setLocalViewMode('list')}>列表</button>
              <button className={localViewMode === 'cards' ? 'active' : ''} onClick={() => setLocalViewMode('cards')}>卡片</button>
            </div>
          </div>
        </div>

        {totalChanges === 0 ? (
          <div className="sm2__empty">没有检测到可导入的 Skill。</div>
        ) : localViewMode === 'cards' ? (
          <div className="sm2__local-preview-grid">
            {preview.candidates.map((c) => (
              <LocalCandidateCard key={c.skillId} candidate={c} />
            ))}
            {preview.blockers.map((b) => (
              <LocalBlockerCard
                key={b.skillId}
                blocker={b}
                value={renames[b.skillId] || ''}
                onChange={(value) => setRenames((current) => ({ ...current, [b.skillId]: value }))}
              />
            ))}
          </div>
        ) : (
          <div className="sm2__local-preview-list">
            {preview.candidates.map((c) => (
              <LocalCandidateRow key={c.skillId} candidate={c} />
            ))}
            {preview.blockers.map((b) => (
              <LocalBlockerRow
                key={b.skillId}
                blocker={b}
                value={renames[b.skillId] || ''}
                onChange={(value) => setRenames((current) => ({ ...current, [b.skillId]: value }))}
              />
            ))}
          </div>
        )}

        {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
        <div className="sm2__btn-row sm2__local-preview-footer">
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

function LocalCandidateCard({ candidate }: { candidate: AddCenterSkillPreview['candidates'][number] }) {
  const tone = localActionTone(candidate.action)
  return (
    <div className={`sm2__local-skill-card sm2__local-skill-card--${tone}`}>
      <div className="sm2__local-skill-card-head">
        <div className="sm2__market-skill-icon">{initials(candidate.name || candidate.skillId)}</div>
        <div className="sm2__local-skill-titleblock">
          <strong>{candidate.name || candidate.skillId}</strong>
          <code>{candidate.skillId} → {candidate.proposedSkillId}</code>
        </div>
        <span className={`sm2__tag sm2__tag--${tone}`}>{localActionLabel(candidate.action)}</span>
      </div>
      {candidate.description && <p className="sm2__local-skill-desc">{candidate.description}</p>}
      <code className="sm2__local-skill-path">{candidate.sourceDir}</code>
    </div>
  )
}

function LocalCandidateRow({ candidate }: { candidate: AddCenterSkillPreview['candidates'][number] }) {
  const tone = localActionTone(candidate.action)
  return (
    <div className={`sm2__local-skill-row sm2__local-skill-row--${tone}`}>
      <div className="sm2__market-skill-icon">{initials(candidate.name || candidate.skillId)}</div>
      <div className="sm2__local-skill-row-main">
        <div className="sm2__local-skill-row-title">
          <strong>{candidate.name || candidate.skillId}</strong>
          <code>{candidate.skillId} → {candidate.proposedSkillId}</code>
        </div>
        <code className="sm2__local-skill-path">{candidate.sourceDir}</code>
      </div>
      <span className={`sm2__tag sm2__tag--${tone}`}>{localActionLabel(candidate.action)}</span>
    </div>
  )
}

function LocalBlockerCard({
  blocker,
  value,
  onChange,
}: {
  blocker: AddCenterSkillPreview['blockers'][number]
  value: string
  onChange: (value: string) => void
}) {
  return (
    <div className="sm2__local-skill-card sm2__local-skill-card--conflict">
      <div className="sm2__local-skill-card-head">
        <div className="sm2__market-skill-icon">!</div>
        <div className="sm2__local-skill-titleblock">
          <strong>{blocker.name || blocker.skillId}</strong>
          <code>{blocker.skillId}</code>
        </div>
        <span className="sm2__tag sm2__tag--conflict">需处理</span>
      </div>
      <p className="sm2__local-skill-desc">{blocker.reason || '同名 Skill 需要选择处理方式。'}</p>
      <label className="sm2__local-rename-field">
        <span>重命名为</span>
        <input value={value} onChange={(e) => onChange(e.target.value)} placeholder={`${blocker.skillId}-rename`} />
      </label>
    </div>
  )
}

function LocalBlockerRow({
  blocker,
  value,
  onChange,
}: {
  blocker: AddCenterSkillPreview['blockers'][number]
  value: string
  onChange: (value: string) => void
}) {
  return (
    <div className="sm2__local-skill-row sm2__local-skill-row--conflict">
      <div className="sm2__market-skill-icon">!</div>
      <div className="sm2__local-skill-row-main">
        <div className="sm2__local-skill-row-title">
          <strong>{blocker.name || blocker.skillId}</strong>
          <span>{blocker.reason || '同名 Skill 需要选择处理方式。'}</span>
        </div>
        <label className="sm2__local-rename-field sm2__local-rename-field--inline">
          <span>重命名为</span>
          <input value={value} onChange={(e) => onChange(e.target.value)} placeholder={`${blocker.skillId}-rename`} />
        </label>
      </div>
      <span className="sm2__tag sm2__tag--conflict">需处理</span>
    </div>
  )
}

function localActionLabel(action: string) {
  if (action === 'update') return '更新'
  if (action === 'create') return '新增'
  return action
}

function localActionTone(action: string) {
  return action === 'update' ? 'ok' : 'unmanaged'
}

// ── Git ──────────────────────────────────────────────────────────

type GitRepoSkill = GitHubRepoPreview['skills'][number]
type GitStage = 'input' | 'select' | 'done'
type ConflictMode = 'rename' | 'overwrite'

interface GitDoneSummary {
  imported: number
  skipped: number
  skills: string[]
}

function buildGitRef(url: string, branch: string): string {
  const trimmedUrl = url.trim().replace(/\/+$/, '')
  const trimmedBranch = branch.trim()
  if (!trimmedBranch || /\/tree\//.test(trimmedUrl)) return trimmedUrl
  return `${trimmedUrl}/tree/${trimmedBranch}`
}

function urlHasSubpath(ref: string): boolean {
  const match = ref.match(/\/tree\/[^/]+\/(.+)$/)
  return Boolean(match && match[1])
}

export function GitPanel({ initialUrl, onDone }: { initialUrl?: string; onDone: InstallDoneHandler }) {
  const [url, setUrl] = useState(initialUrl || '')
  const [branch, setBranch] = useState('')
  const [token, setToken] = useState('')
  const [showAdvanced, setShowAdvanced] = useState(false)
  const [viewMode, setViewMode] = useState<MarketViewMode>('cards')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const [stage, setStage] = useState<GitStage>('input')
  const [repo, setRepo] = useState<GitHubRepoPreview | null>(null)
  const [activeRef, setActiveRef] = useState('')
  const [selected, setSelected] = useState<Set<string>>(() => new Set())
  const [conflictMode, setConflictMode] = useState<Record<string, ConflictMode>>({})
  const [renames, setRenames] = useState<Record<string, string>>({})
  const [summary, setSummary] = useState<GitDoneSummary | null>(null)

  const resetToInput = () => {
    setStage('input')
    setRepo(null)
    setSelected(new Set())
    setConflictMode({})
    setRenames({})
    setSummary(null)
    setError(null)
  }

  const buildSelections = (skills: GitRepoSkill[], picked: Set<string>) =>
    skills
      .filter((s) => picked.has(s.sourcePath))
      .map((s) => {
        if (!s.conflict) return { sourcePath: s.sourcePath, resolution: 'overwrite' as const }
        const mode = conflictMode[s.sourcePath] || 'rename'
        if (mode === 'overwrite') return { sourcePath: s.sourcePath, resolution: 'overwrite' as const }
        const renamed = renames[s.sourcePath]?.trim()
        if (!renamed) return { sourcePath: s.sourcePath, resolution: 'skip' as const }
        return { sourcePath: s.sourcePath, resolution: 'rename' as const, renamedSkillId: renamed }
      })

  const importSelections = async (
    ref: string,
    selections: ReturnType<typeof buildSelections>,
    skills: GitRepoSkill[],
    picked: Set<string>,
  ) => {
    const result = await skillApiV2.importGitHubRepoSkills(ref, selections)
    await skillApiV2.refresh()
    const importedCount = result.importedSkills.length || selections.filter((s) => s.resolution !== 'skip').length
    const skippedCount = result.skippedSkills.length || selections.filter((s) => s.resolution === 'skip').length
    setSummary({
      imported: importedCount,
      skipped: skippedCount,
      skills: skills.filter((s) => picked.has(s.sourcePath)).map((s) => s.skillName),
    })
    setStage('done')
    onDone()
  }

  const detect = async () => {
    if (!url.trim()) {
      setError('请输入 Git 仓库 URL')
      return
    }
    const ref = buildGitRef(url, branch)
    setBusy(true)
    setError(null)
    try {
      const preview = await skillApiV2.previewGitHubRepoImport(ref)
      if (preview.skills.length === 0) {
        setError('该仓库未检测到任何 Skill（需含 SKILL.md）')
        return
      }
      setActiveRef(ref)
      setRepo(preview)

      const single = preview.skills[0]
      // 具体 skill 路径 + 唯一 + 无冲突 → 直接安装，零额外点击
      if (preview.skills.length === 1 && urlHasSubpath(ref) && !single.conflict) {
        const picked = new Set([single.sourcePath])
        setSelected(picked)
        await importSelections(
          ref,
          [{ sourcePath: single.sourcePath, resolution: 'overwrite' as const }],
          preview.skills,
          picked,
        )
        return
      }

      // 默认勾选无冲突项；冲突项默认不勾选
      setSelected(new Set(preview.skills.filter((s) => !s.conflict).map((s) => s.sourcePath)))
      setConflictMode({})
      setRenames({})
      setStage('select')
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  const installSelected = async () => {
    if (!repo) return
    const selections = buildSelections(repo.skills, selected)
    const effective = selections.filter((s) => s.resolution !== 'skip')
    if (effective.length === 0) {
      setError('请至少选择一个可安装的 Skill（冲突项需选择覆盖或填写新名称）')
      return
    }
    setBusy(true)
    setError(null)
    try {
      await importSelections(activeRef, selections, repo.skills, selected)
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  const toggle = (sourcePath: string) => {
    setSelected((current) => {
      const next = new Set(current)
      if (next.has(sourcePath)) next.delete(sourcePath)
      else next.add(sourcePath)
      return next
    })
  }

  const renderConflictControls = (s: GitRepoSkill, mode: ConflictMode) => (
    <div className="sm2__git-conflict-ctrl" onClick={(e) => e.stopPropagation()}>
      <div className="sm2__git-conflict-modes">
        <button
          className={mode === 'rename' ? 'active' : ''}
          onClick={() => setConflictMode((m) => ({ ...m, [s.sourcePath]: 'rename' }))}
        >
          重命名
        </button>
        <button
          className={mode === 'overwrite' ? 'active' : ''}
          onClick={() => setConflictMode((m) => ({ ...m, [s.sourcePath]: 'overwrite' }))}
        >
          覆盖
        </button>
      </div>
      {mode === 'rename' && (
        <input
          className="sm2__git-rename-input"
          value={renames[s.sourcePath] || ''}
          onChange={(e) => setRenames((r) => ({ ...r, [s.sourcePath]: e.target.value }))}
          placeholder={`${s.skillId}-copy（留空则跳过）`}
        />
      )}
    </div>
  )

  // ── done ───────────────────────────────────────────────────────
  if (stage === 'done' && summary) {
    return (
      <div className="sm2__git">
        <div className="sm2__git-done">
          <div className="sm2__git-done-icon">✓</div>
          <h3>已导入 {summary.imported} 个 Skill</h3>
          <p>
            导入到中心 Skill 库{summary.skipped > 0 ? `，跳过 ${summary.skipped} 个` : ''}。
            可在「Skill 库」中分发给各个 Agent。
          </p>
          {summary.skills.length > 0 && (
            <div className="sm2__git-done-chips">
              {summary.skills.map((name) => (
                <span key={name} className="sm2__git-chip">{name}</span>
              ))}
            </div>
          )}
          <div className="sm2__btn-row" style={{ justifyContent: 'center' }}>
            <button className="sm2__btn sm2__btn--primary" onClick={resetToInput}>继续安装</button>
          </div>
        </div>
      </div>
    )
  }

  // ── select ─────────────────────────────────────────────────────
  if (stage === 'select' && repo) {
    const owner = repo.repo.owner
    const total = repo.skills.length
    const selectedCount = repo.skills.filter((s) => selected.has(s.sourcePath)).length
    const allSelected = selectedCount === total && total > 0
    return (
      <div className="sm2__git">
        <div className="sm2__git-manifest">
          <GitOwnerAvatar owner={owner} />
          <div className="sm2__git-manifest-main">
            <div className="sm2__git-manifest-repo">
              <strong>{owner}/{repo.repo.repo}</strong>
              <span className="sm2__git-branch-chip">⑂ {repo.repo.branch}</span>
            </div>
            <span className="sm2__git-manifest-count">
              检测到 <strong>{total}</strong> 个 Skill · 已选 <strong>{selectedCount}</strong>
            </span>
          </div>
          <div className="sm2__git-manifest-actions">
            <div className="sm2__view-toggle sm2__view-toggle--soft">
              <button className={viewMode === 'list' ? 'active' : ''} onClick={() => setViewMode('list')}>列表</button>
              <button className={viewMode === 'cards' ? 'active' : ''} onClick={() => setViewMode('cards')}>卡片</button>
            </div>
            <button
              className="sm2__btn sm2__btn--ghost"
              onClick={() =>
                setSelected(allSelected ? new Set() : new Set(repo.skills.map((s) => s.sourcePath)))
              }
            >
              {allSelected ? '全不选' : '全选'}
            </button>
          </div>
        </div>

        {viewMode === 'cards' ? (
          <div className="sm2__git-skill-grid">
            {repo.skills.map((s) => {
              const checked = selected.has(s.sourcePath)
              const mode = conflictMode[s.sourcePath] || 'rename'
              return (
                <div
                  key={s.sourcePath}
                  className={`sm2__git-skill-card${checked ? ' sm2__git-skill-card--on' : ''}${s.conflict ? ' sm2__git-skill-card--conflict' : ''}`}
                >
                  <div className="sm2__git-skill-card-head">
                    <label className="sm2__git-skill-check" onClick={(e) => e.stopPropagation()}>
                      <input type="checkbox" checked={checked} onChange={() => toggle(s.sourcePath)} />
                    </label>
                    <div className="sm2__git-skill-main" onClick={() => toggle(s.sourcePath)}>
                      <div className="sm2__git-skill-titleline">
                        <strong>{s.skillName}</strong>
                        {s.conflict && <span className="sm2__tag sm2__tag--conflict">中心库已存在</span>}
                      </div>
                      {s.description && <p className="sm2__git-skill-desc">{s.description}</p>}
                      <code className="sm2__git-skill-path">{s.sourcePath || repo.repo.repo}</code>
                    </div>
                  </div>
                  {checked && s.conflict && renderConflictControls(s, mode)}
                </div>
              )
            })}
          </div>
        ) : (
          <div className="sm2__git-skill-list">
            {repo.skills.map((s) => {
              const checked = selected.has(s.sourcePath)
              const mode = conflictMode[s.sourcePath] || 'rename'
              return (
                <div
                  key={s.sourcePath}
                  className={`sm2__git-skill-row${checked ? ' sm2__git-skill-row--on' : ''}${s.conflict ? ' sm2__git-skill-row--conflict' : ''}`}
                >
                  <label className="sm2__git-skill-check">
                    <input type="checkbox" checked={checked} onChange={() => toggle(s.sourcePath)} />
                  </label>
                  <div className="sm2__git-skill-main" onClick={() => toggle(s.sourcePath)}>
                    <div className="sm2__git-skill-titleline">
                      <strong>{s.skillName}</strong>
                      {s.conflict && <span className="sm2__tag sm2__tag--conflict">中心库已存在</span>}
                    </div>
                    {s.description && <p className="sm2__git-skill-desc">{s.description}</p>}
                    <code className="sm2__git-skill-path">{s.sourcePath || repo.repo.repo}</code>
                  </div>
                  {checked && s.conflict && renderConflictControls(s, mode)}
                </div>
              )
            })}
          </div>
        )}

        {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
        <div className="sm2__git-footer">
          <button className="sm2__btn sm2__btn--ghost" onClick={resetToInput} disabled={busy}>← 重新输入</button>
          <button className="sm2__btn sm2__btn--primary" onClick={installSelected} disabled={busy || selectedCount === 0}>
            {busy ? '安装中…' : `安装所选 (${selectedCount})`}
          </button>
        </div>
      </div>
    )
  }

  // ── input ──────────────────────────────────────────────────────
  return (
    <div className="sm2__git">
      <div className="sm2__git-intro">
        <h3 className="sm2__install-h">从 Git 仓库导入 Skill</h3>
        <p className="sm2__install-sub">
          粘贴仓库地址检测其中的 Skill。给整个仓库会列出全部供你勾选；给具体 Skill 路径
          （如 <code>.../tree/main/skills/pdf</code>）则直接安装。
        </p>
      </div>

      <div className="sm2__git-input-card">
        <div className="sm2__git-url-field">
          <span className="sm2__git-url-icon">⑂</span>
          <input
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://github.com/anthropics/skills/tree/main"
            onKeyDown={(e) => e.key === 'Enter' && !busy && detect()}
          />
        </div>
        <div className="sm2__git-input-row">
          <label className="sm2__git-branch-field">
            <span>分支</span>
            <input value={branch} onChange={(e) => setBranch(e.target.value)} placeholder="main" />
          </label>
          <button
            className="sm2__git-advanced-toggle"
            onClick={() => setShowAdvanced((v) => !v)}
            type="button"
          >
            {showAdvanced ? '收起高级' : '高级（私有仓库令牌）'}
          </button>
        </div>
        {showAdvanced && (
          <label className="sm2__git-token-field">
            <span>访问令牌</span>
            <input value={token} onChange={(e) => setToken(e.target.value)} type="password" placeholder="ghp_..." />
          </label>
        )}
      </div>

      {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
      <div className="sm2__btn-row">
        <button className="sm2__btn sm2__btn--primary" onClick={detect} disabled={busy}>
          {busy ? '检测中…' : '检测 Skill'}
        </button>
      </div>
    </div>
  )
}

function GitOwnerAvatar({ owner }: { owner: string }) {
  const [failed, setFailed] = useState(false)
  if (!owner || failed) {
    return <div className="sm2__market-skill-icon">{initials(owner || 'git')}</div>
  }
  return (
    <img
      src={`https://github.com/${owner}.png?size=80`}
      alt={owner}
      className="sm2__market-avatar"
      loading="lazy"
      onError={() => setFailed(true)}
    />
  )
}
