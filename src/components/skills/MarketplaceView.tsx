import { useEffect, useMemo, useState } from 'react'
import { InstallDialog } from './InstallDialog'
import { useAgentStore } from '../../stores/agentStore'
import { skillApi, type MarketplaceItem, type MarketplaceSource } from '../../services/skillApi'
import { detectedAgentOptions } from '../../utils/agentPrograms'
import { OFFICIAL_PUBLISHERS, RECOMMENDED_SKILLS } from '../../data/officialSources'

type MarketCategory = 'all' | 'skill' | 'plugin' | 'mcp'
type MarketItem = MarketplaceItem

const categories: { id: MarketCategory; label: string }[] = [
  { id: 'all', label: '全部' },
  { id: 'skill', label: 'Skills' },
  { id: 'plugin', label: '插件' },
  { id: 'mcp', label: 'MCP 服务' },
]

const officialSkillItems: MarketplaceItem[] = [
  ...RECOMMENDED_SKILLS.map((skill, index) => ({
    id: `official:${skill.publisher.toLowerCase()}:${skill.name}`,
    name: skill.name,
    description: skill.description,
    category: 'skill' as const,
    sourceType: 'url' as const,
    source: skill.downloadUrl,
    subPath: null,
    author: skill.publisher,
    accent: officialAccent(index),
    mcp: null,
    plugin: null,
  })),
  ...OFFICIAL_PUBLISHERS.flatMap((publisher, publisherIndex) =>
    publisher.repos.map((repo, repoIndex) => ({
      id: `official-repo:${repo.fullName}`,
      name: repo.fullName,
      description: repo.description ?? `${publisher.name} 官方 Skill 仓库，包含 ${repo.skillCount} 个能力条目。`,
      category: 'skill' as const,
      sourceType: 'github' as const,
      source: repo.fullName,
      subPath: null,
      author: publisher.name,
      accent: officialAccent(publisherIndex + repoIndex),
      mcp: null,
      plugin: null,
    }))
  ),
]

function officialAccent(index: number) {
  const palette = ['#1d1d1f', '#007aff', '#34c759', '#ff9500', '#5856d6', '#ff2d55', '#5ac8fa', '#af52de']
  return palette[index % palette.length]
}

function mergeMarketplaceItems(items: MarketplaceItem[]) {
  const merged = new Map<string, MarketplaceItem>()
  for (const item of [...items, ...officialSkillItems]) {
    if (!merged.has(item.id)) merged.set(item.id, item)
  }
  return Array.from(merged.values())
}

export function MarketplaceView() {
  const { agents, loadAgents } = useAgentStore()
  const [items, setItems] = useState<MarketplaceItem[]>([])
  const [sources, setSources] = useState<MarketplaceSource[]>([])
  const [marketMessage, setMarketMessage] = useState('')
  const [query, setQuery] = useState('')
  const [category, setCategory] = useState<MarketCategory>('all')
  const [installing, setInstalling] = useState<MarketItem | null>(null)
  const [mcpInstalling, setMcpInstalling] = useState<MarketItem | null>(null)
  const [pluginInstalling, setPluginInstalling] = useState<MarketItem | null>(null)
  const [mcpName, setMcpName] = useState('')
  const [mcpCommand, setMcpCommand] = useState('')
  const [mcpArgs, setMcpArgs] = useState('')
  const [mcpEnv, setMcpEnv] = useState('')
  const [mcpTargets, setMcpTargets] = useState<Set<string>>(new Set())
  const [mcpMessage, setMcpMessage] = useState('')
  const [pluginTargets, setPluginTargets] = useState<Set<string>>(new Set())
  const [pluginMessage, setPluginMessage] = useState('')
  const [sourceName, setSourceName] = useState('')
  const [sourceUrl, setSourceUrl] = useState('')
  const targetAgents = useMemo(() => detectedAgentOptions(agents), [agents])
  const pluginAgents = useMemo(() => targetAgents.filter(agent => agent.id === 'claude-code' || agent.id === 'codex'), [targetAgents])

  useEffect(() => {
    if (agents.length === 0) loadAgents()
  }, [agents.length, loadAgents])

  const loadMarketplace = async () => {
    try {
      const [marketItems, marketSources] = await Promise.all([
        skillApi.listMarketplaceItems(),
        skillApi.listMarketplaceSources(),
      ])
      setItems(mergeMarketplaceItems(marketItems))
      setSources(marketSources)
      setMarketMessage('')
    } catch (error) {
      setMarketMessage(String(error))
    }
  }

  useEffect(() => {
    let cancelled = false
    Promise.all([
      skillApi.listMarketplaceItems(),
      skillApi.listMarketplaceSources(),
    ])
      .then(([marketItems, marketSources]) => {
        if (cancelled) return
        setItems(mergeMarketplaceItems(marketItems))
        setSources(marketSources)
        setMarketMessage('')
      })
      .catch((error) => {
        if (!cancelled) setMarketMessage(String(error))
      })
    return () => {
      cancelled = true
    }
  }, [])

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    return items
      .filter((item) => category === 'all' || item.category === category)
      .filter((item) => !q || item.name.toLowerCase().includes(q) || item.description.toLowerCase().includes(q) || item.author.toLowerCase().includes(q))
  }, [category, items, query])

  const openMcpInstall = (item: MarketItem) => {
    setMcpInstalling(item)
    setMcpName(item.id)
    setMcpCommand(item.mcp?.command ?? '')
    setMcpArgs(item.mcp?.args.join(' ') ?? '')
    setMcpEnv(Object.entries(item.mcp?.env ?? {}).map(([key, value]) => `${key}=${value}`).join('\n'))
    setMcpTargets(new Set(targetAgents.slice(0, 1).map(agent => agent.id)))
    setMcpMessage('')
  }

  const openPluginInstall = (item: MarketItem) => {
    setPluginInstalling(item)
    setPluginTargets(new Set(pluginAgents.slice(0, 1).map(agent => agent.id)))
    setPluginMessage('')
  }

  const toggleMcpTarget = (agentId: string) => {
    setMcpTargets(prev => {
      const next = new Set(prev)
      if (next.has(agentId)) next.delete(agentId)
      else next.add(agentId)
      return next
    })
  }

  const togglePluginTarget = (agentId: string) => {
    setPluginTargets(prev => {
      const next = new Set(prev)
      if (next.has(agentId)) next.delete(agentId)
      else next.add(agentId)
      return next
    })
  }

  const installMcp = async () => {
    if (!mcpName.trim() || !mcpCommand.trim() || mcpTargets.size === 0) return
    const args = mcpArgs.split(/\s+/).map(part => part.trim()).filter(Boolean)
    const env = parseEnvLines(mcpEnv)
    try {
      for (const agent of mcpTargets) {
        await skillApi.upsertMcpServer(agent, {
          name: mcpName.trim(),
          command: mcpCommand.trim(),
          args,
          env,
        })
      }
      setMcpMessage('MCP 已写入目标 Agent 配置。')
      setMcpInstalling(null)
    } catch (error) {
      setMcpMessage(String(error))
    }
  }

  const installPlugin = async () => {
    if (!pluginInstalling || pluginTargets.size === 0) return
    const source = pluginInstalling.sourceType === 'github'
      ? `github:${[pluginInstalling.source, pluginInstalling.subPath].filter(Boolean).join('/')}`
      : pluginInstalling.source
    try {
      for (const agent of pluginTargets) {
        await skillApi.installPlugin({ source, agent })
      }
      setPluginMessage('插件已安装到目标 Agent。')
      setPluginInstalling(null)
    } catch (error) {
      setPluginMessage(String(error))
    }
  }

  const addMarketplaceSource = async () => {
    if (!sourceName.trim() || !sourceUrl.trim()) return
    const id = sourceName.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '')
    try {
      await skillApi.upsertMarketplaceSource({
        id,
        name: sourceName.trim(),
        url: sourceUrl.trim(),
        enabled: true,
      })
      setSourceName('')
      setSourceUrl('')
      await loadMarketplace()
    } catch (error) {
      setMarketMessage(String(error))
    }
  }

  const removeMarketplaceSource = async (id: string) => {
    try {
      await skillApi.removeMarketplaceSource(id)
      await loadMarketplace()
    } catch (error) {
      setMarketMessage(String(error))
    }
  }

  const handleInstall = (item: MarketItem) => {
    if (item.category === 'mcp') {
      openMcpInstall(item)
    } else if (item.category === 'plugin') {
      openPluginInstall(item)
    } else {
      setInstalling(item)
    }
  }

  return (
    <div className="capability-page">
      <div className="capability-page-head">
        <h1>🏪 技能市场</h1>
        <p>发现并安装 Skills、插件和 MCP 服务。安装会走真实的 GitHub / URL 安装流程。</p>
      </div>

      <div className="capability-page-body market-view">
        <div className="market-hero">
          <div>
            <h3>可安装能力</h3>
            <p>包含本地市场源、MCP 服务、插件，以及 skills-manage 官方/推荐 Skill 仓库。</p>
          </div>
          <div className="market-hero__count">{filtered.length}</div>
        </div>

        <div className="market-sources">
          <div className="market-sources__form">
            <input value={sourceName} onChange={event => setSourceName(event.target.value)} placeholder="市场源名称" />
            <input value={sourceUrl} onChange={event => setSourceUrl(event.target.value)} placeholder="manifest URL 或本地 JSON 路径" />
            <button type="button" className="skills-btn skills-btn--small" onClick={addMarketplaceSource} disabled={!sourceName.trim() || !sourceUrl.trim()}>
              添加源
            </button>
          </div>
          {sources.length > 0 && (
            <div className="market-sources__list">
              {sources.map(source => (
                <span key={source.id}>
                  {source.name}
                  <button type="button" onClick={() => removeMarketplaceSource(source.id)}>移除</button>
                </span>
              ))}
            </div>
          )}
          {marketMessage && <div className="sync-status">{marketMessage}</div>}
        </div>

        <div className="plugin-manager-toolbar">
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="搜索市场..."
          />
        </div>

        <div className="mkt-tabs">
          {categories.map((item) => (
            <button
              key={item.id}
              type="button"
              className={`mkt-tab ${category === item.id ? 'active' : ''}`}
              onClick={() => setCategory(item.id)}
            >
              {item.label}
            </button>
          ))}
        </div>

        <div className="market-grid">
          {filtered.map((item) => (
            <div key={item.id} className="market-card">
              <div className="market-card__banner" style={{ background: `linear-gradient(135deg, ${item.accent}, color-mix(in srgb, ${item.accent} 58%, #fff))` }}>
                {item.category.toUpperCase()}
              </div>
              <div className="market-card__body">
                <div className="market-card__name">{item.name}</div>
                <div className="market-card__desc">{item.description}</div>
              </div>
              <div className="market-card__footer">
                <span>{item.author}</span>
                <button type="button" onClick={() => handleInstall(item)}>安装</button>
              </div>
            </div>
          ))}
        </div>

        {filtered.length === 0 && (
          <div className="skills-empty">
            <div className="skills-empty__icon">🏪</div>
            <div className="skills-empty__text">市场中没有匹配项</div>
            <div className="skills-empty__hint">换一个关键词或分类再试。</div>
          </div>
        )}

        {installing && (
          <InstallDialog
            initialSourceType={installing.sourceType}
            initialGithubRepo={installing.sourceType === 'github' ? installing.source : ''}
            initialGithubPath={installing.subPath ?? ''}
            initialUrl={installing.sourceType === 'url' ? installing.source : ''}
            onClose={() => setInstalling(null)}
          />
        )}

        {pluginInstalling && (
          <div className="skills-dialog-overlay" onClick={() => setPluginInstalling(null)}>
            <div className="skills-dialog" onClick={event => event.stopPropagation()}>
              <div className="skills-dialog__header">
                <div className="skills-dialog__title">安装插件</div>
              </div>
              <div className="skills-dialog__body">
                <div className="install-form-row">
                  <label className="install-form-label">插件</label>
                  <div className="install-form-static">{pluginInstalling.name}</div>
                </div>
                <div className="install-form-row">
                  <label className="install-form-label">目标 Agent</label>
                  <div className="install-targets">
                    {pluginAgents.map(agent => (
                      <button
                        key={agent.id}
                        type="button"
                        className={`install-target-chip ${pluginTargets.has(agent.id) ? 'install-target-chip--selected' : ''}`}
                        onClick={() => togglePluginTarget(agent.id)}
                      >
                        {agent.displayName}
                      </button>
                    ))}
                  </div>
                </div>
                {pluginMessage && <div className="sync-status">{pluginMessage}</div>}
              </div>
              <div className="skills-dialog__footer">
                <button className="skills-btn" onClick={() => setPluginInstalling(null)}>取消</button>
                <button className="skills-btn skills-btn--primary" onClick={installPlugin} disabled={pluginTargets.size === 0}>
                  安装插件
                </button>
              </div>
            </div>
          </div>
        )}

        {mcpInstalling && (
          <div className="skills-dialog-overlay" onClick={() => setMcpInstalling(null)}>
            <div className="skills-dialog" onClick={event => event.stopPropagation()}>
              <div className="skills-dialog__header">
                <div className="skills-dialog__title">安装 MCP Server</div>
              </div>
              <div className="skills-dialog__body">
                <div className="install-form-row">
                  <label className="install-form-label">名称</label>
                  <input className="install-form-input" value={mcpName} onChange={event => setMcpName(event.target.value)} />
                </div>
                <div className="install-form-row">
                  <label className="install-form-label">Command</label>
                  <input className="install-form-input" value={mcpCommand} onChange={event => setMcpCommand(event.target.value)} />
                </div>
                <div className="install-form-row">
                  <label className="install-form-label">Args</label>
                  <input className="install-form-input" value={mcpArgs} onChange={event => setMcpArgs(event.target.value)} />
                </div>
                <div className="install-form-row">
                  <label className="install-form-label">Env</label>
                  <textarea
                    className="install-form-input install-form-textarea"
                    value={mcpEnv}
                    onChange={event => setMcpEnv(event.target.value)}
                    placeholder="KEY=value，每行一个"
                  />
                </div>
                <div className="install-form-row">
                  <label className="install-form-label">目标 Agent</label>
                  <div className="install-targets">
                    {targetAgents.map(agent => (
                      <button
                        key={agent.id}
                        type="button"
                        className={`install-target-chip ${mcpTargets.has(agent.id) ? 'install-target-chip--selected' : ''}`}
                        onClick={() => toggleMcpTarget(agent.id)}
                      >
                        {agent.displayName}
                      </button>
                    ))}
                  </div>
                </div>
                {mcpMessage && <div className="sync-status">{mcpMessage}</div>}
              </div>
              <div className="skills-dialog__footer">
                <button className="skills-btn" onClick={() => setMcpInstalling(null)}>取消</button>
                <button className="skills-btn skills-btn--primary" onClick={installMcp} disabled={!mcpName.trim() || !mcpCommand.trim() || mcpTargets.size === 0}>
                  写入 MCP 配置
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

function parseEnvLines(value: string) {
  return value
    .split('\n')
    .map(line => line.trim())
    .filter(Boolean)
    .reduce<Record<string, string>>((env, line) => {
      const index = line.indexOf('=')
      if (index <= 0) return env
      env[line.slice(0, index).trim()] = line.slice(index + 1).trim()
      return env
    }, {})
}
