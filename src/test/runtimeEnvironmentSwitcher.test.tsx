import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { SettingsSidebar } from '../components/settings/SettingsSidebar'
import { SkillManagerShell } from '../components/skills-v2/SkillManagerShell'
import { agentApi } from '../services/agentApi'
import { skillApiV2 } from '../services/skillApiV2'
import { useConfigStore } from '../stores/configStore'
import {
  LOCAL_RUNTIME_ENVIRONMENT_ID,
  useRuntimeEnvironmentStore,
} from '../stores/runtimeEnvironmentStore'
import { useSkillStoreV2 } from '../stores/skillStoreV2'
import { useRemoteServerStore } from '../stores/remoteServerStore'
import '../i18n'

const tauriCoreMocks = vi.hoisted(() => ({ invoke: vi.fn() }))

vi.mock('@tauri-apps/api/core', () => ({ invoke: tauriCoreMocks.invoke }))

const remoteHost = {
  id: 'gpu-box',
  name: 'GPU Box',
  sshTarget: 'agent@gpu-box',
  port: 22,
  remoteSocketPath: '/tmp/agentbro.sock',
  autoConnect: true,
  connectionStatus: 'connected' as const,
}

const remoteOverview = {
  metrics: {
    centerSkillCount: 1,
    targetCount: 1,
    unmanagedCount: 0,
    issueCount: 0,
  },
  skills: [{
    id: 'remote-sample',
    name: 'Remote Sample Skill',
    description: 'Loaded over SSH',
    skillType: 'skill',
    sourceType: 'remote_center',
    sourceUri: null,
    centerPath: '/home/agent/.agents/skills/remote-sample',
    currentHash: 'abc',
    status: 'ok',
    installedAgents: [{
      agentId: 'claude-code',
      displayName: 'Claude Code',
      iconKey: 'claude-code',
      mode: 'copy' as const,
      status: 'ok',
    }],
  }],
  agents: [{
    id: 'claude-code',
    displayName: 'Claude Code',
    iconKey: 'claude-code',
    enabled: true,
    skillsDir: '/home/agent/.claude/skills',
    version: null,
    latestVersion: null,
    installed: true,
    managedSkillCount: 1,
    unmanagedSkillCount: 0,
    readOnlySkillCount: 0,
  }],
  packs: [{
    id: 'default',
    name: '全量技能包',
    description: 'Remote default pack',
    tags: [],
    memberCount: 1,
    appliedAgentCount: 0,
    healthy: true,
  }],
  issues: [],
  settings: {
    centerPath: '/home/agent/.agents/skills',
    sqlitePath: '/home/agent/.agentbro/skill-manager-remote.json',
    defaultDistributeMode: 'link' as const,
    linkFailPolicy: 'ask' as const,
    startupScan: true,
    showUnmanaged: true,
  },
}

describe('runtime environment switcher', () => {
  beforeEach(() => {
    window.localStorage.removeItem('agentbro-runtime-environment')
    useConfigStore.setState({
      remoteHostEntries: [remoteHost],
      sshHosts: [],
    })
    useRuntimeEnvironmentStore.setState({
      selectedEnvironmentId: LOCAL_RUNTIME_ENVIRONMENT_ID,
    })
    useRemoteServerStore.setState({
      remoteHosts: [],
      remoteStatuses: {},
      loaded: true,
      loading: false,
      error: null,
    })
    useSkillStoreV2.setState({
      runtimeEnvironmentId: LOCAL_RUNTIME_ENVIRONMENT_ID,
      activeTab: 'library',
      selectedAgentId: null,
      selectedAgentDetail: null,
    })
    tauriCoreMocks.invoke.mockReset()
  })

  afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
    delete (window as Window & { isTauri?: boolean }).isTauri
  })

  it('switches the global target from this Mac to a configured remote server', () => {
    const onSelect = vi.fn()
    render(
      <SettingsSidebar
        activeSection="skill-manager-v2"
        activeIslandView="overview"
        activeMonitorView="overview"
        collapsed={false}
        onCollapsedChange={() => {}}
        onSelect={onSelect}
        onIslandViewChange={() => {}}
        onMonitorViewChange={() => {}}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Current runtime environment: This Mac' }))
    expect(screen.getByRole('listbox', { name: 'Switch runtime environment' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('option', { name: /GPU Box/ }))

    expect(useRuntimeEnvironmentStore.getState().selectedEnvironmentId).toBe('gpu-box')
    expect(JSON.parse(window.localStorage.getItem('agentbro-runtime-environment') || '{}')).toMatchObject({
      state: { selectedEnvironmentId: 'gpu-box' },
    })
    expect(screen.getByRole('button', { name: 'Current runtime environment: GPU Box' })).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Current runtime environment: GPU Box' }))
    fireEvent.click(screen.getByRole('button', { name: 'Manage servers' }))
    expect(onSelect).toHaveBeenCalledWith('remote-servers')
  })

  it('keeps the original Agent management interface for a remote server', () => {
    useRuntimeEnvironmentStore.setState({ selectedEnvironmentId: 'gpu-box' })
    useSkillStoreV2.setState({
      runtimeEnvironmentId: 'gpu-box',
      activeTab: 'agents',
      agents: remoteOverview.agents,
      packs: remoteOverview.packs,
      skills: remoteOverview.skills,
      overview: remoteOverview,
      settings: remoteOverview.settings,
      initialized: true,
      loading: false,
      error: null,
    })

    render(<SkillManagerShell />)

    expect(screen.getByRole('status', { name: 'Current runtime environment: GPU Box' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Agent 管理' })).toBeInTheDocument()
    expect(screen.getByText('查看每个 Agent 的 Skills、技能包、MCP、插件与 Hook 状态。')).toBeInTheDocument()
  })

  it('keeps the original Skill library interface for a remote server', async () => {
    useRuntimeEnvironmentStore.setState({ selectedEnvironmentId: 'gpu-box' })
    useSkillStoreV2.setState({
      runtimeEnvironmentId: 'gpu-box',
      activeTab: 'library',
      overview: remoteOverview,
      settings: remoteOverview.settings,
      skills: remoteOverview.skills,
      agents: remoteOverview.agents,
      packs: remoteOverview.packs,
      issues: [],
      unmanaged: [],
      initialized: true,
      loading: false,
      error: null,
    })

    render(<SkillManagerShell />)
    expect(await screen.findByText('Remote Sample Skill')).toBeInTheDocument()
    expect(screen.getByText('统一管理中心库 Skills，查看安装到哪些 Agent，并处理分发、更新和删除。')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '批量管理' })).toBeInTheDocument()
  })

  it('offers a direct connection action for the selected disconnected server', async () => {
    const connectServer = vi
      .spyOn(useRemoteServerStore.getState(), 'connectServer')
      .mockResolvedValue(undefined)
    useConfigStore.setState({
      remoteHostEntries: [{ ...remoteHost, connectionStatus: 'disconnected' }],
    })
    useRuntimeEnvironmentStore.setState({ selectedEnvironmentId: 'gpu-box' })
    useSkillStoreV2.setState({
      runtimeEnvironmentId: 'gpu-box',
      activeTab: 'library',
      overview: remoteOverview,
      settings: remoteOverview.settings,
      skills: remoteOverview.skills,
      agents: remoteOverview.agents,
      packs: remoteOverview.packs,
      issues: [],
      unmanaged: [],
      initialized: true,
      loading: false,
      error: null,
    })

    render(<SkillManagerShell />)
    fireEvent.click(screen.getByRole('button', {
      name: /Connect the live event channel for GPU Box|连接 GPU Box 的实时事件通道/,
    }))

    expect(connectServer).toHaveBeenCalledWith('gpu-box')
  })

  it('routes the existing Skill API to the selected remote host', async () => {
    ;(window as Window & { isTauri?: boolean }).isTauri = true
    useRuntimeEnvironmentStore.setState({ selectedEnvironmentId: 'gpu-box' })
    tauriCoreMocks.invoke.mockResolvedValue(remoteOverview)

    await skillApiV2.overview()

    expect(tauriCoreMocks.invoke).toHaveBeenCalledWith('remote_skill_manager_invoke', {
      id: 'gpu-box',
      command: 'skill_manager_overview',
      args: {},
    })
  })

  it('routes the existing Agent actions to the selected remote host', async () => {
    ;(window as Window & { isTauri?: boolean }).isTauri = true
    useRuntimeEnvironmentStore.setState({ selectedEnvironmentId: 'gpu-box' })
    tauriCoreMocks.invoke.mockResolvedValue([])

    await agentApi.refresh()
    await agentApi.installHook('claude-code')

    expect(tauriCoreMocks.invoke).toHaveBeenNthCalledWith(1, 'remote_skill_manager_invoke', {
      id: 'gpu-box',
      command: 'agent_refresh',
      args: {},
    })
    expect(tauriCoreMocks.invoke).toHaveBeenNthCalledWith(2, 'install_remote_agent_hooks', {
      id: 'gpu-box',
      agentId: 'claude-code',
    })
  })
})
