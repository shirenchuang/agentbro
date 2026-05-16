import { useEffect, useMemo, useState } from 'react'
import { useAgentStore } from '../../stores/agentStore'
import { useSkillStore } from '../../stores/skillStore'
import { skillApi, type DiscoveredSkill, type ObsidianVault } from '../../services/skillApi'
import { detectedAgentOptions } from '../../utils/agentPrograms'

export function ObsidianView() {
  const { obsidianVaults, loadAll, loadObsidianVaults } = useSkillStore()
  const { agents, loadAgents } = useAgentStore()
  const [selectedVault, setSelectedVault] = useState<ObsidianVault | null>(null)
  const [skills, setSkills] = useState<DiscoveredSkill[]>([])
  const [targetAgent, setTargetAgent] = useState('')
  const [loadingSkills, setLoadingSkills] = useState(false)
  const [installingId, setInstallingId] = useState('')
  const [message, setMessage] = useState('')
  const targetAgents = useMemo(() => {
    const installed = detectedAgentOptions(agents)
    return installed.length > 0 ? installed : agents
  }, [agents])

  useEffect(() => {
    loadObsidianVaults()
    if (agents.length === 0) loadAgents()
  }, [agents.length, loadAgents, loadObsidianVaults])

  useEffect(() => {
    if (!targetAgent && targetAgents.length > 0) setTargetAgent(targetAgents[0].id)
  }, [targetAgent, targetAgents])

  const openVault = async (vault: ObsidianVault) => {
    setSelectedVault(vault)
    setLoadingSkills(true)
    setMessage('')
    try {
      setSkills(await skillApi.getObsidianVaultSkills(vault.path))
    } catch (error) {
      setMessage(String(error))
    } finally {
      setLoadingSkills(false)
    }
  }

  const installSkill = async (skill: DiscoveredSkill) => {
    if (!targetAgent) return
    setInstallingId(skill.id)
    setMessage('')
    try {
      await skillApi.install(skill.dirPath, [{ agent: targetAgent, installMode: 'symlink' }], 'symlink')
      await loadAll()
      setMessage(`已导入 ${skill.name}。`)
    } catch (error) {
      setMessage(String(error))
    } finally {
      setInstallingId('')
    }
  }

  return (
    <div className="capability-page">
      <div className="capability-page-head">
        <h1>📝 Obsidian Vaults</h1>
        <p>扫描带有 .obsidian 的知识库，并把 vault 内的项目级 Skills 导入到目标 Agent。</p>
      </div>

      <div className="capability-page-body obsidian-view">
        <div className="plugin-manager-toolbar">
          <select className="sync-agent-select" value={targetAgent} onChange={event => setTargetAgent(event.target.value)}>
            {targetAgents.map(agent => <option key={agent.id} value={agent.id}>{agent.displayName}</option>)}
          </select>
          <button className="skills-btn skills-btn--small" onClick={loadObsidianVaults}>重新扫描 Vault</button>
        </div>

        {message && <div className="sync-status">{message}</div>}

        <div className="obsidian-grid">
          <div className="obsidian-vault-list">
            {obsidianVaults.map(vault => (
              <button
                key={vault.id}
                type="button"
                className={`obsidian-vault-card ${selectedVault?.id === vault.id ? 'obsidian-vault-card--active' : ''}`}
                onClick={() => openVault(vault)}
              >
                <strong>{vault.name}</strong>
                <span>{vault.skillCount} Skills</span>
                <code>{vault.path}</code>
              </button>
            ))}
            {obsidianVaults.length === 0 && (
              <div className="skills-empty">
                <div className="skills-empty__icon">📝</div>
                <div className="skills-empty__text">没有发现 Obsidian Vault</div>
                <div className="skills-empty__hint">在“项目发现”里添加包含 vault 的扫描根目录后再扫描。</div>
              </div>
            )}
          </div>

          <div className="obsidian-skill-list">
            {selectedVault && (
              <div className="skills-group-label">{selectedVault.name}</div>
            )}
            {loadingSkills && <div className="skills-scanning"><div className="skills-spinner" />扫描中...</div>}
            {!loadingSkills && skills.map(skill => (
              <div key={skill.id} className="discover-card">
                <div>
                  <div className="discover-card__title">{skill.name}</div>
                  <div className="discover-card__desc">{skill.description || '暂无描述'}</div>
                  <div className="discover-card__meta">
                    <span>{skill.sourceKind}</span>
                    <code>{skill.dirPath}</code>
                  </div>
                </div>
                <button
                  type="button"
                  className="skills-btn skills-btn--small"
                  disabled={!targetAgent || installingId === skill.id}
                  onClick={() => installSkill(skill)}
                >
                  {installingId === skill.id ? '导入中...' : '导入'}
                </button>
              </div>
            ))}
            {!loadingSkills && selectedVault && skills.length === 0 && (
              <div className="skills-empty">
                <div className="skills-empty__icon">🧩</div>
                <div className="skills-empty__text">这个 Vault 里没有项目 Skill</div>
                <div className="skills-empty__hint">支持 .skills、.agents/skills 和 .claude/skills。</div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
