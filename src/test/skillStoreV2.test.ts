import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { useSkillStoreV2, filteredSkills } from '../stores/skillStoreV2'
import type { SkillSummary, SkillManagerOverview } from '../services/skillApiV2'
import { skillApiV2 } from '../services/skillApiV2'

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

describe('skillStoreV2 tab navigation', () => {
  it('switches active tab', () => {
    useSkillStoreV2.getState().setTab('packs')
    expect(useSkillStoreV2.getState().activeTab).toBe('packs')
    useSkillStoreV2.getState().setTab('diagnostics')
    expect(useSkillStoreV2.getState().activeTab).toBe('diagnostics')
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
