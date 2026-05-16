import { useMemo, useState } from 'react'
import { useSkillStore } from '../../stores/skillStore'
import { skillApi } from '../../services/skillApi'
import { SkillCard } from './SkillCard'
import { InstallDialog } from './InstallDialog'

function isCentralSkill(skill: { source: string; agents: { agent: string; installPath: string }[]; filePath: string }) {
  return (
    skill.source === 'island' ||
    skill.agents.some((agent) => agent.agent === 'central' || agent.installPath.includes('/.agents/skills/') || agent.installPath.includes('/.agentbro/skills/')) ||
    skill.filePath.includes('/.agents/skills/') ||
    skill.filePath.includes('/.agentbro/skills/')
  )
}

export function CentralSkillListView() {
  const { skills, scanning, loadAll } = useSkillStore()
  const [query, setQuery] = useState('')
  const [bundleFilter, setBundleFilter] = useState('')
  const [installOpen, setInstallOpen] = useState(false)
  const [deletingBundle, setDeletingBundle] = useState('')
  const [message, setMessage] = useState('')

  const centralSkills = useMemo(() => {
    const q = query.trim().toLowerCase()
    return skills
      .filter(isCentralSkill)
      .filter((skill) => !bundleFilter || centralBundleName(skill) === bundleFilter)
      .filter((skill) => !q || skill.name.toLowerCase().includes(q) || skill.description.toLowerCase().includes(q))
  }, [bundleFilter, query, skills])

  const bundles = useMemo(() => {
    const grouped = new Map<string, { name: string; count: number; linked: number; path: string }>()
    for (const skill of skills.filter(isCentralSkill)) {
      const name = centralBundleName(skill)
      const current = grouped.get(name) ?? { name, count: 0, linked: 0, path: centralBundlePath(skill) }
      current.count += 1
      if (skill.agents.some((agent) => agent.agent !== 'central')) current.linked += 1
      grouped.set(name, current)
    }
    return Array.from(grouped.values()).sort((a, b) => a.name.localeCompare(b.name))
  }, [skills])

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

  const deleteBundle = async (bundleName: string) => {
    setDeletingBundle(bundleName)
    setMessage('')
    try {
      const preview = await skillApi.previewDeleteCentralSkillBundle(bundleName)
      const removeLinked = preview.linkedInstallPaths.length > 0
        ? window.confirm(`这个 Bundle 还有 ${preview.linkedInstallPaths.length} 个 Agent 分发位置。是否一并删除这些分发位置？`)
        : false
      const confirmed = window.confirm(`确认删除 ${bundleName}？将删除 ${preview.removablePaths.length} 个中央库路径。`)
      if (!confirmed) return
      await skillApi.deleteCentralSkillBundle(bundleName, removeLinked)
      setBundleFilter('')
      await loadAll()
      setMessage(`已删除 ${bundleName}。`)
    } catch (error) {
      setMessage(String(error))
    } finally {
      setDeletingBundle('')
    }
  }

  return (
    <div className="capability-page">
      <div className="capability-page-head">
        <h1>▣ 中央技能库</h1>
        <p>管理 canonical 目录 ~/.agents/skills，并兼容旧的 ~/.agentbro/skills，从这里分发到不同 Agent。</p>
      </div>

      <div className="capability-page-body skills-global">
        <div className="skills-global-stats">
          <div><strong>{centralSkills.length}</strong><span>中央条目</span></div>
          <div><strong>{linkedCount}</strong><span>已分发</span></div>
          <div><strong>{pluginCount}</strong><span>插件</span></div>
          <div><strong>{mcpCount}</strong><span>MCP 服务</span></div>
          <div><strong>{centralSkills.filter((skill) => skill.hasUpdate).length}</strong><span>可更新</span></div>
        </div>

        {message && <div className="sync-status">{message}</div>}

        <div className="skills-toolbar">
          <input
            className="skills-search"
            placeholder="搜索中央技能库..."
            value={query}
            onChange={event => setQuery(event.target.value)}
          />
          {bundleFilter && (
            <button className="skills-btn skills-btn--small" onClick={() => setBundleFilter('')}>
              清除分组
            </button>
          )}
          <button className="skills-btn skills-btn--primary skills-btn--small" onClick={() => setInstallOpen(true)}>
            + 安装到中央库
          </button>
        </div>

        {bundles.length > 0 && (
          <>
            <div className="skills-group-label">中央库目录 / Bundle</div>
            <div className="central-bundle-list">
              {bundles.map(bundle => (
                <div
                  key={bundle.name}
                  className={`central-bundle-card ${bundleFilter === bundle.name ? 'central-bundle-card--active' : ''}`}
                  title={bundle.path}
                >
                  <button
                    type="button"
                    className="central-bundle-card__main"
                    onClick={() => setBundleFilter(bundleFilter === bundle.name ? '' : bundle.name)}
                  >
                    <strong>{bundle.name}</strong>
                    <span>{bundle.count} Skills · {bundle.linked} 已分发</span>
                    <code>{bundle.path}</code>
                  </button>
                  <button
                    type="button"
                    className="skills-btn skills-btn--small skills-btn--danger"
                    disabled={deletingBundle === bundle.name}
                    onClick={() => deleteBundle(bundle.name)}
                  >
                    {deletingBundle === bundle.name ? '删除中...' : '删除'}
                  </button>
                </div>
              ))}
            </div>
          </>
        )}

        {scanning ? (
          <div className="skills-scanning">
            <div className="skills-spinner" />
            正在扫描中央技能库...
          </div>
        ) : centralSkills.length === 0 ? (
          <div className="skills-empty">
            <div className="skills-empty__icon">▣</div>
            <div className="skills-empty__text">中央技能库为空</div>
            <div className="skills-empty__hint">安装时选择 symlink 模式，或把 Skill 放入 ~/.agents/skills。</div>
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

function centralBundleName(skill: { filePath: string; agents: { installPath: string }[] }) {
  const path = centralPathForSkill(skill)
  const marker = path.includes('/.agents/skills/') ? '/.agents/skills/' : '/.agentbro/skills/'
  const rest = path.split(marker)[1] ?? path
  return rest.split('/').filter(Boolean)[0] ?? 'root'
}

function centralBundlePath(skill: { filePath: string; agents: { installPath: string }[] }) {
  const path = centralPathForSkill(skill)
  const marker = path.includes('/.agents/skills/') ? '/.agents/skills/' : '/.agentbro/skills/'
  const [root, rest = ''] = path.split(marker)
  const first = rest.split('/').filter(Boolean)[0]
  return first ? `${root}${marker}${first}` : path
}

function centralPathForSkill(skill: { filePath: string; agents: { installPath: string }[] }) {
  return skill.agents.find(agent => agent.installPath.includes('/.agents/skills/') || agent.installPath.includes('/.agentbro/skills/'))?.installPath ?? skill.filePath
}
