import { afterEach, describe, expect, it, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(() => Promise.resolve(undefined)),
}))

async function seedWithPlatform(platform: string) {
  vi.resetModules()
  Object.defineProperty(window.navigator, 'platform', {
    configurable: true,
    value: platform,
  })
  Object.defineProperty(window.navigator, 'userAgentData', {
    configurable: true,
    value: undefined,
  })
  const { seedAgentPrograms } = await import('../services/agentApi')
  return seedAgentPrograms()
}

describe('agentApi seedAgentPrograms', () => {
  afterEach(() => {
    vi.resetModules()
    vi.mocked(invoke).mockClear()
    delete (window as Window & { __TAURI_INTERNALS__?: unknown; isTauri?: boolean }).__TAURI_INTERNALS__
    delete (window as Window & { __TAURI_INTERNALS__?: unknown; isTauri?: boolean }).isTauri
  })

  it('does not expose macOS app bundle paths on Windows', async () => {
    const agents = await seedWithPlatform('Win32')
    const appAgents = agents.filter((agent) => agent.kind === 'app')

    expect(appAgents.length).toBeGreaterThan(0)
    expect(appAgents.every((agent) => !agent.appPath?.startsWith('/Applications/'))).toBe(true)
    expect(agents.find((agent) => agent.id === 'cursor')?.appPath).toBeNull()
  })

  it('keeps macOS app bundle hints on macOS', async () => {
    const agents = await seedWithPlatform('MacIntel')

    expect(agents.find((agent) => agent.id === 'cursor')?.appPath).toBe('/Applications/Cursor.app')
    expect(agents.find((agent) => agent.id === 'zcode')?.appPath).toBe('/Applications/ZCode.app')
    expect(agents.find((agent) => agent.id === 'doubao')?.appPath).toBe('/Applications/Doubao.app')
  })

  it('checks Tauri runtime at call time for install actions', async () => {
    vi.resetModules()
    delete (window as Window & { __TAURI_INTERNALS__?: unknown; isTauri?: boolean }).__TAURI_INTERNALS__

    const { agentApi } = await import('../services/agentApi')
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      value: {},
    })

    await agentApi.install('codex')

    expect(invoke).toHaveBeenCalledWith('agent_install', { agentId: 'codex' })
  })

  it('uses the confirmed runtime for uninstall IPC after the selection changes', async () => {
    const { agentApi } = await import('../services/agentApi')
    const { skillApiV2 } = await import('../services/skillApiV2')
    const { LOCAL_RUNTIME_ENVIRONMENT_ID, useRuntimeEnvironmentStore } = await import('../stores/runtimeEnvironmentStore')
    Object.defineProperty(window, '__TAURI_INTERNALS__', { configurable: true, value: {} })
    const selectEnvironment = useRuntimeEnvironmentStore.getState().selectEnvironment

    try {
      selectEnvironment('other-host')
      await agentApi.uninstall('codex', LOCAL_RUNTIME_ENVIRONMENT_ID)
      await agentApi.uninstall('codex', 'confirmed-host')
      await agentApi.uninstallHook('cursor', 'confirmed-host')
      await skillApiV2.deleteSkillTargetDistributions(['target'], 'confirmed-host')
      await skillApiV2.deleteUnmanagedAgentSkill('codex', 'unmanaged', 'confirmed-host')
      await skillApiV2.listUnmanaged('confirmed-host')

      expect(vi.mocked(invoke).mock.calls).toEqual([
        ['agent_uninstall', { agentId: 'codex' }],
        ['remote_skill_manager_invoke', { id: 'confirmed-host', command: 'agent_uninstall', args: { agentId: 'codex' } }],
        ['uninstall_remote_agent_hooks', { id: 'confirmed-host', agentId: 'cursor-cli' }],
        ['remote_skill_manager_invoke', { id: 'confirmed-host', command: 'delete_skill_target_distributions', args: { targetIds: ['target'] } }],
        ['remote_skill_manager_invoke', { id: 'confirmed-host', command: 'delete_unmanaged_agent_skill', args: { agentId: 'codex', unmanagedId: 'unmanaged' } }],
        ['remote_skill_manager_invoke', { id: 'confirmed-host', command: 'list_unmanaged_v2', args: {} }],
      ])
    } finally {
      selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    }
  })

  it('routes Agent Management program and custom Agent commands to the requested runtime', async () => {
    const { agentApi } = await import('../services/agentApi')
    const { LOCAL_RUNTIME_ENVIRONMENT_ID, useRuntimeEnvironmentStore } = await import('../stores/runtimeEnvironmentStore')
    Object.defineProperty(window, '__TAURI_INTERNALS__', { configurable: true, value: {} })
    const { selectEnvironment } = useRuntimeEnvironmentStore.getState()
    const config = { displayName: 'Team Agent', globalSkillsDir: '/team/.claude/skills' }
    try {
      selectEnvironment('different-host')
      await agentApi.refresh('original-host')
      await agentApi.install('codex', LOCAL_RUNTIME_ENVIRONMENT_ID)
      await agentApi.update('codex', 'original-host')
      await agentApi.addCustom(config, 'original-host')
      await agentApi.removeCustom('custom-team', LOCAL_RUNTIME_ENVIRONMENT_ID)

      expect(vi.mocked(invoke).mock.calls).toEqual([
        ['remote_skill_manager_invoke', { id: 'original-host', command: 'agent_refresh', args: {} }],
        ['agent_install', { agentId: 'codex' }],
        ['remote_skill_manager_invoke', { id: 'original-host', command: 'agent_update', args: { agentId: 'codex' } }],
        ['remote_skill_manager_invoke', { id: 'original-host', command: 'add_custom_agent', args: { config } }],
        ['remove_custom_agent', { agentId: 'custom-team' }],
      ])
    } finally {
      selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    }
  })

  it('routes batch adoption and pack sync IPC to the confirmed runtime', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const { LOCAL_RUNTIME_ENVIRONMENT_ID, useRuntimeEnvironmentStore } = await import('../stores/runtimeEnvironmentStore')
    Object.defineProperty(window, '__TAURI_INTERNALS__', { configurable: true, value: {} })
    const selectEnvironment = useRuntimeEnvironmentStore.getState().selectEnvironment
    const items = [{ agentId: 'codex', unmanagedId: 'unmanaged', option: 'import_keep', renamedId: null }]
    const pack = { id: 'team', name: 'Team Skills', description: '', tags: [], skillIds: ['existing', 'adopted'] }

    try {
      selectEnvironment('first-host')
      await skillApiV2.executeAdoptBatch(items, 'first-host')
      selectEnvironment('second-host')
      await skillApiV2.getPackDetail(pack.id, 'first-host')
      await skillApiV2.upsertPack(pack, { deferSync: true, environmentId: 'first-host' })

      expect(vi.mocked(invoke).mock.calls).toEqual([
        ['remote_skill_manager_invoke', { id: 'first-host', command: 'execute_adopt_agent_skills', args: { items } }],
        ['remote_skill_manager_invoke', { id: 'first-host', command: 'get_skill_pack_detail', args: { packId: pack.id } }],
        ['remote_skill_manager_invoke', { id: 'first-host', command: 'execute_upsert_skill_pack', args: { pack, deferSync: true } }],
      ])
    } finally {
      selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    }
  })

  it('routes unmanaged Skill deletion to the confirmed local or remote runtime', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const { LOCAL_RUNTIME_ENVIRONMENT_ID, useRuntimeEnvironmentStore } = await import('../stores/runtimeEnvironmentStore')
    Object.defineProperty(window, '__TAURI_INTERNALS__', { configurable: true, value: {} })
    const selectEnvironment = useRuntimeEnvironmentStore.getState().selectEnvironment
    const args = { agentId: 'codex', unmanagedIds: ['unmanaged'] }

    try {
      selectEnvironment('other-host')
      await skillApiV2.deleteUnmanagedAgentSkills(args.agentId, args.unmanagedIds, LOCAL_RUNTIME_ENVIRONMENT_ID)
      await skillApiV2.deleteUnmanagedAgentSkills(args.agentId, args.unmanagedIds, 'confirmed-host')

      expect(vi.mocked(invoke).mock.calls).toEqual([
        ['delete_unmanaged_agent_skills', args],
        ['remote_skill_manager_invoke', { id: 'confirmed-host', command: 'delete_unmanaged_agent_skills', args }],
      ])
    } finally {
      selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    }
  })

  it('routes one-click takeover and its inventory to the confirmed runtime', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const { LOCAL_RUNTIME_ENVIRONMENT_ID, useRuntimeEnvironmentStore } = await import('../stores/runtimeEnvironmentStore')
    Object.defineProperty(window, '__TAURI_INTERNALS__', { configurable: true, value: {} })
    const selectEnvironment = useRuntimeEnvironmentStore.getState().selectEnvironment

    try {
      selectEnvironment('other-host')
      await skillApiV2.takeoverCenterSkills('codex', ['unmanaged'], 'confirmed-host')
      await skillApiV2.refreshAgentSkillView('codex', 'confirmed-host')

      expect(vi.mocked(invoke).mock.calls).toEqual([
        ['remote_skill_manager_invoke', { id: 'confirmed-host', command: 'takeover_center_agent_skills', args: { agentId: 'codex', unmanagedIds: ['unmanaged'] } }],
        ['remote_skill_manager_invoke', { id: 'confirmed-host', command: 'refresh_agent_skill_view_v2', args: { agentId: 'codex' } }],
      ])
    } finally {
      selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    }
  })

  it('routes the Skill Pack move preview and execution to the reviewed runtime', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const { LOCAL_RUNTIME_ENVIRONMENT_ID, useRuntimeEnvironmentStore } = await import('../stores/runtimeEnvironmentStore')
    Object.defineProperty(window, '__TAURI_INTERNALS__', { configurable: true, value: {} })
    const selectEnvironment = useRuntimeEnvironmentStore.getState().selectEnvironment
    const args = { targetId: 'target', packId: 'pack' }

    try {
      selectEnvironment('other-host')
      await skillApiV2.previewMoveDirectSkillToPack(args.targetId, args.packId, 'reviewed-host')
      await skillApiV2.moveDirectSkillToPack(args.targetId, args.packId, [], 'reviewed-host')

      expect(vi.mocked(invoke).mock.calls).toEqual([
        ['remote_skill_manager_invoke', { id: 'reviewed-host', command: 'preview_move_direct_skill_to_pack', args }],
        ['remote_skill_manager_invoke', {
          id: 'reviewed-host',
          command: 'execute_move_direct_skill_to_pack',
          args: { ...args, blockerDecisions: [] },
        }],
      ])
    } finally {
      selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    }
  })
})
