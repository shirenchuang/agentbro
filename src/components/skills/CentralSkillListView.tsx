import { useMemo, useState } from 'react'
import { useSkillStore } from '../../stores/skillStore'
import { SkillCard } from './SkillCard'
import { InstallDialog } from './InstallDialog'

function isCentralSkill(skill: { source: string; agents: { agent: string; installPath: string }[]; filePath: string }) {
  return (
    skill.source === 'island' ||
    skill.agents.some((agent) => agent.agent === 'central' || agent.installPath.includes('/.agentbro/skills/')) ||
    skill.filePath.includes('/.agentbro/skills/')
  )
}

export function CentralSkillListView() {
  const { skills, scanning, loadAll } = useSkillStore()
  const [query, setQuery] = useState('')
  const [installOpen, setInstallOpen] = useState(false)

  const centralSkills = useMemo(() => {
    const q = query.trim().toLowerCase()
    return skills
      .filter(isCentralSkill)
      .filter((skill) => !q || skill.name.toLowerCase().includes(q) || skill.description.toLowerCase().includes(q))
  }, [query, skills])

  const linkedCount = useMemo(
    () => centralSkills.filter((skill) => skill.agents.some((agent) => agent.agent !== 'central')).length,
    [centralSkills]
  )
  const pluginCount = useMemo(
    () => centralSkills.filter((skill) => skill.skillType === 'plugin').length,
    [centralSkills]
  )
  const mcpCount = useMemo(
    () => centralSkills.filter((skill) => skill.skillType === 'mcp').length,
    [centralSkills]
  )

  return (
    <div className="capability-page">
      <div className="capability-page-head">
        <h1>▣ 中央技能库</h1>
        <p>管理 AgentBro canonical 目录 ~/.agentbro/skills，并从这里分发到不同 Agent。</p>
      </div>

      <div className="capability-page-body skills-global">
        <div className="skills-global-stats">
          <div><strong>{centralSkills.length}</strong><span>中央条目</span></div>
          <div><strong>{linkedCount}</strong><span>已分发</span></div>
          <div><strong>{pluginCount}</strong><span>插件</span></div>
          <div><strong>{mcpCount}</strong><span>MCP 服务</span></div>
          <div><strong>{centralSkills.filter((skill) => skill.hasUpdate).length}</strong><span>可更新</span></div>
        </div>

        <div className="skills-toolbar">
          <input
            className="skills-search"
            placeholder="搜索中央技能库..."
            value={query}
            onChange={event => setQuery(event.target.value)}
          />
          <button className="skills-btn skills-btn--primary skills-btn--small" onClick={() => setInstallOpen(true)}>
            + 安装到中央库
          </button>
        </div>

        {scanning ? (
          <div className="skills-scanning">
            <div className="skills-spinner" />
            正在扫描中央技能库...
          </div>
        ) : centralSkills.length === 0 ? (
          <div className="skills-empty">
            <div className="skills-empty__icon">▣</div>
            <div className="skills-empty__text">中央技能库为空</div>
            <div className="skills-empty__hint">安装时选择 symlink 模式，或把 Skill 放入 ~/.agentbro/skills。</div>
          </div>
        ) : (
          <div className="skills-list skills-list--grid">
            {centralSkills.map(skill => <SkillCard key={skill.id} skill={skill} onRefresh={loadAll} />)}
          </div>
        )}

        {installOpen && (
          <InstallDialog onClose={() => { setInstallOpen(false); loadAll() }} />
        )}
      </div>
    </div>
  )
}
