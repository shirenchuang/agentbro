import { useMemo, useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useSkillStore } from '../../stores/skillStore'
import { useAgentStore } from '../../stores/agentStore'
import { skillApi } from '../../services/skillApi'
import { SkillCard } from './SkillCard'
import { InstallDialog } from './InstallDialog'
import { agentMatchesId, detectedAgentOptions } from '../../utils/agentPrograms'

function skillMatchesAgent(skill: { agents: { agent: string }[] }, agentFilter: string) {
  if (agentFilter === 'all') return true
  return skill.agents.some(a => agentMatchesId(a.agent, agentFilter))
}

export function SkillListView() {
  const { t } = useTranslation()
  const {
    skills, packs, scanning, searchQuery, typeFilter, agentFilter,
    setSearchQuery, setTypeFilter, setAgentFilter,
    loadAll, batchMode, toggleBatchMode, batchSelected, clearBatch,
  } = useSkillStore()
  const { agents, loadAgents } = useAgentStore()
  const [installOpen, setInstallOpen] = useState(false)
  const [batchPackId, setBatchPackId] = useState('')
  const detectedAgents = useMemo(() => detectedAgentOptions(agents), [agents])

  useEffect(() => {
    if (agents.length === 0) loadAgents()
  }, [agents.length, loadAgents])

  const filtered = useMemo(() => {
    let list = skills
    if (searchQuery) {
      const q = searchQuery.toLowerCase()
      list = list.filter(s => s.name.toLowerCase().includes(q) || s.description.toLowerCase().includes(q))
    }
    if (typeFilter !== 'all') {
      list = list.filter(s => s.skillType === typeFilter)
    }
    if (agentFilter !== 'all') {
      list = list.filter(s => skillMatchesAgent(s, agentFilter))
    }
    return list
  }, [skills, searchQuery, typeFilter, agentFilter])

  const installed = useMemo(() => filtered.filter(s => s.source === 'island'), [filtered])
  const discovered = useMemo(() => filtered.filter(s => s.source === 'local'), [filtered])
  const mcpCount = useMemo(() => skills.filter(s => s.skillType === 'mcp').length, [skills])
  const pluginCount = useMemo(() => skills.filter(s => s.skillType === 'plugin').length, [skills])
  const updateCount = useMemo(() => skills.filter(s => s.hasUpdate).length, [skills])

  const handleBatchToggle = useCallback(async (enabled: boolean) => {
    for (const id of batchSelected) {
      const skill = skills.find(s => s.id === id)
      if (skill) {
        for (const agent of skill.agents) {
          await skillApi.toggle(id, agent.agent, enabled)
        }
      }
    }
    clearBatch()
    loadAll()
  }, [batchSelected, skills, clearBatch, loadAll])

  const handleBatchAddToPack = useCallback(async () => {
    if (!batchPackId || batchSelected.size === 0) return
    const pack = packs.find(p => p.id === batchPackId)
    if (!pack) return
    await skillApi.updatePack({
      ...pack,
      skills: Array.from(new Set([...pack.skills, ...Array.from(batchSelected)])),
    })
    clearBatch()
    setBatchPackId('')
    loadAll()
  }, [batchPackId, batchSelected, clearBatch, loadAll, packs])

  const handleBatchUninstall = useCallback(async () => {
    if (batchSelected.size === 0) return
    const confirmed = window.confirm(`确认卸载选中的 ${batchSelected.size} 个 Skill？这个操作会删除它们当前的安装位置。`)
    if (!confirmed) return
    for (const id of batchSelected) {
      const skill = skills.find(s => s.id === id)
      if (skill?.filePath) {
        await skillApi.uninstall(skill.filePath)
      }
    }
    clearBatch()
    loadAll()
  }, [batchSelected, clearBatch, loadAll, skills])

  if (scanning) {
    return (
      <div className="capability-page">
        <div className="capability-page-head">
          <h1>🧩 全部 Skills</h1>
          <p>统一查看本机发现和通过 AgentBro 安装的 Skills。</p>
        </div>
        <div className="capability-page-body">
          <div className="skills-scanning">
            <div className="skills-spinner" />
            {t('skills.scanning')}
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="capability-page">
      <div className="capability-page-head">
        <h1>🧩 全部 Skills</h1>
        <p>统一查看本机发现和通过 AgentBro 安装的 Skills，支持按类型和 Agent 过滤。</p>
      </div>

      <div className="capability-page-body skills-global">
        <div className="skills-global-stats">
          <div><strong>{skills.length}</strong><span>全部 Skills</span></div>
          <div><strong>{installed.length}</strong><span>AgentBro 安装</span></div>
          <div><strong>{pluginCount}</strong><span>插件</span></div>
          <div><strong>{mcpCount}</strong><span>MCP 服务</span></div>
          <div><strong>{updateCount}</strong><span>可更新</span></div>
        </div>

        <div className="skills-toolbar">
          <input
            className="skills-search"
            placeholder={t('skills.searchPlaceholder')}
            value={searchQuery}
            onChange={e => setSearchQuery(e.target.value)}
          />
          <button className="skills-btn skills-btn--small" onClick={toggleBatchMode}>
            {batchMode ? t('skills.cancelBatch') : t('skills.batchMode')}
          </button>
          <button className="skills-btn skills-btn--primary skills-btn--small" onClick={() => setInstallOpen(true)}>
            + 安装 Skill
          </button>
        </div>

        <div className="skills-filter-chips">
          {(['all', 'skill', 'plugin', 'mcp'] as const).map(f => (
            <button
              key={f}
              className={`skills-chip ${typeFilter === f ? 'skills-chip--active' : ''}`}
              onClick={() => setTypeFilter(f)}
            >
              {f === 'all' ? '全部类型' : f === 'skill' ? 'Skills' : f === 'plugin' ? '插件' : 'MCP'}
            </button>
          ))}
          <span className="skills-filter-spacer" />
          <button
            className={`skills-chip ${agentFilter === 'all' ? 'skills-chip--active' : ''}`}
            onClick={() => setAgentFilter('all')}
          >
            全部 Agent
          </button>
          {detectedAgents.map(a => (
            <button
              key={a.id}
              className={`skills-chip ${agentFilter === a.id ? 'skills-chip--active' : ''}`}
              onClick={() => setAgentFilter(a.id)}
            >
              {a.displayName}
            </button>
          ))}
        </div>

        {batchMode && batchSelected.size > 0 && (
          <div className="skills-batch-bar">
            <span className="skills-batch-bar__count">
              {t('skills.selectedCount', { count: batchSelected.size })}
            </span>
            <button className="skills-btn skills-btn--small" onClick={() => handleBatchToggle(true)}>
              {t('skills.enableAll')}
            </button>
            <button className="skills-btn skills-btn--small" onClick={() => handleBatchToggle(false)}>
              {t('skills.disableAll')}
            </button>
            {packs.length > 0 && (
              <>
                <select
                  className="skills-batch-select"
                  value={batchPackId}
                  onChange={(event) => setBatchPackId(event.target.value)}
                >
                  <option value="">加入技能包...</option>
                  {packs.map(pack => <option key={pack.id} value={pack.id}>{pack.name}</option>)}
                </select>
                <button className="skills-btn skills-btn--small" onClick={handleBatchAddToPack} disabled={!batchPackId}>
                  添加
                </button>
              </>
            )}
            <button className="skills-btn skills-btn--small skills-btn--danger" onClick={handleBatchUninstall}>
              批量卸载
            </button>
          </div>
        )}

        {filtered.length === 0 ? (
          <div className="skills-empty">
            <div className="skills-empty__icon">📦</div>
            <div className="skills-empty__text">{t('skills.noSkills')}</div>
            <div className="skills-empty__hint">{t('skills.noSkillsHint')}</div>
          </div>
        ) : (
          <>
            {installed.length > 0 && (
              <>
                <div className="skills-group-label">AgentBro 安装</div>
                <div className="skills-list skills-list--grid">
                  {installed.map(s => <SkillCard key={s.id} skill={s} onRefresh={loadAll} />)}
                </div>
              </>
            )}
            {discovered.length > 0 && (
              <>
                <div className="skills-group-label">本地发现</div>
                <div className="skills-list skills-list--grid">
                  {discovered.map(s => <SkillCard key={s.id} skill={s} onRefresh={loadAll} />)}
                </div>
              </>
            )}
          </>
        )}

        {installOpen && (
          <InstallDialog onClose={() => { setInstallOpen(false); loadAll() }} />
        )}
      </div>
    </div>
  )
}
