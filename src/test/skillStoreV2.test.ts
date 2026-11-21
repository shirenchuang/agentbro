import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { useSkillStoreV2, filteredSkills } from '../stores/skillStoreV2'
import type { AgentDetail, SkillSummary, SkillManagerOverview, SkillTargetDetail, UnmanagedItemDto } from '../services/skillApiV2'
import { skillApiV2 } from '../services/skillApiV2'
import { LOCAL_RUNTIME_ENVIRONMENT_ID, useRuntimeEnvironmentStore } from '../stores/runtimeEnvironmentStore'

function makeSkill(overrides: Partial<SkillSummary> = {}): SkillSummary {
  return {
    id: 'test-driven-development',
    name: 'Test Driven Development',
    description: 'Write tests first',
    skillType: 'skill',
    sourceType: 'local_folder',
    sourceUri: null,
    centerPath: '/center/tdd',
    currentHash: 'abc123',
    status: 'ok',
    installedAgents: [],
    ...overrides,
  }
}

function makeOverview(overrides: Partial<SkillManagerOverview> = {}): SkillManagerOverview {
  return {
    metrics: { centerSkillCount: 0, targetCount: 0, unmanagedCount: 0, issueCount: 0 },
    skills: [],
    agents: [],
    packs: [],
    issues: [],
    settings: {
      centerPath: '~/.agentbro/skills',
      sqlitePath: '~/.agentbro/skill-manager.db',
      defaultDistributeMode: 'link',
      linkFailPolicy: 'ask',
      startupScan: true,
      showUnmanaged: true,
    },
    ...overrides,
  }
}

function makeAgentDetail(id: string): AgentDetail {
  return {
    id,
    displayName: id,
    iconKey: id,
    version: null,
    latestVersion: null,
    skillsDir: null,
    configPath: null,
    mcpConfigPath: null,
    pluginDir: null,
    skills: [],
    inheritedManagedSkills: [],
    inheritedUnmanagedSkills: [],
    appliedPacks: [],
    availablePacks: [],
    mcpServers: [],
    plugins: [],
    health: [],
  }
}

describe('skillStoreV2 view mode + filters', () => {
  beforeEach(() => {
    useSkillStoreV2.setState({
      viewMode: 'cards',
      filters: { query: '', source: '', status: '', type: '' },
      skills: [
        makeSkill(),
        makeSkill({ id: 'github-code-review', name: 'GitHub Code Review', sourceType: 'github', status: 'conflict' }),
        makeSkill({ id: 'database-debugging', name: 'Database Debugging', status: 'copyDiverged' }),
      ],
    })
  })

  it('toggles view mode between cards and list', () => {
    expect(useSkillStoreV2.getState().viewMode).toBe('cards')
    useSkillStoreV2.getState().setViewMode('list')
    expect(useSkillStoreV2.getState().viewMode).toBe('list')
    useSkillStoreV2.getState().setViewMode('cards')
    expect(useSkillStoreV2.getState().viewMode).toBe('cards')
  })

  it('filters by query across name/description/source', () => {
    const state = useSkillStoreV2.getState()
    state.setFilter('query', 'github')
    const result = filteredSkills(useSkillStoreV2.getState())
    expect(result.map((s) => s.id)).toEqual(['github-code-review'])
  })

  it('filters by status', () => {
    const state = useSkillStoreV2.getState()
    state.setFilter('status', 'copyDiverged')
    const result = filteredSkills(useSkillStoreV2.getState())
    expect(result.map((s) => s.id)).toEqual(['database-debugging'])
  })

  it('finds changed copy installs by their visible status text', () => {
    useSkillStoreV2.setState({
      filters: { query: '副本分叉', source: '', status: '', type: '' },
      skills: [
        makeSkill({
          id: 'ai-daily-briefing',
          name: 'AI Daily Briefing',
          status: 'ok',
          installedAgents: [
            { agentId: 'hermes', displayName: 'Hermes', iconKey: 'hermes', mode: 'copy', status: 'copy_modified' },
          ],
        }),
      ],
    })

    const result = filteredSkills(useSkillStoreV2.getState())

    expect(result.map((s) => s.id)).toEqual(['ai-daily-briefing'])
  })

  it('filters by source', () => {
    const state = useSkillStoreV2.getState()
    state.setFilter('source', 'github')
    const result = filteredSkills(useSkillStoreV2.getState())
    expect(result.map((s) => s.id)).toEqual(['github-code-review'])
  })

  it('clearing filters restores all', () => {
    const state = useSkillStoreV2.getState()
    state.setFilter('query', 'github')
    expect(filteredSkills(useSkillStoreV2.getState()).length).toBe(1)
    state.setFilter('query', '')
    expect(filteredSkills(useSkillStoreV2.getState()).length).toBe(3)
  })
})

describe('skillStoreV2 startup scan setting', () => {
  afterEach(() => {
    vi.restoreAllMocks()
    useSkillStoreV2.setState({ startupScanInFlight: false, busyAction: null })
  })

  beforeEach(() => {
    useSkillStoreV2.setState({
      overview: null,
      settings: null,
      skills: [],
      agents: [],
      packs: [],
      issues: [],
      unmanaged: [],
      loading: false,
      error: null,
      initialized: false,
      startupScanInFlight: false,
      lastOverviewLoadedAt: 0,
    })
  })

  it('runs a full skill manager scan on init when startupScan is enabled', async () => {
    const overview: SkillManagerOverview = {
      metrics: { centerSkillCount: 0, targetCount: 0, unmanagedCount: 0, issueCount: 0 },
      skills: [],
      agents: [],
      packs: [],
      issues: [],
      settings: {
        centerPath: '~/.agentbro/skills',
        sqlitePath: '~/.agentbro/skill-manager.db',
        defaultDistributeMode: 'link',
        linkFailPolicy: 'ask',
        startupScan: true,
        showUnmanaged: true,
      },
    }
    vi.spyOn(skillApiV2, 'bootstrap').mockResolvedValue(undefined)
    const fullScan = vi.spyOn(skillApiV2, 'init').mockResolvedValue(undefined)
    vi.spyOn(skillApiV2, 'overview').mockResolvedValue(overview)

    await useSkillStoreV2.getState().init()

    expect(fullScan).toHaveBeenCalledTimes(1)
  })

  it('does not wait for the startup scan before init resolves', async () => {
    let resolveScan: (() => void) | undefined
    const scanPromise = new Promise<void>((resolve) => {
      resolveScan = resolve
    })
    const overview: SkillManagerOverview = {
      metrics: { centerSkillCount: 1, targetCount: 0, unmanagedCount: 0, issueCount: 0 },
      skills: [makeSkill()],
      agents: [],
      packs: [],
      issues: [],
      settings: {
        centerPath: '~/.agentbro/skills',
        sqlitePath: '~/.agentbro/skill-manager.db',
        defaultDistributeMode: 'link',
        linkFailPolicy: 'ask',
        startupScan: true,
        showUnmanaged: true,
      },
    }
    vi.spyOn(skillApiV2, 'bootstrap').mockResolvedValue(undefined)
    const fullScan = vi.spyOn(skillApiV2, 'init').mockReturnValue(scanPromise)
    vi.spyOn(skillApiV2, 'overview').mockResolvedValue(overview)

    await useSkillStoreV2.getState().init()

    expect(fullScan).toHaveBeenCalledTimes(1)
    expect(useSkillStoreV2.getState().initialized).toBe(true)
    expect(useSkillStoreV2.getState().loading).toBe(false)
    expect(useSkillStoreV2.getState().startupScanInFlight).toBe(true)

    resolveScan?.()
    await Promise.resolve()
    await Promise.resolve()
    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(useSkillStoreV2.getState().startupScanInFlight).toBe(false)
  })

  it('skips the full skill manager scan on init when startupScan is disabled', async () => {
    const overview: SkillManagerOverview = {
      metrics: { centerSkillCount: 0, targetCount: 0, unmanagedCount: 0, issueCount: 0 },
      skills: [],
      agents: [],
      packs: [],
      issues: [],
      settings: {
        centerPath: '~/.agentbro/skills',
        sqlitePath: '~/.agentbro/skill-manager.db',
        defaultDistributeMode: 'link',
        linkFailPolicy: 'ask',
        startupScan: false,
        showUnmanaged: true,
      },
    }
    vi.spyOn(skillApiV2, 'bootstrap').mockResolvedValue(undefined)
    const fullScan = vi.spyOn(skillApiV2, 'init').mockResolvedValue(undefined)
    vi.spyOn(skillApiV2, 'overview').mockResolvedValue(overview)

    await useSkillStoreV2.getState().init()

    expect(fullScan).not.toHaveBeenCalled()
  })
})

describe('skillStoreV2 refresh', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    useSkillStoreV2.setState({
      overview: null,
      skills: [],
      agents: [],
      packs: [],
      issues: [],
      loading: false,
      error: null,
      initialized: false,
      lastOverviewLoadedAt: 0,
    })
  })

  it('loads the refreshed overview and unmanaged list without running a second overview', async () => {
    const overview = makeOverview({
      metrics: { centerSkillCount: 1, targetCount: 0, unmanagedCount: 0, issueCount: 0 },
      skills: [makeSkill({ id: 'fast-refresh', name: 'Fast Refresh' })],
    })
    const refreshOverview = vi.spyOn(skillApiV2, 'refreshOverview').mockResolvedValue(overview)
    const refresh = vi.spyOn(skillApiV2, 'refresh').mockResolvedValue(undefined)
    const loadOverview = vi.spyOn(skillApiV2, 'overview').mockResolvedValue(makeOverview())
    const listUnmanaged = vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])

    await useSkillStoreV2.getState().refresh()

    expect(refreshOverview).toHaveBeenCalledTimes(1)
    expect(refreshOverview).toHaveBeenCalledWith(LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(refresh).not.toHaveBeenCalled()
    expect(loadOverview).not.toHaveBeenCalled()
    expect(listUnmanaged).toHaveBeenCalledTimes(1)
    expect(listUnmanaged).toHaveBeenCalledWith(LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(useSkillStoreV2.getState().skills.map((skill) => skill.id)).toEqual(['fast-refresh'])
  })

  it.each([false, true])('drops an old full refresh after changing runtime (returned: %s)', async (returnToLocal) => {
    let finishScan!: (value: SkillManagerOverview) => void
    const scan = new Promise<SkillManagerOverview>((resolve) => { finishScan = resolve })
    vi.spyOn(skillApiV2, 'refreshOverview').mockReturnValueOnce(scan)
    const unmanaged = vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    const { selectEnvironment } = useRuntimeEnvironmentStore.getState()
    const previous = useSkillStoreV2.getState().refresh()

    try {
      selectEnvironment('different-skill-runtime')
      if (returnToLocal) selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
      useSkillStoreV2.setState({
        runtimeEnvironmentId: returnToLocal ? LOCAL_RUNTIME_ENVIRONMENT_ID : 'different-skill-runtime',
        loading: true,
        error: 'Current runtime status',
      })
      finishScan(makeOverview({ skills: [makeSkill()] }))
      await previous

      expect(unmanaged).not.toHaveBeenCalled()
      expect(useSkillStoreV2.getState()).toMatchObject({
        overview: null, loading: true, error: 'Current runtime status', initialized: false,
      })
    } finally {
      selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
      useSkillStoreV2.setState({ runtimeEnvironmentId: LOCAL_RUNTIME_ENVIRONMENT_ID, loading: false })
    }
  })

  it('does not initialize the new runtime when an old refresh finishes loading projects', async () => {
    vi.spyOn(skillApiV2, 'refreshOverview').mockResolvedValue(makeOverview())
    vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    let projectReadStarted!: () => void
    let finishProjects!: () => void
    const readingProjects = new Promise<void>((resolve) => { projectReadStarted = resolve })
    const projects = new Promise<[]>((resolve) => { finishProjects = () => resolve([]) })
    vi.spyOn(skillApiV2, 'listProjects').mockImplementationOnce(() => {
      projectReadStarted()
      return projects
    })
    const { selectEnvironment } = useRuntimeEnvironmentStore.getState()
    const previous = useSkillStoreV2.getState().refresh()
    await readingProjects

    try {
      selectEnvironment('different-skill-runtime')
      useSkillStoreV2.setState({ runtimeEnvironmentId: 'different-skill-runtime', initialized: false, loading: true })
      finishProjects()
      await previous

      expect(useSkillStoreV2.getState()).toMatchObject({ initialized: false, loading: true })
    } finally {
      selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
      useSkillStoreV2.setState({ runtimeEnvironmentId: LOCAL_RUNTIME_ENVIRONMENT_ID, loading: false })
    }
  })

  it('keeps the results of the newest full refresh when an earlier one finishes late', async () => {
    let finishOldScan!: (value: SkillManagerOverview) => void
    const oldScan = new Promise<SkillManagerOverview>((resolve) => { finishOldScan = resolve })
    const current = makeOverview({ skills: [makeSkill({ id: 'current-skill' })] })
    vi.spyOn(skillApiV2, 'refreshOverview').mockReturnValueOnce(oldScan).mockResolvedValueOnce(current)
    const unmanaged = vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    const refresh = useSkillStoreV2.getState().refresh
    const previous = refresh()
    await refresh()
    finishOldScan(makeOverview())
    await previous

    expect(unmanaged).toHaveBeenCalledTimes(1)
    expect(useSkillStoreV2.getState().overview).toEqual(current)
  })

  it('reports only the current full refresh failure', async () => {
    let failOldScan!: (error: Error) => void
    const oldScan = new Promise<SkillManagerOverview>((_resolve, reject) => { failOldScan = reject })
    vi.spyOn(skillApiV2, 'refreshOverview')
      .mockReturnValueOnce(oldScan)
      .mockRejectedValueOnce(new Error('Current scan failed'))
    const refresh = useSkillStoreV2.getState().refresh

    const previous = refresh()
    await refresh()
    failOldScan(new Error('Old scan failed'))
    await previous

    expect(useSkillStoreV2.getState()).toMatchObject({ error: 'Error: Current scan failed', loading: false })
  })

  it('clears stale unmanaged rows when loading overview after Agent sync', async () => {
    const staleUnmanaged: UnmanagedItemDto = {
      id: 'unm-cursor-alpha',
      itemType: 'agent_skill',
      agentId: 'cursor',
      path: '/Users/me/.cursor/skills/alpha',
      inferredSkillId: 'alpha',
      hash: 'hash-alpha',
      reason: 'not_in_center_library',
    }
    useSkillStoreV2.setState({ unmanaged: [staleUnmanaged], lastOverviewLoadedAt: 0 })
    vi.spyOn(skillApiV2, 'overview').mockResolvedValue(makeOverview())
    vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])

    await useSkillStoreV2.getState().loadOverview(true)

    expect(useSkillStoreV2.getState().unmanaged).toEqual([])
  })

  it('does not fetch inventory from a new runtime during an in-flight overview', async () => {
    const selectEnvironment = useRuntimeEnvironmentStore.getState().selectEnvironment
    let resolveOverview!: (overview: SkillManagerOverview) => void
    const pendingOverview = new Promise<SkillManagerOverview>((resolve) => { resolveOverview = resolve })
    vi.spyOn(skillApiV2, 'overview').mockReturnValue(pendingOverview)
    const listUnmanaged = vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    const refresh = useSkillStoreV2.getState().loadOverview(true)

    try {
      selectEnvironment('new-runtime')
      resolveOverview(makeOverview())
      await refresh

      expect(listUnmanaged).not.toHaveBeenCalled()
      expect(useSkillStoreV2.getState().overview).toBeNull()
    } finally {
      selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    }
  })
})

describe('skillStoreV2 Pack selection', () => {
  afterEach(() => {
    vi.restoreAllMocks()
    useRuntimeEnvironmentStore.getState().selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    useSkillStoreV2.setState({ selectedPackId: null, selectedPackDetail: null, error: null })
  })

  it.each(['cleared', 'switched', 'returned'] as const)('ignores an old pack error after the selection was %s', async (scenario) => {
    const { selectPack } = useSkillStoreV2.getState()
    const { selectEnvironment } = useRuntimeEnvironmentStore.getState()
    let rejectDetail!: (error: Error) => void
    const detail = vi.spyOn(skillApiV2, 'getPackDetail').mockReturnValueOnce(new Promise((_resolve, reject) => {
      rejectDetail = reject
    }))
    const previous = selectPack('previous-pack')
    expect(detail).toHaveBeenCalledOnce()

    if (scenario === 'cleared') {
      await selectPack(null)
    } else {
      selectEnvironment('other-pack-runtime')
      if (scenario === 'returned') selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    }
    useSkillStoreV2.setState({ error: 'Current status' })
    rejectDetail(new Error('Stale pack failure'))
    await previous

    expect(useSkillStoreV2.getState().error).toBe('Current status')
  })

  it('reports an error from the latest request for the selected pack', async () => {
    vi.spyOn(skillApiV2, 'getPackDetail').mockRejectedValueOnce(new Error('Active pack failure'))

    await useSkillStoreV2.getState().selectPack('active-pack')

    expect(useSkillStoreV2.getState().error).toBe('Error: Active pack failure')
  })
})

describe('skillStoreV2 tab navigation', () => {
  it('switches active tab', () => {
    useSkillStoreV2.getState().setTab('packs')
    expect(useSkillStoreV2.getState().activeTab).toBe('packs')
    useSkillStoreV2.getState().setTab('diagnostics')
    expect(useSkillStoreV2.getState().activeTab).toBe('diagnostics')
  })

  it('switches the install subtab', () => {
    useSkillStoreV2.getState().setInstallTab('agent')
    expect(useSkillStoreV2.getState().activeInstallTab).toBe('agent')
  })
})

describe('skillStoreV2 Agent selection', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  beforeEach(() => {
    useSkillStoreV2.setState({
      selectedAgentId: null,
      selectedAgentDetail: null,
      agentDetailLoading: false,
      error: null,
    })
  })

  it('ignores an older detail response after selecting another Agent', async () => {
    let resolveClaude!: (detail: AgentDetail) => void
    let resolveCodex!: (detail: AgentDetail) => void
    vi.spyOn(skillApiV2, 'getAgentDetail').mockImplementation((agentId) => (
      new Promise<AgentDetail>((resolve) => {
        if (agentId === 'claude-code') resolveClaude = resolve
        else resolveCodex = resolve
      })
    ))

    const claudeSelection = useSkillStoreV2.getState().selectAgent('claude-code')
    const codexSelection = useSkillStoreV2.getState().selectAgent('codex')

    resolveClaude(makeAgentDetail('claude-code'))
    await claudeSelection

    expect(useSkillStoreV2.getState().selectedAgentId).toBe('codex')
    expect(useSkillStoreV2.getState().selectedAgentDetail).toBeNull()
    expect(useSkillStoreV2.getState().agentDetailLoading).toBe(true)

    resolveCodex(makeAgentDetail('codex'))
    await codexSelection

    expect(useSkillStoreV2.getState().selectedAgentDetail?.id).toBe('codex')
    expect(useSkillStoreV2.getState().agentDetailLoading).toBe(false)
  })

  it('keeps the latest detail when returning to the same Agent', async () => {
    let resolveOlder!: (detail: AgentDetail) => void
    vi.spyOn(skillApiV2, 'getAgentDetail')
      .mockImplementationOnce(() => new Promise((resolve) => { resolveOlder = resolve }))
      .mockResolvedValueOnce(makeAgentDetail('codex'))
      .mockResolvedValueOnce({ ...makeAgentDetail('claude-code'), version: '2.0.0' })

    const olderSelection = useSkillStoreV2.getState().selectAgent('claude-code')
    await useSkillStoreV2.getState().selectAgent('codex')
    await useSkillStoreV2.getState().selectAgent('claude-code')
    resolveOlder({ ...makeAgentDetail('claude-code'), version: '1.0.0' })
    await olderSelection

    expect(useSkillStoreV2.getState().selectedAgentDetail?.version).toBe('2.0.0')
    expect(useSkillStoreV2.getState().agentDetailLoading).toBe(false)
  })

  it('leaves the selected Agent ready when an action refreshes another Agent', async () => {
    const detail = makeAgentDetail('claude-code')
    useSkillStoreV2.setState({ selectedAgentId: detail.id, selectedAgentDetail: detail })
    const getAgentDetail = vi.spyOn(skillApiV2, 'getAgentDetail').mockResolvedValue(makeAgentDetail('codex'))

    await useSkillStoreV2.getState().loadAgentDetail('codex', true)

    expect(getAgentDetail).not.toHaveBeenCalled()
    expect(useSkillStoreV2.getState().selectedAgentDetail).toEqual(detail)
    expect(useSkillStoreV2.getState().agentDetailLoading).toBe(false)
  })

  it('keeps a published Agent snapshot when an older detail read finishes later', async () => {
    let resolveOlder!: (detail: AgentDetail) => void
    vi.spyOn(skillApiV2, 'getAgentDetail')
      .mockImplementationOnce(() => new Promise((resolve) => { resolveOlder = resolve }))
    useSkillStoreV2.setState({ runtimeEnvironmentId: 'local', selectedAgentId: 'codex' })

    const olderRead = useSkillStoreV2.getState().loadAgentDetail('codex', true)
    const snapshot = {
      agentDetail: { ...makeAgentDetail('codex'), version: '2.0.0' },
      overview: makeOverview(),
      unmanaged: [],
    }
    expect(useSkillStoreV2.getState().applyAgentSkillViewSnapshot('local', 'codex', snapshot)).toBe(true)
    resolveOlder({ ...makeAgentDetail('codex'), version: '1.0.0' })
    await olderRead

    expect(useSkillStoreV2.getState().selectedAgentDetail).toEqual(snapshot.agentDetail)
    expect(useSkillStoreV2.getState().agentDetailLoading).toBe(false)
  })
})

describe('skillStoreV2 overview shape', () => {
  it('accepts a v2 overview payload', () => {
    const overview: SkillManagerOverview = {
      metrics: { centerSkillCount: 3, targetCount: 2, unmanagedCount: 1, issueCount: 1 },
      skills: [makeSkill()],
      agents: [],
      packs: [],
      issues: [],
      settings: {
        centerPath: '~/.agentbro/skills',
        sqlitePath: '~/.agentbro/skill-manager.db',
        defaultDistributeMode: 'link',
        linkFailPolicy: 'ask',
        startupScan: true,
        showUnmanaged: true,
      },
    }
    expect(overview.metrics.centerSkillCount).toBe(3)
    expect(overview.skills[0].id).toBe('test-driven-development')
    expect(overview.settings.defaultDistributeMode).toBe('link')
  })
})

describe('skillStoreV2 Agent Skill delete reconciliation', () => {
  const regularTarget: SkillTargetDetail = {
    id: 'target-codex',
    skillId: 'regular',
    agentId: 'codex',
    targetPath: '/Users/me/.codex/skills/regular',
    resolvedTargetPath: null,
    installMode: 'link',
    actualMode: 'link',
    sourceHash: 'hash',
    currentHash: null,
    status: 'ok',
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    claims: [],
  }
  const sharedTarget: SkillTargetDetail = {
    ...regularTarget,
    id: 'target-shared',
    skillId: 'shared',
    agentId: 'agents',
    targetPath: '/Users/me/.agents/skills/shared',
  }
  const regularUnmanaged: UnmanagedItemDto = {
    id: 'unmanaged-codex',
    itemType: 'skill',
    agentId: 'codex',
    path: '/Users/me/.codex/skills/unmanaged',
    inferredSkillId: 'unmanaged',
    hash: null,
    reason: 'not_in_center_library',
  }
  const sharedUnmanaged: UnmanagedItemDto = {
    ...regularUnmanaged,
    id: 'unmanaged-shared',
    agentId: 'agents',
    path: '/Users/me/.agents/skills/unmanaged-shared',
  }

  beforeEach(() => {
    const detail = makeAgentDetail('codex')
    detail.skills = [regularTarget]
    detail.inheritedManagedSkills = [sharedTarget]
    detail.inheritedUnmanagedSkills = [sharedUnmanaged]
    const overview = makeOverview({
      metrics: { centerSkillCount: 2, targetCount: 2, unmanagedCount: 2, issueCount: 0 },
      agents: [{
        id: 'codex',
        displayName: 'Codex',
        iconKey: 'codex',
        enabled: true,
        skillsDir: '/Users/me/.codex/skills',
        version: null,
        latestVersion: null,
        installed: true,
        managedSkillCount: 1,
        unmanagedSkillCount: 1,
      }],
    })
    useSkillStoreV2.setState({
      runtimeEnvironmentId: 'local',
      selectedAgentId: 'codex',
      selectedAgentDetail: detail,
      overview,
      agents: overview.agents,
      unmanaged: [regularUnmanaged, sharedUnmanaged],
    })
  })

  it('removes successful regular and inherited items atomically', () => {
    const applied = useSkillStoreV2.getState().removeAgentSkillItems(
      'local',
      'codex',
      ['target-shared'],
      ['unmanaged-shared'],
    )

    expect(applied).toBe(true)
    expect(useSkillStoreV2.getState().selectedAgentDetail?.skills).toEqual([regularTarget])
    expect(useSkillStoreV2.getState().selectedAgentDetail?.inheritedManagedSkills).toEqual([])
    expect(useSkillStoreV2.getState().selectedAgentDetail?.inheritedUnmanagedSkills).toEqual([])
    expect(useSkillStoreV2.getState().unmanaged).toEqual([regularUnmanaged])
    expect(useSkillStoreV2.getState().overview?.metrics).toMatchObject({ targetCount: 1, unmanagedCount: 1 })
  })

  it('rejects snapshots from a stale runtime or Agent', () => {
    const snapshot = {
      agentDetail: makeAgentDetail('codex'),
      overview: makeOverview(),
      unmanaged: [],
    }

    expect(useSkillStoreV2.getState().applyAgentSkillViewSnapshot('remote', 'codex', snapshot)).toBe(false)
    expect(useSkillStoreV2.getState().applyAgentSkillViewSnapshot('local', 'claude-code', snapshot)).toBe(false)
    expect(useSkillStoreV2.getState().selectedAgentDetail?.skills).toEqual([regularTarget])
    expect(useSkillStoreV2.getState().applyAgentSkillViewSnapshot('local', 'codex', snapshot)).toBe(true)
    expect(useSkillStoreV2.getState().selectedAgentDetail?.skills).toEqual([])
  })
})
