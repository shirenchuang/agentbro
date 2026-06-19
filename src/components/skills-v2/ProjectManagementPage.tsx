import { useEffect, useMemo, useState } from 'react'
import { useSkillStoreV2 } from '../../stores/skillStoreV2'
import { skillApiV2 } from '../../services/skillApiV2'
import type { ProjectAgentDetail, ProjectDetail, ProjectSkillItem } from '../../services/skillApiV2'
import { AgentIconBadge } from './AgentIconBadge'

type ProjectTab = 'overview' | 'skills' | 'mcp' | 'plugins' | 'instructions' | 'config'

export function ProjectManagementPage() {
  const state = useSkillStoreV2()
  const [rootPath, setRootPath] = useState('')
  const [query, setQuery] = useState('')
  const [tab, setTab] = useState<ProjectTab>('overview')
  const [agentFilter, setAgentFilter] = useState('all')
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    state.loadOverview()
    state.loadProjects(true)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    if (state.selectedProjectId || state.projects.length === 0) return
    state.selectProject(state.projects[0].id)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state.projects, state.selectedProjectId])

  useEffect(() => {
    setTab('overview')
    setAgentFilter('all')
  }, [state.selectedProjectId])

  const filteredProjects = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return state.projects
    return state.projects.filter((project) =>
      [project.name, project.rootPath].join(' ').toLowerCase().includes(q),
    )
  }, [query, state.projects])

  const detail = state.selectedProjectDetail

  const addProject = async () => {
    const value = rootPath.trim()
    if (!value) return
    await state.addProject(value)
    setRootPath('')
  }

  const refreshProjects = async () => {
    setBusy(true)
    try {
      await state.loadProjects(true)
      if (state.selectedProjectId) await state.selectProject(state.selectedProjectId)
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="sm2 sm2--projects">
      <div className="sm2__header sm2__header--stacked">
        <div>
          <h2 className="sm2__title">项目</h2>
          <p className="sm2__header-subtitle">按项目查看 Claude / Codex 的 Skills、技能包、MCP、插件与指令文件。</p>
        </div>
        <div className="sm2__tabs">
          <button className="sm2__btn" disabled={busy || state.projectDetailLoading} onClick={refreshProjects}>
            {busy ? '刷新中…' : '刷新项目'}
          </button>
        </div>
      </div>

      {state.error && <div className="sm2__error">{state.error}</div>}

      <div className="sm2__project-layout">
        <aside className="sm2__project-sidebar settings-scroll">
          <div className="sm2__project-import">
            <label>导入项目</label>
            <div className="sm2__project-import-row">
              <input
                className="sm2__search sm2__search--full"
                value={rootPath}
                onChange={(event) => setRootPath(event.target.value)}
                placeholder="/Users/me/work/repo"
              />
              <button className="sm2__btn sm2__btn--primary" disabled={!rootPath.trim() || state.projectDetailLoading} onClick={addProject}>
                添加
              </button>
            </div>
          </div>

          <input
            className="sm2__search sm2__search--full"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="搜索项目名或路径"
          />

          {state.projects.length === 0 ? (
            <div className="sm2__project-empty">
              <strong>还没有导入项目</strong>
              <span>粘贴项目根目录后，AgentBro 会扫描项目级 Skills、MCP、插件和指令文件。</span>
            </div>
          ) : filteredProjects.length === 0 ? (
            <div className="sm2__empty sm2__empty--compact">没有匹配的项目</div>
          ) : (
            <div className="sm2__project-list">
              {filteredProjects.map((project) => (
                <button
                  key={project.id}
                  className={`sm2__project-item${state.selectedProjectId === project.id ? ' sm2__project-item--active' : ''}`}
                  onClick={() => state.selectProject(project.id)}
                >
                  <strong>{project.name}</strong>
                  <code>{project.rootPath}</code>
                  <span>
                    {project.detectedAgentCount} Agent · {project.skillCount} Skills · {project.mcpCount + project.pluginCount} MCP/插件
                  </span>
                </button>
              ))}
            </div>
          )}
        </aside>

        <main className="sm2__project-canvas settings-scroll">
          {!detail ? (
            <div className="sm2__empty">
              {state.projectDetailLoading ? '加载项目详情…' : '选择或导入一个项目'}
            </div>
          ) : (
            <ProjectDetailView
              detail={detail}
              tab={tab}
              onTab={setTab}
              agentFilter={agentFilter}
              onAgentFilter={setAgentFilter}
              busy={state.projectDetailLoading}
              onScan={() => state.scanProject(detail.id)}
              onRemove={() => state.removeProject(detail.id)}
            />
          )}
        </main>
      </div>
    </div>
  )
}

function ProjectDetailView({
  detail,
  tab,
  onTab,
  agentFilter,
  onAgentFilter,
  busy,
  onScan,
  onRemove,
}: {
  detail: ProjectDetail
  tab: ProjectTab
  onTab: (tab: ProjectTab) => void
  agentFilter: string
  onAgentFilter: (agentId: string) => void
  busy: boolean
  onScan: () => void
  onRemove: () => void
}) {
  const tabs: Array<{ id: ProjectTab; label: string }> = [
    { id: 'overview', label: '概览' },
    { id: 'skills', label: `Skills (${detail.skillCount})` },
    { id: 'mcp', label: `MCP (${detail.mcpCount})` },
    { id: 'plugins', label: `Plugins (${detail.pluginCount})` },
    { id: 'instructions', label: `指令 (${detail.instructionCount})` },
    { id: 'config', label: '路径' },
  ]
  const visibleAgents = agentFilter === 'all'
    ? detail.agents
    : detail.agents.filter((agent) => agent.agentId === agentFilter)

  return (
    <div className="sm2__project-workspace">
      <div className="sm2__project-hero">
        <div className="sm2__project-hero-main">
          <h3>{detail.name}</h3>
          <code>{detail.rootPath}</code>
          <span>{detail.lastScannedAt ? `上次扫描 ${formatDate(detail.lastScannedAt)}` : '尚未扫描'}</span>
        </div>
        <div className="sm2__project-agent-chips">
          <button className={agentFilter === 'all' ? 'active' : ''} onClick={() => onAgentFilter('all')}>
            全部
          </button>
          {detail.agents.map((agent) => (
            <button
              key={agent.agentId}
              className={agentFilter === agent.agentId ? 'active' : ''}
              onClick={() => onAgentFilter(agent.agentId)}
            >
              <AgentIconBadge iconKey={agent.iconKey} title={agent.displayName} size={18} />
              {agent.displayName}
            </button>
          ))}
        </div>
        <div className="sm2__btn-row sm2__project-actions">
          <button className="sm2__btn" disabled={busy} onClick={() => skillApiV2.openPath(detail.rootPath)}>打开</button>
          <button className="sm2__btn" disabled={busy} onClick={onScan}>{busy ? '扫描中…' : '重新扫描'}</button>
          <button
            className="sm2__btn sm2__btn--danger-ghost"
            disabled={busy}
            onClick={() => {
              if (window.confirm(`从列表移除项目 ${detail.name}？不会删除项目文件。`)) onRemove()
            }}
          >
            移除
          </button>
        </div>
      </div>

      {(detail.health.length > 0 || visibleAgents.some((agent) => agent.health.length > 0)) && (
        <div className="sm2__notice sm2__notice--warn">
          {[...detail.health, ...visibleAgents.flatMap((agent) => agent.health)].map((issue) => issue.message).join('；')}
        </div>
      )}

      <div className="sm2__project-stat-grid">
        <ProjectStat value={detail.detectedAgentCount} label="Agent" />
        <ProjectStat value={detail.skillCount} label="Skills" />
        <ProjectStat value={detail.mcpCount} label="MCP" />
        <ProjectStat value={detail.pluginCount} label="Plugins" />
        <ProjectStat value={detail.instructionCount} label="指令文件" />
      </div>

      <ProjectInstallPanel detail={detail} />

      <div className="sm2__subtabs">
        {tabs.map((item) => (
          <button
            key={item.id}
            className={`sm2__subtab${tab === item.id ? ' sm2__subtab--active' : ''}`}
            onClick={() => onTab(item.id)}
          >
            {item.label}
          </button>
        ))}
      </div>

      <div className="sm2__subtab-body">
        {tab === 'overview' && <ProjectOverview agents={visibleAgents} />}
        {tab === 'skills' && <ProjectSkills agents={visibleAgents} />}
        {tab === 'mcp' && <ProjectMcp agents={visibleAgents} />}
        {tab === 'plugins' && <ProjectPlugins agents={visibleAgents} />}
        {tab === 'instructions' && <ProjectInstructions detail={detail} agentFilter={agentFilter} />}
        {tab === 'config' && <ProjectConfig agents={visibleAgents} />}
      </div>
    </div>
  )
}

function ProjectInstallPanel({ detail }: { detail: ProjectDetail }) {
  const state = useSkillStoreV2()
  const agentChoices = useMemo(() => {
    if (detail.agents.length > 0) {
      return detail.agents.map((agent) => ({ agentId: agent.agentId, displayName: agent.displayName }))
    }
    return [
      { agentId: 'codex', displayName: 'Codex' },
      { agentId: 'claude-code', displayName: 'Claude Code' },
    ]
  }, [detail.agents])
  const [agentId, setAgentId] = useState(agentChoices[0]?.agentId ?? 'codex')
  const [skillId, setSkillId] = useState('')
  const [packId, setPackId] = useState('')
  const [mode, setMode] = useState<'link' | 'copy'>(state.settings?.defaultDistributeMode || 'link')
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    if (agentChoices.some((agent) => agent.agentId === agentId)) return
    setAgentId(agentChoices[0]?.agentId ?? 'codex')
  }, [agentChoices, agentId])

  useEffect(() => {
    if (!skillId && state.skills[0]) setSkillId(state.skills[0].id)
  }, [skillId, state.skills])

  useEffect(() => {
    if (!packId && state.packs[0]) setPackId(state.packs[0].id)
  }, [packId, state.packs])

  const installSkill = async () => {
    if (!skillId) return
    setBusy(true)
    state.setError(null)
    try {
      const next = await skillApiV2.installCenterSkillsToProject(detail.id, agentId, [skillId], mode)
      useSkillStoreV2.setState({ selectedProjectDetail: next })
      await state.loadProjects(true)
    } catch (e) {
      state.setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  const applyPack = async () => {
    if (!packId) return
    setBusy(true)
    state.setError(null)
    try {
      const next = await skillApiV2.installSkillPackToProject(detail.id, agentId, packId, mode)
      useSkillStoreV2.setState({ selectedProjectDetail: next })
      await state.loadProjects(true)
    } catch (e) {
      state.setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <section className="sm2__project-install-panel">
      <div className="sm2__project-install-controls">
        <label>
          <span>目标 Agent</span>
          <select className="sm2__select" value={agentId} onChange={(event) => setAgentId(event.target.value)}>
            {agentChoices.map((agent) => (
              <option key={agent.agentId} value={agent.agentId}>{agent.displayName}</option>
            ))}
          </select>
        </label>
        <label>
          <span>模式</span>
          <select className="sm2__select" value={mode} onChange={(event) => setMode(event.target.value as 'link' | 'copy')}>
            <option value="link">链接</option>
            <option value="copy">复制</option>
          </select>
        </label>
        <label>
          <span>中心库 Skill</span>
          <select className="sm2__select" value={skillId} onChange={(event) => setSkillId(event.target.value)} disabled={state.skills.length === 0}>
            {state.skills.length === 0 ? (
              <option value="">中心库为空</option>
            ) : state.skills.map((skill) => (
              <option key={skill.id} value={skill.id}>{skill.name || skill.id}</option>
            ))}
          </select>
        </label>
        <button className="sm2__btn sm2__btn--primary" disabled={busy || !skillId} onClick={installSkill}>
          安装 Skill
        </button>
        <label>
          <span>技能包</span>
          <select className="sm2__select" value={packId} onChange={(event) => setPackId(event.target.value)} disabled={state.packs.length === 0}>
            {state.packs.length === 0 ? (
              <option value="">暂无技能包</option>
            ) : state.packs.map((pack) => (
              <option key={pack.id} value={pack.id}>{pack.name}</option>
            ))}
          </select>
        </label>
        <button className="sm2__btn" disabled={busy || !packId} onClick={applyPack}>
          应用技能包
        </button>
      </div>
    </section>
  )
}

function ProjectOverview({ agents }: { agents: ProjectAgentDetail[] }) {
  if (agents.length === 0) return <div className="sm2__empty sm2__empty--compact">没有检测到项目级 Agent 配置</div>
  return (
    <section className="sm2__panel sm2__project-matrix">
      <div className="sm2__project-matrix-head">
        <span>Agent</span>
        <span>Skills</span>
        <span>MCP</span>
        <span>Plugins</span>
        <span>配置</span>
      </div>
      {agents.map((agent) => (
        <div key={agent.agentId} className="sm2__project-matrix-row">
          <strong><AgentIconBadge iconKey={agent.iconKey} title={agent.displayName} size={22} />{agent.displayName}</strong>
          <span>{agent.skills.length}</span>
          <span>{agent.mcpServers.length}</span>
          <span>{agent.plugins.length}</span>
          <code>{agent.configPaths[0] || agent.skillsDirs[0] || '未检测'}</code>
        </div>
      ))}
    </section>
  )
}

function ProjectSkills({ agents }: { agents: ProjectAgentDetail[] }) {
  const state = useSkillStoreV2()
  const [importing, setImporting] = useState<string | null>(null)
  const skills = agents.flatMap((agent) => agent.skills.map((skill) => ({ agent, skill })))

  const importSkill = async (agent: ProjectAgentDetail, skill: ProjectSkillItem) => {
    setImporting(`${agent.agentId}:${skill.path}`)
    state.setError(null)
    try {
      const input = {
        sourcePath: skill.path,
        sourceType: 'project_import',
        importedFromAgent: agent.agentId,
        importedFromPath: skill.path,
        importMode: 'copy' as const,
      }
      const preview = await skillApiV2.previewAddCenterSkill(input)
      if (preview.blockers.length > 0) {
        state.setError(preview.blockers.map((blocker) => blocker.reason || `${blocker.proposedSkillId} 无法导入`).join('；'))
        return
      }
      await skillApiV2.executeAddCenterSkill(input, preview.candidates.map((candidate) => ({
        skillId: candidate.skillId,
        proposedSkillId: candidate.proposedSkillId,
        resolution: candidate.action === 'update' ? 'update' : 'create',
      })))
      await state.loadOverview(true)
      if (state.selectedProjectId) await state.scanProject(state.selectedProjectId)
    } catch (e) {
      state.setError(String(e))
    } finally {
      setImporting(null)
    }
  }

  if (skills.length === 0) return <div className="sm2__empty sm2__empty--compact">没有项目级 Skills</div>
  return (
    <div className="sm2__project-skill-grid">
      {skills.map(({ agent, skill }) => {
        const importKey = `${agent.agentId}:${skill.path}`
        return (
          <article key={importKey} className="sm2__project-skill-card">
            <div className="sm2__project-skill-head">
              <AgentIconBadge iconKey={agent.iconKey} title={agent.displayName} size={24} />
              <div>
                <strong>{skill.name || skill.id}</strong>
                <span>{projectSkillStatusLabel(skill.status)}</span>
              </div>
            </div>
            {skill.description && <p>{skill.description}</p>}
            <code>{skill.path}</code>
            <div className="sm2__btn-row">
              <button className="sm2__btn" onClick={() => skillApiV2.revealPath(skill.path)}>定位</button>
              <button className="sm2__btn sm2__btn--primary" disabled={importing === importKey} onClick={() => importSkill(agent, skill)}>
                {importing === importKey ? '导入中…' : skill.status === 'centerDiff' ? '更新中心库' : '导入中心库'}
              </button>
            </div>
          </article>
        )
      })}
    </div>
  )
}

function ProjectMcp({ agents }: { agents: ProjectAgentDetail[] }) {
  const servers = agents.flatMap((agent) => agent.mcpServers.map((server) => ({ agent, server })))
  if (servers.length === 0) return <div className="sm2__empty sm2__empty--compact">没有项目级 MCP 配置</div>
  return (
    <div className="sm2__project-object-list">
      {servers.map(({ agent, server }) => (
        <div key={`${agent.agentId}:${server.name}`} className="sm2__project-object-row">
          <AgentIconBadge iconKey={agent.iconKey} title={agent.displayName} size={22} />
          <div>
            <strong>{server.name}</strong>
            <span>{server.valid ? 'configured' : server.message}</span>
            <code>{[server.command, ...server.args].filter(Boolean).join(' ')}</code>
          </div>
        </div>
      ))}
    </div>
  )
}

function ProjectPlugins({ agents }: { agents: ProjectAgentDetail[] }) {
  const plugins = agents.flatMap((agent) => agent.plugins.map((plugin) => ({ agent, plugin })))
  if (plugins.length === 0) return <div className="sm2__empty sm2__empty--compact">没有项目级 Plugin 配置</div>
  return (
    <div className="sm2__project-object-list">
      {plugins.map(({ agent, plugin }) => (
        <div key={`${agent.agentId}:${plugin.id}`} className="sm2__project-object-row">
          <AgentIconBadge iconKey={agent.iconKey} title={agent.displayName} size={22} />
          <div>
            <strong>{plugin.name || plugin.id}</strong>
            <span>{plugin.enabled ? '已启用' : '已禁用'}{plugin.source ? ` · ${plugin.source}` : ''}</span>
            <code>{plugin.id}</code>
          </div>
        </div>
      ))}
    </div>
  )
}

function ProjectInstructions({ detail, agentFilter }: { detail: ProjectDetail; agentFilter: string }) {
  const files = agentFilter === 'all'
    ? detail.instructions
    : detail.instructions.filter((file) => file.agentId === agentFilter)
  if (files.length === 0) return <div className="sm2__empty sm2__empty--compact">没有检测到指令文件</div>
  return (
    <div className="sm2__project-object-list">
      {files.map((file) => (
        <div key={`${file.agentId}:${file.path}`} className="sm2__project-object-row">
          <AgentIconBadge iconKey={file.agentId} title={file.agentId} size={22} />
          <div>
            <strong>{instructionLabel(file.agentId)}</strong>
            <span>{file.bytes ? `${Math.round(file.bytes / 1024)} KB` : '已检测'}</span>
            <code>{file.path}</code>
          </div>
          <button className="sm2__btn" onClick={() => skillApiV2.openPath(file.path)}>打开</button>
        </div>
      ))}
    </div>
  )
}

function ProjectConfig({ agents }: { agents: ProjectAgentDetail[] }) {
  const paths = agents.flatMap((agent) => [
    ...agent.skillsDirs.map((path) => ({ agent, label: 'Skills 目录', path })),
    ...agent.configPaths.map((path) => ({ agent, label: '配置', path })),
    ...agent.mcpConfigPaths.map((path) => ({ agent, label: 'MCP 配置', path })),
    ...agent.pluginConfigPaths.map((path) => ({ agent, label: 'Plugin 配置', path })),
  ])
  if (paths.length === 0) return <div className="sm2__empty sm2__empty--compact">没有检测到项目级路径</div>
  return (
    <div className="sm2__project-object-list">
      {paths.map(({ agent, label, path }) => (
        <div key={`${agent.agentId}:${label}:${path}`} className="sm2__project-object-row">
          <AgentIconBadge iconKey={agent.iconKey} title={agent.displayName} size={22} />
          <div>
            <strong>{label}</strong>
            <span>{agent.displayName}</span>
            <code>{path}</code>
          </div>
          <button className="sm2__btn" onClick={() => skillApiV2.revealPath(path)}>定位</button>
        </div>
      ))}
    </div>
  )
}

function ProjectStat({ value, label }: { value: number; label: string }) {
  return (
    <div className="sm2__project-stat">
      <strong>{value}</strong>
      <span>{label}</span>
    </div>
  )
}

function projectSkillStatusLabel(status: string) {
  const labels: Record<string, string> = {
    projectOnly: '仅项目内',
    centerSynced: '已同步中心库',
    centerDiff: '与中心库不同',
  }
  return labels[status] || status
}

function instructionLabel(agentId: string) {
  if (agentId === 'codex') return 'Codex 指令'
  if (agentId === 'claude-code') return 'Claude 指令'
  return '项目指令'
}

function formatDate(value: string) {
  try {
    return new Date(value).toLocaleString()
  } catch {
    return value
  }
}
