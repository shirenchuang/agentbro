import { useCallback, useEffect, useMemo, useState } from 'react'
import { useSkillStore } from '../../stores/skillStore'
import { skillApi, type SkillPack } from '../../services/skillApi'
import { PackDialog } from './PackDialog'
import { displayVersionValue } from '../../utils/versions'
import { useAgentStore } from '../../stores/agentStore'
import { detectedAgentOptions } from '../../utils/agentPrograms'

type PluginFilter = 'all' | 'plugin' | 'pack' | 'mcp'

const filters: { id: PluginFilter; label: string }[] = [
  { id: 'all', label: '全部' },
  { id: 'plugin', label: '插件' },
  { id: 'pack', label: '技能包' },
  { id: 'mcp', label: 'MCP 服务' },
]

export function PluginListView() {
  const { skills, packs, loadAll, selectSkill } = useSkillStore()
  const { agents, loadAgents } = useAgentStore()
  const [query, setQuery] = useState('')
  const [filter, setFilter] = useState<PluginFilter>('all')
  const [editingPack, setEditingPack] = useState<SkillPack | undefined>()
  const [dialogOpen, setDialogOpen] = useState(false)
  const [pluginDialogOpen, setPluginDialogOpen] = useState(false)
  const [mcpDialogOpen, setMcpDialogOpen] = useState(false)
  const [pluginSource, setPluginSource] = useState('')
  const [pluginAgent, setPluginAgent] = useState('claude-code')
  const [mcpName, setMcpName] = useState('')
  const [mcpCommand, setMcpCommand] = useState('')
  const [mcpArgs, setMcpArgs] = useState('')
  const [mcpEnv, setMcpEnv] = useState('')
  const [mcpTargets, setMcpTargets] = useState<Set<string>>(new Set())
  const [message, setMessage] = useState('')
  const [saving, setSaving] = useState(false)
  const targetAgents = useMemo(() => {
    const installed = detectedAgentOptions(agents)
    return installed.length > 0 ? installed : agents
  }, [agents])
  const pluginAgents = useMemo(() => {
    const supported = targetAgents.filter(agent => agent.id === 'claude-code' || agent.id === 'codex')
    return supported.length > 0 ? supported : targetAgents
  }, [targetAgents])

  useEffect(() => {
    if (agents.length === 0) loadAgents()
  }, [agents.length, loadAgents])

  useEffect(() => {
    if (!pluginAgents.some(agent => agent.id === pluginAgent) && pluginAgents[0]) {
      setPluginAgent(pluginAgents[0].id)
    }
  }, [pluginAgent, pluginAgents])

  useEffect(() => {
    if (mcpTargets.size > 0 || targetAgents.length === 0) return
    setMcpTargets(new Set([targetAgents[0].id]))
  }, [mcpTargets.size, targetAgents])

  const mcps = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (filter === 'pack' || filter === 'plugin') return []
    return skills
      .filter((skill) => skill.skillType === 'mcp')
      .filter((skill) => !q || skill.name.toLowerCase().includes(q) || skill.description.toLowerCase().includes(q))
  }, [filter, query, skills])

  const plugins = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (filter === 'pack' || filter === 'mcp') return []
    return skills
      .filter((skill) => skill.skillType === 'plugin')
      .filter((skill) => !q || skill.name.toLowerCase().includes(q) || skill.description.toLowerCase().includes(q))
  }, [filter, query, skills])

  const filteredPacks = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (filter === 'mcp' || filter === 'plugin') return []
    return packs.filter((pack) => !q || pack.name.toLowerCase().includes(q) || pack.description.toLowerCase().includes(q))
  }, [filter, packs, query])

  const handleApplyPack = useCallback(async (pack: SkillPack) => {
    await skillApi.applyPack(pack)
    loadAll()
  }, [loadAll])

  const handleDeletePack = useCallback(async (pack: SkillPack) => {
    await skillApi.deletePack(pack.id)
    loadAll()
  }, [loadAll])

  const handleInstallPlugin = useCallback(async () => {
    if (!pluginSource.trim() || !pluginAgent) return
    setSaving(true)
    setMessage('')
    try {
      await skillApi.installPlugin({ source: pluginSource.trim(), agent: pluginAgent })
      setPluginSource('')
      setPluginDialogOpen(false)
      await loadAll()
      setMessage('插件已安装。')
    } catch (error) {
      setMessage(String(error))
    } finally {
      setSaving(false)
    }
  }, [loadAll, pluginAgent, pluginSource])

  const toggleMcpTarget = useCallback((agentId: string) => {
    setMcpTargets(prev => {
      const next = new Set(prev)
      if (next.has(agentId)) next.delete(agentId)
      else next.add(agentId)
      return next
    })
  }, [])

  const handleAddMcp = useCallback(async () => {
    if (!mcpName.trim() || !mcpCommand.trim() || mcpTargets.size === 0) return
    setSaving(true)
    setMessage('')
    try {
      const server = {
        name: mcpName.trim(),
        command: mcpCommand.trim(),
        args: splitArgs(mcpArgs),
        env: parseEnvLines(mcpEnv),
      }
      for (const agent of mcpTargets) {
        await skillApi.upsertMcpServer(agent, server)
      }
      setMcpName('')
      setMcpCommand('')
      setMcpArgs('')
      setMcpEnv('')
      setMcpDialogOpen(false)
      await loadAll()
      setMessage('MCP 服务已写入目标 Agent 配置。')
    } catch (error) {
      setMessage(String(error))
    } finally {
      setSaving(false)
    }
  }, [loadAll, mcpArgs, mcpCommand, mcpEnv, mcpName, mcpTargets])

  const enabledMcpCount = mcps.filter((mcp) => mcp.agents.some((agent) => agent.enabled)).length

  return (
    <div className="capability-page">
      <div className="capability-page-head">
        <h1>🔌 插件与 MCP</h1>
        <p>管理技能包插件和 MCP 服务，点击卡片可查看或编辑详情。</p>
      </div>

      <div className="capability-page-body plugin-manager">
        <div className="plugin-manager-stats">
          <div><strong>{filteredPacks.length}</strong><span>技能包</span></div>
          <div><strong>{plugins.length}</strong><span>插件</span></div>
          <div><strong>{mcps.length}</strong><span>MCP 服务</span></div>
          <div><strong>{enabledMcpCount}</strong><span>已启用 MCP</span></div>
          <div><strong>{packs.reduce((total, pack) => total + pack.skills.length, 0)}</strong><span>技能包内 Skills</span></div>
        </div>

        <div className="plugin-manager-toolbar">
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="搜索插件、技能包、MCP..."
          />
          <button
            type="button"
            className="skills-btn"
            onClick={() => setPluginDialogOpen(true)}
          >
            + 安装插件
          </button>
          <button
            type="button"
            className="skills-btn"
            onClick={() => setMcpDialogOpen(true)}
          >
            + 添加 MCP
          </button>
          <button
            type="button"
            className="skills-btn skills-btn--primary"
            onClick={() => { setEditingPack(undefined); setDialogOpen(true) }}
          >
            + 新建技能包
          </button>
        </div>

        {message && <div className="sync-status">{message}</div>}

        <div className="mkt-tabs">
          {filters.map((item) => (
            <button
              key={item.id}
              type="button"
              className={`mkt-tab ${filter === item.id ? 'active' : ''}`}
              onClick={() => setFilter(item.id)}
            >
              {item.label}
            </button>
          ))}
        </div>

        {plugins.length > 0 && (
          <>
            <div className="skills-group-label">已安装插件</div>
            <div className="plugin-list">
              {plugins.map((plugin) => (
                <button key={plugin.id} type="button" className="plugin-card plugin-card--button" onClick={() => selectSkill(plugin.id)}>
                  <div className="plugin-card__icon">PL</div>
                  <div className="plugin-card__body">
                    <div className="plugin-card__name">{plugin.name}</div>
                    <div className="plugin-card__desc">{plugin.description || 'Claude Code 插件'}</div>
                    <div className="plugin-card__chips">
                      {plugin.agents.map((agent) => (
                        <span key={`${plugin.id}-${agent.agent}-${agent.installPath}-${agent.linkTarget ?? ''}`}>
                          {agent.agent}
                        </span>
                      ))}
                      {plugin.frontmatter.version && <span>{displayVersionValue(plugin.frontmatter.version)}</span>}
                    </div>
                  </div>
                  <span className="plugin-card__tag">Plugin</span>
                </button>
              ))}
            </div>
          </>
        )}

        {filteredPacks.length > 0 && (
          <>
            <div className="skills-group-label">技能包插件</div>
            <div className="plugin-list">
              {filteredPacks.map((pack) => (
                <div key={pack.id} className="plugin-card" onClick={() => { setEditingPack(pack); setDialogOpen(true) }}>
                  <div className="plugin-card__icon">PK</div>
                  <div className="plugin-card__body">
                    <div className="plugin-card__name">{pack.name}</div>
                    <div className="plugin-card__desc">{pack.description || '按场景批量安装和启用 Skills'}</div>
                    <div className="plugin-card__chips">
                      {pack.skills.slice(0, 10).map((skill) => <span key={skill}>Skill · {skill}</span>)}
                    </div>
                  </div>
                  <div className="plugin-card__actions" onClick={(event) => event.stopPropagation()}>
                    <button type="button" className="skills-btn skills-btn--small skills-btn--primary" onClick={() => handleApplyPack(pack)}>应用</button>
                    <button type="button" className="skills-btn skills-btn--small" onClick={() => { setEditingPack(pack); setDialogOpen(true) }}>编辑</button>
                    <button type="button" className="skills-btn skills-btn--small skills-btn--danger" onClick={() => handleDeletePack(pack)}>删除</button>
                  </div>
                </div>
              ))}
            </div>
          </>
        )}

        {mcps.length > 0 && (
          <>
            <div className="skills-group-label">MCP 服务</div>
            <div className="plugin-list">
              {mcps.map((mcp) => (
                <button key={mcp.id} type="button" className="plugin-card plugin-card--button" onClick={() => selectSkill(mcp.id)}>
                  <div className="plugin-card__icon plugin-card__icon--mcp">MC</div>
                  <div className="plugin-card__body">
                    <div className="plugin-card__name">{mcp.name}</div>
                    <div className="plugin-card__desc">{mcp.description || 'MCP Server'}</div>
                    <div className="plugin-card__chips">
                      {mcp.agents.map((agent) => (
                        <span key={`${mcp.id}-${agent.agent}-${agent.installPath}-${agent.linkTarget ?? ''}`}>
                          {agent.agent}
                        </span>
                      ))}
                    </div>
                  </div>
                  <span className="plugin-card__tag">MCP</span>
                </button>
              ))}
            </div>
          </>
        )}

        {plugins.length === 0 && filteredPacks.length === 0 && mcps.length === 0 && (
          <div className="skills-empty">
            <div className="skills-empty__icon">🔌</div>
            <div className="skills-empty__text">没有找到匹配的插件或 MCP</div>
            <div className="skills-empty__hint">可以新建技能包，或从市场安装包含 MCP 的能力包。</div>
          </div>
        )}

        {dialogOpen && (
          <PackDialog pack={editingPack} onClose={() => { setDialogOpen(false); setEditingPack(undefined) }} />
        )}

        {pluginDialogOpen && (
          <div className="skills-dialog-overlay" onClick={() => setPluginDialogOpen(false)}>
            <div className="skills-dialog" onClick={event => event.stopPropagation()}>
              <div className="skills-dialog__header">
                <div className="skills-dialog__title">安装插件</div>
              </div>
              <div className="skills-dialog__body">
                <div className="install-form-row">
                  <label className="install-form-label">来源</label>
                  <input
                    className="install-form-input"
                    value={pluginSource}
                    onChange={event => setPluginSource(event.target.value)}
                    placeholder="本地路径、GitHub owner/repo/path 或 URL"
                  />
                </div>
                <div className="install-form-row">
                  <label className="install-form-label">目标 Agent</label>
                  <select className="install-form-input" value={pluginAgent} onChange={event => setPluginAgent(event.target.value)}>
                    {pluginAgents.map(agent => <option key={agent.id} value={agent.id}>{agent.displayName}</option>)}
                  </select>
                </div>
              </div>
              <div className="skills-dialog__footer">
                <button className="skills-btn" onClick={() => setPluginDialogOpen(false)}>取消</button>
                <button className="skills-btn skills-btn--primary" disabled={saving || !pluginSource.trim() || !pluginAgent} onClick={handleInstallPlugin}>
                  {saving ? '安装中...' : '安装'}
                </button>
              </div>
            </div>
          </div>
        )}

        {mcpDialogOpen && (
          <div className="skills-dialog-overlay" onClick={() => setMcpDialogOpen(false)}>
            <div className="skills-dialog" onClick={event => event.stopPropagation()}>
              <div className="skills-dialog__header">
                <div className="skills-dialog__title">添加 MCP Server</div>
              </div>
              <div className="skills-dialog__body">
                <div className="install-form-row">
                  <label className="install-form-label">名称</label>
                  <input className="install-form-input" value={mcpName} onChange={event => setMcpName(event.target.value)} placeholder="github" />
                </div>
                <div className="install-form-row">
                  <label className="install-form-label">Command</label>
                  <input className="install-form-input" value={mcpCommand} onChange={event => setMcpCommand(event.target.value)} placeholder="npx / docker / uvx" />
                </div>
                <div className="install-form-row">
                  <label className="install-form-label">Args</label>
                  <input className="install-form-input" value={mcpArgs} onChange={event => setMcpArgs(event.target.value)} placeholder="-y @modelcontextprotocol/server-filesystem" />
                </div>
                <div className="install-form-row">
                  <label className="install-form-label">Env</label>
                  <textarea className="install-form-input install-form-textarea" value={mcpEnv} onChange={event => setMcpEnv(event.target.value)} placeholder="KEY=value，每行一个" />
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
              </div>
              <div className="skills-dialog__footer">
                <button className="skills-btn" onClick={() => setMcpDialogOpen(false)}>取消</button>
                <button className="skills-btn skills-btn--primary" disabled={saving || !mcpName.trim() || !mcpCommand.trim() || mcpTargets.size === 0} onClick={handleAddMcp}>
                  {saving ? '写入中...' : '写入配置'}
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

function splitArgs(value: string) {
  return value.split(/\s+/).map(part => part.trim()).filter(Boolean)
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
