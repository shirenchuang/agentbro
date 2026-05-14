import { useCallback, useMemo, useState } from 'react'
import { useSkillStore } from '../../stores/skillStore'
import { skillApi, type SkillPack } from '../../services/skillApi'
import { PackDialog } from './PackDialog'
import { displayVersionValue } from '../../utils/versions'

type PluginFilter = 'all' | 'plugin' | 'pack' | 'mcp'

const filters: { id: PluginFilter; label: string }[] = [
  { id: 'all', label: '全部' },
  { id: 'plugin', label: '插件' },
  { id: 'pack', label: '技能包' },
  { id: 'mcp', label: 'MCP 服务' },
]

export function PluginListView() {
  const { skills, packs, loadAll, selectSkill } = useSkillStore()
  const [query, setQuery] = useState('')
  const [filter, setFilter] = useState<PluginFilter>('all')
  const [editingPack, setEditingPack] = useState<SkillPack | undefined>()
  const [dialogOpen, setDialogOpen] = useState(false)

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
            className="skills-btn skills-btn--primary"
            onClick={() => { setEditingPack(undefined); setDialogOpen(true) }}
          >
            + 新建技能包
          </button>
        </div>

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
      </div>
    </div>
  )
}
