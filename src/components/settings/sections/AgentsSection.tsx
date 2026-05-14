import { useCallback, useEffect, useMemo, useState } from 'react'
import { agentApi, type AgentProgramInfo } from '../../../services/agentApi'
import { useAgentStore } from '../../../stores/agentStore'
import { useSkillStore } from '../../../stores/skillStore'
import { skillApi, type ScannedSkill, type SkillPack } from '../../../services/skillApi'
import { AgentDetailSlider } from './AgentDetailSlider'
import { SkillListView } from '../../skills/SkillListView'
import { CentralSkillListView } from '../../skills/CentralSkillListView'
import { PluginListView } from '../../skills/PluginListView'
import { PackListView } from '../../skills/PackListView'
import { DiscoverView } from '../../skills/DiscoverView'
import { MarketplaceView } from '../../skills/MarketplaceView'
import { SyncView } from '../../skills/SyncView'
import { InstallDialog } from '../../skills/InstallDialog'
import { SkillDetailSlider } from '../../skills/SkillDetailSlider'
import type { CapabilityView } from '../../../types/capability'
import { agentColor, isAgentProgramInstalled } from '../../../utils/agentPrograms'
import { displayVersionValue } from '../../../utils/versions'
import { PlatformIcon } from '../../platform/PlatformIcon'
import { CustomAgentDialog } from './CustomAgentDialog'
import {
  getAllHookStatus,
  installAgentHook,
  reinstallAllHooks,
  uninstallAgentHook,
  type HookStatus,
} from '../../../services/tauriApi'
import './SkillsSection.css'
import './AgentsSection.css'

type AgentTab = 'skills' | 'plugins' | 'mcps' | 'hooks' | 'config'

const agentVisuals: Record<string, { accent: string; icon: string; emoji: string }> = {
  'claude-code': { accent: '#5856d6', icon: '⚡', emoji: '⚡' },
  codex: { accent: '#34c759', icon: '✦', emoji: '🔮' },
  gemini: { accent: '#ff9500', icon: '◆', emoji: '💎' },
  'gemini-cli': { accent: '#ff9500', icon: '◆', emoji: '💎' },
  opencode: { accent: '#007aff', icon: 'OC', emoji: '🐾' },
  hermes: { accent: '#ff2d55', icon: 'H', emoji: '🪽' },
  cursor: { accent: '#5856d6', icon: 'C', emoji: '🖱' },
  'cursor-cli': { accent: '#5856d6', icon: 'C', emoji: '🖱' },
}

function isInstalled(agent: AgentProgramInfo) {
  return isAgentProgramInstalled(agent)
}

function visualFor(agent: AgentProgramInfo) {
  return agentVisuals[agent.id] ?? { accent: agentColor(agent.id), icon: agent.displayName.slice(0, 2).toUpperCase(), emoji: '' }
}

function skillAgentMatches(skill: ScannedSkill, agentId: string) {
  if (agentId === 'gemini') return skill.agents.some((agent) => agent.agent === 'gemini' || agent.agent === 'gemini-cli')
  if (agentId === 'cursor-cli') return skill.agents.some((agent) => agent.agent === 'cursor' || agent.agent === 'cursor-cli')
  return skill.agents.some((agent) => agent.agent === agentId)
}

function stateForAgent(skill: ScannedSkill, agentId: string) {
  if (agentId === 'gemini') return skill.agents.find((agent) => agent.agent === 'gemini' || agent.agent === 'gemini-cli')
  if (agentId === 'cursor-cli') return skill.agents.find((agent) => agent.agent === 'cursor' || agent.agent === 'cursor-cli')
  return skill.agents.find((agent) => agent.agent === agentId)
}

function packMatchesAgent(pack: SkillPack, agentId: string) {
  if (agentId === 'gemini') return pack.targetAgents.includes('gemini') || pack.targetAgents.includes('gemini-cli')
  if (agentId === 'cursor-cli') return pack.targetAgents.includes('cursor') || pack.targetAgents.includes('cursor-cli')
  return pack.targetAgents.includes(agentId)
}

function displayVersion(agent: AgentProgramInfo) {
  const version = displayVersionValue(agent.installedVersion)
  return version ? `当前 ${version}` : ''
}

function displayLatestVersion(agent: AgentProgramInfo) {
  const version = displayVersionValue(agent.latestVersion)
  return agent.status === 'updateAvailable' && version ? `最新 ${version}` : ''
}

function statusLabel(agent: AgentProgramInfo) {
  if (agent.status === 'installed') return '运行中'
  if (agent.status === 'updateAvailable') return '可更新'
  if (agent.status === 'notInstalled') return '未安装'
  return '不可用'
}

function installModeLabel(mode: string | undefined) {
  return mode === 'symlink' ? 'symlink' : 'copy'
}

function hookToolId(hook: HookStatus) {
  return hook.toolId || hook.name
}

function hookInstallStatus(hook: HookStatus) {
  if (hook.installStatus) return hook.installStatus
  return hook.installed ? 'installed' : 'not_installed'
}

function hookStatusLabel(hook: HookStatus) {
  const status = hookInstallStatus(hook)
  if (status === 'installed') return '已安装'
  if (status === 'error') return '异常'
  return '未安装'
}

interface AgentsSectionProps {
  activeView: CapabilityView
  onViewChange: (view: CapabilityView) => void
  customAgentDialogOpen: boolean
  onCustomAgentDialogOpenChange: (open: boolean) => void
}

export function AgentsSection({
  activeView,
  onViewChange,
  customAgentDialogOpen,
  onCustomAgentDialogOpenChange,
}: AgentsSectionProps) {
  const {
    agents,
    loading,
    selectedAgentId,
    detailOpen,
    operations,
    loadAgents,
    refreshAgents,
    focusAgent,
    closeDetail,
    runOperation,
    handleOutput,
  } = useAgentStore()
  const {
    skills,
    packs,
    scanning,
    loadAll: loadSkills,
    selectSkill,
  } = useSkillStore()
  const [agentTab, setAgentTab] = useState<AgentTab>('skills')
  const [skillQuery, setSkillQuery] = useState('')
  const [installOpen, setInstallOpen] = useState(false)
  const [notice, setNotice] = useState('')
  const [syncing, setSyncing] = useState(false)
  const [hookStatuses, setHookStatuses] = useState<HookStatus[]>([])
  const [hooksLoading, setHooksLoading] = useState(false)
  const [hookActions, setHookActions] = useState<Record<string, string>>({})
  const [checkingUpdate, setCheckingUpdate] = useState(false)
  const [editingCustomAgent, setEditingCustomAgent] = useState<AgentProgramInfo | null>(null)

  useEffect(() => {
    loadAgents()
    loadSkills()
  }, [loadAgents, loadSkills])

  useEffect(() => {
    let unlisten: (() => void) | undefined
    agentApi.onOutput(handleOutput).then((fn) => { unlisten = fn })
    return () => unlisten?.()
  }, [handleOutput])

  useEffect(() => {
    if (!selectedAgentId && agents.length > 0) {
      focusAgent((agents.find(isInstalled) ?? agents[0]).id)
    }
  }, [agents, focusAgent, selectedAgentId])

  const selectedAgent = agents.find((agent) => agent.id === selectedAgentId) ?? agents.find(isInstalled) ?? agents[0] ?? null
  const selectedVisual = selectedAgent ? visualFor(selectedAgent) : null

  const selectedSkills = useMemo(() => {
    if (!selectedAgent) return []
    const query = skillQuery.trim().toLowerCase()
    return skills
      .filter((skill) => skill.skillType === 'skill' && skillAgentMatches(skill, selectedAgent.id))
      .filter((skill) => !query || skill.name.toLowerCase().includes(query) || skill.description.toLowerCase().includes(query))
  }, [selectedAgent, skillQuery, skills])

  const selectedMcps = useMemo(() => {
    if (!selectedAgent) return []
    return skills.filter((skill) => skill.skillType === 'mcp' && skillAgentMatches(skill, selectedAgent.id))
  }, [selectedAgent, skills])

  const selectedPlugins = useMemo(() => {
    if (!selectedAgent) return []
    return skills.filter((skill) => skill.skillType === 'plugin' && skillAgentMatches(skill, selectedAgent.id))
  }, [selectedAgent, skills])

  const selectedPacks = useMemo(() => {
    if (!selectedAgent) return []
    return packs.filter((pack) => packMatchesAgent(pack, selectedAgent.id))
  }, [packs, selectedAgent])

  const updateCount = skills.filter((skill) => skill.hasUpdate).length + agents.filter((agent) => agent.status === 'updateAvailable').length
  const selectedOperation = selectedAgent ? operations[selectedAgent.id] : null
  const installedHookCount = hookStatuses.filter((hook) => hookInstallStatus(hook) === 'installed').length

  const loadHookStatuses = useCallback(async () => {
    setHooksLoading(true)
    try {
      setHookStatuses(await getAllHookStatus())
    } catch (error) {
      setNotice(String(error))
    } finally {
      setHooksLoading(false)
    }
  }, [])

  useEffect(() => {
    if (agentTab !== 'hooks') return
    loadHookStatuses()
  }, [agentTab, loadHookStatuses])

  const setHookAction = useCallback((toolId: string, action: string | null) => {
    setHookActions((prev) => {
      const next = { ...prev }
      if (action) next[toolId] = action
      else delete next[toolId]
      return next
    })
  }, [])

  const toggleHook = useCallback(async (hook: HookStatus, checked: boolean) => {
    const toolId = hookToolId(hook)
    setHookAction(toolId, checked ? 'install' : 'uninstall')
    setNotice('')
    try {
      if (checked) await installAgentHook(toolId)
      else await uninstallAgentHook(toolId)
      await loadHookStatuses()
      await refreshAgents()
    } catch (error) {
      setNotice(String(error))
    } finally {
      setHookAction(toolId, null)
    }
  }, [loadHookStatuses, refreshAgents, setHookAction])

  const handleReinstallHooks = useCallback(async () => {
    setHooksLoading(true)
    setNotice('')
    try {
      const errors = await reinstallAllHooks()
      if (errors.length > 0) setNotice(errors.join('；'))
      else setNotice('已重新安装所有 Hook。')
      await loadHookStatuses()
      await refreshAgents()
    } catch (error) {
      setNotice(String(error))
    } finally {
      setHooksLoading(false)
    }
  }, [loadHookStatuses, refreshAgents])

  const handleToggleSkill = useCallback(async (skill: ScannedSkill) => {
    if (!selectedAgent) return
    const state = stateForAgent(skill, selectedAgent.id)
    if (!state) return
    await skillApi.toggle(skill.id, state.agent, !state.enabled)
    loadSkills()
  }, [loadSkills, selectedAgent])

  const handleUninstallSkill = useCallback(async (skill: ScannedSkill) => {
    const state = selectedAgent ? stateForAgent(skill, selectedAgent.id) : null
    await skillApi.uninstall(state?.installPath || skill.filePath)
    loadSkills()
  }, [loadSkills, selectedAgent])

  const handleUpdateSkill = useCallback(async (skill: ScannedSkill) => {
    const state = selectedAgent ? stateForAgent(skill, selectedAgent.id) : null
    if (!selectedAgent || !state || !skill.originUrl) {
      setNotice('这个 Skill 没有记录可更新来源。')
      return
    }
    await skillApi.install(skill.originUrl, [{ agent: state.agent, installMode: state.installMode }], state.installMode)
    setNotice(`已重新安装 ${skill.name}。`)
    loadSkills()
  }, [loadSkills, selectedAgent])

  const handleOpenPath = useCallback(async (path: string | null | undefined) => {
    if (!path) return
    try {
      await agentApi.openPath(path)
    } catch (error) {
      setNotice(String(error))
    }
  }, [])

  const handleCopyCommand = useCallback(async (command: string | null | undefined) => {
    if (!command) return
    await navigator.clipboard?.writeText(command)
    setNotice('命令已复制。')
  }, [])

  const openAddCustomAgent = useCallback(() => {
    setEditingCustomAgent(null)
    onCustomAgentDialogOpenChange(true)
  }, [onCustomAgentDialogOpenChange])

  const openEditCustomAgent = useCallback((agent: AgentProgramInfo) => {
    setEditingCustomAgent(agent)
    onCustomAgentDialogOpenChange(true)
  }, [onCustomAgentDialogOpenChange])

  const closeCustomAgentDialog = useCallback(() => {
    onCustomAgentDialogOpenChange(false)
    setEditingCustomAgent(null)
  }, [onCustomAgentDialogOpenChange])

  const handleRemoveCustomAgent = useCallback(async () => {
    if (!selectedAgent?.isCustom) return
    if (!window.confirm(`确定删除自定义 Agent「${selectedAgent.displayName}」吗？Skills 文件不会被删除。`)) return
    try {
      await agentApi.removeCustom(selectedAgent.id)
      setNotice(`已移除自定义 Agent：${selectedAgent.displayName}`)
      await refreshAgents()
      loadSkills()
    } catch (error) {
      setNotice(String(error))
    }
  }, [loadSkills, refreshAgents, selectedAgent])

  const handleSyncToNextAgent = useCallback(async () => {
    if (!selectedAgent) return
    const target = agents.find((agent) => agent.id !== selectedAgent.id && isInstalled(agent))
    if (!target) {
      setNotice('没有其他已安装 Agent 可同步。')
      return
    }
    setSyncing(true)
    setNotice('')
    try {
      const preview = await skillApi.syncAgentPreview(selectedAgent.id, target.id)
      await skillApi.executeAgentSync(selectedAgent.id, target.id)
      setNotice(`已同步到 ${target.displayName}：复制 ${preview.toCopy}，更新 ${preview.toUpdate}，跳过 ${preview.toSkip}。`)
      loadSkills()
    } catch (error) {
      setNotice(String(error))
    } finally {
      setSyncing(false)
    }
  }, [agents, loadSkills, selectedAgent])

  const renderAgentSkills = () => {
    if (scanning) return <div className="agent-empty-state">正在扫描本机 Skills...</div>
    if (selectedSkills.length === 0) return <div className="agent-empty-state">当前 Agent 暂未发现 Skill。</div>

    return (
      <table className="agent-skill-table agent-skill-table--demo">
        <thead>
          <tr>
            <th>状态</th>
            <th>名称</th>
            <th>来源</th>
            <th>版本</th>
            <th>安装</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          {selectedSkills.map((skill) => {
            const state = selectedAgent ? stateForAgent(skill, selectedAgent.id) : undefined
            const version = skill.frontmatter.version || skill.frontmatter.versionName || '-'
            return (
              <tr
                key={`${skill.id}:${state?.agent ?? 'unknown'}:${state?.installPath ?? skill.filePath}`}
                className="agent-skill-row"
                role="button"
                tabIndex={0}
                onClick={() => selectSkill(skill.id)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' || event.key === ' ') {
                    event.preventDefault()
                    selectSkill(skill.id)
                  }
                }}
              >
                <td>
                  <button
                    type="button"
                    className={`demo-toggle ${state?.enabled ? 'demo-toggle--on' : ''}`}
                    onClick={(event) => {
                      event.stopPropagation()
                      handleToggleSkill(skill)
                    }}
                    aria-label={state?.enabled ? '停用 Skill' : '启用 Skill'}
                  />
                </td>
                <td>
                  <button type="button" className="agent-skill-name" onClick={(event) => { event.stopPropagation(); selectSkill(skill.id) }}>
                    {skill.name}
                  </button>
                  <div className="agent-skill-desc">{skill.description || '暂无描述'}</div>
                </td>
                <td>{skill.source === 'island' ? 'AgentBro' : 'local'}</td>
                <td className="agent-skill-version">{version}</td>
                <td>
                  <span className={`agent-install-badge agent-install-badge--${state?.installMode ?? 'direct'}`}>
                    {installModeLabel(state?.installMode)}
                  </span>
                </td>
                <td>
                  <div className="agent-row-actions">
                    <button
                      type="button"
                      title="更新"
                      disabled={!skill.hasUpdate}
                      onClick={(event) => {
                        event.stopPropagation()
                        handleUpdateSkill(skill)
                      }}
                    >
                      ⬆
                    </button>
                    <button
                      type="button"
                      title="卸载"
                      onClick={(event) => {
                        event.stopPropagation()
                        handleUninstallSkill(skill)
                      }}
                    >
                      🗑
                    </button>
                  </div>
                </td>
              </tr>
            )
          })}
        </tbody>
      </table>
    )
  }

  const renderAgentPlugins = () => {
    if (selectedPlugins.length === 0) return <div className="agent-empty-state">当前 Agent 暂无插件。</div>
    return (
      <div className="agent-inline-cards">
        {selectedPlugins.map((plugin) => (
          <button key={plugin.id} type="button" className="agent-inline-card agent-inline-card--button" onClick={() => selectSkill(plugin.id)}>
            <div>
              <div className="agent-inline-card__title">{plugin.name}</div>
              <div className="agent-inline-card__desc">{plugin.description || 'Claude Code 插件'}</div>
              <div className="agent-inline-card__chips">
                {plugin.frontmatter.version && <span>{displayVersionValue(plugin.frontmatter.version)}</span>}
                {plugin.originUrl && <span>{plugin.originUrl}</span>}
              </div>
            </div>
            <span className="agent-install-badge agent-install-badge--direct">Plugin</span>
          </button>
        ))}
      </div>
    )
  }

  const renderAgentMcps = () => {
    if (selectedMcps.length === 0) return <div className="agent-empty-state">当前 Agent 暂无 MCP Server。</div>
    return (
      <div className="agent-inline-cards">
        {selectedMcps.map((mcp) => (
          <button key={mcp.id} type="button" className="agent-inline-card agent-inline-card--button" onClick={() => selectSkill(mcp.id)}>
            <div>
              <div className="agent-inline-card__title">{mcp.name}</div>
              <div className="agent-inline-card__desc">{mcp.description}</div>
            </div>
            <span className="agent-install-badge agent-install-badge--direct">MCP</span>
          </button>
        ))}
      </div>
    )
  }

  const renderAgentHooks = () => {
    if (hooksLoading && hookStatuses.length === 0) return <div className="agent-empty-state">正在读取 Hook 状态...</div>
    if (hookStatuses.length === 0) return <div className="agent-empty-state">暂未发现可管理的 Hook。</div>

    return (
      <div className="agent-hooks-panel">
        <div className="agent-hooks-toolbar">
          <button type="button" className="skills-btn" onClick={loadHookStatuses} disabled={hooksLoading}>
            {hooksLoading ? '扫描中...' : '重新扫描'}
          </button>
          <button type="button" className="skills-btn" onClick={handleReinstallHooks} disabled={hooksLoading}>
            重装所有 Hook
          </button>
        </div>
        <div className="agent-hook-list">
          {hookStatuses.map((hook) => {
            const toolId = hookToolId(hook)
            const installStatus = hookInstallStatus(hook)
            const busy = hookActions[toolId] !== undefined
            const isCurrentAgent = selectedAgent && (hook.adapterId || hook.name) === selectedAgent.id
            return (
              <div key={`${toolId}:${hook.configPath || hook.displayName}`} className="agent-hook-card">
                <div className="agent-hook-card__identity">
                  <div className={`agent-hook-card__dot agent-hook-card__dot--${installStatus}`} />
                  <div>
                    <div className="agent-hook-card__title">
                      {hook.displayName || hook.name}
                      {isCurrentAgent && <span>当前 Agent</span>}
                    </div>
                    <div className="agent-hook-card__path">{hook.configPath || hook.status || toolId}</div>
                  </div>
                </div>
                <div className={`agent-hook-badge agent-hook-badge--${installStatus}`}>
                  {hookStatusLabel(hook)}
                </div>
                <button
                  type="button"
                  className={`demo-toggle ${installStatus === 'installed' ? 'demo-toggle--on' : ''}`}
                  disabled={busy}
                  onClick={() => toggleHook(hook, installStatus !== 'installed')}
                  aria-label={installStatus === 'installed' ? '移除 Hook' : '安装 Hook'}
                />
              </div>
            )
          })}
        </div>
      </div>
    )
  }

  const renderAgentConfig = () => {
    if (!selectedAgent) return null
    return (
      <div className="agent-config-panel">
        <div className="agent-config-grid">
          <div><span>包管理器</span><strong>{selectedAgent.packageManager || '-'}</strong></div>
          <div><span>包名</span><strong>{selectedAgent.packageName || '-'}</strong></div>
          <div><span>执行文件</span><strong>{selectedAgent.binaryPath || selectedAgent.appPath || '-'}</strong></div>
          <div><span>配置目录</span><strong>{selectedAgent.configDir || '-'}</strong></div>
          <div><span>安装状态</span><strong>{statusLabel(selectedAgent)}</strong></div>
        </div>

        <div className="agent-config-actions">
          <button type="button" className="skills-btn" onClick={() => handleOpenPath(selectedAgent.configDir)} disabled={!selectedAgent.configDir}>
            打开配置目录
          </button>
          <button type="button" className="skills-btn" onClick={() => handleOpenPath(selectedAgent.binaryPath || selectedAgent.appPath)} disabled={!selectedAgent.binaryPath && !selectedAgent.appPath}>
            打开执行位置
          </button>
          <button type="button" className="skills-btn" onClick={() => { refreshAgents(); loadSkills() }}>
            重新扫描
          </button>
          {selectedAgent.isCustom && (
            <>
              <button type="button" className="skills-btn" onClick={() => openEditCustomAgent(selectedAgent)}>
                编辑自定义 Agent
              </button>
              <button type="button" className="skills-btn skills-btn--danger" onClick={handleRemoveCustomAgent}>
                删除自定义 Agent
              </button>
            </>
          )}
        </div>

        <div className="agent-command-list">
          {[
            ['安装命令', selectedAgent.installCommand],
            ['更新命令', selectedAgent.updateCommand],
            ['卸载命令', selectedAgent.uninstallCommand],
          ].map(([label, command]) => (
            <button key={label} type="button" className="agent-command-row" disabled={!command} onClick={() => handleCopyCommand(command)}>
              <span>{label}</span>
              <code>{command || '当前 Agent 不支持该命令'}</code>
            </button>
          ))}
        </div>
      </div>
    )
  }

  const renderAgentView = () => {
    if (loading) return <div className="agent-empty-state">正在检测本机 Agent...</div>
    if (!selectedAgent || !selectedVisual) return <div className="agent-empty-state">未发现 Agent。</div>

    return (
      <>
        <div className="demo-agent-header">
          <div className="demo-agent-title">
            <div className="demo-agent-avatar" style={{ background: `linear-gradient(135deg, ${selectedVisual.accent}, color-mix(in srgb, ${selectedVisual.accent} 62%, #fff))` }}>
              <PlatformIcon agentId={selectedAgent.icon || selectedAgent.id} displayName={selectedAgent.displayName} size={34} />
            </div>
            <div>
              <h3>{selectedAgent.displayName}</h3>
              <div className="demo-agent-meta">
                {displayVersion(selectedAgent) && (
                  <span>
                    {displayVersion(selectedAgent)}
                    {displayLatestVersion(selectedAgent) && (
                      <span className="demo-agent-update-version" style={{ marginLeft: 6 }}>{displayLatestVersion(selectedAgent)}</span>
                    )}
                  </span>
                )}
                {selectedAgent.status !== 'installed' && (
                  <span className={`demo-agent-status demo-agent-status--${selectedAgent.status}`}>● {statusLabel(selectedAgent)}</span>
                )}
                {selectedAgent.configDir && <code>{selectedAgent.configDir}</code>}
              </div>
            </div>
          </div>
          <div className="demo-agent-actions">
            {isInstalled(selectedAgent) && selectedAgent.status !== 'updateAvailable' && (
              <button type="button" className="demo-btn demo-btn--update" disabled={checkingUpdate} onClick={async () => { setCheckingUpdate(true); await refreshAgents(); setCheckingUpdate(false) }}>
                {checkingUpdate ? '检查中...' : '↻ 检查更新'}
              </button>
            )}
            {selectedAgent.status === 'updateAvailable' && (
              <button type="button" className="demo-btn demo-btn--update" onClick={selectedAgent.updateCommand ? () => runOperation(selectedAgent.id, 'update') : refreshAgents}>
                ⬆ 更新版本
              </button>
            )}
            {isInstalled(selectedAgent) && selectedAgent.uninstallCommand && (
              <button type="button" className="demo-btn demo-btn--danger" onClick={() => runOperation(selectedAgent.id, 'uninstall')}>🗑 卸载</button>
            )}
            {!isInstalled(selectedAgent) && (
              <button type="button" className="demo-btn demo-btn--install" onClick={() => runOperation(selectedAgent.id, selectedAgent.installCommand ? 'install' : 'open')}>
                📥 {selectedAgent.installCommand ? '安装' : '下载'}
              </button>
            )}
            <button type="button" className="demo-btn" onClick={() => setAgentTab('config')}>⚙ 配置</button>
            <button type="button" className="demo-btn" onClick={() => onViewChange('sync')}>📤 导出</button>
            <button type="button" className="demo-btn" onClick={openAddCustomAgent}>＋ 自定义 Agent</button>
            <button type="button" className="demo-btn demo-btn--primary" onClick={handleSyncToNextAgent} disabled={syncing}>↻ 同步到其他 Agent</button>
          </div>
        </div>

        <div className="demo-profile-bar">
          <span>技能包：</span>
          {selectedPacks.length > 0 ? selectedPacks.map((pack, index) => (
            <button key={pack.id} type="button" className={index === 0 ? 'active' : ''}>{pack.name}</button>
          )) : (
            <>
              <button type="button" className="active">🛠 开发模式</button>
              <button type="button">📝 写作模式</button>
              <button type="button">📊 数据模式</button>
              <button type="button">🎨 设计模式</button>
            </>
          )}
          <button type="button" className="ghost" onClick={() => onViewChange('profiles')}>+ 新建</button>
        </div>

        <div className="demo-stat-row">
          <div role="button" tabIndex={0} onClick={() => setAgentTab('skills')} onKeyDown={e => e.key === 'Enter' && setAgentTab('skills')}><strong>{selectedSkills.length}</strong><span>已安装 Skills</span></div>
          <div role="button" tabIndex={0} onClick={() => onViewChange('profiles')} onKeyDown={e => e.key === 'Enter' && onViewChange('profiles')}><strong>{selectedPacks.length}</strong><span>技能包</span></div>
          <div role="button" tabIndex={0} onClick={() => setAgentTab('plugins')} onKeyDown={e => e.key === 'Enter' && setAgentTab('plugins')}><strong>{selectedPlugins.length}</strong><span>插件</span></div>
          <div role="button" tabIndex={0} onClick={() => setAgentTab('mcps')} onKeyDown={e => e.key === 'Enter' && setAgentTab('mcps')}><strong>{selectedMcps.length}</strong><span>MCP 服务</span></div>
          <div role="button" tabIndex={0} onClick={() => setAgentTab('hooks')} onKeyDown={e => e.key === 'Enter' && setAgentTab('hooks')}><strong>{hookStatuses.length ? installedHookCount : selectedAgent.hooksInstalled ? 1 : 0}</strong><span>Hooks</span></div>
          <div><strong className="orange">{updateCount}</strong><span>可更新</span></div>
        </div>

        <div className="demo-agent-tabs">
          {[
            { id: 'skills' as const, label: `Skills (${selectedSkills.length})` },
            { id: 'plugins' as const, label: `插件 (${selectedPlugins.length})` },
            { id: 'mcps' as const, label: `MCP 服务 (${selectedMcps.length})` },
            { id: 'hooks' as const, label: hookStatuses.length ? `Hooks (${installedHookCount}/${hookStatuses.length})` : 'Hooks' },
            { id: 'config' as const, label: '配置' },
          ].map((tab) => (
            <button
              key={tab.id}
              type="button"
              className={agentTab === tab.id ? 'active' : ''}
              onClick={() => setAgentTab(tab.id)}
            >
              {tab.label}
            </button>
          ))}
        </div>

        {agentTab === 'skills' && (
          <>
            <div className="demo-toolbar">
              <input value={skillQuery} onChange={(event) => setSkillQuery(event.target.value)} placeholder="搜索 Skills..." />
              <button type="button">分组 ▾</button>
              <button type="button">排序 ▾</button>
              <button type="button" className="primary" onClick={() => setInstallOpen(true)}>+ 安装 Skill</button>
            </div>
            <div className="agent-skill-scope-note">
              当前只展示 {selectedAgent.displayName} 已发现的 Skills。查看本机完整列表请切到左侧“全部 Skills”。
            </div>
            {renderAgentSkills()}
          </>
        )}
        {agentTab === 'plugins' && renderAgentPlugins()}
        {agentTab === 'mcps' && renderAgentMcps()}
        {agentTab === 'hooks' && renderAgentHooks()}
        {agentTab === 'config' && renderAgentConfig()}

        {selectedOperation && selectedOperation.lines.length > 0 && (
          <div className={`agent-operation-log agent-operation-log--${selectedOperation.status}`}>
            <div className="agent-operation-log__head">
              <span>{selectedOperation.name} · {selectedOperation.status}</span>
              {selectedOperation.error && <strong>{selectedOperation.error}</strong>}
            </div>
            <pre>{selectedOperation.lines.slice(-8).map((line) => `${line.stream}: ${line.text}`).join('\n')}</pre>
          </div>
        )}
      </>
    )
  }

  return (
    <div className="agent-capability-section">
      <div className="capability-manager">
        <div className="capability-content">
          {activeView === 'agent' && renderAgentView()}
          {activeView === 'central' && <CentralSkillListView />}
          {activeView === 'skills' && <SkillListView />}
          {activeView === 'plugins' && <PluginListView />}
          {activeView === 'profiles' && <PackListView />}
          {activeView === 'discover' && <DiscoverView />}
          {activeView === 'market' && <MarketplaceView />}
          {activeView === 'sync' && <SyncView />}
        </div>
      </div>

      {notice && <div className="agent-sync-message">{notice}</div>}
      {installOpen && <InstallDialog onClose={() => { setInstallOpen(false); loadSkills() }} />}
      <SkillDetailSlider />
      <AgentDetailSlider
        agent={selectedAgent}
        open={detailOpen}
        onClose={closeDetail}
        onRefresh={refreshAgents}
        onRun={runOperation}
      />
      {customAgentDialogOpen && (
        <CustomAgentDialog
          agent={editingCustomAgent}
          onClose={closeCustomAgentDialog}
          onSaved={() => { refreshAgents(); loadSkills() }}
        />
      )}
    </div>
  )
}
