import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, screen, fireEvent, cleanup, waitFor, act, within } from '@testing-library/react'
import '@testing-library/jest-dom/vitest'
import { AgentIconBadge } from '../components/skills-v2/AgentIconBadge'
import { useSkillStoreV2 } from '../stores/skillStoreV2'
import { useSessionStore } from '../stores/sessionStore'
import { skillApiV2 } from '../services/skillApiV2'
import { agentApi, type AgentOutputEvent, type AgentPackageOutput, type AgentProgramInfo } from '../services/agentApi'
import * as tauriApi from '../services/tauriApi'
import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog'
import { open as openShell } from '@tauri-apps/plugin-shell'
import i18n from '../i18n'
import type { SkillSummary, AgentSummary, AgentDetail, AgentSkillInventoryAgent, AgentSkillViewSnapshot, AdoptPreview, DistributionPreview, MoveDirectSkillToPackPreview, SkillPackDetail, SkillPackManifest, SkillDetail, SkillTargetDetail, UnmanagedItemDto } from '../services/skillApiV2'
import type { AgentType, SessionState } from '../types/agent'
import { useConfigStore } from '../stores/configStore'
import {
  LOCAL_RUNTIME_ENVIRONMENT_ID,
  useRuntimeEnvironmentStore,
} from '../stores/runtimeEnvironmentStore'

// SkillManagerShell imports pages that call skillApiV2 at mount; we stub the api
// so tests run without the Tauri runtime.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(undefined) }))
vi.mock('@tauri-apps/api/path', async (importOriginal) => ({
  ...await importOriginal<typeof import('@tauri-apps/api/path')>(),
  homeDir: vi.fn().mockResolvedValue('/Profiles/owner/'),
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn().mockResolvedValue(null), save: vi.fn().mockResolvedValue(null) }))
vi.mock('@tauri-apps/plugin-shell', () => ({ open: vi.fn().mockResolvedValue(undefined) }))

function makeSkill(overrides: Partial<SkillSummary> = {}): SkillSummary {
  return {
    id: 'release-checklist',
    name: 'Release Checklist',
    description: 'Pre-release QA skill',
    skillType: 'skill',
    sourceType: 'local_folder',
    sourceUri: null,
    centerPath: '/center/release-checklist',
    currentHash: 'hash1',
    status: 'ok',
    installedAgents: [
      { agentId: 'claude-code', displayName: 'Claude Code', iconKey: 'claude-code', mode: 'link', status: 'ok' },
      { agentId: 'codex', displayName: 'Codex', iconKey: 'codex', mode: 'copy', status: 'ok' },
    ],
    ...overrides,
  }
}

function makeTarget(overrides: Partial<SkillTargetDetail> = {}): SkillTargetDetail {
  return {
    id: 'target-claude',
    skillId: 'release-checklist',
    agentId: 'claude-code',
    targetPath: '/Users/me/.claude/skills/release-checklist',
    resolvedTargetPath: null,
    installMode: 'link',
    actualMode: 'link',
    sourceHash: 'hash1',
    currentHash: 'hash1',
    status: 'ok',
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    claims: [],
    ...overrides,
  }
}

function makeSharedTarget(skillId: string, overrides: Partial<SkillTargetDetail> = {}): SkillTargetDetail {
  return makeTarget({
    id: `target-${skillId}`,
    skillId,
    agentId: 'agents',
    targetPath: `/Users/me/.agents/skills/${skillId}`,
    resolvedTargetPath: `/Users/me/.agentbro/skills/${skillId}`,
    claims: [{
      id: `claim-${skillId}`,
      claimType: 'direct',
      packId: null,
      packName: null,
      createdAt: '2026-01-01T00:00:00Z',
    }],
    ...overrides,
  })
}

function makeSharedUnmanaged(skillId: string, overrides: Partial<UnmanagedItemDto> = {}): UnmanagedItemDto {
  return {
    id: `raw-${skillId}`,
    itemType: 'skill',
    agentId: 'agents',
    path: `/Users/me/.agents/skills/${skillId}`,
    inferredSkillId: skillId,
    hash: `hash-${skillId}`,
    reason: 'not_in_center_library',
    ...overrides,
  }
}

function makeSidebarAgent(id: string, displayName: string, counts: { managed?: number; unmanaged?: number } = {}): AgentSummary {
  return {
    id,
    displayName,
    iconKey: id,
    enabled: true,
    skillsDir: `/${id}`,
    version: null,
    latestVersion: null,
    installed: true,
    managedSkillCount: counts.managed ?? 0,
    unmanagedSkillCount: counts.unmanaged ?? 0,
  }
}

function makeSidebarSession(agentType: AgentType, overrides: Partial<SessionState> = {}): SessionState {
  const now = Date.now()
  return {
    id: `${agentType}-session`,
    agentType,
    project: 'Project',
    terminal: 'Terminal',
    phase: 'processing',
    startedAt: now,
    duration: 0,
    tokens: { input: 0, output: 0, cacheRead: 0, cacheCreate: 0 },
    chatHistory: [],
    subagents: [],
    activeTools: [],
    ...overrides,
  }
}

describe('AgentIconBadge', () => {
  beforeEach(cleanup)

  it('renders an agent label and copy modifier', () => {
    const { container } = render(<AgentIconBadge iconKey="codex" mode="copy" />)
    // codex has a real icon asset → renders an <img>
    expect(container.querySelector('img')).not.toBeNull()
    expect(container.querySelector('.sm2__agent-badge--copy')).not.toBeNull()
  })

  it('falls back gracefully for unknown agents', () => {
    const { container } = render(<AgentIconBadge iconKey="mystery-agent" />)
    expect(container.textContent).toContain('my')
  })
})

describe('Skill library view mode (no Agent matrix)', () => {
  beforeEach(() => {
    cleanup()
    vi.restoreAllMocks()
    useSkillStoreV2.setState({
      activeTab: 'library',
      activeInstallTab: 'official',
      viewMode: 'cards',
      filters: { query: '', source: '', status: '', type: '' },
      skills: [
        makeSkill(),
        makeSkill({ id: 'db-debug', name: 'Database Debugging', status: 'copyDiverged', installedAgents: [] }),
      ],
      overview: {
        metrics: { centerSkillCount: 2, targetCount: 2, unmanagedCount: 0, issueCount: 0 },
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
      },
      settings: useSkillStoreV2.getState().overview?.settings ?? null,
      agents: [
        {
          id: 'codex',
          displayName: 'Codex',
          iconKey: 'codex',
          enabled: true,
          skillsDir: '/Users/me/.codex/skills',
          version: '1.0.0',
          latestVersion: null,
          installed: true,
          managedSkillCount: 0,
          unmanagedSkillCount: 0,
        },
      ],
      packs: [],
      loading: false,
      error: null,
      initialized: true,
      selectedSkillId: null,
      selectedSkillDetail: null,
    })
  })

  it('opens Agent sync from the unmanaged metric', async () => {
    const overview = useSkillStoreV2.getState().overview!
    useSkillStoreV2.setState({
      overview: {
        ...overview,
        metrics: { ...overview.metrics, unmanagedCount: 1 },
      },
    })
    vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValue([])

    const { SkillManagerShell } = await import('../components/skills-v2/SkillManagerShell')
    render(<SkillManagerShell />)

    fireEvent.click(screen.getByRole('button', { name: '未管理 1' }))

    expect(useSkillStoreV2.getState().activeTab).toBe('install')
    expect(useSkillStoreV2.getState().activeInstallTab).toBe('agent')
    expect(screen.getByRole('button', { name: /Agent 同步/ })).toHaveClass('sm2__install-page-tab--active')
    expect(await screen.findByText('待处理收纳箱')).toBeInTheDocument()
  })

  it('renders both skills as cards by default', async () => {
    const { SkillLibraryPage } = await import('../components/skills-v2/SkillLibraryPage')
    render(<SkillLibraryPage />)
    expect(screen.getByText('Release Checklist')).toBeInTheDocument()
    expect(screen.getByText('Database Debugging')).toBeInTheDocument()
  })

  it('marks skills with changed copy installs in card and list views', async () => {
    useSkillStoreV2.setState({
      skills: [
        makeSkill({
          status: 'copyDiverged',
          installedAgents: [
            { agentId: 'codex', displayName: 'Codex', iconKey: 'codex', mode: 'copy', status: 'copy_modified' },
          ],
        }),
      ],
    })

    const { SkillLibraryPage } = await import('../components/skills-v2/SkillLibraryPage')
    render(<SkillLibraryPage />)

    expect(screen.getByText('Diff')).toBeInTheDocument()
    expect(screen.getByText('1 个副本有变更')).toBeInTheDocument()
    expect(document.body.querySelector('.sm2__copy-diff-strip')).toBeNull()

    fireEvent.click(screen.getByText('列表'))
    expect(screen.getByText(/Local folder · 副本分叉/)).toBeInTheDocument()
    expect(screen.getByText('1 个副本有变更')).toBeInTheDocument()
  })

  it('switches to list view and keeps content', async () => {
    const { SkillLibraryPage } = await import('../components/skills-v2/SkillLibraryPage')
    render(<SkillLibraryPage />)
    fireEvent.click(screen.getByText('列表'))
    expect(useSkillStoreV2.getState().viewMode).toBe('list')
    expect(screen.getByText('Release Checklist')).toBeInTheDocument()
    fireEvent.click(screen.getByText('卡片'))
    expect(useSkillStoreV2.getState().viewMode).toBe('cards')
  })

  it('filters center skills by skill pack membership', async () => {
    vi.spyOn(skillApiV2, 'getPackDetail').mockResolvedValue({
      id: 'writing-pack',
      name: '写作包',
      description: 'Writing workflow',
      tags: [],
      members: [
        { skillId: 'release-checklist', skillName: 'Release Checklist', required: true, sortOrder: 0, missing: false },
      ],
      appliedAgents: [],
      createdAt: '2026-01-01T00:00:00Z',
      updatedAt: '2026-01-01T00:00:00Z',
    })
    useSkillStoreV2.setState({
      packs: [
        { id: 'default', name: '中心库全量', description: '', tags: [], memberCount: 2, appliedAgentCount: 0, healthy: true },
        { id: 'writing-pack', name: '写作包', description: 'Writing workflow', tags: [], memberCount: 1, appliedAgentCount: 0, healthy: true },
      ],
    })

    const { SkillLibraryPage } = await import('../components/skills-v2/SkillLibraryPage')
    render(<SkillLibraryPage />)

    await waitFor(() => {
      expect(skillApiV2.getPackDetail).toHaveBeenCalledWith('writing-pack')
    })
    fireEvent.click(screen.getByRole('button', { name: /技能包/ }))
    fireEvent.click(screen.getByRole('option', { name: /写作包/ }))

    expect(screen.getByText('Release Checklist')).toBeInTheDocument()
    expect(screen.queryByText('Database Debugging')).not.toBeInTheDocument()
  })

  it('shows every skill pack membership in card and list views', async () => {
    vi.spyOn(skillApiV2, 'getPackDetail').mockImplementation(async (packId) => ({
      id: packId,
      name: packId === 'writing-pack' ? '写作包' : '发布包',
      description: '',
      tags: [],
      members: [
        { skillId: 'release-checklist', skillName: 'Release Checklist', required: true, sortOrder: 0, missing: false },
      ],
      appliedAgents: [],
      createdAt: '2026-01-01T00:00:00Z',
      updatedAt: '2026-01-01T00:00:00Z',
    }))
    useSkillStoreV2.setState({
      packs: [
        { id: 'default', name: '全量技能包', description: '', tags: [], memberCount: 2, appliedAgentCount: 0, healthy: true },
        { id: 'writing-pack', name: '写作包', description: '', tags: [], memberCount: 1, appliedAgentCount: 0, healthy: true },
        { id: 'release-pack', name: '发布包', description: '', tags: [], memberCount: 1, appliedAgentCount: 0, healthy: true },
      ],
    })

    const { SkillLibraryPage } = await import('../components/skills-v2/SkillLibraryPage')
    render(<SkillLibraryPage />)

    expect(await screen.findByLabelText('所属技能包：写作包、发布包')).toBeInTheDocument()
    expect(screen.getByLabelText('未加入技能包')).toBeInTheDocument()
    expect(skillApiV2.getPackDetail).not.toHaveBeenCalledWith('default')

    fireEvent.click(screen.getByText('列表'))

    expect(screen.getByLabelText('所属技能包：写作包、发布包')).toBeInTheDocument()
    expect(screen.getByLabelText('未加入技能包')).toBeInTheDocument()
  })

  it('does not render an Agent column matrix', async () => {
    const { SkillLibraryPage } = await import('../components/skills-v2/SkillLibraryPage')
    const { container } = render(<SkillLibraryPage />)
    // there must be no <table> in the library main area
    expect(container.querySelector('table')).toBeNull()
  })

  it('localizes source type labels in library cards and list rows', async () => {
    await i18n.changeLanguage('zh')
    useSkillStoreV2.setState({
      skills: [
        makeSkill({
          id: 'lark-wiki',
          name: 'lark-wiki',
          sourceType: 'agent_import',
          installedAgents: [],
        }),
      ],
    })

    const { SkillLibraryPage } = await import('../components/skills-v2/SkillLibraryPage')
    render(<SkillLibraryPage />)

    expect(screen.getAllByText('Agent 导入').length).toBeGreaterThan(0)
    expect(screen.queryByText('agent_import')).not.toBeInTheDocument()

    fireEvent.click(screen.getByText('列表'))
    expect(screen.getByText(/Agent 导入 · 正常/)).toBeInTheDocument()
    expect(screen.queryByText(/agent_import/)).not.toBeInTheDocument()
  })

  it('previews distribution for multiple selected skills at once', async () => {
    const previewDistribute = vi.spyOn(skillApiV2, 'previewDistribute').mockResolvedValue({
      skillIds: ['release-checklist', 'db-debug'],
      targetAgents: ['codex'],
      requestedMode: 'link',
      changes: [],
      blockers: [],
      blockerDecisions: [],
    })

    const { SkillLibraryPage } = await import('../components/skills-v2/SkillLibraryPage')
    render(<SkillLibraryPage />)

    fireEvent.click(screen.getByRole('button', { name: '批量管理' }))
    fireEvent.click(screen.getByLabelText('选择 Release Checklist'))
    fireEvent.click(screen.getByLabelText('选择 Database Debugging'))
    fireEvent.click(screen.getByRole('button', { name: /^分发 2 个 Skill$/ }))

    fireEvent.click(screen.getByText('Codex'))
    fireEvent.click(screen.getByRole('button', { name: '预览影响' }))

    await waitFor(() => {
      expect(previewDistribute).toHaveBeenCalledWith(['release-checklist', 'db-debug'], ['codex'], 'link')
    })
  })

  it('renders center delete choices as sibling footer actions', async () => {
    vi.spyOn(skillApiV2, 'getSkillDetail').mockResolvedValue({
      ...makeSkill(),
      frontmatter: {},
      files: null,
      targets: [],
      source: null,
    })
    vi.spyOn(skillApiV2, 'previewDeleteCenterSkill').mockResolvedValue({
      skillId: 'release-checklist',
      affectedTargets: [
        {
          targetId: 'target-claude',
          agentId: 'claude-code',
          displayName: 'Claude Code',
          targetPath: '/Users/me/.claude/skills/release-checklist',
          mode: 'link',
          claimCount: 1,
        },
        {
          targetId: 'target-codex',
          agentId: 'codex',
          displayName: 'Codex',
          targetPath: '/Users/me/.codex/skills/release-checklist',
          mode: 'copy',
          claimCount: 1,
        },
      ],
      removable: false,
      warnings: [],
    })

    const { SkillLibraryPage } = await import('../components/skills-v2/SkillLibraryPage')
    render(<SkillLibraryPage />)

    fireEvent.click(screen.getByText('Release Checklist'))
    fireEvent.click(await screen.findByRole('button', { name: '删除' }))

    const preserve = await screen.findByRole('button', { name: '删除但保留Agent副本' })
    const removeAll = screen.getByRole('button', { name: '删除并且移除Agent安装' })
    const actions = preserve.closest('.sm2__modal-actions')

    expect(actions).not.toBeNull()
    expect(actions).toContainElement(removeAll)
    expect(preserve).toHaveClass('sm2-delete-skill__choice')
    expect(removeAll).toHaveClass('sm2-delete-skill__choice')
    expect(preserve).not.toHaveClass('sm2__btn--primary')
    expect(removeAll).not.toHaveClass('sm2__btn--primary')
    expect(document.body.querySelector('.sm2-delete-skill__note')).not.toBeNull()
    expect(document.body.querySelector('.sm2-delete-skill__warnings')).toBeNull()
    expect(document.body.querySelector('.sm2-delete-skill__inline-action')).toBeNull()
  })

  it('deletes multiple selected center skills from the library', async () => {
    const previewDelete = vi.spyOn(skillApiV2, 'previewDeleteCenterSkills').mockResolvedValue({
      skillId: 'release-checklist',
      skillIds: ['release-checklist', 'db-debug'],
      affectedTargets: [
        {
          targetId: 'target-claude',
          agentId: 'claude-code',
          displayName: 'Claude Code',
          targetPath: '/Users/me/.claude/skills/release-checklist',
          mode: 'link',
          claimCount: 1,
        },
      ],
      removable: false,
      warnings: [],
    })
    const executeDelete = vi.spyOn(skillApiV2, 'executeDeleteCenterSkills').mockResolvedValue(undefined)

    const { SkillLibraryPage } = await import('../components/skills-v2/SkillLibraryPage')
    render(<SkillLibraryPage />)

    fireEvent.click(screen.getByRole('button', { name: '批量管理' }))
    fireEvent.click(screen.getByLabelText('选择 Release Checklist'))
    fireEvent.click(screen.getByLabelText('选择 Database Debugging'))
    fireEvent.click(screen.getByRole('button', { name: /^删除 2 个 Skill$/ }))

    expect(await screen.findByRole('heading', { name: '批量删除 2 个 Skill' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '删除但保留Agent副本' }))

    await waitFor(() => {
      expect(previewDelete).toHaveBeenCalledWith(['release-checklist', 'db-debug'])
      expect(executeDelete).toHaveBeenCalledWith(['release-checklist', 'db-debug'], false)
    })
  })

  it('can delete multiple agent distributions from the skill detail agent tab', async () => {
    const initialDetail: SkillDetail = {
      ...makeSkill(),
      frontmatter: {},
      files: null,
      targets: [
        makeTarget(),
        makeTarget({
          id: 'target-codex',
          agentId: 'codex',
          targetPath: '/Users/me/.codex/skills/release-checklist',
          installMode: 'copy',
          actualMode: 'copy',
        }),
      ],
      source: null,
    }
    const refreshedDetail: SkillDetail = {
      ...initialDetail,
      targets: [],
      installedAgents: [],
    }
    vi.spyOn(skillApiV2, 'getSkillDetail')
      .mockResolvedValueOnce(initialDetail)
      .mockResolvedValueOnce(refreshedDetail)
    const deleteDistribution = vi.spyOn(skillApiV2, 'deleteSkillTargetDistribution').mockResolvedValue(undefined)

    const { SkillDetailSlider } = await import('../components/skills-v2/SkillDetailSlider')
    render(
      <SkillDetailSlider
        skillId="release-checklist"
        open
        onClose={() => {}}
      />,
    )

    fireEvent.click(await screen.findByRole('button', { name: 'Agent (2)' }))
    fireEvent.click(screen.getByRole('button', { name: '批量删除分发' }))
    fireEvent.click(screen.getByLabelText('选择 Claude Code 的 Skill 分发'))
    fireEvent.click(screen.getByLabelText('选择 Codex 的 Skill 分发'))
    fireEvent.click(screen.getByRole('button', { name: '删除 2 个分发' }))
    fireEvent.click(await screen.findByRole('button', { name: '确认删除' }))

    await waitFor(() => {
      expect(deleteDistribution).toHaveBeenCalledWith('target-claude')
      expect(deleteDistribution).toHaveBeenCalledWith('target-codex')
    })
    expect(deleteDistribution).toHaveBeenCalledTimes(2)
  })
})

describe('market install state', () => {
  it('matches installed skills by v2 source URI across skills.sh and GitHub aliases', async () => {
    const { isMarketItemInstalled } = await import('../components/skills-v2/marketInstallState')
    const marketSkill = {
      id: 'skillssh:vercel-labs@skills@find-skills',
      registryId: 'skills-sh',
      name: 'find-skills',
      description: null,
      source: 'vercel-labs/skills',
      installCount: 1,
      downloadUrl: 'skillssh:vercel-labs/skills/find-skills',
      webUrl: 'https://skills.sh/vercel-labs/skills/find-skills',
      isInstalled: false,
      syncedAt: '2026-01-01T00:00:00Z',
      cacheUpdatedAt: null,
    }

    expect(isMarketItemInstalled(marketSkill, [
      makeSkill({
        id: 'find-skills',
        sourceType: 'github',
        sourceUri: 'github:vercel-labs/skills/find-skills',
      }),
    ])).toBe(true)
  })
})

describe('Marketplace install flow', () => {
  beforeEach(async () => {
    cleanup()
    vi.restoreAllMocks()
    await i18n.changeLanguage('zh')
    useSkillStoreV2.setState({
      skills: [],
      packs: [
        {
          id: 'anthropics-skills',
          name: 'anthropics/skills',
          description: 'Anthropic skills',
          tags: ['anthropics/skills'],
          memberCount: 8,
          appliedAgentCount: 0,
          healthy: true,
        },
      ],
      agents: [],
      overview: null,
      settings: null,
      lastOverviewLoadedAt: Date.now(),
      marketplaceInstallTask: null,
      loadOverview: vi.fn().mockResolvedValue(undefined),
      loadProjects: vi.fn().mockResolvedValue(undefined),
    } as Partial<ReturnType<typeof useSkillStoreV2.getState>>)
  })

  afterEach(() => {
    useSkillStoreV2.setState({ marketplaceInstallTask: null })
  })

  function marketSkill(overrides: Partial<Awaited<ReturnType<typeof skillApiV2.searchMarketplaceSkills>>[number]> = {}) {
    return {
      id: 'skillssh:anthropics@skills@brand-guidelines',
      registryId: 'skills-sh',
      name: 'brand-guidelines',
      description: 'Brand skill',
      source: 'anthropics/skills',
      installCount: 100,
      downloadUrl: 'skillssh:anthropics/skills/brand-guidelines',
      webUrl: 'https://skills.sh/anthropics/skills/brand-guidelines',
      isInstalled: false,
      syncedAt: '2026-01-01T00:00:00Z',
      cacheUpdatedAt: null,
      ...overrides,
    }
  }

  it('installs an outer marketplace card directly without opening the pack dialog', async () => {
    vi.spyOn(skillApiV2, 'searchMarketplaceSkills').mockResolvedValue([marketSkill()])
    const executeAdd = vi.spyOn(skillApiV2, 'executeAddCenterSkill').mockResolvedValue({
      skillIds: ['brand-guidelines'],
      updated: [],
      skipped: [],
    })
    const onDone = vi.fn()

    const { MarketPanel } = await import('../components/skills-v2/InstallView')
    render(<MarketPanel onInstall={() => {}} onDone={onDone} />)

    const addButton = await screen.findByTitle('安装')
    fireEvent.click(addButton)

    await waitFor(() => expect(executeAdd).toHaveBeenCalled())
    expect(screen.queryByText('安装「brand-guidelines」')).not.toBeInTheDocument()
    expect(onDone).toHaveBeenCalledWith('brand-guidelines')
  })

  it('labels an installed outer marketplace card action as distribution', async () => {
    useSkillStoreV2.setState({
      skills: [
        makeSkill({
          id: 'brand-guidelines',
          name: 'brand-guidelines',
          sourceType: 'skillssh',
          sourceUri: 'skillssh:anthropics/skills/brand-guidelines',
          installedAgents: [],
        }),
      ],
    })
    vi.spyOn(skillApiV2, 'searchMarketplaceSkills').mockResolvedValue([marketSkill()])

    const { MarketPanel } = await import('../components/skills-v2/InstallView')
    render(<MarketPanel onInstall={() => {}} onDone={() => {}} />)

    const distributeButton = await screen.findByTitle('分发到 Agent')
    expect(distributeButton).toHaveTextContent('分发')
  })

  it('opens the pack dialog only after entering a source market and completes without distribution handoff', async () => {
    vi.spyOn(skillApiV2, 'searchMarketplaceSkills').mockResolvedValue([marketSkill()])
    vi.spyOn(skillApiV2, 'executeAddCenterSkill').mockResolvedValue({
      skillIds: ['brand-guidelines'],
      updated: [],
      skipped: [],
    })
    vi.spyOn(skillApiV2, 'getPackDetail').mockResolvedValue({
      id: 'anthropics-skills',
      name: 'anthropics/skills',
      description: 'Anthropic skills',
      tags: ['anthropics/skills'],
      appliedAgents: [],
      members: [],
      createdAt: '2026-01-01T00:00:00Z',
      updatedAt: '2026-01-01T00:00:00Z',
    })
    vi.spyOn(skillApiV2, 'upsertPack').mockResolvedValue({
      id: 'anthropics-skills',
      name: 'anthropics/skills',
      description: 'Anthropic skills',
      tags: ['anthropics/skills'],
      members: [
        {
          skillId: 'brand-guidelines',
          skillName: 'brand-guidelines',
          required: true,
          sortOrder: 0,
          missing: false,
        },
      ],
      appliedAgents: [],
      createdAt: '2026-01-01T00:00:00Z',
      updatedAt: '2026-01-01T00:00:00Z',
    })
    const onDone = vi.fn()

    const { MarketPanel } = await import('../components/skills-v2/InstallView')
    render(<MarketPanel onInstall={() => {}} onDone={onDone} />)

    fireEvent.click(await screen.findByTitle('查看这个创建者的所有市场'))
    fireEvent.click(await screen.findByText('anthropics/skills'))
    fireEvent.click(await screen.findByTitle('安装选项'))

    expect(await screen.findByText('安装「brand-guidelines」')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '安装' }))

    await waitFor(() => expect(onDone).toHaveBeenCalledWith())
    expect(onDone).not.toHaveBeenCalledWith('brand-guidelines')
  })

  it('closes the dialog and installs selected source-market Skills with one repository batch', async () => {
    const secondSkill = marketSkill({
      id: 'skillssh:anthropics@skills@frontend-design',
      name: 'frontend-design',
      downloadUrl: 'skillssh:anthropics/skills/frontend-design',
      webUrl: 'https://skills.sh/anthropics/skills/frontend-design',
    })
    vi.spyOn(skillApiV2, 'searchMarketplaceSkills').mockResolvedValue([marketSkill(), secondSkill])

    let finishBatch: ((result: Awaited<ReturnType<typeof skillApiV2.executeMarketplaceSkillBatch>>) => void) | null = null
    const executeBatch = vi.spyOn(skillApiV2, 'executeMarketplaceSkillBatch').mockImplementation(() => new Promise((resolve) => {
      finishBatch = resolve
    }))
    vi.spyOn(skillApiV2, 'getPackDetail').mockResolvedValue({
      id: 'anthropics-skills',
      name: 'anthropics/skills',
      description: 'Anthropic skills',
      tags: ['anthropics/skills'],
      appliedAgents: [],
      members: [],
      createdAt: '2026-01-01T00:00:00Z',
      updatedAt: '2026-01-01T00:00:00Z',
    })
    const upsertPack = vi.spyOn(skillApiV2, 'upsertPack').mockResolvedValue({
      id: 'anthropics-skills',
      name: 'anthropics/skills',
      description: 'Anthropic skills',
      tags: ['anthropics/skills'],
      appliedAgents: [],
      members: [],
      createdAt: '2026-01-01T00:00:00Z',
      updatedAt: '2026-01-01T00:00:00Z',
    })
    const onDone = vi.fn()

    const { MarketPanel } = await import('../components/skills-v2/InstallView')
    render(<MarketPanel onInstall={() => {}} onDone={onDone} />)

    fireEvent.change(screen.getByPlaceholderText('搜索 skills.sh 市场…'), { target: { value: 'batch-install-test' } })
    fireEvent.click(await screen.findAllByTitle('查看这个创建者的所有市场').then((buttons) => buttons[0]))
    fireEvent.click(await screen.findByText('anthropics/skills'))

    fireEvent.click(await screen.findByLabelText('选择 brand-guidelines'))
    fireEvent.click(screen.getByLabelText('选择 frontend-design'))
    fireEvent.click(screen.getByRole('button', { name: '安装已选（2）' }))

    expect(await screen.findByRole('heading', { name: '安装已选的 2 个 Skills' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '开始安装 2 个 Skills' }))

    await waitFor(() => expect(executeBatch).toHaveBeenCalledTimes(1))
    expect(screen.queryByRole('heading', { name: '安装已选的 2 个 Skills' })).not.toBeInTheDocument()
    expect(screen.getByText('正在读取仓库目录并下载选中的 Skills…')).toBeInTheDocument()
    expect(executeBatch).toHaveBeenCalledWith(
      expect.stringMatching(/^market-batch-/),
      'github:anthropics/skills',
      [
        {
          itemId: 'skillssh:anthropics@skills@brand-guidelines',
          skillId: 'brand-guidelines',
          sourceUri: 'skillssh:anthropics/skills/brand-guidelines',
        },
        {
          itemId: 'skillssh:anthropics@skills@frontend-design',
          skillId: 'frontend-design',
          sourceUri: 'skillssh:anthropics/skills/frontend-design',
        },
      ],
    )

    await act(async () => {
      finishBatch?.({
        items: [
          { itemId: 'skillssh:anthropics@skills@brand-guidelines', skillId: 'brand-guidelines', success: true, error: null },
          { itemId: 'skillssh:anthropics@skills@frontend-design', skillId: 'frontend-design', success: true, error: null },
        ],
        cancelled: false,
      })
    })

    await waitFor(() => expect(upsertPack).toHaveBeenCalledWith(expect.objectContaining({
      id: 'anthropics-skills',
      skillIds: ['brand-guidelines', 'frontend-design'],
    })))
    expect(await screen.findByText('批量安装完成')).toBeInTheDocument()
    expect(onDone).toHaveBeenCalledWith()
  })

  it('reports a repository download error without marking every queued Skill as failed', async () => {
    useSkillStoreV2.setState({ packs: [] })
    vi.spyOn(skillApiV2, 'searchMarketplaceSkills').mockResolvedValue([marketSkill()])
    vi.spyOn(skillApiV2, 'executeMarketplaceSkillBatch').mockRejectedValue(new Error('download timeout'))

    const { MarketPanel } = await import('../components/skills-v2/InstallView')
    render(<MarketPanel onInstall={() => {}} onDone={() => {}} />)

    fireEvent.change(screen.getByPlaceholderText('搜索 skills.sh 市场…'), { target: { value: 'source-failure-test' } })
    fireEvent.click(await screen.findByTitle('查看这个创建者的所有市场'))
    fireEvent.click(await screen.findByText('anthropics/skills'))
    fireEvent.click(await screen.findByLabelText('选择 brand-guidelines'))
    fireEvent.click(screen.getByRole('button', { name: '安装已选（1）' }))
    fireEvent.click(await screen.findByRole('radio', { name: /仅安装到中心库/ }))
    fireEvent.click(screen.getByRole('button', { name: '开始安装 1 个 Skills' }))

    await waitFor(() => expect(useSkillStoreV2.getState().marketplaceInstallTask?.busy).toBe(false))
    const task = useSkillStoreV2.getState().marketplaceInstallTask
    expect(task?.phase).toBe('source_failed')
    expect(task?.result).toMatchObject({ successCount: 0, failedCount: 0 })
    expect(task?.items['skillssh:anthropics@skills@brand-guidelines'].status).toBe('queued')
    expect(screen.getByText('来源仓库下载失败')).toBeInTheDocument()
    expect(screen.queryByText('0 / 1 completed · 0 failed')).not.toBeInTheDocument()
    expect(screen.getByText('已完成 0 / 1 · 失败 0')).toBeInTheDocument()
  })

  it('creates one new skill pack for all selected source-market Skills', async () => {
    useSkillStoreV2.setState({ packs: [] })
    const secondSkill = marketSkill({
      id: 'skillssh:anthropics@skills@frontend-design-new-pack',
      name: 'frontend-design',
      downloadUrl: 'skillssh:anthropics/skills/frontend-design',
      webUrl: 'https://skills.sh/anthropics/skills/frontend-design',
    })
    vi.spyOn(skillApiV2, 'searchMarketplaceSkills').mockResolvedValue([marketSkill(), secondSkill])
    vi.spyOn(skillApiV2, 'executeMarketplaceSkillBatch').mockResolvedValue({
      items: [
        { itemId: 'skillssh:anthropics@skills@brand-guidelines', skillId: 'brand-guidelines', success: true, error: null },
        { itemId: 'skillssh:anthropics@skills@frontend-design-new-pack', skillId: 'frontend-design', success: true, error: null },
      ],
      cancelled: false,
    })
    const upsertPack = vi.spyOn(skillApiV2, 'upsertPack').mockResolvedValue({
      id: 'new-market-pack',
      name: 'anthropics/skills',
      description: 'Anthropic market selection',
      tags: ['market', 'anthropics/skills'],
      appliedAgents: [],
      members: [],
      createdAt: '2026-01-01T00:00:00Z',
      updatedAt: '2026-01-01T00:00:00Z',
    })

    const { MarketPanel } = await import('../components/skills-v2/InstallView')
    render(<MarketPanel onInstall={() => {}} onDone={() => {}} />)

    fireEvent.change(screen.getByPlaceholderText('搜索 skills.sh 市场…'), { target: { value: 'batch-new-pack-test' } })
    fireEvent.click(await screen.findAllByTitle('查看这个创建者的所有市场').then((buttons) => buttons[0]))
    fireEvent.click(await screen.findByText('anthropics/skills'))
    fireEvent.click(await screen.findByText('全选本页'))
    fireEvent.click(screen.getByRole('button', { name: '安装已选（2）' }))

    expect(await screen.findByRole('radio', { name: /创建新的技能包/ })).toBeChecked()
    fireEvent.change(screen.getByDisplayValue('anthropics/skills'), { target: { value: 'Anthropic Picks' } })
    fireEvent.click(screen.getByRole('button', { name: '开始安装 2 个 Skills' }))

    await waitFor(() => expect(upsertPack).toHaveBeenCalledTimes(1))
    expect(upsertPack).toHaveBeenCalledWith(expect.objectContaining({
      id: '',
      name: 'Anthropic Picks',
      tags: ['market', 'anthropics/skills'],
      skillIds: ['brand-guidelines', 'frontend-design'],
    }))
  })

  it('cancels the active repository download from the background progress bar', async () => {
    useSkillStoreV2.setState({ packs: [] })
    vi.spyOn(skillApiV2, 'searchMarketplaceSkills').mockResolvedValue([marketSkill()])
    let finishBatch: ((result: Awaited<ReturnType<typeof skillApiV2.executeMarketplaceSkillBatch>>) => void) | null = null
    const executeBatch = vi.spyOn(skillApiV2, 'executeMarketplaceSkillBatch').mockImplementation(() => new Promise((resolve) => {
      finishBatch = resolve
    }))
    const cancelBatch = vi.spyOn(skillApiV2, 'cancelMarketplaceSkillBatch').mockResolvedValue(true)

    const { MarketPanel } = await import('../components/skills-v2/InstallView')
    render(<MarketPanel onInstall={() => {}} onDone={() => {}} />)

    fireEvent.change(screen.getByPlaceholderText('搜索 skills.sh 市场…'), { target: { value: 'batch-cancel-test' } })
    fireEvent.click(await screen.findByTitle('查看这个创建者的所有市场'))
    fireEvent.click(await screen.findByText('anthropics/skills'))
    fireEvent.click(await screen.findByLabelText('选择 brand-guidelines'))
    fireEvent.click(screen.getByRole('button', { name: '安装已选（1）' }))
    fireEvent.click(await screen.findByRole('button', { name: '开始安装 1 个 Skills' }))

    await waitFor(() => expect(executeBatch).toHaveBeenCalledTimes(1))
    fireEvent.click(screen.getByRole('button', { name: '取消安装' }))
    const jobId = executeBatch.mock.calls[0][0]
    await waitFor(() => expect(cancelBatch).toHaveBeenCalledWith(jobId))

    await act(async () => {
      finishBatch?.({ items: [], cancelled: true })
    })

    expect(await screen.findByText('批量安装已取消')).toBeInTheDocument()
    expect(screen.getByText('已取消安装，取消前完成了 0 个 Skills。')).toBeInTheDocument()
  })

  it('keeps the marketplace task visible after leaving the market page', async () => {
    useSkillStoreV2.setState({ packs: [] })
    vi.spyOn(skillApiV2, 'searchMarketplaceSkills').mockResolvedValue([marketSkill()])
    let finishBatch: ((result: Awaited<ReturnType<typeof skillApiV2.executeMarketplaceSkillBatch>>) => void) | null = null
    vi.spyOn(skillApiV2, 'executeMarketplaceSkillBatch').mockImplementation(() => new Promise((resolve) => {
      finishBatch = resolve
    }))

    const { MarketPanel } = await import('../components/skills-v2/InstallView')
    const market = render(<MarketPanel onInstall={() => {}} onDone={() => {}} />)
    fireEvent.change(screen.getByPlaceholderText('搜索 skills.sh 市场…'), { target: { value: 'background-navigation-test' } })
    fireEvent.click(await screen.findByTitle('查看这个创建者的所有市场'))
    fireEvent.click(await screen.findByText('anthropics/skills'))
    fireEvent.click(await screen.findByLabelText('选择 brand-guidelines'))
    fireEvent.click(screen.getByRole('button', { name: '安装已选（1）' }))
    fireEvent.click(await screen.findByRole('radio', { name: /仅安装到中心库/ }))
    fireEvent.click(screen.getByRole('button', { name: '开始安装 1 个 Skills' }))
    await waitFor(() => expect(useSkillStoreV2.getState().marketplaceInstallTask?.busy).toBe(true))

    market.unmount()
    const onOpen = vi.fn()
    const { MarketplaceInstallTaskDock } = await import('../components/skills-v2/MarketplaceInstallTaskDock')
    render(<MarketplaceInstallTaskDock onOpen={onOpen} />)

    expect(screen.getByRole('status', { name: '市场 Skill 安装任务' })).toHaveTextContent('0/1')
    fireEvent.click(screen.getByRole('button', { name: '查看安装' }))
    expect(onOpen).toHaveBeenCalledTimes(1)

    await act(async () => {
      finishBatch?.({
        items: [
          { itemId: 'skillssh:anthropics@skills@brand-guidelines', skillId: 'brand-guidelines', success: true, error: null },
        ],
        cancelled: false,
      })
    })

    await waitFor(() => expect(useSkillStoreV2.getState().marketplaceInstallTask?.busy).toBe(false))
    expect(screen.getByRole('status', { name: '市场 Skill 安装任务' })).toHaveTextContent('批量安装完成')
    expect(screen.getByRole('status', { name: '市场 Skill 安装任务' })).toHaveTextContent('1/1')
  })

  it('shows the running marketplace task on the Skill install sidebar entry', async () => {
    useSkillStoreV2.getState().beginMarketplaceInstallTask(
      'sidebar-market-job',
      'anthropics/skills',
      [{ id: 'brand-guidelines', name: 'brand-guidelines' }],
    )
    useSkillStoreV2.setState({ activeTab: 'library', activeInstallTab: 'git' })

    const { SettingsSidebar } = await import('../components/settings/SettingsSidebar')
    render(
      <SettingsSidebar
        activeSection="skill-manager-v2"
        activeIslandView="overview"
        activeMonitorView="overview"
        collapsed={false}
        onCollapsedChange={() => {}}
        onSelect={() => {}}
        onIslandViewChange={() => {}}
        onMonitorViewChange={() => {}}
      />,
    )

    const installEntry = screen.getByRole('button', { name: '安装 Skill' })
    expect(installEntry).toHaveTextContent('0/1')
    fireEvent.click(installEntry)
    expect(useSkillStoreV2.getState().activeTab).toBe('install')
    expect(useSkillStoreV2.getState().activeInstallTab).toBe('official')
  })
})

describe('Git install skill preview view modes', () => {
  beforeEach(() => {
    cleanup()
    vi.restoreAllMocks()
    useRuntimeEnvironmentStore.getState().selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    vi.spyOn(skillApiV2, 'previewGitHubRepoImport').mockResolvedValue({
      repo: {
        owner: 'anthropics',
        repo: 'skills',
        branch: 'main',
        normalizedUrl: 'https://github.com/anthropics/skills/tree/main',
      },
      skills: [
        {
          sourcePath: 'algorithmic-art',
          skillId: 'algorithmic-art',
          skillName: 'algorithmic-art',
          description: 'Creating algorithmic art using p5.js.',
          rootDirectory: '',
          skillDirectoryName: 'algorithmic-art',
          downloadUrl: 'github:anthropics/skills/algorithmic-art',
          conflict: null,
        },
        {
          sourcePath: 'brand-guidelines',
          skillId: 'brand-guidelines',
          skillName: 'brand-guidelines',
          description: 'Applies official brand colors and typography.',
          rootDirectory: '',
          skillDirectoryName: 'brand-guidelines',
          downloadUrl: 'github:anthropics/skills/brand-guidelines',
          conflict: null,
        },
      ],
    })
  })

  it('shows Git repo skills as cards by default and can switch to list view', async () => {
    const { GitPanel } = await import('../components/skills-v2/InstallView')
    const { container } = render(<GitPanel initialUrl="https://github.com/anthropics/skills" onDone={() => {}} />)

    fireEvent.click(screen.getByText('检测 Skill'))
    expect(await screen.findAllByText('algorithmic-art')).toHaveLength(2)

    expect(container.querySelector('.sm2__git-skill-grid')).not.toBeNull()
    expect(container.querySelector('.sm2__git-skill-list')).toBeNull()

    fireEvent.click(screen.getByText('列表'))
    expect(container.querySelector('.sm2__git-skill-list')).not.toBeNull()
    expect(container.querySelector('.sm2__git-skill-grid')).toBeNull()
  })

  it('forwards the private repository token only when requesting GitHub data', async () => {
    const preview = vi.mocked(skillApiV2.previewGitHubRepoImport)
    const install = vi.spyOn(skillApiV2, 'importGitHubRepoSkills').mockResolvedValue({
      repo: {
        owner: 'acme',
        repo: 'private-skills',
        branch: 'main',
        normalizedUrl: 'https://github.com/acme/private-skills',
      },
      importedSkills: [],
      skippedSkills: [],
    })
    const { GitPanel } = await import('../components/skills-v2/InstallView')
    render(<GitPanel initialUrl="https://github.com/acme/private-skills" onDone={() => {}} />)

    fireEvent.click(screen.getByText('高级（私有仓库令牌）'))
    fireEvent.change(screen.getByPlaceholderText('ghp_...'), { target: { value: 'github-test-token' } })
    fireEvent.click(screen.getByText('检测 Skill'))

    await waitFor(() => {
      expect(preview).toHaveBeenCalledWith(
        'https://github.com/acme/private-skills',
        'github-test-token',
        LOCAL_RUNTIME_ENVIRONMENT_ID,
      )
    })

    await screen.findAllByText('algorithmic-art')
    fireEvent.click(screen.getByText('安装所选 (2)'))
    await waitFor(() => {
      expect(install).toHaveBeenCalledWith(
        'https://github.com/acme/private-skills',
        expect.any(Array),
        'github-test-token',
        { environmentId: LOCAL_RUNTIME_ENVIRONMENT_ID, expectedCommit: undefined },
      )
    })
  })
  it.each([false, true])('discards a Git preview after switching runtime, return=%s', async (returnToHost) => {
    const host = 'preview-ssh'
    const runtime = useRuntimeEnvironmentStore.getState()
    runtime.selectEnvironment(host)
    let finishPreview!: (preview: Awaited<ReturnType<typeof skillApiV2.previewGitHubRepoImport>>) => void
    const preview = vi.mocked(skillApiV2.previewGitHubRepoImport)
    preview.mockReturnValueOnce(new Promise((resolve) => { finishPreview = resolve }))
    const install = vi.spyOn(skillApiV2, 'importGitHubRepoSkills')
    const { GitPanel } = await import('../components/skills-v2/InstallView')
    const url = 'https://github.com/acme/skills/tree/main/skills/demo'
    render(<GitPanel initialUrl={url} onDone={() => {}} />)
    fireEvent.click(screen.getByText('检测 Skill'))
    expect(preview).toHaveBeenCalledWith(url, '', host)

    await act(async () => {
      runtime.selectEnvironment('other-ssh')
      if (returnToHost) runtime.selectEnvironment(host)
      finishPreview({
        repo: { owner: 'acme', repo: 'skills', branch: 'main', normalizedUrl: url, revision: 'a'.repeat(40) },
        skills: [{
          sourcePath: 'skills/demo', skillId: 'demo', skillName: 'Demo', description: null,
          rootDirectory: '', skillDirectoryName: 'demo', downloadUrl: url, conflict: null,
        }],
      })
    })
    expect(install).not.toHaveBeenCalled()
    expect(screen.getByText('检测 Skill')).toBeInTheDocument()
    expect(screen.queryByText('Demo')).not.toBeInTheDocument()
  })

  it('keeps review open when a slash is part of the SSH Git branch name', async () => {
    useRuntimeEnvironmentStore.getState().selectEnvironment('preview-ssh')
    const url = 'https://github.com/acme/skills/tree/feature/review'
    vi.mocked(skillApiV2.previewGitHubRepoImport).mockResolvedValueOnce({
      repo: {
        owner: 'acme', repo: 'skills', branch: 'feature/review', normalizedUrl: url,
        revision: 'b'.repeat(40), requestedPath: null,
      },
      skills: [{
        sourcePath: 'skills/demo', skillId: 'demo', skillName: 'Demo', description: null,
        rootDirectory: '', skillDirectoryName: 'demo', downloadUrl: url, conflict: null,
      }],
    })
    const install = vi.spyOn(skillApiV2, 'importGitHubRepoSkills')
    const { GitPanel } = await import('../components/skills-v2/InstallView')
    render(<GitPanel initialUrl={url} onDone={() => {}} />)
    fireEvent.click(screen.getByText('检测 Skill'))
    expect(await screen.findByText('安装所选 (1)')).toBeInTheDocument()
    expect(install).not.toHaveBeenCalled()
  })

  it('uses the reviewed commit and SSH host for a direct Git Skill install', async () => {
    const host = 'preview-ssh'
    useRuntimeEnvironmentStore.getState().selectEnvironment(host)
    const url = 'https://github.com/acme/skills/tree/main/skills/demo'
    const revision = 'b'.repeat(40)
    const repo = { owner: 'acme', repo: 'skills', branch: 'main', normalizedUrl: url, revision }
    vi.mocked(skillApiV2.previewGitHubRepoImport).mockResolvedValueOnce({
      repo,
      skills: [{
        sourcePath: 'skills/demo', skillId: 'demo', skillName: 'Demo', description: null,
        rootDirectory: '', skillDirectoryName: 'demo', downloadUrl: url, conflict: null,
      }],
    })
    const install = vi.spyOn(skillApiV2, 'importGitHubRepoSkills').mockResolvedValue({
      repo, importedSkills: [], skippedSkills: [],
    })
    const { GitPanel } = await import('../components/skills-v2/InstallView')
    render(<GitPanel initialUrl={url} onDone={() => {}} />)
    fireEvent.click(screen.getByText('检测 Skill'))
    await waitFor(() => expect(install).toHaveBeenCalledExactlyOnceWith(
      url, [{ sourcePath: 'skills/demo', resolution: 'overwrite' }], '',
      { environmentId: host, expectedCommit: revision },
    ))
  })
})

describe('Local skill import', () => {
  beforeEach(() => {
    cleanup()
    vi.restoreAllMocks()
    vi.clearAllMocks()
    useConfigStore.setState({ remoteHostEntries: [] })
    useRuntimeEnvironmentStore.setState({
      selectedEnvironmentId: LOCAL_RUNTIME_ENVIRONMENT_ID,
    })
  })

  it('passes link import mode when importing a local source folder as a symlink', async () => {
    const previewAdd = vi.spyOn(skillApiV2, 'previewAddCenterSkill').mockResolvedValue({
      centerPath: '/Users/me/.agentbro/skills',
      candidates: [
        {
          skillId: 'live-review',
          proposedSkillId: 'live-review',
          name: 'live-review',
          description: 'Local development skill',
          sourceDir: '/Users/me/code/szskills/live-review',
          hash: 'hash-link',
          action: 'create',
          existingSourceType: null,
          reason: null,
        },
      ],
      blockers: [],
    })
    const executeAdd = vi.spyOn(skillApiV2, 'executeAddCenterSkill').mockResolvedValue({
      skillIds: ['live-review'],
      updated: [],
      skipped: [],
    })
    vi.spyOn(window, 'alert').mockImplementation(() => {})

    const { LocalPanel } = await import('../components/skills-v2/InstallView')
    render(<LocalPanel onDone={() => {}} />)

    expect(screen.getByText(/常见使用场景：本地已有 Skill/)).toBeInTheDocument()

    fireEvent.change(screen.getByPlaceholderText('选择或粘贴包含 SKILL.md 的目录 / .zip'), {
      target: { value: '/Users/me/code/szskills/live-review' },
    })
    fireEvent.click(screen.getByLabelText('软链导入，本地目录作为源'))
    fireEvent.click(screen.getByRole('button', { name: '预览导入' }))

    await waitFor(() => {
      expect(previewAdd).toHaveBeenCalledWith({
        sourcePath: '/Users/me/code/szskills/live-review',
        sourceType: 'local_folder',
        sourceUri: '/Users/me/code/szskills/live-review',
        importMode: 'link',
      })
    })

    fireEvent.click(await screen.findByRole('button', { name: '执行导入' }))
    await waitFor(() => {
      expect(executeAdd).toHaveBeenCalledWith({
        sourcePath: '/Users/me/code/szskills/live-review',
        sourceType: 'local_folder',
        sourceUri: '/Users/me/code/szskills/live-review',
        importMode: 'link',
      }, [])
    })
  })

  it('can bulk overwrite conflicting local imports', async () => {
    vi.spyOn(skillApiV2, 'previewAddCenterSkill').mockResolvedValue({
      centerPath: '/Users/me/.agentbro/skills',
      candidates: [],
      blockers: [
        {
          skillId: 'sz-news-video',
          proposedSkillId: 'sz-news-video',
          name: 'sz-news-video',
          description: 'Video skill',
          sourceDir: '/Users/me/code/szskills/sz-news-video',
          hash: 'hash-video',
          action: 'blocked_same_name_diff_source',
          existingSourceType: 'local_folder',
          reason: "A different skill already uses id 'sz-news-video'. Choose overwrite, rename, or skip.",
        },
        {
          skillId: 'sz-x-feed',
          proposedSkillId: 'sz-x-feed',
          name: 'sz-x-feed',
          description: 'Feed skill',
          sourceDir: '/Users/me/code/szskills/sz-x-feed',
          hash: 'hash-feed',
          action: 'blocked_same_name_diff_source',
          existingSourceType: 'local_folder',
          reason: "A different skill already uses id 'sz-x-feed'. Choose overwrite, rename, or skip.",
        },
      ],
    })
    const executeAdd = vi.spyOn(skillApiV2, 'executeAddCenterSkill').mockResolvedValue({
      skillIds: [],
      updated: ['sz-news-video', 'sz-x-feed'],
      skipped: [],
    })
    vi.spyOn(window, 'alert').mockImplementation(() => {})

    const { LocalPanel } = await import('../components/skills-v2/InstallView')
    render(<LocalPanel onDone={() => {}} />)

    fireEvent.change(screen.getByPlaceholderText('选择或粘贴包含 SKILL.md 的目录 / .zip'), {
      target: { value: '/Users/me/code/szskills' },
    })
    fireEvent.click(screen.getByRole('button', { name: '预览导入' }))

    fireEvent.click(await screen.findByLabelText('覆盖冲突项'))
    fireEvent.click(screen.getByRole('button', { name: '执行导入' }))

    await waitFor(() => {
      expect(executeAdd).toHaveBeenCalledWith({
        sourcePath: '/Users/me/code/szskills',
        sourceType: 'local_folder',
        sourceUri: '/Users/me/code/szskills',
        importMode: 'copy',
      }, [
        { skillId: 'sz-news-video', resolution: 'update' },
        { skillId: 'sz-x-feed', resolution: 'update' },
      ])
    })
  })

  it('hides symlink import choices for zip archives', async () => {
    const previewAdd = vi.spyOn(skillApiV2, 'previewAddCenterSkill').mockResolvedValue({
      centerPath: '/Users/me/.agentbro/skills',
      candidates: [],
      blockers: [],
    })

    const { LocalPanel } = await import('../components/skills-v2/InstallView')
    render(<LocalPanel onDone={() => {}} />)

    fireEvent.change(screen.getByPlaceholderText('选择或粘贴包含 SKILL.md 的目录 / .zip'), {
      target: { value: '/Users/me/Downloads/skills.zip' },
    })

    expect(screen.queryByLabelText('软链导入，本地目录作为源')).not.toBeInTheDocument()
    expect(screen.queryByText('批量导入（该目录包含多个 Skill）')).not.toBeInTheDocument()
    expect(screen.getByText('压缩包会解压后复制导入中心库，不支持软链导入。')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '预览导入' }))
    await waitFor(() => {
      expect(previewAdd).toHaveBeenCalledWith({
        sourcePath: '/Users/me/Downloads/skills.zip',
        sourceType: 'archive',
        sourceUri: undefined,
        importMode: 'copy',
      })
    })
  })

  it('shows no unchanged skills and prevents a redundant import', async () => {
    vi.spyOn(skillApiV2, 'previewAddCenterSkill').mockResolvedValue({
      centerPath: '/Users/me/.agentbro/skills',
      candidates: [],
      blockers: [],
      unchangedCount: 40,
    })

    const { LocalPanel } = await import('../components/skills-v2/InstallView')
    render(<LocalPanel onDone={() => {}} />)

    fireEvent.change(screen.getByPlaceholderText('选择或粘贴包含 SKILL.md 的目录 / .zip'), {
      target: { value: '/Users/me/code/szskills' },
    })
    fireEvent.click(screen.getByRole('button', { name: '预览导入' }))

    expect(await screen.findByText('没有检测到新增或变更，40 个 Skill 均无需重复导入。')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '无需导入' })).toBeDisabled()
  })

  it('browses and imports paths from the selected remote server without opening the Mac dialog', async () => {
    useConfigStore.setState({
      remoteHostEntries: [{
        id: 'ubuntu',
        name: 'ubuntu',
        sshTarget: 'agent@ubuntu',
        port: 22,
        remoteSocketPath: '/tmp/agentbro.sock',
        autoConnect: false,
        connectionStatus: 'disconnected',
      }],
    })
    useRuntimeEnvironmentStore.setState({ selectedEnvironmentId: 'ubuntu' })
    const browse = vi.spyOn(skillApiV2, 'browseRemoteSkillSources')
      .mockResolvedValueOnce({
        path: '/home/agent',
        parentPath: null,
        entries: [{
          name: 'skills',
          path: '/home/agent/skills',
          entryType: 'directory',
          hasSkillManifest: true,
        }],
      })
      .mockResolvedValueOnce({
        path: '/home/agent/skills',
        parentPath: '/home/agent',
        entries: [],
      })
    const previewAdd = vi.spyOn(skillApiV2, 'previewAddCenterSkill').mockResolvedValue({
      centerPath: '/home/agent/.agents/skills',
      candidates: [],
      blockers: [],
    })

    const { LocalPanel } = await import('../components/skills-v2/InstallView')
    render(<LocalPanel onDone={() => {}} />)

    expect(screen.getByRole('heading', { name: '从 ubuntu 导入' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '浏览远程文件夹' }))

    expect(await screen.findByRole('dialog', { name: '选择 ubuntu 上的文件夹' })).toBeInTheDocument()
    await waitFor(() => expect(browse).toHaveBeenCalledWith('~', 'ubuntu'))
    fireEvent.click(await screen.findByRole('button', { name: /skills.*Skill 文件夹/ }))
    await waitFor(() => expect(screen.getByLabelText('远程目录路径')).toHaveValue('/home/agent/skills'))
    fireEvent.click(screen.getByRole('button', { name: '选择当前文件夹' }))

    expect(screen.getByPlaceholderText('浏览或输入远程服务器上的目录 / .zip'))
      .toHaveValue('/home/agent/skills')
    expect(openDialog).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: '预览导入' }))

    await waitFor(() => {
      expect(previewAdd).toHaveBeenCalledWith({
        sourcePath: '/home/agent/skills',
        sourceType: 'local_folder',
        sourceUri: '/home/agent/skills',
        importMode: 'copy',
        sourceLocation: 'remote',
      })
    })
  })
})

describe('Agent sync local agent chips', () => {
  beforeEach(() => {
    cleanup()
    vi.restoreAllMocks()
  })

  it('shows a task-focused pending inbox by default and hides managed skills', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const inventory: AgentSkillInventoryAgent[] = [
      {
        agentId: 'claude-code',
        displayName: 'Claude Code',
        iconKey: 'claude-code',
        skillsDir: '/Users/me/.claude/skills',
        installed: true,
        managedCount: 3,
        unmanagedCount: 2,
        importableCount: 1,
        items: [
          {
            id: 'managed-alpha',
            agentId: 'claude-code',
            skillId: 'managed-alpha',
            name: 'managed-alpha',
            path: '/Users/me/.claude/skills/managed-alpha',
            managed: true,
            canImport: false,
            status: 'managed',
            statusLabel: '已管理',
            reason: null,
            targetId: 'target-managed-alpha',
            actualMode: 'link',
            hash: 'hash-managed-alpha',
          },
          {
            id: 'local-alpha',
            agentId: 'claude-code',
            skillId: 'alpha',
            name: 'alpha',
            path: '/Users/me/.claude/skills/alpha',
            managed: false,
            canImport: true,
            status: 'unmanaged',
            statusLabel: '未管理',
            reason: null,
            targetId: null,
            actualMode: null,
            hash: 'hash-alpha',
          },
          {
            id: 'local-bird',
            agentId: 'claude-code',
            skillId: 'bird',
            name: 'bird',
            path: '/Users/me/.claude/skills/bird',
            managed: false,
            canImport: false,
            status: 'conflict',
            statusLabel: '未管理 · 同名冲突',
            reason: 'same_name_as_center_skill',
            targetId: null,
            actualMode: null,
            hash: 'hash-bird',
          },
        ],
      },
    ]
    vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValueOnce(inventory)

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    render(<AgentSyncPanel onDone={() => {}} />)

    expect(await screen.findByText('发现 1 个可接管 Skill，1 个同名冲突')).toBeInTheDocument()
    expect(screen.queryByText('把散落在各 Agent 里的 Skills 收进中心库')).not.toBeInTheDocument()
    expect(screen.getByText('待处理收纳箱')).toBeInTheDocument()
    expect(screen.getByText('alpha')).toBeInTheDocument()
    expect(screen.getByText('bird')).toBeInTheDocument()
    expect(screen.queryByText('managed-alpha')).not.toBeInTheDocument()
    expect(screen.getByText('3 已管理，默认隐藏')).toBeInTheDocument()
  })

  it('can reveal managed skills from advanced controls', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const inventory: AgentSkillInventoryAgent[] = [
      {
        agentId: 'claude-code',
        displayName: 'Claude Code',
        iconKey: 'claude-code',
        skillsDir: '/Users/me/.claude/skills',
        installed: true,
        managedCount: 1,
        unmanagedCount: 1,
        importableCount: 1,
        items: [
          {
            id: 'managed-alpha',
            agentId: 'claude-code',
            skillId: 'managed-alpha',
            name: 'managed-alpha',
            path: '/Users/me/.claude/skills/managed-alpha',
            managed: true,
            canImport: false,
            status: 'managed',
            statusLabel: '已管理',
            reason: null,
            targetId: 'target-managed-alpha',
            actualMode: 'link',
            hash: 'hash-managed-alpha',
          },
          {
            id: 'local-alpha',
            agentId: 'claude-code',
            skillId: 'alpha',
            name: 'alpha',
            path: '/Users/me/.claude/skills/alpha',
            managed: false,
            canImport: true,
            status: 'unmanaged',
            statusLabel: '未管理',
            reason: null,
            targetId: null,
            actualMode: null,
            hash: 'hash-alpha',
          },
        ],
      },
    ]
    vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValueOnce(inventory)

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    render(<AgentSyncPanel onDone={() => {}} />)

    expect(await screen.findByText('alpha')).toBeInTheDocument()
    expect(screen.queryByText('managed-alpha')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '高级查看' }))
    fireEvent.click(screen.getByLabelText('显示已管理 Skills'))
    expect(screen.getByText('managed-alpha')).toBeInTheDocument()
  })

  it('deletes a managed skill distribution from the agent sync detail', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const before: AgentSkillInventoryAgent[] = [
      {
        agentId: 'claude-code',
        displayName: 'Claude Code',
        iconKey: 'claude-code',
        skillsDir: '/Users/me/.claude/skills',
        installed: true,
        managedCount: 1,
        unmanagedCount: 0,
        importableCount: 0,
        items: [
          {
            id: 'managed-alpha',
            agentId: 'claude-code',
            skillId: 'managed-alpha',
            name: 'managed-alpha',
            path: '/Users/me/.claude/skills/managed-alpha',
            managed: true,
            canImport: false,
            status: 'managed',
            statusLabel: '已管理',
            reason: null,
            targetId: 'target-managed-alpha',
            actualMode: 'link',
            hash: 'hash-managed-alpha',
          },
        ],
      },
    ]
    const after: AgentSkillInventoryAgent[] = [
      { ...before[0], managedCount: 0, items: [] },
    ]
    vi.spyOn(skillApiV2, 'listAgentSkillInventory')
      .mockResolvedValueOnce(before)
      .mockResolvedValueOnce(after)
    vi.spyOn(skillApiV2, 'readFileTree').mockResolvedValueOnce({
      name: 'managed-alpha',
      nodeType: 'dir',
      path: '/Users/me/.claude/skills/managed-alpha',
      children: [],
    })
    vi.spyOn(skillApiV2, 'readFileContent').mockResolvedValueOnce('# Managed Alpha')
    const deleteTargets = vi.spyOn(skillApiV2, 'deleteSkillTargetDistributions').mockResolvedValue({ deleted: 1, failures: [] })
    const onDone = vi.fn()

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    render(<AgentSyncPanel onDone={onDone} />)

    expect(await screen.findByText('1 已管理，默认隐藏')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '高级查看' }))
    fireEvent.click(screen.getByLabelText('显示已管理 Skills'))
    fireEvent.click(screen.getByText('managed-alpha'))
    fireEvent.click(await screen.findByRole('button', { name: '删除分发' }))
    expect(screen.getByRole('dialog', { name: '删除 Agent 分发「managed-alpha」' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '确认删除' }))

    await waitFor(() => expect(deleteTargets).toHaveBeenCalledWith(['target-managed-alpha']))
    expect(onDone).toHaveBeenCalled()
    expect(await screen.findByText('本机 Agent Skills 已完成整理')).toBeInTheDocument()
  })

  it('keeps list and card views on the same pending dataset', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const inventory: AgentSkillInventoryAgent[] = [
      {
        agentId: 'claude-code',
        displayName: 'Claude Code',
        iconKey: 'claude-code',
        skillsDir: '/Users/me/.claude/skills',
        installed: true,
        managedCount: 1,
        unmanagedCount: 1,
        importableCount: 1,
        items: [
          {
            id: 'managed-alpha',
            agentId: 'claude-code',
            skillId: 'managed-alpha',
            name: 'managed-alpha',
            path: '/Users/me/.claude/skills/managed-alpha',
            managed: true,
            canImport: false,
            status: 'managed',
            statusLabel: '已管理',
            reason: null,
            targetId: 'target-managed-alpha',
            actualMode: 'link',
            hash: 'hash-managed-alpha',
          },
          {
            id: 'local-alpha',
            agentId: 'claude-code',
            skillId: 'alpha',
            name: 'alpha',
            path: '/Users/me/.claude/skills/alpha',
            managed: false,
            canImport: true,
            status: 'unmanaged',
            statusLabel: '未管理',
            reason: null,
            targetId: null,
            actualMode: null,
            hash: 'hash-alpha',
          },
        ],
      },
    ]
    vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValueOnce(inventory)

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    const { container } = render(<AgentSyncPanel onDone={() => {}} />)

    expect(await screen.findByText('alpha')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '列表' }))
    await waitFor(() => expect(container.querySelector('.sm2__agent-sync-listview')).not.toBeNull())
    expect(screen.queryByText('managed-alpha')).not.toBeInTheDocument()

    const listCheckbox = container.querySelector('.sm2__agent-sync-listview .sm2__agent-sync-checkbox') as HTMLInputElement
    fireEvent.click(listCheckbox)
    expect(listCheckbox).toBeChecked()

    fireEvent.click(screen.getByRole('button', { name: '卡片' }))

    expect(container.querySelector('.sm2__install-grid')).not.toBeNull()
    expect(screen.getByText('alpha')).toBeInTheDocument()
    expect(screen.queryByText('managed-alpha')).not.toBeInTheDocument()
    const selectedCard = container.querySelector('.sm2__agent-sync-card--selected')
    expect(selectedCard).not.toBeNull()
    expect(selectedCard?.querySelector('.sm2__agent-sync-checkbox')).toBeChecked()

    fireEvent.click(selectedCard!)
    expect(document.body.querySelector('.sm2__slideover-title')).toHaveTextContent('alpha')
    expect(document.body.querySelector('.sm2__slideover--skill-detail')).toHaveTextContent('/Users/me/.claude/skills/alpha')
  })

  it('only shows locally installed agents', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const inventory: AgentSkillInventoryAgent[] = [
      {
        agentId: 'claude-code',
        displayName: 'Claude Code',
        iconKey: 'claude-code',
        skillsDir: '/Users/me/.claude/skills',
        installed: true,
        managedCount: 0,
        unmanagedCount: 1,
        importableCount: 1,
        items: [
          {
            id: 'claude-local-skill',
            agentId: 'claude-code',
            skillId: 'local-skill',
            name: 'local-skill',
            path: '/Users/me/.claude/skills/local-skill',
            managed: false,
            canImport: true,
            status: 'unmanaged',
            statusLabel: '未管理',
            reason: null,
            targetId: null,
            actualMode: null,
            hash: 'hash-1',
          },
        ],
      },
      {
        agentId: 'deepseek',
        displayName: 'DeepSeek',
        iconKey: 'deepseek',
        skillsDir: null,
        installed: false,
        managedCount: 0,
        unmanagedCount: 0,
        importableCount: 0,
        items: [],
      },
    ]
    vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValueOnce(inventory)

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    const { container } = render(<AgentSyncPanel onDone={() => {}} />)

    const select = await screen.findByLabelText('选择 Agent') as HTMLSelectElement
    await within(select).findByRole('option', { name: 'Claude Code · 1 可接管' })
    expect(Array.from(select.options).map((option) => option.textContent)).toEqual(['全部 Agent', 'Claude Code · 1 可接管'])
    expect(container.querySelector('.sm2__agent-sync-agent-strip')).toHaveTextContent('Claude Code')
    expect(screen.queryByText('DeepSeek')).not.toBeInTheDocument()
    expect(container.querySelector('.sm2__agent-sync-summary')).toHaveTextContent('1 Agent')
  })

  it('uses a compact agent dropdown sorted by local skill count', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const inventory: AgentSkillInventoryAgent[] = [
      {
        agentId: 'claude-code',
        displayName: 'Claude Code',
        iconKey: 'claude-code',
        skillsDir: '/Users/me/.claude/skills',
        installed: true,
        managedCount: 2,
        unmanagedCount: 1,
        importableCount: 1,
        items: [],
      },
      {
        agentId: 'codex',
        displayName: 'Codex',
        iconKey: 'codex',
        skillsDir: '/Users/me/.codex/skills',
        installed: true,
        managedCount: 4,
        unmanagedCount: 3,
        importableCount: 2,
        items: [],
      },
      {
        agentId: 'deepseek',
        displayName: 'DeepSeek',
        iconKey: 'deepseek',
        skillsDir: null,
        installed: false,
        managedCount: 20,
        unmanagedCount: 20,
        importableCount: 20,
        items: [],
      },
    ]
    vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValueOnce(inventory)

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    const { container } = render(<AgentSyncPanel onDone={() => {}} />)

    const select = await screen.findByLabelText('选择 Agent') as HTMLSelectElement
    const agentOptions = Array.from(select.options).map((option) => option.textContent)

    expect(agentOptions).toEqual(['全部 Agent', 'Codex · 2 可接管', 'Claude Code · 1 可接管'])
    expect(screen.queryByText('DeepSeek')).not.toBeInTheDocument()

    fireEvent.change(select, { target: { value: 'claude-code' } })
    expect(select.value).toBe('claude-code')
    expect(container.querySelector('.sm2__agent-sync-agent-card--active')).toHaveTextContent('Claude Code')
  })

  it('scopes summary actions and accessibility state to the selected agent', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const inventory: AgentSkillInventoryAgent[] = [
      {
        agentId: 'codex',
        displayName: 'Codex',
        iconKey: 'codex',
        skillsDir: '/Users/me/.codex/skills',
        installed: true,
        managedCount: 0,
        unmanagedCount: 2,
        importableCount: 2,
        items: [
          {
            id: 'codex-alpha',
            agentId: 'codex',
            skillId: 'alpha',
            name: 'alpha',
            path: '/Users/me/.codex/skills/alpha',
            managed: false,
            canImport: true,
            status: 'unmanaged',
            statusLabel: '未管理',
            reason: null,
            targetId: null,
            actualMode: null,
            hash: 'hash-alpha',
          },
          {
            id: 'codex-beta',
            agentId: 'codex',
            skillId: 'beta',
            name: 'beta',
            path: '/Users/me/.codex/skills/beta',
            managed: false,
            canImport: true,
            status: 'unmanaged',
            statusLabel: '未管理',
            reason: null,
            targetId: null,
            actualMode: null,
            hash: 'hash-beta',
          },
        ],
      },
      {
        agentId: 'claude-code',
        displayName: 'Claude Code',
        iconKey: 'claude-code',
        skillsDir: '/Users/me/.claude/skills',
        installed: true,
        managedCount: 1,
        unmanagedCount: 1,
        importableCount: 0,
        items: [
          {
            id: 'claude-bird',
            agentId: 'claude-code',
            skillId: 'bird',
            name: 'bird',
            path: '/Users/me/.claude/skills/bird',
            managed: false,
            canImport: false,
            status: 'conflict',
            statusLabel: '未管理 · 同名冲突',
            reason: 'same_name_as_center_skill',
            targetId: null,
            actualMode: null,
            hash: 'hash-bird',
          },
          {
            id: 'claude-managed',
            agentId: 'claude-code',
            skillId: 'managed',
            name: 'managed',
            path: '/Users/me/.claude/skills/managed',
            managed: true,
            canImport: false,
            status: 'managed',
            statusLabel: '已管理',
            reason: null,
            targetId: 'target-managed',
            actualMode: 'link',
            hash: 'hash-managed',
          },
        ],
      },
    ]
    vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValueOnce(inventory)
    const preview = vi.spyOn(skillApiV2, 'previewAdopt').mockResolvedValueOnce({
      agentId: 'claude-code',
      unmanagedId: 'claude-bird',
      skillPath: '/Users/me/.claude/skills/bird',
      inferredSkillId: 'bird',
      hash: 'hash-bird',
      centerHasSameId: true,
      canQuickAdopt: false,
      options: [
        { value: 'overwrite_center', label: 'Overwrite center skill with this one', destructive: true },
        { value: 'rename', label: 'Import under a new id', destructive: false },
        { value: 'skip', label: 'Keep as unmanaged', destructive: false },
      ],
    })

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    render(<AgentSyncPanel onDone={() => {}} />)

    expect(await screen.findByText('发现 2 个可接管 Skill，1 个同名冲突')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '一键整理 2 个' })).toBeEnabled()
    expect(screen.getByRole('button', { name: '重新扫描' })).toBeEnabled()
    expect(screen.getByRole('button', { name: /全部 Agent/ })).toHaveAttribute('aria-pressed', 'true')

    fireEvent.click(screen.getByRole('button', { name: /Claude Code/ }))

    expect(screen.getByText('发现 1 个同名冲突')).toBeInTheDocument()
    expect(screen.getAllByRole('button', { name: '处理冲突' })[0]).toHaveClass('sm2__btn--featured')
    expect(screen.getAllByRole('button', { name: '处理冲突' })[0]).toBeEnabled()
    expect(screen.getByRole('button', { name: '重新扫描' })).toBeEnabled()
    fireEvent.click(screen.getAllByRole('button', { name: '处理冲突' })[0])
    await waitFor(() => expect(preview).toHaveBeenCalledWith('claude-code', 'claude-bird'))
    expect(screen.getByRole('button', { name: /全部 Agent/ })).toHaveAttribute('aria-pressed', 'false')
    expect(screen.getByRole('button', { name: /Claude Code/ })).toHaveAttribute('aria-pressed', 'true')
    expect(screen.getByText('1 已管理，默认隐藏')).toBeInTheDocument()
  })

  it('shows a rescanable empty state when no agent skill directories are installed', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValue([])
    const refresh = vi.spyOn(skillApiV2, 'refresh').mockResolvedValue(undefined)

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    render(<AgentSyncPanel onDone={() => {}} />)

    expect(await screen.findByText('未发现可同步的 Agent Skills 目录')).toBeInTheDocument()
    expect(screen.getAllByText('没有找到可同步的 Agent Skills 目录。可以点击「重新扫描」再试。')).toHaveLength(2)

    fireEvent.click(screen.getAllByRole('button', { name: '重新扫描' }).at(-1)!)

    await waitFor(() => {
      expect(refresh).toHaveBeenCalled()
    })
  })

  it('labels import buttons in both list and card views', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const inventory: AgentSkillInventoryAgent[] = [
      {
        agentId: 'claude-code',
        displayName: 'Claude Code',
        iconKey: 'claude-code',
        skillsDir: '/Users/me/.claude/skills',
        installed: true,
        managedCount: 0,
        unmanagedCount: 1,
        importableCount: 1,
        items: [
          {
            id: 'local-alpha',
            agentId: 'claude-code',
            skillId: 'alpha',
            name: 'alpha',
            path: '/Users/me/.claude/skills/alpha',
            managed: false,
            canImport: true,
            status: 'unmanaged',
            statusLabel: '未管理',
            reason: null,
            targetId: null,
            actualMode: null,
            hash: 'hash-alpha',
          },
        ],
      },
    ]
    vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValueOnce(inventory)

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    render(<AgentSyncPanel onDone={() => {}} />)

    expect(await screen.findByRole('button', { name: '接管到中心库：alpha' })).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '卡片' }))

    expect(screen.getByRole('button', { name: '接管到中心库：alpha' })).toBeInTheDocument()
  })

  it('shows progress while adopting selected local agent skills', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const inventory: AgentSkillInventoryAgent[] = [
      {
        agentId: 'claude-code',
        displayName: 'Claude Code',
        iconKey: 'claude-code',
        skillsDir: '/Users/me/.claude/skills',
        installed: true,
        managedCount: 0,
        unmanagedCount: 2,
        importableCount: 2,
        items: [
          {
            id: 'local-alpha',
            agentId: 'claude-code',
            skillId: 'alpha',
            name: 'alpha',
            path: '/Users/me/.claude/skills/alpha',
            managed: false,
            canImport: true,
            status: 'unmanaged',
            statusLabel: '未管理',
            reason: null,
            targetId: null,
            actualMode: null,
            hash: 'hash-alpha',
          },
          {
            id: 'local-beta',
            agentId: 'claude-code',
            skillId: 'beta',
            name: 'beta',
            path: '/Users/me/.claude/skills/beta',
            managed: false,
            canImport: true,
            status: 'unmanaged',
            statusLabel: '未管理',
            reason: null,
            targetId: null,
            actualMode: null,
            hash: 'hash-beta',
          },
        ],
      },
    ]
    let finishFirstAdopt!: () => void
    let finishInventory!: () => void
    const inventoryRequest = new Promise<AgentSkillInventoryAgent[]>((resolve) => {
      finishInventory = () => resolve(inventory)
    })
    vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockReturnValue(inventoryRequest)
    vi.spyOn(skillApiV2, 'executeAdopt').mockImplementationOnce(
      () => new Promise<string>((resolve) => {
        finishFirstAdopt = () => resolve('')
      }),
    )

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    render(<AgentSyncPanel onDone={() => {}} />)

    const selectVisible = screen.getByRole('button', { name: '选择当前可接管' })
    expect(selectVisible).toBeDisabled()
    await act(async () => finishInventory())
    await waitFor(() => expect(selectVisible).toBeEnabled())
    fireEvent.click(selectVisible)
    const adoptSelected = screen.getByRole('button', { name: '接管到中心库' })
    expect(adoptSelected).toBeEnabled()
    fireEvent.click(adoptSelected)

    expect(await screen.findByRole('status')).toHaveTextContent('正在接管 1 / 2')
    expect(screen.getByRole('status')).toHaveTextContent('alpha')
    expect(screen.getByRole('status')).toHaveClass('sm2__agent-sync-progress--floating')
    expect(screen.getByLabelText('选择 Agent')).not.toBeDisabled()
    fireEvent.click(screen.getByText('beta'))
    expect(document.body.querySelector('.sm2__slideover-title')).toHaveTextContent('beta')
    await act(async () => finishFirstAdopt())
  })

  it('localizes stale unmanaged adoption errors instead of showing database text', async () => {
    await i18n.changeLanguage('zh')
    const { skillApiV2 } = await import('../services/skillApiV2')
    const inventory: AgentSkillInventoryAgent[] = [
      {
        agentId: 'openclaw',
        displayName: 'OpenClaw',
        iconKey: 'openclaw',
        skillsDir: '/Users/me/.agents/skills',
        installed: true,
        managedCount: 0,
        unmanagedCount: 1,
        importableCount: 1,
        items: [
          {
            id: 'unm-openclaw-stale',
            agentId: 'openclaw',
            skillId: 'skill-yuque-doc-polisher',
            name: 'skill-yuque-doc-polisher',
            path: '/Users/me/.agents/skills/skill-yuque-doc-polisher',
            managed: false,
            canImport: true,
            status: 'unmanaged',
            statusLabel: '未管理',
            reason: null,
            targetId: null,
            actualMode: null,
            hash: 'hash-yuque',
          },
        ],
      },
    ]
    vi.spyOn(skillApiV2, 'listAgentSkillInventory')
      .mockResolvedValueOnce(inventory)
      .mockResolvedValueOnce([])
    vi.spyOn(skillApiV2, 'executeAdopt').mockRejectedValueOnce(new Error('SKILL_UNMANAGED_STALE:unm-openclaw-stale'))

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    render(<AgentSyncPanel onDone={() => {}} />)

    fireEvent.click(await screen.findByLabelText('选择 skill-yuque-doc-polisher'))
    fireEvent.click(screen.getByRole('button', { name: '接管到中心库' }))

    expect(await screen.findByText(/该 Skill 已不在待处理列表中，请重新扫描后重试。/)).toBeInTheDocument()
    expect(screen.queryByText(/SKILL_UNMANAGED_STALE/)).not.toBeInTheDocument()
    expect(screen.queryByText(/Query returned no rows/)).not.toBeInTheDocument()
  })

  it('lists shared .agents skills separately and batch adopts them as center symlinks', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const inventory: AgentSkillInventoryAgent[] = [
      {
        agentId: 'agents',
        displayName: '.agents',
        iconKey: 'agents',
        skillsDir: 'C:\\Users\\me\\.agents\\skills',
        installed: true,
        managedCount: 0,
        unmanagedCount: 1,
        importableCount: 1,
        items: [
          {
            id: 'shared-local-alpha',
            agentId: 'agents',
            skillId: 'alpha',
            name: 'alpha',
            path: 'C:\\Users\\me\\.agents\\skills\\alpha',
            managed: false,
            canImport: true,
            status: 'unmanaged',
            statusLabel: '未管理',
            reason: null,
            targetId: null,
            actualMode: null,
            hash: 'hash-alpha',
          },
        ],
      },
    ]
    vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValue(inventory)
    const execute = vi.spyOn(skillApiV2, 'executeAdopt').mockResolvedValue('alpha')

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    render(<AgentSyncPanel onDone={() => {}} />)

    expect(await screen.findByText('本地 .agents Skills')).toBeInTheDocument()
    expect(screen.getByLabelText('选择 Agent')).not.toHaveTextContent('.agents · 1 可接管')
    fireEvent.click(screen.getByText('选择当前可接管'))
    fireEvent.click(screen.getByText('接管到中心库'))

    await waitFor(() => {
      expect(execute).toHaveBeenCalledWith('agents', 'shared-local-alpha', 'import_cleanup')
    })
  })

  it('cleans managed shared .agents skills from the shared source card', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const before: AgentSkillInventoryAgent[] = [
      {
        agentId: 'agents',
        displayName: '.agents',
        iconKey: 'agents',
        skillsDir: '/Users/me/.agents/skills',
        installed: true,
        managedCount: 2,
        unmanagedCount: 0,
        importableCount: 0,
        items: [
          {
            id: 'target-alpha',
            agentId: 'agents',
            skillId: 'alpha',
            name: 'alpha',
            path: '/Users/me/.agents/skills/alpha',
            managed: true,
            canImport: false,
            status: 'ok',
            statusLabel: '已管理',
            reason: null,
            targetId: 'target-alpha',
            actualMode: 'link',
            hash: null,
          },
          {
            id: 'target-beta',
            agentId: 'agents',
            skillId: 'beta',
            name: 'beta',
            path: '/Users/me/.agents/skills/beta',
            managed: true,
            canImport: false,
            status: 'ok',
            statusLabel: '已管理',
            reason: null,
            targetId: 'target-beta',
            actualMode: 'copy',
            hash: 'hash-beta',
          },
        ],
      },
    ]
    const after: AgentSkillInventoryAgent[] = [
      { ...before[0], managedCount: 0, items: [] },
    ]
    vi.spyOn(skillApiV2, 'listAgentSkillInventory')
      .mockResolvedValueOnce(before)
      .mockResolvedValueOnce(after)
    const cleanup = vi.spyOn(skillApiV2, 'deleteSkillTargetDistributions').mockResolvedValue({ deleted: 2, failures: [] })

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    render(<AgentSyncPanel onDone={() => {}} />)

    expect(await screen.findByText('本地 .agents Skills')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '清理已管理 .agents Skills' }))

    await waitFor(() => {
      expect(cleanup).toHaveBeenCalledWith(['target-alpha', 'target-beta'])
    })
    expect(await screen.findByRole('status')).toHaveTextContent('已清理 2 个 .agents 已管理 Skill')
  })

  it('previews one-click organize and defaults to center symlinks', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const inventory: AgentSkillInventoryAgent[] = [
      {
        agentId: 'claude-code',
        displayName: 'Claude Code',
        iconKey: 'claude-code',
        skillsDir: '/Users/me/.claude/skills',
        installed: true,
        managedCount: 0,
        unmanagedCount: 2,
        importableCount: 1,
        items: [
          {
            id: 'local-alpha',
            agentId: 'claude-code',
            skillId: 'alpha',
            name: 'alpha',
            path: '/Users/me/.claude/skills/alpha',
            managed: false,
            canImport: true,
            status: 'unmanaged',
            statusLabel: '未管理',
            reason: null,
            targetId: null,
            actualMode: null,
            hash: 'hash-alpha',
          },
          {
            id: 'local-bird',
            agentId: 'claude-code',
            skillId: 'bird',
            name: 'bird',
            path: '/Users/me/.claude/skills/bird',
            managed: false,
            canImport: true,
            status: 'conflict',
            statusLabel: '未管理 · 同名冲突',
            reason: 'same_name_as_center_skill',
            targetId: null,
            actualMode: null,
            hash: 'hash-bird',
          },
        ],
      },
    ]
    vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValue(inventory)
    const execute = vi.spyOn(skillApiV2, 'executeAdopt').mockResolvedValue('alpha')
    execute.mockClear()

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    render(<AgentSyncPanel onDone={() => {}} />)

    fireEvent.click(await screen.findByRole('button', { name: /一键整理 1 个/ }))

    expect(await screen.findByText('一键整理 Skills')).toBeInTheDocument()
    expect(screen.getByText('将整理 1 个可接管 Skill')).toBeInTheDocument()
    expect(screen.getByText('1 个同名冲突会保留给原来的冲突处理流程。')).toBeInTheDocument()
    expect(screen.getByLabelText('软连接（推荐）')).toHaveAttribute('aria-checked', 'true')
    expect(screen.getByLabelText('复制到 Agent')).toBeInTheDocument()
    expect(screen.getByLabelText('保留现有文件')).toBeInTheDocument()
    expect(execute).not.toHaveBeenCalled()

    fireEvent.click(screen.getByText('开始整理'))

    await waitFor(() => {
      expect(execute).toHaveBeenCalledWith('claude-code', 'local-alpha', 'import_link', null)
    })
    expect(execute).not.toHaveBeenCalledWith('claude-code', 'local-bird', 'import_link', null)
    expect(await screen.findByText(/已整理 1 个 Skill/)).toBeInTheDocument()
    expect(screen.getByText(/1 个需要处理冲突/)).toBeInTheDocument()
  })

  it('uses the selected one-click organize mode', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const inventory: AgentSkillInventoryAgent[] = [
      {
        agentId: 'claude-code',
        displayName: 'Claude Code',
        iconKey: 'claude-code',
        skillsDir: '/Users/me/.claude/skills',
        installed: true,
        managedCount: 0,
        unmanagedCount: 1,
        importableCount: 1,
        items: [
          {
            id: 'local-alpha',
            agentId: 'claude-code',
            skillId: 'alpha',
            name: 'alpha',
            path: '/Users/me/.claude/skills/alpha',
            managed: false,
            canImport: true,
            status: 'unmanaged',
            statusLabel: '未管理',
            reason: null,
            targetId: null,
            actualMode: null,
            hash: 'hash-alpha',
          },
        ],
      },
    ]
    vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValue(inventory)
    const execute = vi.spyOn(skillApiV2, 'executeAdopt').mockResolvedValue('alpha')

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    render(<AgentSyncPanel onDone={() => {}} />)

    fireEvent.click(await screen.findByRole('button', { name: /一键整理 1 个/ }))
    fireEvent.click(await screen.findByLabelText('保留现有文件'))
    fireEvent.click(screen.getByText('开始整理'))

    await waitFor(() => {
      expect(execute).toHaveBeenCalledWith('claude-code', 'local-alpha', 'import_keep', null)
    })
  })

  it('uses cleanup mode for shared .agents skills during one-click organize', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const inventory: AgentSkillInventoryAgent[] = [
      {
        agentId: 'agents',
        displayName: '本地 .agents Skills',
        iconKey: 'agents',
        skillsDir: '/Users/me/.agents/skills',
        installed: true,
        managedCount: 0,
        unmanagedCount: 1,
        importableCount: 1,
        items: [
          {
            id: 'shared-alpha',
            agentId: 'agents',
            skillId: 'alpha',
            name: 'alpha',
            path: '/Users/me/.agents/skills/alpha',
            managed: false,
            canImport: true,
            status: 'unmanaged',
            statusLabel: '未管理',
            reason: null,
            targetId: null,
            actualMode: null,
            hash: 'hash-alpha',
          },
        ],
      },
    ]
    vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValue(inventory)
    const execute = vi.spyOn(skillApiV2, 'executeAdopt').mockResolvedValue('alpha')

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    render(<AgentSyncPanel onDone={() => {}} />)

    fireEvent.click(await screen.findByRole('button', { name: /一键整理 1 个/ }))
    fireEvent.click(screen.getByText('开始整理'))

    await waitFor(() => {
      expect(execute).toHaveBeenCalledWith('agents', 'shared-alpha', 'import_cleanup', null)
    })
  })

  it('previews a single agent skill before adopting with the selected mode', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const inventory: AgentSkillInventoryAgent[] = [
      {
        agentId: 'claude-code',
        displayName: 'Claude Code',
        iconKey: 'claude-code',
        skillsDir: '/Users/me/.claude/skills',
        installed: true,
        managedCount: 0,
        unmanagedCount: 1,
        importableCount: 1,
        items: [
          {
            id: 'local-bird',
            agentId: 'claude-code',
            skillId: 'bird',
            name: 'bird',
            path: '/Users/me/.claude/skills/bird',
            managed: false,
            canImport: true,
            status: 'unmanaged',
            statusLabel: '未管理',
            reason: 'not_in_center_library',
            targetId: null,
            actualMode: null,
            hash: 'hash-bird',
          },
        ],
      },
    ]
    vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValueOnce(inventory)
    const preview = vi.spyOn(skillApiV2, 'previewAdopt').mockResolvedValueOnce({
      agentId: 'claude-code',
      unmanagedId: 'local-bird',
      skillPath: '/Users/me/.claude/skills/bird',
      inferredSkillId: 'bird',
      hash: 'hash-bird',
      centerHasSameId: false,
      canQuickAdopt: true,
      options: [
        { value: 'import_keep', label: 'Import to center, keep agent file as-is', destructive: false },
        { value: 'import_link', label: 'Import to center and replace agent file with link', destructive: true },
        { value: 'import_copy', label: 'Import to center and replace agent file with copy', destructive: true },
      ],
    })
    const execute = vi.spyOn(skillApiV2, 'executeAdopt').mockResolvedValueOnce('bird')

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    const { container } = render(<AgentSyncPanel onDone={() => {}} />)

    await screen.findByText('bird')
    fireEvent.click(container.querySelector('.sm2__icon-btn--add')!)

    expect(preview).toHaveBeenCalledWith('claude-code', 'local-bird')
    expect(await screen.findByText('接管 bird')).toBeInTheDocument()
    expect(screen.getAllByText('Claude Code').length).toBeGreaterThan(0)
    expect(screen.getAllByText('/Users/me/.claude/skills/bird').length).toBeGreaterThan(0)
    expect(screen.getByLabelText('替换为软连接')).toHaveAttribute('aria-checked', 'true')
    expect(screen.getByLabelText('保留 Agent 文件')).toHaveAttribute('aria-checked', 'false')

    fireEvent.click(screen.getByText('确认接管'))

    await waitFor(() => expect(execute).toHaveBeenCalledWith('claude-code', 'local-bird', 'import_link', null))
  })

  it('allows conflicting agent skills to be adopted with a rename decision', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const inventory: AgentSkillInventoryAgent[] = [
      {
        agentId: 'codex',
        displayName: 'Codex',
        iconKey: 'codex',
        skillsDir: '/Users/me/.codex/skills',
        installed: true,
        managedCount: 0,
        unmanagedCount: 1,
        importableCount: 0,
        items: [
          {
            id: 'codex-bird',
            agentId: 'codex',
            skillId: 'bird',
            name: 'bird',
            path: '/Users/me/.codex/skills/bird',
            managed: false,
            canImport: false,
            status: 'conflict',
            statusLabel: '未管理 · 同名冲突',
            reason: 'same_name_as_center_skill',
            targetId: null,
            actualMode: null,
            hash: 'hash-codex-bird',
          },
        ],
      },
    ]
    vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValueOnce(inventory)
    vi.spyOn(skillApiV2, 'previewAdopt').mockResolvedValueOnce({
      agentId: 'codex',
      unmanagedId: 'codex-bird',
      skillPath: '/Users/me/.codex/skills/bird',
      inferredSkillId: 'bird',
      hash: 'hash-codex-bird',
      centerHasSameId: true,
      canQuickAdopt: false,
      options: [
        { value: 'overwrite_center', label: 'Overwrite center skill with this one', destructive: true },
        { value: 'rename', label: 'Import under a new id', destructive: false },
        { value: 'skip', label: 'Keep as unmanaged', destructive: false },
      ],
    })
    const execute = vi.spyOn(skillApiV2, 'executeAdopt').mockResolvedValueOnce('bird-codex')

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    render(<AgentSyncPanel onDone={() => {}} />)

    fireEvent.click(await screen.findByText('bird'))
    fireEvent.click((await screen.findAllByText('处理冲突')).at(-1)!)
    fireEvent.click(await screen.findByLabelText('重命名导入'))
    fireEvent.change(screen.getByPlaceholderText('bird-import'), { target: { value: 'bird-codex' } })
    fireEvent.click(screen.getByText('确认接管'))

    await waitFor(() => expect(execute).toHaveBeenCalledWith('codex', 'codex-bird', 'rename', 'bird-codex'))
  })

  it('defaults conflicting agent skills to the center-library version when available', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const inventory: AgentSkillInventoryAgent[] = [
      {
        agentId: 'codex',
        displayName: 'Codex',
        iconKey: 'codex',
        skillsDir: '/Users/me/.codex/skills',
        installed: true,
        managedCount: 0,
        unmanagedCount: 1,
        importableCount: 0,
        items: [
          {
            id: 'codex-bird',
            agentId: 'codex',
            skillId: 'bird',
            name: 'bird',
            path: '/Users/me/.codex/skills/bird',
            managed: false,
            canImport: false,
            status: 'conflict',
            statusLabel: '未管理 · 同名冲突',
            reason: 'same_name_as_center_skill',
            targetId: null,
            actualMode: null,
            hash: 'hash-codex-bird',
          },
        ],
      },
    ]
    vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValueOnce(inventory)
    vi.spyOn(skillApiV2, 'previewAdopt').mockResolvedValueOnce({
      agentId: 'codex',
      unmanagedId: 'codex-bird',
      skillPath: '/Users/me/.codex/skills/bird',
      inferredSkillId: 'bird',
      hash: 'hash-codex-bird',
      centerHasSameId: true,
      canQuickAdopt: false,
      options: [
        { value: 'center_over_agent', label: 'Use center skill and replace agent file with link', destructive: true },
        { value: 'overwrite_center', label: 'Overwrite center skill with this one', destructive: true },
        { value: 'rename', label: 'Import under a new id', destructive: false },
        { value: 'skip', label: 'Keep as unmanaged', destructive: false },
      ],
    })
    const execute = vi.spyOn(skillApiV2, 'executeAdopt').mockResolvedValueOnce('bird')

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    render(<AgentSyncPanel onDone={() => {}} />)

    fireEvent.click(await screen.findByText('bird'))
    fireEvent.click((await screen.findAllByText('处理冲突')).at(-1)!)

    expect(await screen.findByLabelText('中心库为准')).toHaveAttribute('aria-checked', 'true')
    fireEvent.click(screen.getByText('确认接管'))

    await waitFor(() => expect(execute).toHaveBeenCalledWith('codex', 'codex-bird', 'center_over_agent', null))
  })

  it('batch adds conflicting agent skills with generated rename ids', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const inventory: AgentSkillInventoryAgent[] = [
      {
        agentId: 'codex',
        displayName: 'Codex',
        iconKey: 'codex',
        skillsDir: '/Users/me/.codex/skills',
        installed: true,
        managedCount: 0,
        unmanagedCount: 2,
        importableCount: 0,
        items: [
          {
            id: 'codex-bird',
            agentId: 'codex',
            skillId: 'bird',
            name: 'bird',
            path: '/Users/me/.codex/skills/bird',
            managed: false,
            canImport: false,
            status: 'conflict',
            statusLabel: '未管理 · 同名冲突',
            reason: 'same_name_as_center_skill',
            targetId: null,
            actualMode: null,
            hash: 'hash-codex-bird',
          },
          {
            id: 'codex-flight',
            agentId: 'codex',
            skillId: 'flight.plan',
            name: 'flight.plan',
            path: '/Users/me/.codex/skills/flight.plan',
            managed: false,
            canImport: false,
            status: 'conflict',
            statusLabel: '未管理 · 同名冲突',
            reason: 'same_name_as_center_skill',
            targetId: null,
            actualMode: null,
            hash: 'hash-codex-flight',
          },
        ],
      },
    ]
    vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValue(inventory)
    const execute = vi.spyOn(skillApiV2, 'executeAdopt').mockResolvedValue('ok')

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    render(<AgentSyncPanel onDone={() => {}} />)

    fireEvent.click(await screen.findByRole('button', { name: '批量处理冲突' }))
    expect(await screen.findByRole('heading', { name: '批量处理冲突' })).toBeInTheDocument()
    expect(screen.getByLabelText('中心库为准（推荐）')).toHaveAttribute('aria-checked', 'true')

    fireEvent.click(screen.getByLabelText('重命名新增'))
    fireEvent.click(screen.getByRole('button', { name: '批量新增' }))

    await waitFor(() => {
      expect(execute).toHaveBeenCalledWith('codex', 'codex-bird', 'rename', 'bird-codex')
      expect(execute).toHaveBeenCalledWith('codex', 'codex-flight', 'rename', 'flight-plan-codex')
    })
  })

  it('opens local agent skills in the skill-library detail layout', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const inventory: AgentSkillInventoryAgent[] = [
      {
        agentId: 'claude-code',
        displayName: 'Claude Code',
        iconKey: 'claude-code',
        skillsDir: '/Users/me/.claude/skills',
        installed: true,
        managedCount: 0,
        unmanagedCount: 1,
        importableCount: 1,
        items: [
          {
            id: 'local-find-skills',
            agentId: 'claude-code',
            skillId: 'find-skills',
            name: 'find-skills',
            path: '/Users/me/.claude/skills/find-skills',
            managed: false,
            canImport: true,
            status: 'unmanaged',
            statusLabel: '未管理',
            reason: 'not_in_center_library',
            targetId: null,
            actualMode: null,
            hash: '38d9217b9d12',
          },
        ],
      },
    ]
    vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValueOnce(inventory)
    vi.spyOn(skillApiV2, 'readFileTree').mockResolvedValueOnce({
      name: 'find-skills',
      nodeType: 'dir',
      path: '/Users/me/.claude/skills/find-skills',
      children: [
        {
          name: 'SKILL.md',
          nodeType: 'file',
          path: '/Users/me/.claude/skills/find-skills/SKILL.md',
          children: null,
        },
        {
          name: 'scripts',
          nodeType: 'dir',
          path: '/Users/me/.claude/skills/find-skills/scripts',
          children: [
            {
              name: 'find.js',
              nodeType: 'file',
              path: '/Users/me/.claude/skills/find-skills/scripts/find.js',
              children: null,
            },
          ],
        },
      ],
    })
    vi.spyOn(skillApiV2, 'readFileContent').mockResolvedValueOnce('# Find Skills\n\nLocal documentation.')

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    const { container } = render(<AgentSyncPanel onDone={() => {}} />)

    fireEvent.click(await screen.findByText('find-skills'))

    expect(document.body.querySelector('.sm2__slideover--skill-detail')).not.toBeNull()
    expect(document.body.querySelector('.sm2__slideover--market-detail')).toBeNull()
    expect(await screen.findByText('说明文档')).toBeInTheDocument()
    expect(screen.getByText('Agent 安装')).toBeInTheDocument()
    expect(screen.getByText('文件')).toBeInTheDocument()
    expect(screen.getAllByText('来源').length).toBeGreaterThan(0)
    expect(screen.getByText('Local documentation.')).toBeInTheDocument()
    expect(document.body.querySelector('.sm2__markdown--skilldoc > :first-child')?.tagName).toBe('H1')
    expect(document.body.querySelector('.sm2__skill-frontmatter')).toBeNull()
    fireEvent.click(screen.getByText('文件'))
    expect(document.body.querySelector('.sm2__filetree-pane')).not.toBeNull()
    expect(screen.getAllByText('find-skills').length).toBeGreaterThan(0)
    fireEvent.click(screen.getByText('scripts'))
    expect(screen.getByText('find.js')).toBeInTheDocument()
    expect(container).toBeTruthy()
  })

  it('reveals a local agent skill directory from the detail action', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const inventory: AgentSkillInventoryAgent[] = [
      {
        agentId: 'claude-code',
        displayName: 'Claude Code',
        iconKey: 'claude-code',
        skillsDir: '/Users/me/.claude/skills',
        installed: true,
        managedCount: 0,
        unmanagedCount: 1,
        importableCount: 1,
        items: [
          {
            id: 'local-alpha',
            agentId: 'claude-code',
            skillId: 'alpha',
            name: 'alpha',
            path: '/Users/me/.claude/skills/alpha',
            managed: false,
            canImport: true,
            status: 'unmanaged',
            statusLabel: '未管理',
            reason: null,
            targetId: null,
            actualMode: null,
            hash: 'hash-alpha',
          },
        ],
      },
    ]
    vi.mocked(openShell).mockClear()
    vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValueOnce(inventory)
    vi.spyOn(skillApiV2, 'readFileTree').mockResolvedValueOnce({
      name: 'alpha',
      nodeType: 'dir',
      path: '/Users/me/.claude/skills/alpha',
      children: [],
    })
    vi.spyOn(skillApiV2, 'readFileContent').mockResolvedValueOnce('# Alpha\n\nLocal documentation.')
    const revealPath = vi.spyOn(skillApiV2, 'revealPath').mockResolvedValueOnce(undefined)

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    render(<AgentSyncPanel onDone={() => {}} />)

    fireEvent.click(await screen.findByText('alpha'))
    fireEvent.click(screen.getByRole('button', { name: '打开目录 ↗' }))

    await waitFor(() => expect(revealPath).toHaveBeenCalledWith('/Users/me/.claude/skills/alpha'))
    expect(openShell).not.toHaveBeenCalled()
    expect(screen.queryByText('已在 Finder 中定位 Skill 目录')).not.toBeInTheDocument()
  })
})

describe('selecting a skill updates detail', () => {
  beforeEach(() => {
    cleanup()
    const skill = makeSkill()
    useSkillStoreV2.setState({
      viewMode: 'cards',
      filters: { query: '', source: '', status: '', type: '' },
      skills: [skill],
      overview: null,
      settings: {
        centerPath: '~/.agentbro/skills',
        sqlitePath: '~/.agentbro/skill-manager.db',
        defaultDistributeMode: 'link',
        linkFailPolicy: 'ask',
        startupScan: true,
        showUnmanaged: true,
      },
      loading: false,
      error: null,
      selectedSkillId: null,
      selectedSkillDetail: null,
      agents: [] as AgentSummary[],
    })
  })

  it('clicking a skill opens the detail slide-over', async () => {
    const { SkillLibraryPage } = await import('../components/skills-v2/SkillLibraryPage')
    render(<SkillLibraryPage />)
    fireEvent.click(screen.getByText('Release Checklist'))
    // SlideOver renders into document.body via portal; its title shows the id
    const title = document.body.querySelector('.sm2__slideover-title')
    expect(title?.textContent).toContain('release-checklist')
  })
})

describe('Skill detail slider + agent page render without crashing', () => {
  const agentDetail: AgentDetail = {
    id: 'claude-code',
    displayName: 'Claude Code',
    iconKey: 'claude-code',
    version: null,
    latestVersion: null,
    skillsDir: '/c',
    configPath: '/c/config.json',
    mcpConfigPath: '/c/mcp.json',
    pluginDir: '/c/plugins',
    skills: [
      {
        id: 'target-1',
        skillId: 'release-checklist',
        agentId: 'claude-code',
        targetPath: '/c/skills/release-checklist',
        resolvedTargetPath: null,
        installMode: 'link',
        actualMode: 'link',
        sourceHash: 'hash1',
        currentHash: 'hash1',
        status: 'ok',
        createdAt: '2026-01-01T00:00:00Z',
        updatedAt: '2026-01-01T00:00:00Z',
        claims: [{ id: 'claim-1', claimType: 'direct', packId: null, packName: null, createdAt: '2026-01-01T00:00:00Z' }],
      },
    ],
    inheritedManagedSkills: [],
    inheritedUnmanagedSkills: [],
    appliedPacks: [],
    availablePacks: [],
    mcpServers: [],
    plugins: [],
    health: [],
  }

  beforeEach(() => {
    cleanup()
    vi.restoreAllMocks()
    i18n.changeLanguage('zh')
    useRuntimeEnvironmentStore.setState({ selectedEnvironmentId: LOCAL_RUNTIME_ENVIRONMENT_ID })
    useSkillStoreV2.setState({
      runtimeEnvironmentId: LOCAL_RUNTIME_ENVIRONMENT_ID,
      agentProgramSnapshot: null,
      agentProgramOperation: null,
      viewMode: 'cards',
      filters: { query: '', source: '', status: '', type: '' },
      skills: [makeSkill()],
      agents: [
        { id: 'claude-code', displayName: 'Claude Code', iconKey: 'claude-code', enabled: true, skillsDir: '/c', version: null, latestVersion: null, installed: true, managedSkillCount: 1, unmanagedSkillCount: 0 } as AgentSummary,
      ],
      packs: [],
      overview: null,
      settings: {
        centerPath: '~/.agents/skills', sqlitePath: '~/.agentbro/skill-manager.db',
        defaultDistributeMode: 'link', linkFailPolicy: 'ask', startupScan: true, showUnmanaged: true,
      },
      loading: false, error: null,
      selectedSkillId: null, selectedSkillDetail: null,
      selectedAgentId: 'claude-code', selectedAgentDetail: agentDetail, agentDetailLoading: false,
      selectedPackId: null, selectedPackDetail: null,
      unmanaged: [
        {
          id: 'unmanaged-1',
          agentId: 'claude-code',
          itemType: 'skill',
          path: '/c/skills/manual-skill',
          inferredSkillId: 'manual-skill',
          hash: null,
          reason: 'not_in_center_library',
        },
      ],
      issues: [],
    })
    vi.spyOn(skillApiV2, 'refreshAgentSkillView').mockImplementation(async (agentId) => {
      const current = useSkillStoreV2.getState()
      return {
        agentDetail: current.selectedAgentDetail?.id === agentId ? current.selectedAgentDetail : agentDetail,
        overview: {
          ...makeOverview(),
          skills: current.skills,
          agents: current.agents,
          packs: current.packs,
        },
        unmanaged: current.unmanaged,
      }
    })
  })

  it('SkillDetailSlider renders when open', async () => {
    const { SkillDetailSlider } = await import('../components/skills-v2/SkillDetailSlider')
    const { container } = render(<SkillDetailSlider skillId="release-checklist" open={true} onClose={() => {}} onDistribute={() => {}} onDelete={() => {}} />)
    // portal renders the slide-over
    expect(document.body.querySelector('.sm2__slideover')).not.toBeNull()
    expect(container).toBeTruthy()
  })

  it('checks a Git Skill source and applies the selected version after confirmation', async () => {
    const sourceUri = 'skillssh:example/skills/release-checklist'
    const oldVersion = 'a'.repeat(40)
    const newVersion = 'b'.repeat(40)
    const source = {
      sourceType: 'skillssh', sourceUri, sourceRef: oldVersion,
      importedFromAgent: null, importedFromPath: null, installedVia: 'agentbro',
      createdAt: '2026-11-19T12:00:00Z', updatedAt: '2026-11-19T12:00:00Z',
    }
    const detail: SkillDetail = {
      ...makeSkill({ sourceType: 'skillssh', sourceUri }), source,
      centerResolvedPath: null, files: null, frontmatter: {}, targets: [],
    }
    const checked = {
      sourceUri, installedRef: oldVersion, latestRef: newVersion, currentHash: 'check-hash', updateAvailable: true,
      fileChanges: [
        { path: 'examples/说明.md', kind: 'added' as const },
        { path: 'SKILL.md', kind: 'modified' as const },
        { path: 'old-guide.md', kind: 'removed' as const },
      ],
    }
    vi.spyOn(skillApiV2, 'getSkillDetail').mockResolvedValue(detail)
    vi.spyOn(skillApiV2, 'overview').mockResolvedValue(makeOverview())
    const check = vi.spyOn(skillApiV2, 'checkSkillSourceUpdate').mockResolvedValue(checked)
    const apply = vi.spyOn(skillApiV2, 'applySkillSourceUpdate').mockResolvedValue({
      ...detail, source: { ...source, sourceRef: newVersion },
    })

    const { SkillDetailSlider } = await import('../components/skills-v2/SkillDetailSlider')
    render(<SkillDetailSlider skillId={detail.id} open onClose={() => {}} />)
    fireEvent.click(await screen.findByRole('button', { name: '来源' }))
    fireEvent.click(screen.getByRole('button', { name: '私有仓库' }))
    fireEvent.change(screen.getByLabelText('GitHub 访问令牌（仅本次使用）'), { target: { value: 'fixture-token' } })
    fireEvent.click(screen.getByRole('button', { name: '检查更新' }))
    expect(await screen.findByText('来源有可用更新 · bbbbbbbbbbbb')).toBeInTheDocument()
    expect(check).toHaveBeenCalledWith(detail.id, 'fixture-token')
    fireEvent.click(screen.getByRole('button', { name: '更新 Skill' }))
    const dialog = await screen.findByRole('dialog', { name: '更新 Skill' })
    expect(apply).not.toHaveBeenCalled()
    const changes = within(dialog).getByRole('list', { name: '文件变更' })
    expect(within(changes).getAllByRole('listitem').map((row) => row.textContent)).toEqual([
      'examples/说明.md新增',
      'SKILL.md修改',
      'old-guide.md删除',
    ])
    fireEvent.click(within(dialog).getByRole('button', { name: '更新 Skill' }))
    expect(await screen.findByText('Skill 已更新到 bbbbbbbbbbbb。')).toBeInTheDocument()
    expect(apply).toHaveBeenCalledWith(detail.id, checked, 'fixture-token')
    expect(screen.queryByLabelText('GitHub 访问令牌（仅本次使用）')).not.toBeInTheDocument()
    expect(screen.getByText(newVersion)).toBeInTheDocument()
  })

  it('loads the file tree for unmanaged fallback skills', async () => {
    vi.spyOn(skillApiV2, 'getSkillDetail').mockRejectedValueOnce(new Error('not in center'))
    const readFileTree = vi.spyOn(skillApiV2, 'readFileTree').mockResolvedValueOnce({
      name: 'manual-skill',
      nodeType: 'dir',
      path: '/c/skills/manual-skill',
      children: [
        {
          name: 'SKILL.md',
          nodeType: 'file',
          path: '/c/skills/manual-skill/SKILL.md',
          children: null,
        },
        {
          name: 'reference.md',
          nodeType: 'file',
          path: '/c/skills/manual-skill/reference.md',
          children: null,
        },
      ],
    })
    vi.spyOn(skillApiV2, 'readFileContent').mockResolvedValueOnce('# Manual Skill\n\nManual doc.')

    const { SkillDetailSlider } = await import('../components/skills-v2/SkillDetailSlider')
    render(
      <SkillDetailSlider
        skillId="manual-skill"
        open={true}
        onClose={() => {}}
        fallbackSkill={{
          id: 'manual-skill',
          name: 'manual-skill',
          centerPath: '/c/skills/manual-skill',
          sourceType: 'unmanaged_agent',
          sourceUri: '/c/skills/manual-skill',
        }}
      />,
    )

    expect(await screen.findByText('Manual doc.')).toBeInTheDocument()
    expect(readFileTree).toHaveBeenCalledWith('/c/skills/manual-skill')
    expect(document.body.querySelector('.sm2__markdown--skilldoc > :first-child')?.tagName).toBe('H1')
    expect(document.body.querySelector('.sm2__skill-frontmatter')).toBeNull()

    fireEvent.click(screen.getByText('文件'))
    expect(document.body.querySelector('.sm2__filetree-pane')).not.toBeNull()
    expect(screen.getAllByText('SKILL.md').length).toBeGreaterThan(0)
    expect(screen.getByText('reference.md')).toBeInTheDocument()
    expect(screen.getByText('2 个文件')).toBeInTheDocument()
  })

  it('labels linked center skills with their real source directory', async () => {
    vi.spyOn(skillApiV2, 'getSkillDetail').mockResolvedValueOnce({
      ...makeSkill({
        id: 'live-review',
        name: 'live-review',
        sourceType: 'local_folder',
        sourceUri: null,
        centerPath: '/Users/me/.agentbro/skills/live-review',
        installedAgents: [],
      }),
      centerResolvedPath: '/Users/me/code/szskills/live-review',
      frontmatter: { description: 'Linked local skill' },
      files: {
        name: 'live-review',
        nodeType: 'dir',
        path: '/Users/me/.agentbro/skills/live-review',
        children: [
          {
            name: 'SKILL.md',
            nodeType: 'file',
            path: '/Users/me/.agentbro/skills/live-review/SKILL.md',
            children: null,
          },
        ],
      },
      targets: [],
      source: {
        sourceType: 'local_folder',
        sourceUri: null,
        sourceRef: null,
        importedFromAgent: null,
        importedFromPath: null,
        installedVia: 'agentbro',
        createdAt: '2026-01-01T00:00:00Z',
        updatedAt: '2026-01-01T00:00:00Z',
      },
    })
    vi.spyOn(skillApiV2, 'readFileContent').mockResolvedValue('# live-review\n\nLinked local skill.')

    const { SkillDetailSlider } = await import('../components/skills-v2/SkillDetailSlider')
    render(<SkillDetailSlider skillId="live-review" open onClose={() => {}} />)

    expect(await screen.findByText('软链中心目录')).toBeInTheDocument()
    expect(screen.getByTitle('本地文件夹导入的 Skill，真实地址是：/Users/me/code/szskills/live-review')).toHaveTextContent('本地软链')
    expect(screen.getByText('/Users/me/code/szskills/live-review')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '来源' }))
    expect(screen.getByText('真实源目录')).toBeInTheDocument()
  })

  it('does not repeat the agent id under the agent display name', async () => {
    vi.spyOn(skillApiV2, 'getSkillDetail').mockResolvedValueOnce({
      ...makeSkill({
        installedAgents: [
          { agentId: 'claude-code', displayName: 'Claude Code', iconKey: 'claude-code', mode: 'link', status: 'ok' },
        ],
      }),
      frontmatter: {},
      files: null,
      targets: agentDetail.skills,
      source: null,
    })

    const { SkillDetailSlider } = await import('../components/skills-v2/SkillDetailSlider')
    render(<SkillDetailSlider skillId="release-checklist" open={true} onClose={() => {}} />)

    fireEvent.click(await screen.findByText('Agent (1)'))
    const title = document.body.querySelector('.sm2__agent-target-title')
    expect(title).toHaveTextContent('Claude Code')
    expect(title).not.toHaveTextContent('claude-code')
  })

  it('marks rendered skill markdown as selectable for copying text', async () => {
    vi.spyOn(skillApiV2, 'getSkillDetail').mockResolvedValueOnce({
      ...makeSkill(),
      frontmatter: {},
      files: {
        name: 'release-checklist',
        nodeType: 'dir',
        path: '/center/release-checklist',
        children: [
          {
            name: 'SKILL.md',
            nodeType: 'file',
            path: '/center/release-checklist/SKILL.md',
            children: null,
          },
        ],
      },
      targets: [],
      source: null,
    })
    vi.spyOn(skillApiV2, 'readFileContent').mockResolvedValue('# Release Checklist\n\nCopy this text.')

    const { SkillDetailSlider } = await import('../components/skills-v2/SkillDetailSlider')
    render(<SkillDetailSlider skillId="release-checklist" open={true} onClose={() => {}} />)

    const paragraph = await screen.findByText('Copy this text.')
    const overviewMarkdown = paragraph.closest('.sm2__markdown')
    expect(overviewMarkdown).toHaveClass('selectable')
    const title = document.body.querySelector('.sm2__slideover-title .selectable')
    expect(title).toHaveTextContent('Release Checklist')

    fireEvent.click(screen.getByText('文件'))
    const filePreview = document.body.querySelector('.sm2__markdown--file')
    expect(filePreview).toHaveClass('selectable')

    vi.spyOn(window, 'getSelection').mockReturnValue({ toString: () => 'Release Checklist' } as Selection)
    fireEvent.contextMenu(title as Element, { clientX: 80, clientY: 60 })
    expect(screen.getByRole('menuitem', { name: '复制' })).toBeInTheDocument()
  })

  it('marks source paths and hashes as selectable for drag copy', async () => {
    vi.spyOn(skillApiV2, 'getSkillDetail').mockResolvedValueOnce({
      ...makeSkill({
        centerPath: '/Users/me/.agentbro/skills/release-checklist',
        currentHash: 'abcdef1234567890',
      }),
      frontmatter: {},
      files: null,
      targets: [],
      source: {
        sourceType: 'agent_import',
        sourceUri: null,
        sourceRef: null,
        importedFromAgent: 'claude-code',
        importedFromPath: '/Users/me/.claude/skills/release-checklist',
        installedVia: 'agentbro',
        createdAt: '2026-01-01T00:00:00Z',
        updatedAt: '2026-01-01T00:00:00Z',
      },
    })

    const { SkillDetailSlider } = await import('../components/skills-v2/SkillDetailSlider')
    render(<SkillDetailSlider skillId="release-checklist" open={true} onClose={() => {}} />)

    fireEvent.click(await screen.findByRole('button', { name: '来源' }))

    expect(screen.getByText('/Users/me/.claude/skills/release-checklist')).toHaveClass('selectable')
    expect(screen.getByText('/Users/me/.agentbro/skills/release-checklist')).toHaveClass('selectable')
    expect(screen.getByText('abcdef1234567890')).toHaveClass('selectable')
  })

  it('opens a center-to-copy diff from a changed copy target', async () => {
    const centerLines = Array.from({ length: 275 }, (_, index) => `line ${index + 1}`)
    const copyLines = [...centerLines]
    copyLines[2] = 'line 3 123'
    vi.spyOn(skillApiV2, 'getSkillDetail').mockResolvedValueOnce({
      ...makeSkill({
        status: 'copyDiverged',
        installedAgents: [
          { agentId: 'codex', displayName: 'Codex', iconKey: 'codex', mode: 'copy', status: 'copy_modified' },
        ],
      }),
      frontmatter: {},
      files: null,
      targets: [
        {
          id: 'target-codex',
          skillId: 'release-checklist',
          agentId: 'codex',
          targetPath: '/Users/me/.codex/skills/release-checklist',
          resolvedTargetPath: null,
          installMode: 'copy',
          actualMode: 'copy',
          sourceHash: 'hash1',
          currentHash: 'hash2',
          status: 'copy_modified',
          createdAt: '2026-01-01T00:00:00Z',
          updatedAt: '2026-01-01T00:00:00Z',
          claims: [],
        },
      ],
      source: null,
    })
    const previewDiff = vi.spyOn(skillApiV2, 'previewCopyTargetDiff').mockResolvedValueOnce({
      targetId: 'target-codex',
      skillId: 'release-checklist',
      targetPath: '/Users/me/.codex/skills/release-checklist',
      centerPath: '/center/release-checklist',
      state: 'copy_modified',
      files: [
        {
          path: 'SKILL.md',
          changeType: 'modified',
          centerContent: centerLines.join('\n'),
          copyContent: copyLines.join('\n'),
        },
      ],
    })

    const { SkillDetailSlider } = await import('../components/skills-v2/SkillDetailSlider')
    render(<SkillDetailSlider skillId="release-checklist" open={true} onClose={() => {}} />)

    await waitFor(() => {
      expect(document.body.querySelector('.sm2__detail-status--copy-diff')).not.toBeNull()
      expect(document.body.querySelector('.sm2__install-mini--copy-diff')).not.toBeNull()
    })

    fireEvent.click(await screen.findByText('Agent (1)'))
    fireEvent.click(screen.getByRole('button', { name: '查看 diff' }))

    await waitFor(() => expect(previewDiff).toHaveBeenCalledWith('target-codex'))
    expect(await screen.findByRole('dialog', { name: 'Agent 副本 diff' })).toBeInTheDocument()
    expect(screen.getByText('目录树')).toBeInTheDocument()
    expect(screen.getAllByText('SKILL.md').length).toBeGreaterThan(0)
    expect(screen.getByText('中心库')).toBeInTheDocument()
    expect(screen.getByText('Agent 副本')).toBeInTheDocument()
    expect(screen.getByText('line 3')).toBeInTheDocument()
    expect(screen.getByText('line 3 123')).toBeInTheDocument()
    expect(document.body.querySelectorAll('.sm2__copy-diff-side-scroll')).toHaveLength(2)
    expect(document.body.querySelectorAll('.sm2__copy-diff-side-scroll[data-scroll-mode="independent-x-fixed-y-sync"]')).toHaveLength(2)
    expect(document.body.querySelectorAll('.sm2__copy-diff-cell--remove')).toHaveLength(1)
    expect(document.body.querySelectorAll('.sm2__copy-diff-cell--add')).toHaveLength(1)
  })

  it('deletes a single agent distribution from the Agent tab', async () => {
    const afterDelete = makeSkill({
      installedAgents: [],
    })
    vi.spyOn(skillApiV2, 'getSkillDetail')
      .mockResolvedValueOnce({
        ...makeSkill({
          installedAgents: [
            { agentId: 'codex', displayName: 'Codex', iconKey: 'codex', mode: 'copy', status: 'ok' },
          ],
        }),
        frontmatter: {},
        files: null,
        targets: [
          {
            id: 'target-codex',
            skillId: 'release-checklist',
            agentId: 'codex',
            targetPath: '/Users/me/.codex/skills/release-checklist',
            resolvedTargetPath: null,
            installMode: 'copy',
            actualMode: 'copy',
            sourceHash: 'hash1',
            currentHash: 'hash1',
            status: 'ok',
            createdAt: '2026-01-01T00:00:00Z',
            updatedAt: '2026-01-01T00:00:00Z',
            claims: [],
          },
        ],
        source: null,
      })
      .mockResolvedValueOnce({
        ...afterDelete,
        frontmatter: {},
        files: null,
        targets: [],
        source: null,
      })
    const deleteTarget = vi.spyOn(skillApiV2, 'deleteSkillTargetDistribution').mockResolvedValueOnce()

    const { SkillDetailSlider } = await import('../components/skills-v2/SkillDetailSlider')
    render(<SkillDetailSlider skillId="release-checklist" open={true} onClose={() => {}} />)

    fireEvent.click(await screen.findByText('Agent (1)'))
    fireEvent.click(screen.getByRole('button', { name: '删除分发' }))
    fireEvent.click(await screen.findByRole('button', { name: '确认删除' }))

    await waitFor(() => expect(deleteTarget).toHaveBeenCalledWith('target-codex'))
    expect(await screen.findByText('尚未分发到任何 Agent')).toBeInTheDocument()
  })

  it('opens skill documentation links with the system browser', async () => {
    Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {}, configurable: true })
    try {
      vi.mocked(openShell).mockClear()
      vi.spyOn(skillApiV2, 'getSkillDetail').mockResolvedValueOnce({
        ...makeSkill(),
        frontmatter: {},
        files: {
          name: 'release-checklist',
          nodeType: 'dir',
          path: '/center/release-checklist',
          children: [
            {
              name: 'SKILL.md',
              nodeType: 'file',
              path: '/center/release-checklist/SKILL.md',
              children: null,
            },
          ],
        },
        targets: [],
        source: null,
      })
      vi.spyOn(skillApiV2, 'readFileContent').mockResolvedValue('Read the [docs](https://example.com/docs).')

      const { SkillDetailSlider } = await import('../components/skills-v2/SkillDetailSlider')
      render(<SkillDetailSlider skillId="release-checklist" open={true} onClose={() => {}} />)

      fireEvent.click(await screen.findByRole('link', { name: 'docs' }))

      expect(openShell).toHaveBeenCalledWith('https://example.com/docs')
    } finally {
      delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__
    }
  })

  it('AgentManagementPage uses the sidebar agent list and renders no picker in the content pane', async () => {
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    const { container } = render(<AgentManagementPage />)
    expect(container.querySelector('.sm2__rail')).toBeNull()
    expect(container.querySelector('.sm2__agent-picker')).toBeNull()
    expect(container.querySelector('.sm2__main--full')).not.toBeNull()
    expect(screen.getByText('Claude Code')).toBeInTheDocument()
  })

  it('summarizes Agent health and opens capability details from the overview', async () => {
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    expect(screen.getByText('能力快照')).toBeInTheDocument()
    expect(screen.getByText('1 项需要关注')).toBeInTheDocument()
    const skillsCard = screen.getByText('1 个待接管').closest('button')
    expect(skillsCard).not.toBeNull()

    fireEvent.click(skillsCard!)
    expect(screen.getByRole('button', { name: 'Skills (2)' })).toHaveClass('sm2__subtab--active')
  })

  it('keeps compact apply and cancel controls for skill packs in the overview', async () => {
    useSkillStoreV2.setState({
      selectedAgentDetail: {
        ...agentDetail,
        appliedPacks: [
          { packId: 'pack-active', packName: 'Active Pack', memberCount: 3, agentId: 'claude-code', displayName: 'Claude Code' },
        ],
        availablePacks: [
          { id: 'pack-active', name: 'Active Pack', description: '', tags: [], memberCount: 3, appliedAgentCount: 1, healthy: true },
          { id: 'pack-ready', name: 'Ready Pack', description: '', tags: [], memberCount: 2, appliedAgentCount: 0, healthy: true },
        ],
      },
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    expect(screen.getByText('已生效')).toBeInTheDocument()
    expect(screen.getByText('可应用')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '取消应用 Active Pack' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '应用 Ready Pack' })).toBeInTheDocument()
  })

  it('adds a custom Claude-compatible agent from manual paths', async () => {
    const addCustom = vi.spyOn(agentApi, 'addCustom').mockResolvedValue(makeProgram({
      id: 'custom-antcc',
      displayName: 'AntCC',
      icon: 'claude-code',
      packageManager: 'custom',
      packageName: null,
      configDir: '/Users/me/.codefuse/engine/cc',
      skillsDir: '/Users/me/.codefuse/engine/cc/skills',
      isCustom: true,
    }))
    const loadOverview = vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue(undefined)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    expect(screen.queryByRole('button', { name: /添加 Claude Code 实例/ })).not.toBeInTheDocument()
    act(() => useSkillStoreV2.getState().requestCustomAgentDialog())
    const dialog = await screen.findByRole('dialog', { name: /添加 Claude Code 实例/ })
    expect(dialog.closest('.sm2-custom-agent-overlay')).toBeInTheDocument()
    expect(dialog.querySelector('#custom-agent-engine')).toBeNull()
    expect(screen.getByPlaceholderText('例如研发团队 Claude Code')).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('显示名称'), { target: { value: 'AntCC' } })
    fireEvent.change(screen.getByLabelText('配置根目录'), { target: { value: '~/.codefuse/engine/cc/' } })
    fireEvent.click(screen.getByRole('button', { name: '保存 Agent' }))

    await waitFor(() => expect(addCustom).toHaveBeenCalledWith({
      id: null,
      displayName: 'AntCC',
      category: 'claude-compatible',
      globalSkillsDir: '~/.codefuse/engine/cc/skills',
      iconName: 'claude-code',
      configDir: '~/.codefuse/engine/cc/',
      settingsFile: '~/.codefuse/engine/cc/settings.json',
      mcpConfig: '~/.codefuse/engine/cc/settings.json',
      pluginDir: '~/.codefuse/engine/cc/plugins/cache',
    }, LOCAL_RUNTIME_ENVIRONMENT_ID))
    await waitFor(() => expect(dialog).not.toBeInTheDocument())
    expect(loadOverview).toHaveBeenCalledWith(true)
  })

  it('browses the selected SSH host when configuring a custom Agent', async () => {
    const runtime = useRuntimeEnvironmentStore.getState()
    const hostId = 'company-ssh'
    runtime.selectEnvironment(hostId)
    useSkillStoreV2.setState({ runtimeEnvironmentId: hostId })
    const openLocal = vi.mocked(openDialog)
    openLocal.mockClear()
    const browse = vi.spyOn(skillApiV2, 'browseRemoteSkillSources').mockResolvedValue({
      path: '/home/remote/branch',
      parentPath: '/home/remote',
      entries: [],
    })
    const add = vi.spyOn(agentApi, 'addCustom').mockRejectedValue(new Error('Fixture ends before persistence'))

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    act(() => useSkillStoreV2.getState().requestCustomAgentDialog())
    fireEvent.change(await screen.findByLabelText('显示名称'), { target: { value: 'Remote branch' } })
    fireEvent.click(screen.getByRole('button', { name: '选择目录' }))
    expect(await screen.findByRole('dialog', { name: `选择 ${hostId} 上的文件夹` })).toBeInTheDocument()
    await waitFor(() => expect(browse).toHaveBeenCalledExactlyOnceWith('~', hostId))
    await waitFor(() => expect(screen.getByRole('button', { name: '选择当前文件夹' })).toBeEnabled())
    fireEvent.click(screen.getByRole('button', { name: '选择当前文件夹' }))

    expect(screen.getByLabelText('配置根目录')).toHaveValue('/home/remote/branch')
    expect(openLocal).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: '保存 Agent' }))
    expect(add).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({
      displayName: 'Remote branch',
      configDir: '/home/remote/branch',
      globalSkillsDir: '/home/remote/branch/skills',
      settingsFile: '/home/remote/branch/settings.json',
    }), hostId)
  })

  it('drops a custom Agent folder browse when the SSH host changes', async () => {
    const runtime = useRuntimeEnvironmentStore.getState()
    runtime.selectEnvironment('earlier-ssh')
    useSkillStoreV2.setState({ runtimeEnvironmentId: 'earlier-ssh' })
    let finishBrowse!: (value: Awaited<ReturnType<typeof skillApiV2.browseRemoteSkillSources>>) => void
    const browse = vi.spyOn(skillApiV2, 'browseRemoteSkillSources').mockReturnValue(
      new Promise((resolve) => { finishBrowse = resolve }),
    )
    const add = vi.spyOn(agentApi, 'addCustom')
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    act(() => useSkillStoreV2.getState().requestCustomAgentDialog())
    fireEvent.click(await screen.findByRole('button', { name: '选择目录' }))
    expect(browse).toHaveBeenCalledExactlyOnceWith('~', 'earlier-ssh')
    await act(async () => {
      runtime.selectEnvironment('other-ssh')
      useSkillStoreV2.setState({ runtimeEnvironmentId: 'other-ssh' })
      finishBrowse({ path: '/home/earlier/branch', parentPath: null, entries: [] })
    })
    expect(screen.queryByRole('button', { name: '选择当前文件夹' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '保存 Agent' })).not.toBeInTheDocument()
    expect(add).not.toHaveBeenCalled()
  })

  it('deletes a custom agent from the agent detail header', async () => {
    const customDetail: AgentDetail = {
      ...agentDetail,
      id: 'custom-antcc',
      displayName: 'AntCC',
      iconKey: 'claude-code',
      skillsDir: '/Users/me/.codefuse/engine/cc/skills',
      configPath: '/Users/me/.codefuse/engine/cc/settings.json',
      mcpConfigPath: '/Users/me/.codefuse/engine/cc/settings.json',
      pluginDir: '/Users/me/.codefuse/engine/cc/plugins/cache',
      skills: [],
    }
    useSkillStoreV2.setState({
      selectedAgentId: 'custom-antcc',
      selectedAgentDetail: customDetail,
      agents: [
        { id: 'custom-antcc', displayName: 'AntCC', iconKey: 'claude-code', enabled: true, skillsDir: '/Users/me/.codefuse/engine/cc/skills', version: null, latestVersion: null, installed: true, managedSkillCount: 0, unmanagedSkillCount: 0 } as AgentSummary,
      ],
      unmanaged: [],
    })
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([
      makeProgram({
        id: 'custom-antcc',
        displayName: 'AntCC',
        icon: 'claude-code',
        packageManager: 'custom',
        packageName: null,
        configDir: '/Users/me/.codefuse/engine/cc',
        skillsDir: '/Users/me/.codefuse/engine/cc/skills',
        isCustom: true,
      }),
    ])
    const removeCustom = vi.spyOn(agentApi, 'removeCustom').mockResolvedValue(undefined)
    const loadOverview = vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue(undefined)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(await screen.findByRole('button', { name: '删除此 Agent' }))
    expect(screen.getByRole('dialog', { name: '删除 Agent「AntCC」' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '确认删除' }))

    await waitFor(() => expect(removeCustom).toHaveBeenCalledWith('custom-antcc', LOCAL_RUNTIME_ENVIRONMENT_ID))
    expect(loadOverview).toHaveBeenCalledWith(true)
    expect(useSkillStoreV2.getState().selectedAgentId).toBeNull()
  })

  it('keeps a same-ID custom Agent in the new runtime after a deletion finishes elsewhere', async () => {
    const id = 'custom-team'
    const local = makeSidebarAgent(id, 'Local Agent')
    const remote = makeSidebarAgent(id, 'Remote Agent')
    useSkillStoreV2.setState({
      agents: [local],
      selectedAgentId: id,
      selectedAgentDetail: { ...agentDetail, ...local },
    })
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([makeProgram({ id, isCustom: true })])
    let finishRemoval!: () => void
    vi.spyOn(agentApi, 'removeCustom').mockReturnValue(new Promise((resolve) => { finishRemoval = resolve }))
    const loadOverview = vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue()

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    const { unmount } = render(<AgentManagementPage />)
    fireEvent.click(await screen.findByRole('button', { name: '删除此 Agent' }))
    fireEvent.click(screen.getByRole('button', { name: '确认删除' }))
    unmount()

    act(() => {
      useRuntimeEnvironmentStore.getState().selectEnvironment('other-host')
      useSkillStoreV2.setState({
        runtimeEnvironmentId: 'other-host',
        agents: [remote],
        selectedAgentId: id,
        selectedAgentDetail: { ...agentDetail, ...remote },
      })
    })
    loadOverview.mockClear()
    await act(async () => finishRemoval())

    expect(useSkillStoreV2.getState().agents).toEqual([remote])
    expect(useSkillStoreV2.getState().selectedAgentId).toBe(id)
    expect(loadOverview).not.toHaveBeenCalled()
  })

  it.each([
    { operation: 'install', status: 'notInstalled', button: '安装此 Agent' },
    { operation: 'update', status: 'updateAvailable', button: '更新此 Agent' },
  ] as const)('ignores completion of an Agent $operation after leaving and returning to a runtime', async ({ operation, status, button }) => {
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([makeProgram({
      status, installedVersion: '1.0.0', latestVersion: '2.0.0',
    })])
    let finishOperation!: () => void
    vi.spyOn(agentApi, operation).mockReturnValue(new Promise((resolve) => { finishOperation = resolve }))
    const loadOverview = vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue()
    const loadAgentDetail = vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue()

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(await screen.findByRole('button', { name: button }))
    await act(async () => {
      const { selectEnvironment } = useRuntimeEnvironmentStore.getState()
      selectEnvironment('other-host')
      selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    })
    loadOverview.mockClear()
    loadAgentDetail.mockClear()
    await act(async () => finishOperation())

    expect(loadOverview).not.toHaveBeenCalled()
    expect(loadAgentDetail).not.toHaveBeenCalled()
    expect(screen.queryByText(/Agent (安装|更新)完成/)).not.toBeInTheDocument()
  })

  it('reports an install whose command exits successfully without a detected Agent', async () => {
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([makeProgram({ status: 'notInstalled', binaryPath: null })])
    vi.spyOn(agentApi, 'install').mockResolvedValue(undefined)
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue()
    vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue()
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(await screen.findByRole('button', { name: '安装此 Agent' }))

    expect(await screen.findByRole('alert')).toHaveTextContent('仍未检测到 Claude Code')
    expect(screen.queryByText('Agent 安装完成')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '安装此 Agent' })).toBeEnabled()
  })

  it('reports an unchanged version and confirms a retry only after detecting the update', async () => {
    const outdated = makeProgram({ status: 'updateAvailable', installedVersion: '1.0.0', latestVersion: '2.0.0' })
    vi.spyOn(agentApi, 'refresh')
      .mockResolvedValueOnce([outdated])
      .mockResolvedValueOnce([outdated])
      .mockResolvedValue([makeProgram({ installedVersion: '2.0.0', latestVersion: '2.0.0' })])
    vi.spyOn(agentApi, 'update').mockResolvedValue(undefined)
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue()
    vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue()
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(await screen.findByRole('button', { name: '更新此 Agent' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Claude Code 仍为 1.0.0')
    expect(screen.queryByText('Agent 更新完成')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '更新此 Agent' }))

    expect(await screen.findByText('Agent 更新完成')).toBeInTheDocument()
    expect(screen.getByText('当前版本 2.0.0')).toBeInTheDocument()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  it('shows desktop-managed updates for the selected bundled Codex', async () => {
    useSkillStoreV2.setState({
      agents: [makeSidebarAgent('codex', 'Codex', { managed: 1 })],
      selectedAgentId: 'codex',
      selectedAgentDetail: { ...agentDetail, id: 'codex', displayName: 'Codex', version: '9.9.9', latestVersion: '9.9.10' },
    })
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue()
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([makeProgram({
      id: 'codex',
      displayName: 'Codex',
      packageManager: 'app',
      packageName: null,
      installedVersion: '0.169.1',
      binaryPath: '/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex',
      installCommand: null,
      updateCommand: null,
      uninstallCommand: null,
      hooksInstalled: true,
    })])
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    expect(await screen.findByText('随桌面客户端更新')).toBeInTheDocument()
    expect(screen.getByText('当前版本 0.169.1')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '无需更新' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '卸载 Agent' })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '清理残留' })).toBeEnabled()
    fireEvent.click(screen.getByRole('button', { name: '路径与设置' }))
    expect(screen.queryByText('9.9.10')).not.toBeInTheDocument()
  })

  it.each(['2.1.89', null])('runs the native Claude updater without a registry version when the previous version is %s', async (previousVersion) => {
    useSkillStoreV2.setState({ selectedAgentDetail: { ...agentDetail, version: '1.0.0' } })
    const native = makeProgram({
      packageManager: 'native',
      packageName: null,
      installedVersion: '2.1.89',
      latestVersion: null,
      installCommand: null,
      updateCommand: 'claude update',
      uninstallCommand: null,
    })
    vi.spyOn(agentApi, 'refresh')
      .mockResolvedValueOnce([{ ...native, installedVersion: previousVersion }])
      .mockResolvedValueOnce([native])
      .mockResolvedValue([makeProgram({ ...native, installedVersion: '2.1.90' })])
    const update = vi.spyOn(agentApi, 'update').mockResolvedValue(undefined)
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue()
    vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue()
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    const check = await screen.findByRole('button', { name: '检查并更新' })
    expect(screen.getByText(previousVersion ? '当前版本 2.1.89' : '当前版本 未知')).toBeInTheDocument()
    fireEvent.click(check)
    expect(update).toHaveBeenCalledExactlyOnceWith('claude-code', LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(await screen.findByText('更新检查完成，Claude Code 当前版本为 2.1.89。')).toBeInTheDocument()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '卸载 Agent' })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '清理残留' })).toBeEnabled()

    fireEvent.click(screen.getByRole('button', { name: '检查并更新' }))
    expect(await screen.findByText('Agent 更新完成')).toBeInTheDocument()
    expect(screen.getByText('当前版本 2.1.90')).toBeInTheDocument()
  })

  it('keeps an update active across Agent Management navigation and keeps its verified version', async () => {
    const oldProgram = makeProgram({ status: 'updateAvailable', installedVersion: '1.0.0', latestVersion: '2.0.0' })
    const newProgram = makeProgram({ installedVersion: '2.0.0', latestVersion: '2.0.0' })
    let finishUpdate!: () => void
    let finishReturnLookup!: (programs: AgentProgramInfo[]) => void
    let finishVerification!: (programs: AgentProgramInfo[]) => void
    const refresh = vi.spyOn(agentApi, 'refresh')
      .mockResolvedValueOnce([oldProgram])
      .mockReturnValueOnce(new Promise((resolve) => { finishReturnLookup = resolve }))
      .mockReturnValueOnce(new Promise((resolve) => { finishVerification = resolve }))
    const update = vi.spyOn(agentApi, 'update').mockReturnValue(new Promise((resolve) => { finishUpdate = resolve }))
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue()
    vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue()

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    const firstPage = render(<AgentManagementPage />)
    fireEvent.click(await screen.findByRole('button', { name: '更新此 Agent' }))
    expect(update).toHaveBeenCalledTimes(1)
    firstPage.unmount()
    render(<AgentManagementPage />)
    await waitFor(() => expect(refresh).toHaveBeenCalledTimes(2))

    expect(screen.getByRole('button', { name: '正在更新' })).toBeDisabled()
    expect(screen.getByRole('button', { name: '卸载 Agent' })).toBeDisabled()
    await act(async () => finishUpdate())
    await waitFor(() => expect(refresh).toHaveBeenCalledTimes(3))
    expect(screen.getByRole('button', { name: '正在更新' })).toBeDisabled()

    await act(async () => finishVerification([newProgram]))
    expect(screen.getByText('当前版本 2.0.0')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '卸载 Agent' })).toBeEnabled()
    await act(async () => finishReturnLookup([oldProgram]))
    expect(screen.getByText('当前版本 2.0.0')).toBeInTheDocument()
    expect(update).toHaveBeenCalledTimes(1)
  })

  it('discards an entry lookup error once Agent Management is closed', async () => {
    let failLookup!: (error: Error) => void
    const refresh = vi.spyOn(agentApi, 'refresh').mockReturnValue(new Promise((_, reject) => { failLookup = reject }))
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    const page = render(<AgentManagementPage />)
    await waitFor(() => expect(refresh).toHaveBeenCalledTimes(1))
    page.unmount()

    await act(async () => failLookup(new Error('Old Agent lookup failed')))
    expect(useSkillStoreV2.getState().error).toBeNull()
  })

  it('refreshes the new Settings window after an external package finishes and ignores the older entry scan', async () => {
    const old = makeProgram({ status: 'updateAvailable', installedVersion: '2.0.0', latestVersion: '999.0.0' })
    const updated = makeProgram({ status: 'installed', installedVersion: '999.0.0', latestVersion: '999.0.0' })
    let finishEntryScan!: (programs: AgentProgramInfo[]) => void
    let finishSubscription!: () => void
    let emit: ((event: AgentOutputEvent) => void) | undefined
    const refresh = vi.spyOn(agentApi, 'refresh')
      .mockReturnValueOnce(new Promise((resolve) => { finishEntryScan = resolve }))
      .mockResolvedValue([updated])
    vi.spyOn(agentApi, 'onOutput').mockImplementation((callback) => {
      emit = callback
      return new Promise((resolve) => {
        finishSubscription = () => resolve(() => { emit = undefined })
      })
    })
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue()
    vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue()
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    const page = render(<AgentManagementPage />)
    expect(refresh).not.toHaveBeenCalled()
    await act(async () => finishSubscription())
    await waitFor(() => expect(refresh).toHaveBeenCalledTimes(1))
    const completion: AgentOutputEvent = {
      agentId: 'claude-code',
      operation: 'update',
      stream: 'info',
      line: 'Operation completed',
      done: true,
      success: true,
    }
    const active = {
      agentId: 'claude-code',
      operation: 'update',
      environmentId: LOCAL_RUNTIME_ENVIRONMENT_ID,
      selectionRevision: useRuntimeEnvironmentStore.getState().selectionRevision,
    } as const
    act(() => {
      useSkillStoreV2.getState().beginAgentProgramOperation(active)
      emit?.(completion)
      useSkillStoreV2.getState().finishAgentProgramOperation(active)
      emit?.({ ...completion, done: false })
    })
    expect(refresh).toHaveBeenCalledTimes(1)

    await act(async () => emit?.(completion))
    expect(await screen.findByText('当前版本 999.0.0')).toBeInTheDocument()
    await act(async () => finishEntryScan([old]))
    expect(screen.getByText('当前版本 999.0.0')).toBeInTheDocument()
    expect(refresh).toHaveBeenCalledTimes(2)

    const oldListener = emit
    await act(async () => {
      useRuntimeEnvironmentStore.getState().selectEnvironment('remote-package-host')
      useSkillStoreV2.setState({ runtimeEnvironmentId: 'remote-package-host' })
    })
    expect(refresh).toHaveBeenLastCalledWith('remote-package-host')
    oldListener?.(completion)
    expect(refresh).toHaveBeenCalledTimes(3)
    page.unmount()
    oldListener?.(completion)
    expect(refresh).toHaveBeenCalledTimes(3)
  })

  it('restores package output on return and ignores late snapshots and other runtimes', async () => {
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([makeProgram()])
    const output = (revision: number, lines: AgentPackageOutput['lines']): AgentPackageOutput => ({
      agentId: 'claude-code',
      revision,
      lines,
    })
    const initial = output(1, [{ stream: 'info', line: 'Started package update' }])
    const whileClosed = output(2, [...initial.lines, { stream: 'stdout', line: 'Downloaded while closed' }])
    const finished = output(3, [...whileClosed.lines, { stream: 'stderr', line: 'Finished package output' }])
    let emit: ((output: AgentPackageOutput) => void) | undefined
    vi.spyOn(agentApi, 'onPackageOutput').mockImplementation(async (handler) => {
      emit = handler
      return () => { emit = undefined }
    })
    let finishReopenedLookup!: (output: AgentPackageOutput) => void
    const readOutput = vi.spyOn(agentApi, 'packageOutput')
      .mockResolvedValueOnce(initial)
      .mockResolvedValueOnce(whileClosed)
      .mockReturnValueOnce(new Promise((resolve) => { finishReopenedLookup = resolve }))
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')

    const firstPage = render(<AgentManagementPage />)
    expect(await screen.findByText('* Started package update')).toBeInTheDocument()
    firstPage.unmount()
    expect(emit).toBeUndefined()

    const reopenedPage = render(<AgentManagementPage />)
    expect(await screen.findByText('> Downloaded while closed')).toBeInTheDocument()
    expect(screen.getAllByText('* Started package update')).toHaveLength(1)
    reopenedPage.unmount()

    render(<AgentManagementPage />)
    await waitFor(() => expect(readOutput).toHaveBeenCalledTimes(3))
    await act(async () => {
      emit?.(finished)
      emit?.({ ...finished, agentId: 'codex', revision: 4, lines: [{ stream: 'info', line: 'Codex output' }] })
      finishReopenedLookup(whileClosed)
    })
    expect(screen.getByText('! Finished package output')).toBeInTheDocument()
    expect(screen.queryByText('* Codex output')).not.toBeInTheDocument()

    act(() => {
      useRuntimeEnvironmentStore.getState().selectEnvironment('remote-output-host')
      useSkillStoreV2.setState({ runtimeEnvironmentId: 'remote-output-host' })
    })
    expect(screen.queryByText('! Finished package output')).not.toBeInTheDocument()
    expect(readOutput).toHaveBeenCalledTimes(3)
  })

  it.each([
    { installedVersion: '2.0.0-beta.1', latestVersion: '1.9.9', status: 'installed' },
    { installedVersion: '1.2.3+local.1', latestVersion: '1.2.3+upstream.2', status: 'installed' },
    { installedVersion: '1.2.3', latestVersion: '1.2.4', status: 'updateAvailable', updateCommand: null },
    { installedVersion: null, latestVersion: '1.2.4', status: 'installed' },
  ] as const)('does not offer an unverified or unsupported update from $installedVersion to $latestVersion', async (versions) => {
    useSkillStoreV2.setState({ selectedAgentDetail: { ...agentDetail, version: null, latestVersion: null } })
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([makeProgram(versions)])
    const update = vi.spyOn(agentApi, 'update').mockResolvedValue(undefined)
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    await act(async () => { render(<AgentManagementPage />) })

    expect(screen.getByRole('button', { name: '无需更新' })).toBeDisabled()
    expect(screen.queryByRole('button', { name: '更新此 Agent' })).not.toBeInTheDocument()
    expect(update).not.toHaveBeenCalled()
  })

  it('refreshes the installed version after a failed update and preserves the command error', async () => {
    vi.spyOn(agentApi, 'refresh')
      .mockResolvedValueOnce([makeProgram({ status: 'updateAvailable', installedVersion: '1.0.0', latestVersion: '2.0.0' })])
      .mockResolvedValue([makeProgram({ installedVersion: '2.0.0', latestVersion: '2.0.0' })])
    vi.spyOn(agentApi, 'update').mockRejectedValue('Update script failed after changing files')
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue()
    vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue()
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(await screen.findByRole('button', { name: '更新此 Agent' }))

    expect(await screen.findByRole('alert')).toHaveTextContent('Update script failed after changing files')
    expect(screen.getByText('当前版本 2.0.0')).toBeInTheDocument()
    expect(screen.queryByText('Agent 更新完成')).not.toBeInTheDocument()
  })

  it('reports an unverified Agent installation when program detection fails', async () => {
    vi.spyOn(agentApi, 'refresh')
      .mockResolvedValueOnce([makeProgram({ status: 'notInstalled', binaryPath: null })])
      .mockRejectedValue('Detection failed')
    vi.spyOn(agentApi, 'install').mockResolvedValue(undefined)
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue()
    vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue()
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(await screen.findByRole('button', { name: '安装此 Agent' }))

    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('无法确认 Claude Code 的安装状态或版本'))
    expect(screen.queryByText('Agent 安装完成')).not.toBeInTheDocument()
  })

  it('does not publish an old Agent update error to the newly selected runtime', async () => {
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([makeProgram({
      status: 'updateAvailable', installedVersion: '1.0.0', latestVersion: '2.0.0',
    })])
    let failUpdate!: (error: Error) => void
    vi.spyOn(agentApi, 'update').mockReturnValue(new Promise((_, reject) => { failUpdate = reject }))

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    const { unmount } = render(<AgentManagementPage />)
    fireEvent.click(await screen.findByRole('button', { name: '更新此 Agent' }))
    unmount()
    act(() => {
      useRuntimeEnvironmentStore.getState().selectEnvironment('other-host')
      useSkillStoreV2.setState({ runtimeEnvironmentId: 'other-host', error: 'Remote host status' })
    })
    await act(async () => failUpdate(new Error('Old update failed')))

    expect(useSkillStoreV2.getState().error).toBe('Remote host status')
  })

  it('keeps a newer Agent version when the page-entry program lookup finishes late', async () => {
    let finishInitialLookup!: (programs: AgentProgramInfo[]) => void
    const refresh = vi.spyOn(agentApi, 'refresh')
      .mockReturnValueOnce(new Promise((resolve) => { finishInitialLookup = resolve }))
      .mockResolvedValueOnce([makeProgram({ installedVersion: '2.0.0', latestVersion: '2.0.0' })])
    vi.spyOn(useSkillStoreV2.getState(), 'refresh').mockResolvedValue()

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    await waitFor(() => expect(refresh).toHaveBeenCalledTimes(1))
    fireEvent.click(screen.getByRole('button', { name: '刷新总览' }))
    expect(await screen.findByText('当前版本 2.0.0')).toBeInTheDocument()
    await act(async () => finishInitialLookup([makeProgram({ installedVersion: '1.0.0' })]))

    expect(screen.getByText('当前版本 2.0.0')).toBeInTheDocument()
  })

  it.each(['detected', 'manual', 'inventory'] as const)('selects the appropriate Agent after startup lookup: %s', async (choice) => {
    const other = { ...makeSidebarAgent('antigravity', 'Antigravity'), installed: choice === 'inventory' }
    const claude = { ...makeSidebarAgent('claude-code', 'Claude Code'), installed: false }
    useSkillStoreV2.setState({
      agents: [other, claude],
      selectedAgentId: null,
      selectedAgentDetail: null,
    })
    let finishLookup!: (programs: AgentProgramInfo[]) => void
    let failLookup!: (error: Error) => void
    const refresh = vi.spyOn(agentApi, 'refresh').mockReturnValue(new Promise((resolve, reject) => {
      finishLookup = resolve
      failLookup = reject
    }))
    vi.spyOn(skillApiV2, 'getAgentDetail').mockImplementation(async (agentId) => ({
      ...agentDetail,
      id: agentId,
      displayName: agentId === other.id ? other.displayName : claude.displayName,
    }))

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    await waitFor(() => expect(refresh).toHaveBeenCalledTimes(1))
    expect(useSkillStoreV2.getState().selectedAgentId).toBeNull()

    if (choice === 'manual') {
      await act(async () => useSkillStoreV2.getState().selectAgent(other.id))
    }
    await act(async () => {
      if (choice === 'inventory') failLookup(new Error('Program detection unavailable'))
      else finishLookup([makeProgram()])
    })

    expect(useSkillStoreV2.getState().selectedAgentId).toBe(choice === 'detected' ? claude.id : other.id)
  })

  it('does not treat the shared .agents directory as an agent in management views', async () => {
    const sharedDetail: AgentDetail = {
      ...agentDetail,
      id: 'agents',
      displayName: '.agents',
      iconKey: 'agents',
      skillsDir: '/Users/me/.agents/skills',
      skills: [],
    }
    useSkillStoreV2.setState({
      activeTab: 'agents',
      selectedAgentId: 'agents',
      selectedAgentDetail: sharedDetail,
      agents: [
        { id: 'agents', displayName: '.agents', iconKey: 'agents', enabled: true, skillsDir: '/Users/me/.agents/skills', version: null, latestVersion: null, installed: true, managedSkillCount: 0, unmanagedSkillCount: 68 } as AgentSummary,
        { id: 'claude-code', displayName: 'Claude Code', iconKey: 'claude-code', enabled: true, skillsDir: '/c', version: null, latestVersion: null, installed: true, managedSkillCount: 1, unmanagedSkillCount: 0 } as AgentSummary,
      ],
    })
    vi.spyOn(skillApiV2, 'getAgentDetail').mockResolvedValue(agentDetail)

    const { SettingsSidebar } = await import('../components/settings/SettingsSidebar')
    render(
      <SettingsSidebar
        activeSection="skill-manager-v2"
        activeIslandView="overview"
        activeMonitorView="overview"
        collapsed={false}
        onCollapsedChange={() => {}}
        onSelect={() => {}}
        onIslandViewChange={() => {}}
        onMonitorViewChange={() => {}}
      />,
    )

    expect(screen.queryByText('.agents')).not.toBeInTheDocument()
    expect(screen.getByText('Claude Code')).toBeInTheDocument()
    expect(screen.getByText('已安装 Agent').parentElement).toHaveTextContent('1')
    const addAgentButton = screen.getByRole('button', { name: /添加 Claude Code 实例/ })
    expect(addAgentButton.parentElement?.lastElementChild).toBe(addAgentButton)
    fireEvent.click(addAgentButton)
    expect(useSkillStoreV2.getState().customAgentDialogRequest).toBe(1)

    cleanup()
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    await waitFor(() => expect(useSkillStoreV2.getState().selectedAgentId).toBe('claude-code'))
    expect(screen.queryByText('.agents')).not.toBeInTheDocument()
  })

  it('groups sidebar Agents by detected program status and updates the grouping after refresh', async () => {
    useSkillStoreV2.setState({
      activeTab: 'agents',
      agents: [
        { ...makeSidebarAgent('claude-code', 'Claude Code'), installed: false },
        makeSidebarAgent('codex', 'Codex'),
      ],
    })
    vi.spyOn(agentApi, 'refresh')
      .mockResolvedValueOnce([
        makeProgram({ installedVersion: '2.0.0' }),
        makeProgram({ id: 'codex', displayName: 'Codex', status: 'notInstalled' }),
      ])
      .mockResolvedValue([
        makeProgram({ status: 'notInstalled', binaryPath: null }),
        makeProgram({ id: 'codex', displayName: 'Codex', status: 'updateAvailable' }),
      ])
    vi.spyOn(useSkillStoreV2.getState(), 'refresh').mockResolvedValue()

    const { SettingsSidebar } = await import('../components/settings/SettingsSidebar')
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    const { container } = render(
      <>
        <SettingsSidebar
          activeSection="skill-manager-v2"
          activeIslandView="overview"
          activeMonitorView="overview"
          collapsed={false}
          onCollapsedChange={() => {}}
          onSelect={() => {}}
          onIslandViewChange={() => {}}
          onMonitorViewChange={() => {}}
        />
        <AgentManagementPage />
      </>,
    )

    expect(await screen.findByText('当前版本 2.0.0')).toBeInTheDocument()
    expect(container.querySelector('[data-agent-id="claude-code"]')).toBeInTheDocument()
    expect(container.querySelector('[data-agent-id="codex"]')).toBeNull()
    expect(useSkillStoreV2.getState().agents.find((agent) => agent.id === 'claude-code')?.installed).toBe(false)

    fireEvent.click(screen.getByRole('button', { name: '刷新总览' }))

    await waitFor(() => expect(container.querySelector('[data-agent-id="codex"]')).toBeInTheDocument())
    expect(container.querySelector('[data-agent-id="claude-code"]')).toBeNull()
    expect(screen.getByText('当前版本 未安装')).toBeInTheDocument()
  })

  it('ignores an Agent program snapshot after leaving and returning to the same runtime', async () => {
    const { selectionRevision } = useRuntimeEnvironmentStore.getState()
    useSkillStoreV2.setState({
      activeTab: 'agents',
      agents: [{ ...makeSidebarAgent('claude-code', 'Claude Code'), installed: false }],
      agentProgramSnapshot: {
        environmentId: LOCAL_RUNTIME_ENVIRONMENT_ID,
        selectionRevision,
        programs: { 'claude-code': makeProgram() },
      },
    })

    const { SettingsSidebar } = await import('../components/settings/SettingsSidebar')
    const { container } = render(
      <SettingsSidebar
        activeSection="skill-manager-v2"
        activeIslandView="overview"
        activeMonitorView="overview"
        collapsed={false}
        onCollapsedChange={() => {}}
        onSelect={() => {}}
        onIslandViewChange={() => {}}
        onMonitorViewChange={() => {}}
      />,
    )

    expect(container.querySelector('[data-agent-id="claude-code"]')).toBeInTheDocument()
    const { selectEnvironment } = useRuntimeEnvironmentStore.getState()
    act(() => {
      selectEnvironment('other-host')
      selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    })
    expect(container.querySelector('[data-agent-id="claude-code"]')).toBeNull()
  })

  it('sorts installed sidebar agents by active usage and supports drag reorder', async () => {
    window.localStorage.removeItem('agentbro.agentManagement.agentOrder.v1')
    useSkillStoreV2.setState({
      activeTab: 'agents',
      selectedAgentId: null,
      selectedAgentDetail: null,
      agents: [
        makeSidebarAgent('claude-code', 'Claude Code', { managed: 8 }),
        makeSidebarAgent('codex', 'Codex', { managed: 1 }),
        makeSidebarAgent('workbuddy', 'WorkBuddy', { managed: 0 }),
      ],
    })
    const workbuddySession = makeSidebarSession('workbuddy', { id: 'workbuddy-active', lastActivityAt: Date.now() })
    useSessionStore.setState({
      sessions: { [workbuddySession.id]: workbuddySession },
      sessionList: [workbuddySession],
      activeSessionId: workbuddySession.id,
    })

    const { SettingsSidebar } = await import('../components/settings/SettingsSidebar')
    const { container } = render(
      <SettingsSidebar
        activeSection="skill-manager-v2"
        activeIslandView="overview"
        activeMonitorView="overview"
        collapsed={false}
        onCollapsedChange={() => {}}
        onSelect={() => {}}
        onIslandViewChange={() => {}}
        onMonitorViewChange={() => {}}
      />,
    )

    const labels = () => Array.from(container.querySelectorAll('.sm2-sidebar__subitem-label')).map((item) => item.textContent)
    expect(labels().slice(0, 3)).toEqual(['WorkBuddy', 'Claude Code', 'Codex'])

    const workbuddyRow = screen.getByText('WorkBuddy').closest('.sm2-sidebar__subitem-row')
    const claudeRow = screen.getByText('Claude Code').closest('.sm2-sidebar__subitem-row')
    expect(workbuddyRow).not.toBeNull()
    expect(claudeRow).not.toBeNull()
    vi.spyOn(workbuddyRow!, 'getBoundingClientRect').mockReturnValue({
      top: 0, bottom: 32, left: 0, right: 220, width: 220, height: 32, x: 0, y: 0, toJSON: () => ({}),
    })
    vi.spyOn(claudeRow!, 'getBoundingClientRect').mockReturnValue({
      top: 32, bottom: 64, left: 0, right: 220, width: 220, height: 32, x: 0, y: 32, toJSON: () => ({}),
    })
    fireEvent.mouseDown(workbuddyRow!, { button: 0, clientX: 20, clientY: 16 })
    fireEvent.mouseMove(window, { clientX: 20, clientY: 58 })
    fireEvent.mouseUp(window, { clientX: 20, clientY: 58 })

    expect(labels().slice(0, 3)).toEqual(['Claude Code', 'WorkBuddy', 'Codex'])
    expect(JSON.parse(window.localStorage.getItem('agentbro.agentManagement.agentOrder.v1') || '[]')).toEqual(['claude-code', 'workbuddy', 'codex'])
    window.localStorage.removeItem('agentbro.agentManagement.agentOrder.v1')
    useSessionStore.setState({ sessions: {}, sessionList: [], activeSessionId: null })
  })

  it('searches installed and folded uninstalled Agents without changing selection', async () => {
    useSkillStoreV2.setState({
      activeTab: 'agents',
      selectedAgentId: 'claude-code',
      agents: [
        makeSidebarAgent('claude-code', 'Claude Code'),
        makeSidebarAgent('codex', 'Codex'),
        { ...makeSidebarAgent('qwenwork-cn', '千问办公'), installed: false },
      ],
    })
    const { SettingsSidebar } = await import('../components/settings/SettingsSidebar')
    const { container } = render(
      <SettingsSidebar
        activeSection="skill-manager-v2"
        activeIslandView="overview"
        activeMonitorView="overview"
        collapsed={false}
        onCollapsedChange={() => {}}
        onSelect={() => {}}
        onIslandViewChange={() => {}}
        onMonitorViewChange={() => {}}
      />,
    )

    const search = screen.getByRole('searchbox')
    expect(screen.queryByText('千问办公')).not.toBeInTheDocument()
    fireEvent.change(search, { target: { value: 'ClAuDe' } })
    expect(screen.getByText('Claude Code')).toBeInTheDocument()
    expect(screen.queryByText('Codex')).not.toBeInTheDocument()

    fireEvent.change(search, { target: { value: ' QWENWORK ' } })
    expect(screen.getByText('千问办公')).toBeInTheDocument()
    expect(screen.queryByText('Claude Code')).not.toBeInTheDocument()
    expect(useSkillStoreV2.getState().selectedAgentId).toBe('claude-code')

    fireEvent.change(search, { target: { value: 'does not match' } })
    expect(container.querySelector('.sm2-sidebar__subgroup-empty[role="status"]')).toBeInTheDocument()

    fireEvent.change(search, { target: { value: '' } })
    expect(screen.getByText('Claude Code')).toBeInTheDocument()
    expect(screen.getByText('Codex')).toBeInTheDocument()
    expect(screen.queryByText('千问办公')).not.toBeInTheDocument()
  })

  it('Agent skills default to cards and can switch to list view', async () => {
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    const { container } = render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    expect(screen.getByRole('button', { name: '已管理 1' })).toHaveClass('active')
    expect(screen.queryByText('未管理 Skills')).not.toBeInTheDocument()
    expect(screen.queryByLabelText('选择 release-checklist')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /批量删除/ })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '批量选择' })).toBeInTheDocument()
    expect(container.querySelector('.sm2__agent-skill-grid')).not.toBeNull()
    const card = container.querySelector('.sm2__agent-skill-card')
    expect(card).not.toBeNull()
    expect(card).toHaveClass('sm2__agent-skill-card--clickable')
    expect(card).toHaveAttribute('role', 'button')
    expect(card).toHaveAttribute('tabindex', '0')
    expect(card?.querySelector('.sm2__agent-skill-card-head')).not.toBeNull()
    expect(card?.querySelector('.sm2__agent-skill-icon')).not.toBeNull()
    expect(card?.querySelector('.sm2__agent-skill-card-titleline')).not.toBeNull()
    expect(card?.querySelector('.sm2__agent-skill-meta')).not.toBeNull()
    expect(card?.querySelector('code')).not.toBeNull()
    fireEvent.click(screen.getByText('列表'))
    const row = container.querySelector('.sm2__agent-skill-list .sm2__object-row')
    expect(row).not.toBeNull()
    expect(row).toHaveClass('sm2__object-row--path', 'sm2__object-row--clickable')
    expect(row).toHaveAttribute('role', 'button')
    expect(row).toHaveAttribute('tabindex', '0')
    expect(row).toHaveTextContent('软连接 · 正常 · 直接分发')
  })

  it.each([
    { id: 'codex', displayName: 'Codex', skillsDir: '/Users/me/.codex/skills' },
    { id: 'kimi', displayName: 'Kimi Code', skillsDir: '/Users/me/.kimi-code/skills' },
    { id: 'openclaw', displayName: 'OpenClaw', skillsDir: '/Users/me/.openclaw/workspace/skills' },
    { id: 'zcode', displayName: 'ZCode', skillsDir: '/Users/me/.zcode/skills' },
  ])('shows $displayName shared skills in layered source and status tabs', async ({ id, displayName, skillsDir }) => {
    const consumerDetail: AgentDetail = {
      ...agentDetail,
      id,
      displayName,
      iconKey: id,
      skillsDir,
      skills: [],
      inheritsSharedSkills: true,
      inheritedManagedSkills: [makeSharedTarget('shared-review')],
      inheritedUnmanagedSkills: [makeSharedUnmanaged('shared-writing')],
    }
    useSkillStoreV2.setState({
      agents: [
        { id, displayName, iconKey: id, enabled: true, skillsDir, version: '3.5.3', latestVersion: null, installed: true, managedSkillCount: 0, unmanagedSkillCount: 0 } as AgentSummary,
      ],
      selectedAgentId: id,
      selectedAgentDetail: consumerDetail,
      unmanaged: [
        {
          id: 'shared-source-item',
          agentId: 'agents',
          itemType: 'skill',
          path: '/Users/me/.agents/skills/shared-source-item',
          inferredSkillId: 'shared-source-item',
          hash: null,
          reason: 'shared_agents_directory',
        },
      ],
    })
    const previewAdopt = vi.spyOn(skillApiV2, 'previewAdopt').mockResolvedValue({
      agentId: 'agents',
      unmanagedId: 'raw-shared-writing',
      skillPath: '/Users/me/.agents/skills/shared-writing',
      inferredSkillId: 'shared-writing',
      hash: 'hash-shared-writing',
      centerHasSameId: false,
      canQuickAdopt: true,
      options: [
        { value: 'import_cleanup', label: 'Import and clean up', destructive: true },
        { value: 'skip', label: 'Keep as unmanaged', destructive: false },
      ],
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    const { container } = render(<AgentManagementPage />)

    expect(screen.getByRole('button', { name: 'Skills (2)' })).toBeInTheDocument()
    expect(screen.getByText('共享继承', { selector: '.sm2__stat span' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Skills (2)' }))

    expect(screen.getByRole('button', { name: 'Agent 专属 0' })).toHaveClass('active')
    expect(screen.getByRole('button', { name: 'Agent 专属 0' })).toHaveAttribute('aria-pressed', 'true')
    expect(screen.getByRole('button', { name: '已管理 0' })).toHaveClass('active')
    expect(screen.getByRole('button', { name: '已管理 0' })).toHaveAttribute('aria-pressed', 'true')
    expect(screen.getByRole('button', { name: '未管理 0' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /^全部(?:\s|$)/ })).not.toBeInTheDocument()
    expect(screen.queryByText('shared-source-item')).not.toBeInTheDocument()

    const inheritedTab = screen.getByRole('button', { name: '共享继承 2' })
    fireEvent.click(inheritedTab)

    expect(inheritedTab).toHaveClass('active')
    expect(screen.getByRole('button', { name: '已管理 1' })).toHaveClass('active')
    expect(screen.getByRole('button', { name: '未管理 1' })).toBeInTheDocument()
    expect(screen.getByText(new RegExp(`${displayName} 默认读取 ~\\/.agents\\/skills`))).toBeInTheDocument()
    expect(screen.getByText('/Users/me/.agents/skills/shared-review')).toBeInTheDocument()
    expect(screen.queryByText('/Users/me/.agents/skills/shared-writing')).not.toBeInTheDocument()
    const inheritedCard = screen.getByText('shared-review').closest('article')
    expect(inheritedCard).not.toBeNull()
    expect(inheritedCard).toHaveClass('sm2__agent-skill-card--clickable')
    expect(inheritedCard).not.toHaveClass('sm2__agent-skill-card--adopting')
    expect(inheritedCard).toHaveAttribute('role', 'button')
    expect(inheritedCard).toHaveAttribute('tabindex', '0')
    expect(inheritedCard).not.toHaveAttribute('aria-busy')
    expect(inheritedCard?.querySelector('.sm2__agent-skill-card-head')).not.toBeNull()
    expect(inheritedCard?.querySelector('.sm2__agent-skill-icon')).not.toBeNull()
    expect(inheritedCard?.querySelector('.sm2__agent-skill-card-titleline')).not.toBeNull()
    expect(inheritedCard?.querySelector('code')).toHaveTextContent('/Users/me/.agents/skills/shared-review')
    expect(within(inheritedCard!).getByText('软连接', { selector: '.sm2__source-pill' })).toBeInTheDocument()
    expect(within(inheritedCard!).getByText('正常')).toBeInTheDocument()
    expect(within(inheritedCard!).getByRole('button', { name: '删除 shared-review' })).toBeInTheDocument()
    expect(within(inheritedCard!).queryByRole('checkbox')).not.toBeInTheDocument()
    expect(within(inheritedCard!).queryByRole('button', { name: /归入技能包/ })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '批量选择' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '新增SKILL' })).not.toBeInTheDocument()

    fireEvent.click(within(inheritedCard!).getByRole('button', { name: '删除 shared-review' }))
    expect(document.body.querySelector('.sm2__slideover--skill-detail')).toBeNull()
    const sharedDeleteDialog = screen.getByRole('heading', { name: '移除共享的已管理 Skill？' }).closest<HTMLElement>('[role="dialog"]')
    expect(sharedDeleteDialog).not.toBeNull()
    expect(within(sharedDeleteDialog!).getByText(/所有读取该共享目录的 Agent/)).toBeInTheDocument()
    expect(within(sharedDeleteDialog!).getByText(/中心库中的 Skill 会保留/)).toBeInTheDocument()
    fireEvent.click(within(sharedDeleteDialog!).getByRole('button', { name: '取消' }))

    fireEvent.click(screen.getByText('列表'))
    const managedInheritedRow = screen.getByText('/Users/me/.agents/skills/shared-review').closest<HTMLElement>('.sm2__object-row')
    expect(managedInheritedRow).not.toBeNull()
    expect(managedInheritedRow).toHaveClass('sm2__object-row--path', 'sm2__object-row--clickable')
    expect(managedInheritedRow).toHaveAttribute('role', 'button')
    expect(managedInheritedRow).toHaveAttribute('tabindex', '0')
    expect(managedInheritedRow).toHaveTextContent('软连接 · 正常 · 直接分发')
    expect(within(managedInheritedRow!).getByRole('button', { name: '删除 shared-review' })).toBeInTheDocument()
    fireEvent.click(screen.getByText('卡片'))

    fireEvent.change(screen.getByPlaceholderText('搜索 Skill 名称 / 路径 / 来源 / 原因'), { target: { value: 'missing-skill' } })
    expect(screen.getByRole('button', { name: '共享继承 2' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '已管理 1' })).toBeInTheDocument()
    expect(screen.getByText('没有匹配的已管理共享 Skill。')).toBeInTheDocument()
    fireEvent.change(screen.getByPlaceholderText('搜索 Skill 名称 / 路径 / 来源 / 原因'), { target: { value: '' } })

    fireEvent.click(screen.getByRole('button', { name: '未管理 1' }))
    expect(screen.getByText('/Users/me/.agents/skills/shared-writing')).toBeInTheDocument()
    expect(screen.queryByText('/Users/me/.agents/skills/shared-review')).not.toBeInTheDocument()
    const unmanagedCard = screen.getByText('shared-writing').closest('article')
    expect(unmanagedCard).not.toBeNull()
    expect(unmanagedCard).toHaveClass('sm2__agent-skill-card--clickable', 'sm2__agent-skill-card--unmanaged')
    expect(unmanagedCard).toHaveAttribute('role', 'button')
    expect(unmanagedCard).toHaveAttribute('tabindex', '0')
    expect(unmanagedCard?.querySelector('.sm2__agent-skill-card-head')).not.toBeNull()
    expect(unmanagedCard?.querySelector('.sm2__agent-skill-icon')).not.toBeNull()
    expect(unmanagedCard?.querySelector('.sm2__agent-skill-card-titleline')).not.toBeNull()
    expect(unmanagedCard?.querySelector('code')).toHaveTextContent('/Users/me/.agents/skills/shared-writing')
    expect(within(unmanagedCard!).getByText('.agents 共享目录', { selector: '.sm2__source-pill' })).toBeInTheDocument()
    expect(within(unmanagedCard!).getByText('未管理')).toBeInTheDocument()
    expect(within(unmanagedCard!).getByRole('button', { name: '接管' })).toBeInTheDocument()
    expect(within(unmanagedCard!).getByRole('button', { name: '删除 shared-writing' })).toBeInTheDocument()
    expect(within(unmanagedCard!).queryByRole('checkbox')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '批量管理' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /一键接管/ })).not.toBeInTheDocument()

    fireEvent.click(within(unmanagedCard!).getByRole('button', { name: '接管' }))
    expect(document.body.querySelector('.sm2__slideover--skill-detail')).toBeNull()
    await waitFor(() => {
      expect(previewAdopt).toHaveBeenCalledTimes(1)
      expect(previewAdopt).toHaveBeenCalledWith('agents', 'raw-shared-writing')
    })
    const adoptHeading = await screen.findByRole('heading', { name: '接管 shared-writing' })
    const adoptDialog = adoptHeading.closest<HTMLElement>('[role="dialog"]')
    expect(adoptDialog).not.toBeNull()
    fireEvent.click(within(adoptDialog!).getByRole('button', { name: '取消' }))

    fireEvent.click(screen.getByText('列表'))
    expect(container.querySelector('.sm2__agent-skill-list')).not.toBeNull()
    const inheritedRow = screen.getByText('/Users/me/.agents/skills/shared-writing').closest<HTMLElement>('.sm2__object-row')
    expect(inheritedRow).not.toBeNull()
    expect(inheritedRow).toHaveClass('sm2__object-row--path', 'sm2__object-row--clickable')
    expect(inheritedRow).toHaveAttribute('role', 'button')
    expect(inheritedRow).toHaveAttribute('tabindex', '0')
    expect(inheritedRow).toHaveTextContent('未在中心库 · .agents 共享目录')
    expect(within(inheritedRow!).getByRole('button', { name: '接管' })).toBeInTheDocument()
    expect(within(inheritedRow!).getByRole('button', { name: '删除 shared-writing' })).toBeInTheDocument()
  })

  it('renders shared managed and unmanaged actions from the English action translations', async () => {
    await i18n.changeLanguage('en')
    useSkillStoreV2.setState({
      agents: [makeSidebarAgent('codex', 'Codex')],
      selectedAgentId: 'codex',
      selectedAgentDetail: {
        ...agentDetail,
        id: 'codex',
        displayName: 'Codex',
        iconKey: 'codex',
        skillsDir: '/Users/me/.codex/skills',
        inheritsSharedSkills: true,
        inheritedManagedSkills: [makeSharedTarget('shared-review')],
        inheritedUnmanagedSkills: [makeSharedUnmanaged('shared-writing')],
      },
      unmanaged: [],
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(screen.getByRole('button', { name: 'Skills (3)' }))
    fireEvent.click(screen.getByRole('button', { name: 'Inherited 2' }))

    expect(screen.getByRole('button', { name: 'Delete shared-review' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Multi-select' }))
    expect(screen.getByText('0 selected')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Select current' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Delete 0 Skills' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Cancel selection' }))

    fireEvent.click(screen.getByRole('button', { name: 'Unmanaged 1' }))
    expect(screen.getByRole('button', { name: 'Adopt' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Delete shared-writing' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Batch manage' }))
    expect(screen.getByRole('button', { name: 'Select current adoptable' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Adopt into center library' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Cancel selection' })).toBeInTheDocument()
  })

  it('deduplicates the top-level Skills count by logical Skill ID across Agent and shared sources', async () => {
    useSkillStoreV2.setState({
      agents: [makeSidebarAgent('codex', 'Codex')],
      selectedAgentId: 'codex',
      selectedAgentDetail: {
        ...agentDetail,
        id: 'codex',
        displayName: 'Codex',
        iconKey: 'codex',
        skillsDir: '/Users/me/.codex/skills',
        inheritsSharedSkills: true,
        inheritedManagedSkills: [makeSharedTarget('release-checklist')],
        inheritedUnmanagedSkills: [],
      },
      unmanaged: [],
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(screen.getByRole('button', { name: 'Skills (1)' }))
    expect(screen.getByRole('button', { name: 'Agent 专属 1' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '共享继承 1' })).toBeInTheDocument()
  })

  it('opens shared managed Skill details through the standard managed card', async () => {
    const consumerDetail: AgentDetail = {
      ...agentDetail,
      id: 'codex',
      displayName: 'Codex',
      iconKey: 'codex',
      skillsDir: '/Users/me/.codex/skills',
      skills: [],
      inheritsSharedSkills: true,
      inheritedManagedSkills: [makeSharedTarget('shared-review')],
      inheritedUnmanagedSkills: [],
    }
    useSkillStoreV2.setState({
      agents: [makeSidebarAgent('codex', 'Codex')],
      selectedAgentId: 'codex',
      selectedAgentDetail: consumerDetail,
      unmanaged: [],
    })
    const getSkillDetail = vi.spyOn(skillApiV2, 'getSkillDetail').mockResolvedValue({
      ...makeSkill({
        id: 'shared-review',
        name: 'shared-review',
        centerPath: '/Users/me/.agentbro/skills/shared-review',
        installedAgents: [
          { agentId: 'agents', displayName: '.agents', iconKey: 'agents', mode: 'link', status: 'ok' },
        ],
      }),
      frontmatter: {},
      files: null,
      targets: [
        makeTarget({
          id: 'target-shared-review',
          skillId: 'shared-review',
          agentId: 'agents',
          targetPath: '/Users/me/.agents/skills/shared-review',
        }),
      ],
      source: null,
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByRole('button', { name: 'Skills (1)' }))
    fireEvent.click(screen.getByRole('button', { name: '共享继承 1' }))
    const inheritedCard = screen.getByText('shared-review', { selector: '.sm2__agent-skill-card-titleline strong' }).closest('article')
    expect(inheritedCard).not.toBeNull()
    fireEvent.click(inheritedCard!)

    const slider = await waitFor(() => {
      const node = document.body.querySelector('.sm2__slideover--skill-detail')
      expect(node).not.toBeNull()
      return node as HTMLElement
    })
    expect(await within(slider).findByRole('button', { name: '概览' })).toBeInTheDocument()
    expect(within(slider).getByRole('button', { name: '文件' })).toBeInTheDocument()
    expect(within(slider).getByRole('button', { name: '来源' })).toBeInTheDocument()
    expect(slider.querySelector('.sm2__detail-pills .sm2__tag--ok')).toHaveTextContent('正常')
    expect(getSkillDetail).toHaveBeenCalledWith('shared-review')
  })

  it('keeps shared adoption open when its inventory refresh fails without adopting twice', async () => {
    const consumerDetail: AgentDetail = {
      ...agentDetail,
      id: 'codex',
      displayName: 'Codex',
      iconKey: 'codex',
      skillsDir: '/Users/me/.codex/skills',
      skills: [],
      inheritsSharedSkills: true,
      inheritedManagedSkills: [],
      inheritedUnmanagedSkills: [makeSharedUnmanaged('shared-retry')],
    }
    const agents = [makeSidebarAgent('codex', 'Codex')]
    const overview = { ...makeOverview(), agents }
    useSkillStoreV2.setState({
      overview,
      lastOverviewLoadedAt: Date.now(),
      agents,
      selectedAgentId: 'codex',
      selectedAgentDetail: consumerDetail,
      unmanaged: [],
    })
    vi.spyOn(skillApiV2, 'previewAdopt').mockResolvedValue({
      agentId: 'agents',
      unmanagedId: 'raw-shared-retry',
      skillPath: '/Users/me/.agents/skills/shared-retry',
      inferredSkillId: 'shared-retry',
      hash: 'hash-shared-retry',
      centerHasSameId: false,
      canQuickAdopt: true,
      options: [{ value: 'import_cleanup', label: 'Import and clean up', destructive: true }],
    })
    const executeAdopt = vi.spyOn(skillApiV2, 'executeAdopt').mockResolvedValue('shared-retry')
    const agentSnapshot = { ...consumerDetail, inheritedUnmanagedSkills: [] }
    const refresh = vi.spyOn(skillApiV2, 'refreshAgentSkillView')
      .mockRejectedValueOnce(new Error('Shared inventory refresh failed'))
      .mockResolvedValue({ agentDetail: agentSnapshot, overview, unmanaged: [] })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByRole('button', { name: 'Skills (1)' }))
    fireEvent.click(screen.getByRole('button', { name: '共享继承 1' }))
    fireEvent.click(screen.getByRole('button', { name: '未管理 1' }))
    fireEvent.click(screen.getByRole('button', { name: '接管' }))
    fireEvent.click(await screen.findByRole('button', { name: '确认接管' }))

    expect(await screen.findByText(/Shared inventory refresh failed/)).toBeInTheDocument()
    expect(useSkillStoreV2.getState().selectedAgentDetail).toBe(consumerDetail)
    expect(screen.getByRole('heading', { name: '接管 shared-retry' })).toBeInTheDocument()
    const retry = screen.getByRole('button', { name: '重试刷新' })
    expect(retry).toBeEnabled()
    fireEvent.click(retry)
    await waitFor(() => expect(screen.queryByRole('heading', { name: '接管 shared-retry' })).not.toBeInTheDocument())

    expect(executeAdopt).toHaveBeenCalledExactlyOnceWith('agents', 'raw-shared-retry', 'import_cleanup', null)
    expect(refresh.mock.calls).toEqual([['codex', LOCAL_RUNTIME_ENVIRONMENT_ID], ['codex', LOCAL_RUNTIME_ENVIRONMENT_ID]])
    expect(useSkillStoreV2.getState().selectedAgentDetail).toBe(agentSnapshot)
  })

  it.each(['runtime', 'runtime and back', 'agent', 'agent and back'])(
    'ignores an adoption preview returned after changing the %s selection', async (selection) => {
      const consumerDetail: AgentDetail = {
        ...agentDetail,
        id: 'codex',
        displayName: 'Codex',
        iconKey: 'codex',
        skillsDir: '/Users/me/.codex/skills',
        skills: [],
        inheritsSharedSkills: true,
        inheritedManagedSkills: [],
        inheritedUnmanagedSkills: [makeSharedUnmanaged('shared-runtime')],
      }
      const otherAgent = makeSidebarAgent('claude-code', 'Claude Code')
      vi.spyOn(skillApiV2, 'getAgentDetail').mockImplementation(async (agentId) => (
        agentId === consumerDetail.id ? consumerDetail : agentDetail
      ))
      useSkillStoreV2.setState({
        runtimeEnvironmentId: LOCAL_RUNTIME_ENVIRONMENT_ID,
        overview: makeOverview(),
        lastOverviewLoadedAt: Date.now(),
        agents: [makeSidebarAgent('codex', 'Codex'), otherAgent],
        selectedAgentId: 'codex',
        selectedAgentDetail: consumerDetail,
        unmanaged: [],
      })
      const preview: AdoptPreview = {
        agentId: 'agents',
        unmanagedId: 'raw-shared-runtime',
        skillPath: '/Users/me/.agents/skills/shared-runtime',
        inferredSkillId: 'shared-runtime',
        hash: 'hash-shared-runtime',
        centerHasSameId: false,
        canQuickAdopt: true,
        options: [
          { value: 'import_cleanup', label: 'Import and clean up', destructive: true },
        ],
      }
      let resolvePreview: (preview: AdoptPreview) => void = () => {}
      const previewPromise = new Promise<AdoptPreview>((resolve) => {
        resolvePreview = resolve
      })
      const previewAdopt = vi.spyOn(skillApiV2, 'previewAdopt').mockReturnValue(previewPromise)

      const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
      render(<AgentManagementPage />)
      fireEvent.click(screen.getByRole('button', { name: 'Skills (1)' }))
      fireEvent.click(screen.getByRole('button', { name: '共享继承 1' }))
      fireEvent.click(screen.getByRole('button', { name: '未管理 1' }))
      fireEvent.click(screen.getByRole('button', { name: '接管' }))
      await waitFor(() => expect(previewAdopt).toHaveBeenCalledTimes(1))

      await act(async () => {
        const runtime = useRuntimeEnvironmentStore.getState()
        const store = useSkillStoreV2.getState()
        if (selection.startsWith('runtime')) {
          runtime.selectEnvironment('remote-runtime')
          if (selection.endsWith('and back')) runtime.selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
        } else {
          await store.selectAgent(otherAgent.id)
          if (selection.endsWith('and back')) await store.selectAgent(consumerDetail.id)
        }
        useSkillStoreV2.getState().setError('Current selection status')
        resolvePreview(preview)
      })

      expect(screen.queryByRole('heading', { name: '接管 shared-runtime' })).not.toBeInTheDocument()
      expect(useSkillStoreV2.getState().error).toBe('Current selection status')
    },
  )

  it('drops the adoption review and stops follow-ups when the runtime changes during adoption', async () => {
    const consumerDetail: AgentDetail = {
      ...agentDetail,
      id: 'codex',
      displayName: 'Codex',
      iconKey: 'codex',
      skillsDir: '/Users/me/.codex/skills',
      skills: [],
      inheritsSharedSkills: true,
      inheritedManagedSkills: [],
      inheritedUnmanagedSkills: [makeSharedUnmanaged('shared-runtime')],
    }
    const localOverview = makeOverview()
    useSkillStoreV2.setState({
      runtimeEnvironmentId: LOCAL_RUNTIME_ENVIRONMENT_ID,
      overview: localOverview,
      lastOverviewLoadedAt: Date.now(),
      agents: [makeSidebarAgent('codex', 'Codex')],
      packs: [{
        id: 'shared-tools',
        name: 'Shared Tools',
        description: '',
        tags: [],
        memberCount: 0,
        appliedAgentCount: 0,
        healthy: true,
      }],
      selectedAgentId: 'codex',
      selectedAgentDetail: consumerDetail,
      unmanaged: [],
    })
    vi.spyOn(skillApiV2, 'previewAdopt').mockResolvedValue({
      agentId: 'agents',
      unmanagedId: 'raw-shared-runtime',
      skillPath: '/Users/me/.agents/skills/shared-runtime',
      inferredSkillId: 'shared-runtime',
      hash: 'hash-shared-runtime',
      centerHasSameId: false,
      canQuickAdopt: true,
      options: [
        { value: 'import_cleanup', label: 'Import and clean up', destructive: true },
      ],
    })
    let resolveExecute: (skillId: string) => void = () => {}
    const executePromise = new Promise<string>((resolve) => {
      resolveExecute = resolve
    })
    const executeAdopt = vi.spyOn(skillApiV2, 'executeAdopt').mockReturnValue(executePromise)
    const getPackDetail = vi.spyOn(skillApiV2, 'getPackDetail')
    const upsertPack = vi.spyOn(skillApiV2, 'upsertPack')
    const listUnmanaged = vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    const getAgentDetail = vi.spyOn(skillApiV2, 'getAgentDetail').mockResolvedValue({
      ...consumerDetail,
      inheritedUnmanagedSkills: [],
    })
    const overview = vi.spyOn(skillApiV2, 'overview').mockResolvedValue(makeOverview())

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByRole('button', { name: 'Skills (1)' }))
    fireEvent.click(screen.getByRole('button', { name: '共享继承 1' }))
    fireEvent.click(screen.getByRole('button', { name: '未管理 1' }))
    fireEvent.click(screen.getByRole('button', { name: '接管' }))
    fireEvent.click(await screen.findByRole('checkbox', { name: /同时加入技能包/ }))
    fireEvent.click(await screen.findByRole('button', { name: '确认接管' }))
    await waitFor(() => expect(executeAdopt).toHaveBeenCalledTimes(1))
    const listCallsBeforeRuntimeChange = listUnmanaged.mock.calls.length
    const detailCallsBeforeRuntimeChange = getAgentDetail.mock.calls.length
    const overviewCallsBeforeRuntimeChange = overview.mock.calls.length

    await act(async () => {
      useRuntimeEnvironmentStore.getState().selectEnvironment('remote-runtime')
      resolveExecute('shared-runtime')
    })

    expect(screen.queryByRole('heading', { name: '接管 shared-runtime' })).not.toBeInTheDocument()
    expect(getPackDetail).not.toHaveBeenCalled()
    expect(upsertPack).not.toHaveBeenCalled()
    expect(listUnmanaged).toHaveBeenCalledTimes(listCallsBeforeRuntimeChange)
    expect(getAgentDetail).toHaveBeenCalledTimes(detailCallsBeforeRuntimeChange)
    expect(overview).toHaveBeenCalledTimes(overviewCallsBeforeRuntimeChange)
    expect(useSkillStoreV2.getState().overview).toBe(localOverview)
  })

  it.each(['runtime', 'runtime and back', 'agent', 'agent and back'])(
    'ignores an adoption refresh returned after changing the %s selection', async (selection) => {
      const consumerDetail: AgentDetail = {
        ...agentDetail,
        id: 'codex',
        displayName: 'Codex',
        iconKey: 'codex',
        skills: [],
        inheritsSharedSkills: true,
        inheritedManagedSkills: [],
        inheritedUnmanagedSkills: [makeSharedUnmanaged('shared-refresh')],
      }
      const agents = [makeSidebarAgent('codex', 'Codex'), makeSidebarAgent('claude-code', 'Claude Code')]
      const overview = { ...makeOverview(), agents }
      useSkillStoreV2.setState({
        overview,
        lastOverviewLoadedAt: Date.now(),
        agents,
        selectedAgentId: 'codex',
        selectedAgentDetail: consumerDetail,
        unmanaged: [],
      })
      vi.spyOn(skillApiV2, 'previewAdopt').mockResolvedValue({
        agentId: 'agents',
        unmanagedId: 'raw-shared-refresh',
        skillPath: '/Users/me/.agents/skills/shared-refresh',
        inferredSkillId: 'shared-refresh',
        hash: 'hash-shared-refresh',
        centerHasSameId: false,
        canQuickAdopt: true,
        options: [{ value: 'import_cleanup', label: 'Import and clean up', destructive: true }],
      })
      const executeAdopt = vi.spyOn(skillApiV2, 'executeAdopt').mockResolvedValue('shared-refresh')
      vi.spyOn(skillApiV2, 'getAgentDetail').mockImplementation(async (id) => (
        id === consumerDetail.id ? consumerDetail : agentDetail
      ))
      let finishRefresh!: (snapshot: AgentSkillViewSnapshot) => void
      const refresh = vi.spyOn(skillApiV2, 'refreshAgentSkillView').mockReturnValue(
        new Promise((resolve) => { finishRefresh = resolve }),
      )

      const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
      render(<AgentManagementPage />)
      fireEvent.click(screen.getByRole('button', { name: 'Skills (1)' }))
      fireEvent.click(screen.getByRole('button', { name: '共享继承 1' }))
      fireEvent.click(screen.getByRole('button', { name: '未管理 1' }))
      fireEvent.click(screen.getByRole('button', { name: '接管' }))
      fireEvent.click(await screen.findByRole('button', { name: '确认接管' }))
      await waitFor(() => expect(refresh).toHaveBeenCalledExactlyOnceWith('codex', LOCAL_RUNTIME_ENVIRONMENT_ID))
      expect(executeAdopt).toHaveBeenCalledExactlyOnceWith('agents', 'raw-shared-refresh', 'import_cleanup', null)

      const currentOverview = { ...overview, metrics: { ...overview.metrics, centerSkillCount: 9 } }
      const currentUnmanaged = [makeSharedUnmanaged('current-skill')]
      let currentDetail!: AgentDetail
      await act(async () => {
        const runtime = useRuntimeEnvironmentStore.getState()
        const store = useSkillStoreV2.getState()
        if (selection.startsWith('runtime')) {
          runtime.selectEnvironment('remote-runtime')
          if (selection.endsWith('and back')) runtime.selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
        } else {
          await store.selectAgent('claude-code')
          if (selection.endsWith('and back')) await store.selectAgent('codex')
        }
        const currentAgentId = selection === 'agent' ? 'claude-code' : 'codex'
        expect(useSkillStoreV2.getState().selectedAgentId).toBe(currentAgentId)
        currentDetail = {
          ...(currentAgentId === 'codex' ? consumerDetail : agentDetail),
          skillsDir: '/current/skills',
        }
        useSkillStoreV2.setState({
          runtimeEnvironmentId: useRuntimeEnvironmentStore.getState().selectedEnvironmentId,
          overview: currentOverview,
          unmanaged: currentUnmanaged,
          selectedAgentDetail: currentDetail,
          error: 'Current selection status',
        })
        finishRefresh({
          agentDetail: { ...consumerDetail, inheritedUnmanagedSkills: [] },
          overview,
          unmanaged: [],
        })
      })

      expect(screen.queryByRole('heading', { name: '接管 shared-refresh' })).not.toBeInTheDocument()
      expect(useSkillStoreV2.getState().selectedAgentDetail).toBe(currentDetail)
      expect(useSkillStoreV2.getState().overview).toBe(currentOverview)
      expect(useSkillStoreV2.getState().unmanaged).toBe(currentUnmanaged)
      expect(useSkillStoreV2.getState().error).toBe('Current selection status')
    },
  )

  it('keeps the inherited scope visible for a consumer with an empty shared directory', async () => {
    useSkillStoreV2.setState({
      agents: [
        {
          id: 'codex',
          displayName: 'Codex',
          iconKey: 'codex',
          enabled: true,
          skillsDir: '/Users/me/.codex/skills',
          version: null,
          latestVersion: null,
          installed: true,
          managedSkillCount: 1,
          unmanagedSkillCount: 0,
        } as AgentSummary,
      ],
      selectedAgentId: 'codex',
      selectedAgentDetail: {
        ...agentDetail,
        id: 'codex',
        displayName: 'Codex',
        iconKey: 'codex',
        skillsDir: '/Users/me/.codex/skills',
        inheritsSharedSkills: true,
        inheritedManagedSkills: [],
        inheritedUnmanagedSkills: [],
      },
      unmanaged: [],
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(screen.getByRole('button', { name: 'Skills (1)' }))
    expect(screen.getByRole('button', { name: 'Agent 专属 1' })).toHaveClass('active')
    fireEvent.click(screen.getByRole('button', { name: '共享继承 0' }))
    expect(screen.getByText(/Codex 默认读取 ~\/.agents\/skills/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '已管理 0' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '未管理 0' })).toBeInTheDocument()
    expect(screen.getByText('没有匹配的已管理共享 Skill。')).toBeInTheDocument()
  })

  it('keeps shared status paging within the selected source', async () => {
    const inheritedUnmanagedSkills = Array.from(
      { length: 29 },
      (_, index) => makeSharedUnmanaged(`shared-${index}`),
    )
    useSkillStoreV2.setState({
      agents: [makeSidebarAgent('codex', 'Codex')],
      selectedAgentId: 'codex',
      selectedAgentDetail: {
        ...agentDetail,
        id: 'codex',
        displayName: 'Codex',
        iconKey: 'codex',
        skillsDir: '/Users/me/.codex/skills',
        skills: [],
        inheritsSharedSkills: true,
        inheritedManagedSkills: [],
        inheritedUnmanagedSkills,
      },
      unmanaged: [],
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByRole('button', { name: 'Skills (29)' }))
    fireEvent.click(screen.getByRole('button', { name: '共享继承 29' }))
    fireEvent.click(screen.getByRole('button', { name: '未管理 29' }))

    expect(screen.getByText('shared-27')).toBeInTheDocument()
    expect(screen.queryByText('shared-28')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '继续显示 1 个' }))
    expect(screen.getByText('shared-28')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '未管理 29' })).toBeInTheDocument()
  })

  it('resets managed paging when the pack filter changes', async () => {
    const inheritedManagedSkills = Array.from({ length: 29 }, (_, index) =>
      makeSharedTarget(`shared-pack-${index}`, {
        claims: [{
          id: `claim-pack-${index}`,
          claimType: 'pack',
          packId: 'shared-pack',
          packName: 'Shared Pack',
          createdAt: '2026-01-01T00:00:00Z',
        }],
      }),
    )
    useSkillStoreV2.setState({
      agents: [makeSidebarAgent('codex', 'Codex')],
      selectedAgentId: 'codex',
      selectedAgentDetail: {
        ...agentDetail,
        id: 'codex',
        displayName: 'Codex',
        iconKey: 'codex',
        skillsDir: '/Users/me/.codex/skills',
        skills: [],
        inheritsSharedSkills: true,
        inheritedManagedSkills,
        inheritedUnmanagedSkills: [],
      },
      unmanaged: [],
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByRole('button', { name: 'Skills (29)' }))
    fireEvent.click(screen.getByRole('button', { name: '共享继承 29' }))

    expect(screen.getByText('shared-pack-27')).toBeInTheDocument()
    expect(screen.queryByText('shared-pack-28')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '继续显示 1 个' }))
    expect(screen.getByText('shared-pack-28')).toBeInTheDocument()

    fireEvent.change(screen.getByLabelText('技能包归属'), { target: { value: 'pack' } })

    expect(screen.queryByText('shared-pack-28')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '继续显示 1 个' })).toBeInTheDocument()
  })

  it('does not expose shared inventory as inherited or unmanaged for a non-consumer', async () => {
    useSkillStoreV2.setState({
      selectedAgentDetail: {
        ...agentDetail,
        inheritsSharedSkills: false,
        inheritedManagedSkills: [],
        inheritedUnmanagedSkills: [],
      },
      unmanaged: [
        {
          id: 'shared-source-item',
          agentId: 'agents',
          itemType: 'skill',
          path: '/Users/me/.agents/skills/shared-source-item',
          inferredSkillId: 'shared-source-item',
          hash: null,
          reason: 'shared_agents_directory',
        },
      ],
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(screen.getByRole('button', { name: 'Skills (1)' }))
    expect(screen.queryByRole('button', { name: /共享继承/ })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /Agent 专属/ })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '已管理 1' })).toHaveClass('active')
    expect(screen.getByRole('button', { name: '未管理 0' })).toBeInTheDocument()
    expect(screen.queryByText('shared-source-item')).not.toBeInTheDocument()
  })

  it('resets shared source and unmanaged status when changing Agents', async () => {
    const codexDetail: AgentDetail = {
      ...agentDetail,
      id: 'codex',
      displayName: 'Codex',
      iconKey: 'codex',
      skillsDir: '/Users/me/.codex/skills',
      skills: [],
      inheritsSharedSkills: true,
      inheritedManagedSkills: [],
      inheritedUnmanagedSkills: [makeSharedUnmanaged('shared-reset')],
    }
    useSkillStoreV2.setState({
      agents: [
        makeSidebarAgent('codex', 'Codex'),
        makeSidebarAgent('claude-code', 'Claude Code', { managed: 1, unmanaged: 1 }),
      ],
      selectedAgentId: 'codex',
      selectedAgentDetail: codexDetail,
      unmanaged: [],
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByRole('button', { name: 'Skills (1)' }))
    fireEvent.click(screen.getByRole('button', { name: '共享继承 1' }))
    fireEvent.click(screen.getByRole('button', { name: '未管理 1' }))
    expect(screen.getByText('shared-reset')).toBeInTheDocument()

    act(() => {
      useSkillStoreV2.setState({
        selectedAgentId: 'claude-code',
        selectedAgentDetail: {
          ...agentDetail,
          inheritsSharedSkills: false,
          inheritedManagedSkills: [],
          inheritedUnmanagedSkills: [],
        },
        unmanaged: [
          {
            id: 'claude-unmanaged-reset',
            agentId: 'claude-code',
            itemType: 'skill',
            path: '/c/skills/unmanaged-reset',
            inferredSkillId: 'unmanaged-reset',
            hash: null,
            reason: 'not_in_center_library',
          },
        ],
      })
    })

    await waitFor(() => expect(screen.getByRole('button', { name: '概览' })).toHaveClass('sm2__subtab--active'))
    fireEvent.click(screen.getByRole('button', { name: 'Skills (2)' }))
    expect(screen.queryByRole('button', { name: /共享继承/ })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /Agent 专属/ })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '已管理 1' })).toHaveClass('active')
    expect(screen.getByText('release-checklist')).toBeInTheDocument()
    expect(screen.queryByText('unmanaged-reset')).not.toBeInTheDocument()
  })

  it('filters managed Agent skills by skill pack membership', async () => {
    useSkillStoreV2.setState({
      selectedAgentDetail: {
        ...agentDetail,
        skills: [
          makeTarget({
            id: 'target-direct',
            skillId: 'direct-tool',
            targetPath: '/c/skills/direct-tool',
            claims: [{ id: 'claim-direct', claimType: 'direct', packId: null, packName: null, createdAt: '2026-01-01T00:00:00Z' }],
          }),
          makeTarget({
            id: 'target-pack',
            skillId: 'pack-tool',
            targetPath: '/c/skills/pack-tool',
            claims: [
              { id: 'claim-pack-direct', claimType: 'direct', packId: null, packName: null, createdAt: '2026-01-01T00:00:00Z' },
              { id: 'claim-pack', claimType: 'pack', packId: 'daily-tools', packName: 'Daily Tools', createdAt: '2026-01-01T00:00:00Z' },
            ],
          }),
        ],
      },
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (3)'))

    const packFilter = screen.getByLabelText('技能包归属')
    expect(screen.getByRole('option', { name: '全部 (2)' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: '技能包 (1)' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: '非技能包 (1)' })).toBeInTheDocument()

    fireEvent.change(packFilter, { target: { value: 'pack' } })
    expect(screen.getByText('pack-tool')).toBeInTheDocument()
    expect(screen.queryByText('direct-tool')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '已管理 2' })).toBeInTheDocument()

    fireEvent.change(packFilter, { target: { value: 'standalone' } })
    expect(screen.getByText('direct-tool')).toBeInTheDocument()
    expect(screen.queryByText('pack-tool')).not.toBeInTheDocument()
  })

  it('opens a center library install dialog from Agent skills and distributes only to the selected agent', async () => {
    useSkillStoreV2.setState({
      skills: [
        makeSkill({
          id: 'release-checklist',
          name: 'Release Checklist',
          description: 'Pre-release QA skill',
          sourceType: 'skills.sh',
          installedAgents: [
            { agentId: 'claude-code', displayName: 'Claude Code', iconKey: 'claude-code', mode: 'link', status: 'ok' },
          ],
        }),
        makeSkill({
          id: 'frontend-design',
          name: 'frontend-design',
          description: 'Visual design guidance',
          sourceType: 'github',
          installedAgents: [],
        }),
      ],
    })
    const preview: DistributionPreview = {
      skillIds: ['frontend-design'],
      targetAgents: ['claude-code'],
      requestedMode: 'link',
      changes: [
        {
          skillId: 'frontend-design',
          agentId: 'claude-code',
          action: 'create',
          actualMode: 'link',
          targetPath: '/c/skills/frontend-design',
        },
      ],
      blockers: [],
      blockerDecisions: [],
    }
    const previewDistribute = vi.spyOn(skillApiV2, 'previewDistribute').mockResolvedValueOnce(preview)
    const executeDistribute = vi.spyOn(skillApiV2, 'executeDistribute').mockResolvedValueOnce(preview)
    const loadAgentDetail = vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue(undefined)
    const loadOverview = vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue(undefined)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '新增SKILL' }))

    expect(screen.getByRole('heading', { name: '从技能库添加' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: '从技能库添加' }).closest('.sm2__modal')).toHaveClass('sm2__modal--light-surface')
    expect(screen.getByText('目标：')).toBeInTheDocument()
    expect(screen.getAllByText('Claude Code').length).toBeGreaterThan(0)
    expect(screen.getByRole('button', { name: '未安装 1' })).toHaveClass('active')
    expect(screen.getByText('frontend-design')).toBeInTheDocument()
    expect(screen.queryByText('Release Checklist')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '已安装 1' }))
    expect(screen.getByText('Release Checklist')).toBeInTheDocument()
    expect(screen.queryByText('frontend-design')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '未安装 1' }))
    fireEvent.click(screen.getByRole('button', { name: 'GitHub' }))
    fireEvent.click(screen.getByLabelText('选择 frontend-design'))
    fireEvent.click(screen.getByRole('button', { name: '添加 1 个 Skill' }))

    await waitFor(() => {
      expect(previewDistribute).toHaveBeenCalledWith(['frontend-design'], ['claude-code'], 'link')
    })
    fireEvent.click(await screen.findByRole('button', { name: '执行分发' }))
    await waitFor(() => expect(executeDistribute).toHaveBeenCalledWith(preview))
    expect(loadAgentDetail).toHaveBeenCalledWith('claude-code', true)
    expect(loadOverview).toHaveBeenCalledWith(true)
  })

  async function openAgentSkillLibrary() {
    useSkillStoreV2.setState({
      skills: [
        makeSkill({ id: 'first-skill', name: 'First skill', installedAgents: [] }),
        makeSkill({ id: 'second-skill', name: 'Second skill', installedAgents: [] }),
      ],
    })
    vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue(undefined)
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue(undefined)
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    await act(async () => { render(<AgentManagementPage />) })
    await act(async () => { fireEvent.click(screen.getByText('Skills (2)')) })
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: '新增SKILL' })) })
    return within(screen.getByRole('dialog', { name: '从技能库添加' }))
  }

  it('discards a late Agent Skill preview after the selection and mode change', async () => {
    const dialog = await openAgentSkillLibrary()
    const older: DistributionPreview = {
      skillIds: ['first-skill'],
      targetAgents: ['claude-code'],
      requestedMode: 'link',
      changes: [],
      blockers: [],
      blockerDecisions: [],
    }
    const latest: DistributionPreview = { ...older, skillIds: ['second-skill'], requestedMode: 'copy' }
    let resolvePreview!: (value: DistributionPreview) => void
    const pending = new Promise<DistributionPreview>((resolve) => { resolvePreview = resolve })
    const preview = vi.spyOn(skillApiV2, 'previewDistribute').mockReturnValueOnce(pending).mockResolvedValue(latest)
    const execute = vi.spyOn(skillApiV2, 'executeDistribute').mockResolvedValue(latest)
    fireEvent.click(dialog.getByRole('checkbox', { name: '选择 First skill' }))
    fireEvent.click(dialog.getByRole('button', { name: '添加 1 个 Skill' }))
    expect(preview).toHaveBeenCalledWith(['first-skill'], ['claude-code'], 'link')

    fireEvent.click(dialog.getByRole('checkbox', { name: '选择 First skill' }))
    fireEvent.click(dialog.getByRole('checkbox', { name: '选择 Second skill' }))
    fireEvent.click(dialog.getByRole('button', { name: /复制/ }))
    await act(async () => { resolvePreview(older) })

    expect(screen.queryByRole('dialog', { name: '确认安装 Skill' })).not.toBeInTheDocument()
    expect(execute).not.toHaveBeenCalled()
    fireEvent.click(dialog.getByRole('button', { name: '添加 1 个 Skill' }))
    expect(preview).toHaveBeenLastCalledWith(['second-skill'], ['claude-code'], 'copy')
    const review = await screen.findByRole('dialog', { name: '确认安装 Skill' })
    expect(review).toHaveTextContent(/1 个 Skill.+复制/)
    fireEvent.click(within(review).getByRole('button', { name: '执行分发' }))
    expect(await screen.findByText('Skill 已安装到当前 Agent')).toBeInTheDocument()
    expect(execute).toHaveBeenCalledWith(latest)
  })

  it('invalidates a reviewed Agent Skill install after switching runtimes', async () => {
    const dialog = await openAgentSkillLibrary()
    vi.spyOn(skillApiV2, 'previewDistribute').mockResolvedValue({
      skillIds: ['first-skill'],
      targetAgents: ['claude-code'],
      requestedMode: 'link',
      changes: [],
      blockers: [],
      blockerDecisions: [],
    })
    const execute = vi.spyOn(skillApiV2, 'executeDistribute')
    fireEvent.click(dialog.getByRole('checkbox', { name: '选择 First skill' }))
    fireEvent.click(dialog.getByRole('button', { name: '添加 1 个 Skill' }))
    const confirm = await screen.findByRole('button', { name: '执行分发' })
    const { selectEnvironment } = useRuntimeEnvironmentStore.getState()
    act(() => {
      selectEnvironment('other-machine')
      selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    })
    fireEvent.click(confirm)

    expect(execute).not.toHaveBeenCalled()
    expect(screen.queryByRole('dialog', { name: '确认安装 Skill' })).not.toBeInTheDocument()
  })

  it('drops the install refresh if the runtime changes while inventory is loading', async () => {
    const dialog = await openAgentSkillLibrary()
    const preview: DistributionPreview = {
      skillIds: ['first-skill'],
      targetAgents: ['claude-code'],
      requestedMode: 'link',
      changes: [],
      blockers: [],
      blockerDecisions: [],
    }
    vi.spyOn(skillApiV2, 'previewDistribute').mockResolvedValue(preview)
    vi.spyOn(skillApiV2, 'executeDistribute').mockResolvedValue(preview)
    let resolveInventory!: (value: UnmanagedItemDto[]) => void
    const inventory = new Promise<UnmanagedItemDto[]>((resolve) => { resolveInventory = resolve })
    const listUnmanaged = vi.spyOn(skillApiV2, 'listUnmanaged').mockReturnValue(inventory)
    const initialInventory = useSkillStoreV2.getState().unmanaged
    const loadAgentDetail = vi.mocked(useSkillStoreV2.getState().loadAgentDetail)
    loadAgentDetail.mockClear()
    fireEvent.click(dialog.getByRole('checkbox', { name: '选择 First skill' }))
    fireEvent.click(dialog.getByRole('button', { name: '添加 1 个 Skill' }))
    fireEvent.click(await screen.findByRole('button', { name: '执行分发' }))
    await waitFor(() => expect(listUnmanaged).toHaveBeenCalled())

    await act(async () => {
      const { selectEnvironment } = useRuntimeEnvironmentStore.getState()
      selectEnvironment('other-machine')
      selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
      resolveInventory([])
    })

    expect(useSkillStoreV2.getState().unmanaged).toEqual(initialInventory)
    expect(loadAgentDetail).not.toHaveBeenCalled()
  })

  it('hides pack controls and center-library install action on unmanaged skills', async () => {
    useSkillStoreV2.setState({
      selectedAgentDetail: {
        ...agentDetail,
        availablePacks: [
          { id: 'default', name: '全量技能包', description: '', tags: [], memberCount: 73, appliedAgentCount: 0, healthy: true },
        ],
      },
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '未管理 1' }))

    expect(screen.queryByLabelText('技能包应用')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '新增SKILL' })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '批量管理' })).toBeInTheDocument()
  })

  it('shows Doubao built-in skills separately without adopt or delete actions', async () => {
    useSkillStoreV2.setState({
      agents: [
        {
          ...makeSidebarAgent('doubao', 'Doubao'),
          readOnlySkillCount: 1,
        },
      ],
      selectedAgentId: 'doubao',
      selectedAgentDetail: {
        ...agentDetail,
        id: 'doubao',
        displayName: 'Doubao',
        iconKey: 'doubao',
        skillsDir: '/Users/me/Doubao/skills',
        skills: [],
      },
      unmanaged: [
        {
          id: 'doubao-builtin-browser-task',
          agentId: 'doubao',
          itemType: 'agent_skill',
          path: '/Users/me/Library/Application Support/Doubao/Default/.doubao/agent_mode/workspace/.skills/browser-task',
          inferredSkillId: 'browser-task',
          hash: 'builtin-hash',
          reason: 'agent_builtin_read_only',
          readOnly: true,
        },
      ],
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (1)'))
    fireEvent.click(screen.getByRole('button', { name: '内置只读 1' }))

    expect(screen.getByText('browser-task')).toBeInTheDocument()
    expect(screen.getByText(/1 个 Skill 由 Doubao 内置提供/)).toBeInTheDocument()
    expect(screen.getAllByText('Agent 内置（只读）').length).toBeGreaterThan(0)
    expect(screen.queryByRole('button', { name: '接管' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /删除 browser-task/ })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '批量管理' })).not.toBeInTheDocument()
  })

  it('enters managed skill multi-select only after a batch action', async () => {
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))

    expect(screen.queryByLabelText('选择 release-checklist')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '批量选择' }))

    expect(screen.getByLabelText('选择 release-checklist')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '批量删除 0 个' })).toBeInTheDocument()
  })

  it.each([
    'same selection', 'switch before confirming', 'switch during move', 'switch during refresh',
  ] as const)('keeps a direct Skill move on its reviewed Agent view with %s', async (scenario) => {
    const pack = {
      id: 'daily-pack',
      name: 'Daily Pack',
      description: 'Daily tools',
      tags: [],
      memberCount: 2,
      appliedAgentCount: 0,
      healthy: true,
    }
    useSkillStoreV2.setState({
      packs: [pack],
      selectedAgentDetail: {
        ...agentDetail,
        availablePacks: [pack],
      },
    })
    const movePreview: MoveDirectSkillToPackPreview = {
      targetId: 'target-1',
      skillId: 'release-checklist',
      skillName: 'Release Checklist',
      agentId: 'claude-code',
      displayName: 'Claude Code',
      packId: 'daily-pack',
      packName: 'Daily Pack',
      alreadyMember: false,
      alreadyApplied: false,
      willAddToPack: true,
      otherMemberCount: 2,
      distribution: {
        skillIds: [],
        targetAgents: ['claude-code'],
        requestedMode: 'link',
        changes: [],
        blockers: [],
        blockerDecisions: [],
      },
    }
    const previewMove = vi.spyOn(skillApiV2, 'previewMoveDirectSkillToPack').mockResolvedValue(movePreview)
    let finishMove!: () => void
    const executeMove = vi.spyOn(skillApiV2, 'moveDirectSkillToPack').mockImplementation(() => {
      if (scenario !== 'switch during move') return Promise.resolve(movePreview)
      return new Promise((resolve) => { finishMove = () => resolve(movePreview) })
    })
    let finishRefresh!: () => void
    const refreshAgentSkillView = vi.mocked(skillApiV2.refreshAgentSkillView)
    const snapshot = {
      agentDetail,
      overview: { ...makeOverview(), agents: useSkillStoreV2.getState().agents },
      unmanaged: [],
    }
    if (scenario === 'switch during refresh') {
      refreshAgentSkillView.mockImplementation(() => new Promise((resolve) => {
        finishRefresh = () => resolve(snapshot)
      }))
    } else refreshAgentSkillView.mockResolvedValue(snapshot)
    const listUnmanaged = vi.spyOn(skillApiV2, 'listUnmanaged')
    const loadAgentDetail = vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue(undefined)
    const loadOverview = vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue(undefined)
    loadOverview.mockClear()

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '归入技能包 release-checklist' }))

    expect(screen.getByRole('heading', { name: '将直接分发归入技能包' })).toBeInTheDocument()
    expect(await screen.findByText('加入技能包成员')).toBeInTheDocument()
    expect(screen.getByText('不处理其他成员')).toBeInTheDocument()
    expect(screen.getByText(/不会安装、更新或删除「Daily Pack」中的其他 2 个成员/)).toBeInTheDocument()
    expect(screen.getByText('切换为技能包控制')).toBeInTheDocument()

    if (scenario === 'switch before confirming') {
      await act(async () => {
        useRuntimeEnvironmentStore.getState().selectEnvironment('other-host')
        useRuntimeEnvironmentStore.getState().selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
      })
      expect(screen.queryByRole('dialog', { name: '将直接分发归入技能包' })).not.toBeInTheDocument()
      expect(executeMove).not.toHaveBeenCalled()
      return
    }

    fireEvent.click(screen.getByRole('button', { name: '移动并交由技能包管理' }))
    expect(executeMove).toHaveBeenCalledTimes(1)

    if (scenario === 'same selection') {
      await waitFor(() => expect(screen.getByText('已将「Release Checklist」归入「Daily Pack」')).toBeInTheDocument())
    } else {
      if (scenario === 'switch during refresh') await waitFor(() => expect(refreshAgentSkillView).toHaveBeenCalled())
      const remoteItem = makeSharedUnmanaged('remote-only', { agentId: 'claude-code' })
      await act(async () => {
        useRuntimeEnvironmentStore.getState().selectEnvironment('other-host')
        useSkillStoreV2.setState({ runtimeEnvironmentId: 'other-host', unmanaged: [remoteItem], error: 'New host status' })
        if (scenario === 'switch during move') finishMove()
        else finishRefresh()
      })
      expect(useSkillStoreV2.getState().unmanaged).toEqual([remoteItem])
      expect(useSkillStoreV2.getState().error).toBe('New host status')
      expect(screen.queryByText(/Release Checklist.*Daily Pack/)).not.toBeInTheDocument()
    }
    expect(previewMove).toHaveBeenCalledExactlyOnceWith('target-1', 'daily-pack', LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(executeMove).toHaveBeenCalledExactlyOnceWith('target-1', 'daily-pack', [], LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(listUnmanaged).not.toHaveBeenCalled()
    expect(loadAgentDetail).not.toHaveBeenCalled()
    expect(loadOverview).not.toHaveBeenCalledWith(true)
    if (scenario === 'switch during move') expect(refreshAgentSkillView).not.toHaveBeenCalled()
    else expect(refreshAgentSkillView).toHaveBeenCalledExactlyOnceWith('claude-code', LOCAL_RUNTIME_ENVIRONMENT_ID)
  })

  it('shows every claim and defaults moving to the skill existing pack', async () => {
    const alphaPack = {
      id: 'alpha-pack',
      name: 'Alpha Pack',
      description: 'First by name',
      tags: [],
      memberCount: 3,
      appliedAgentCount: 1,
      healthy: true,
    }
    const existingPack = {
      id: 'nice-try',
      name: 'NiceTry',
      description: 'Existing claim pack',
      tags: [],
      memberCount: 4,
      appliedAgentCount: 1,
      healthy: true,
    }
    useSkillStoreV2.setState({
      packs: [alphaPack, existingPack],
      selectedAgentDetail: {
        ...agentDetail,
        skills: [{
          ...agentDetail.skills[0],
          claims: [
            ...agentDetail.skills[0].claims,
            {
              id: 'claim-nice-try',
              claimType: 'pack',
              packId: 'nice-try',
              packName: 'NiceTry',
              createdAt: '2026-01-02T00:00:00Z',
            },
          ],
        }],
        appliedPacks: [
          { packId: 'alpha-pack', packName: 'Alpha Pack', memberCount: 3 },
          { packId: 'nice-try', packName: 'NiceTry', memberCount: 4 },
        ],
        availablePacks: [alphaPack, existingPack],
      },
    })
    const movePreview: MoveDirectSkillToPackPreview = {
      targetId: 'target-1',
      skillId: 'release-checklist',
      skillName: 'Release Checklist',
      agentId: 'claude-code',
      displayName: 'Claude Code',
      packId: 'nice-try',
      packName: 'NiceTry',
      alreadyMember: true,
      alreadyApplied: true,
      willAddToPack: false,
      otherMemberCount: 3,
      distribution: {
        skillIds: [],
        targetAgents: ['claude-code'],
        requestedMode: 'link',
        changes: [],
        blockers: [],
        blockerDecisions: [],
      },
    }
    const previewMove = vi.spyOn(skillApiV2, 'previewMoveDirectSkillToPack').mockResolvedValue(movePreview)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))

    const cardTitle = screen.getByText('release-checklist', { selector: '.sm2__agent-skill-card-titleline strong' })
    const meta = cardTitle.closest('.sm2__agent-skill-card')?.querySelector('.sm2__agent-skill-meta')
    expect(meta).toHaveTextContent('直接分发')
    expect(meta).toHaveTextContent('技能包：NiceTry')
    expect(meta?.querySelector('.sm2__source-pill--claim-direct')).toHaveTextContent('直接分发')
    expect(meta?.querySelector('.sm2__source-pill--claim-pack')).toHaveTextContent('技能包：NiceTry')

    fireEvent.click(screen.getByRole('button', { name: '归入技能包 release-checklist' }))

    expect(document.querySelector('#sm2-move-to-pack-select')).toHaveValue('nice-try')
    await waitFor(() => expect(previewMove).toHaveBeenCalledWith('target-1', 'nice-try', LOCAL_RUNTIME_ENVIRONMENT_ID))
  })

  it('shows immediate deleting feedback for a managed skill card', async () => {
    let finishDelete: (() => void) | null = null
    vi.spyOn(skillApiV2, 'deleteSkillTargetDistributions').mockImplementationOnce((targetIds) => new Promise((resolve) => {
      finishDelete = () => resolve({ deleted: targetIds.length, failures: [] })
    }))
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    const { container } = render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))

    fireEvent.click(screen.getByRole('button', { name: '删除 release-checklist' }))

    const deletingButton = (await screen.findAllByRole('button', { name: /删除中/ })).find((element) => element.tagName === 'BUTTON')
    expect(deletingButton).toBeTruthy()
    expect(deletingButton!).toHaveAttribute('aria-busy', 'true')
    expect(container.querySelector('.sm2__agent-skill-card--deleting')).not.toBeNull()

    expect(finishDelete).toBeTypeOf('function')
    ;(finishDelete as unknown as () => void)()
  })

  it('deletes managed skills directly from the agent skill cards and in batches', async () => {
    const deleteTargets = vi.spyOn(skillApiV2, 'deleteSkillTargetDistributions').mockResolvedValue({ deleted: 1, failures: [] })
    deleteTargets.mockClear()
    const refreshAgentSkillView = vi.mocked(skillApiV2.refreshAgentSkillView)
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))

    fireEvent.click(screen.getByRole('button', { name: '删除 release-checklist' }))
    await waitFor(() => expect(deleteTargets).toHaveBeenCalledWith(['target-1'], LOCAL_RUNTIME_ENVIRONMENT_ID))
    await waitFor(() => expect(screen.queryByRole('button', { name: '删除 release-checklist' })).not.toBeInTheDocument())

    act(() => {
      useSkillStoreV2.setState({ selectedAgentDetail: agentDetail })
    })

    fireEvent.click(screen.getByRole('button', { name: '批量选择' }))
    fireEvent.click(screen.getByLabelText('选择 release-checklist'))
    fireEvent.click(screen.getByRole('button', { name: '批量删除 1 个' }))
    expect(screen.getByRole('heading', { name: '确认批量删除 Skill？' })).toBeInTheDocument()
    expect(screen.getByText('1个SKILL 将从当前Agent直接删除，您后续仍旧可以从中心库安装')).toBeInTheDocument()
    expect(deleteTargets).toHaveBeenCalledTimes(1)

    fireEvent.click(screen.getByRole('button', { name: '确认删除' }))
    expect(screen.queryByRole('heading', { name: '确认批量删除 Skill？' })).not.toBeInTheDocument()
    await waitFor(() => expect(deleteTargets).toHaveBeenCalledTimes(2))
    await waitFor(() => expect(refreshAgentSkillView).toHaveBeenCalledWith('claude-code'))
  })

  it('retries the composite Agent Skill refresh once without using the legacy list chain', async () => {
    vi.spyOn(skillApiV2, 'deleteSkillTargetDistributions').mockResolvedValue({ deleted: 1, failures: [] })
    const refreshAgentSkillView = vi.mocked(skillApiV2.refreshAgentSkillView)
    refreshAgentSkillView.mockRejectedValueOnce(new Error('temporary refresh failure'))
    const listUnmanaged = vi.spyOn(skillApiV2, 'listUnmanaged')
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))

    fireEvent.click(screen.getByRole('button', { name: '删除 release-checklist' }))

    await waitFor(() => expect(refreshAgentSkillView).toHaveBeenCalledTimes(2))
    expect(listUnmanaged).not.toHaveBeenCalled()
    expect(useSkillStoreV2.getState().error).toBeNull()
  })

  it('does not retry a deletion refresh after switching away and back to the same runtime', async () => {
    vi.spyOn(skillApiV2, 'deleteSkillTargetDistributions').mockResolvedValue({ deleted: 1, failures: [] })
    const refreshAgentSkillView = vi.mocked(skillApiV2.refreshAgentSkillView)
    let failRefresh!: (error: Error) => void
    refreshAgentSkillView.mockImplementationOnce(() => new Promise((_resolve, reject) => {
      failRefresh = reject
    }))
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '删除 release-checklist' }))
    await waitFor(() => expect(refreshAgentSkillView).toHaveBeenCalledTimes(1))

    await act(async () => {
      useRuntimeEnvironmentStore.getState().selectEnvironment('new-host')
      useRuntimeEnvironmentStore.getState().selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
      useSkillStoreV2.setState({ selectedAgentDetail: agentDetail, error: 'New view status' })
      failRefresh(new Error('old refresh failed'))
    })

    expect(refreshAgentSkillView).toHaveBeenCalledTimes(1)
    expect(useSkillStoreV2.getState().error).toBe('New view status')
    expect(screen.getByRole('button', { name: '删除 release-checklist' })).toBeInTheDocument()
  })

  it('keeps the new Agent view error when an earlier deletion fails after leaving and returning', async () => {
    let failDelete!: (error: Error) => void
    const deleteTargets = vi.spyOn(skillApiV2, 'deleteSkillTargetDistributions').mockImplementationOnce(() => (
      new Promise((_resolve, reject) => { failDelete = reject })
    ))
    vi.spyOn(skillApiV2, 'getAgentDetail').mockImplementation(async (id) => ({ ...agentDetail, id }))
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '删除 release-checklist' }))
    expect(deleteTargets).toHaveBeenCalledTimes(1)

    await act(async () => { await useSkillStoreV2.getState().selectAgent('codex') })
    await act(async () => { await useSkillStoreV2.getState().selectAgent('claude-code') })
    await act(async () => {
      useSkillStoreV2.setState({ error: 'New Agent status' })
      failDelete(new Error('old deletion failed'))
    })

    expect(useSkillStoreV2.getState().error).toBe('New Agent status')
    expect(vi.mocked(skillApiV2.refreshAgentSkillView)).not.toHaveBeenCalled()
  })

  it('invalidates a deletion review when the runtime changes and returns before confirmation', async () => {
    const deleteTargets = vi.spyOn(skillApiV2, 'deleteSkillTargetDistributions')
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '批量选择' }))
    fireEvent.click(screen.getByLabelText('选择 release-checklist'))
    fireEvent.click(screen.getByRole('button', { name: '批量删除 1 个' }))
    expect(screen.getByRole('dialog', { name: '确认批量删除 Skill？' })).toBeInTheDocument()

    await act(async () => {
      useRuntimeEnvironmentStore.getState().selectEnvironment('new-host')
      useRuntimeEnvironmentStore.getState().selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    })

    expect(screen.queryByRole('dialog', { name: '确认批量删除 Skill？' })).not.toBeInTheDocument()
    expect(deleteTargets).not.toHaveBeenCalled()
  })

  it('cancels managed skill batch deletion from the confirmation dialog', async () => {
    const deleteTargets = vi.spyOn(skillApiV2, 'deleteSkillTargetDistributions').mockResolvedValue({ deleted: 1, failures: [] })
    deleteTargets.mockClear()
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '批量选择' }))
    fireEvent.click(screen.getByLabelText('选择 release-checklist'))

    fireEvent.click(screen.getByRole('button', { name: '批量删除 1 个' }))
    fireEvent.click(screen.getByRole('button', { name: '取消' }))

    expect(screen.queryByRole('heading', { name: '确认批量删除 Skill？' })).not.toBeInTheDocument()
    expect(deleteTargets).not.toHaveBeenCalled()
    expect(screen.getByLabelText('选择 release-checklist')).toBeChecked()
  })

  it('confirms global impact before deleting a shared managed Skill by its target ID', async () => {
    useSkillStoreV2.setState({
      selectedAgentDetail: {
        ...agentDetail,
        id: 'codex',
        displayName: 'Codex',
        iconKey: 'codex',
        inheritsSharedSkills: true,
        inheritedManagedSkills: [makeSharedTarget('shared-review', { id: 'target-shared-review' })],
        inheritedUnmanagedSkills: [],
      },
      selectedAgentId: 'codex',
      agents: [makeSidebarAgent('codex', 'Codex')],
      unmanaged: [],
    })
    let finishDelete: (() => void) | null = null
    const deleteTargets = vi.spyOn(skillApiV2, 'deleteSkillTargetDistributions').mockImplementation(() => new Promise((resolve) => {
      finishDelete = () => resolve({ deleted: 1, failures: [] })
    }))
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByRole('button', { name: 'Skills (2)' }))
    fireEvent.click(screen.getByRole('button', { name: '共享继承 1' }))

    fireEvent.click(screen.getByRole('button', { name: '删除 shared-review' }))

    const dialog = screen.getByRole('dialog', { name: '移除共享的已管理 Skill？' })
    expect(dialog).toHaveTextContent('所有读取该共享目录的 Agent 都将无法再使用它')
    expect(dialog).toHaveTextContent('中心库中的 Skill 会保留')
    expect(deleteTargets).not.toHaveBeenCalled()
    fireEvent.click(within(dialog).getByRole('button', { name: '从共享目录删除' }))

    expect(screen.queryByRole('dialog', { name: '移除共享的已管理 Skill？' })).not.toBeInTheDocument()
    await waitFor(() => expect(deleteTargets).toHaveBeenCalledWith(['target-shared-review'], LOCAL_RUNTIME_ENVIRONMENT_ID))
    expect(screen.getByRole('button', { name: '删除中 shared-review' })).toHaveAttribute('aria-busy', 'true')
    expect(finishDelete).toBeTypeOf('function')
    ;(finishDelete as unknown as () => void)()
    await waitFor(() => expect(screen.queryByText('shared-review')).not.toBeInTheDocument())
  })

  it('batch deletes shared managed Skills through the standard selection controls', async () => {
    useSkillStoreV2.setState({
      selectedAgentDetail: {
        ...agentDetail,
        id: 'codex',
        displayName: 'Codex',
        iconKey: 'codex',
        inheritsSharedSkills: true,
        inheritedManagedSkills: [
          makeSharedTarget('shared-review', { id: 'target-shared-review' }),
          makeSharedTarget('shared-writing', { id: 'target-shared-writing' }),
        ],
        inheritedUnmanagedSkills: [],
      },
      selectedAgentId: 'codex',
      agents: [makeSidebarAgent('codex', 'Codex')],
      unmanaged: [],
    })
    const deleteTargets = vi.spyOn(skillApiV2, 'deleteSkillTargetDistributions').mockResolvedValue({ deleted: 2, failures: [] })
    vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue(undefined)
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue(undefined)
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByRole('button', { name: 'Skills (3)' }))
    fireEvent.click(screen.getByRole('button', { name: '共享继承 2' }))
    fireEvent.click(screen.getByRole('button', { name: '批量选择' }))
    fireEvent.click(screen.getByRole('button', { name: '选择当前' }))
    fireEvent.click(screen.getByRole('button', { name: '批量删除 2 个' }))

    const dialog = screen.getByRole('dialog', { name: '移除共享的已管理 Skill？' })
    fireEvent.click(within(dialog).getByRole('button', { name: '从共享目录删除' }))

    await waitFor(() => expect(deleteTargets).toHaveBeenCalledWith(['target-shared-review', 'target-shared-writing'], LOCAL_RUNTIME_ENVIRONMENT_ID))
  })

  it('deletes an unmanaged agent skill from the skill card after confirmation', async () => {
    const deleteUnmanaged = vi.spyOn(skillApiV2, 'deleteUnmanagedAgentSkills').mockResolvedValue({ deleted: 1, failures: [] })
    const refreshAgentSkillView = vi.mocked(skillApiV2.refreshAgentSkillView)
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '未管理 1' }))

    fireEvent.click(screen.getByRole('button', { name: '删除 manual-skill' }))
    const dialog = screen.getByRole('dialog', { name: '删除 Skill「manual-skill」？' })
    expect(dialog).toBeInTheDocument()
    expect(dialog).toHaveTextContent('/c/skills/manual-skill')
    fireEvent.click(screen.getByRole('button', { name: '直接删除' }))
    expect(screen.queryByRole('dialog', { name: '删除 Skill「manual-skill」？' })).not.toBeInTheDocument()

    await waitFor(() => expect(deleteUnmanaged).toHaveBeenCalledWith('claude-code', ['unmanaged-1'], LOCAL_RUNTIME_ENVIRONMENT_ID))
    await waitFor(() => expect(refreshAgentSkillView).toHaveBeenCalledWith('claude-code'))
  })

  it('permanently deletes a shared unmanaged Skill with owner agents after confirmation', async () => {
    useSkillStoreV2.setState({
      selectedAgentDetail: {
        ...agentDetail,
        id: 'codex',
        displayName: 'Codex',
        iconKey: 'codex',
        inheritsSharedSkills: true,
        inheritedManagedSkills: [],
        inheritedUnmanagedSkills: [makeSharedUnmanaged('shared-writing')],
      },
      selectedAgentId: 'codex',
      agents: [makeSidebarAgent('codex', 'Codex')],
      unmanaged: [],
    })
    const deleteUnmanaged = vi.spyOn(skillApiV2, 'deleteUnmanagedAgentSkills').mockResolvedValue({ deleted: 1, failures: [] })
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByRole('button', { name: 'Skills (2)' }))
    fireEvent.click(screen.getByRole('button', { name: '共享继承 1' }))
    fireEvent.click(screen.getByRole('button', { name: '未管理 1' }))

    fireEvent.click(screen.getByRole('button', { name: '删除 shared-writing' }))

    const dialog = screen.getByRole('dialog', { name: '永久删除共享的未管理 Skill？' })
    expect(dialog).toHaveTextContent('所有读取该共享目录的 Agent 都会受到影响')
    expect(dialog).toHaveTextContent('不会复制到中心库')
    expect(deleteUnmanaged).not.toHaveBeenCalled()
    fireEvent.click(within(dialog).getByRole('button', { name: '从共享目录删除' }))
    expect(screen.queryByRole('dialog', { name: '永久删除共享的未管理 Skill？' })).not.toBeInTheDocument()

    await waitFor(() => expect(deleteUnmanaged).toHaveBeenCalledWith('agents', ['raw-shared-writing'], LOCAL_RUNTIME_ENVIRONMENT_ID))
  })

  it('closes the unmanaged confirmation dialog and keeps failed items visible', async () => {
    const codexItem = {
      id: 'unmanaged-codex-bird',
      agentId: 'codex',
      itemType: 'skill' as const,
      path: '/Users/me/.codex/skills/bird',
      inferredSkillId: 'bird',
      hash: null,
      reason: 'not_in_center_library',
    }
    useSkillStoreV2.setState({
      selectedAgentId: 'codex',
      selectedAgentDetail: {
        ...agentDetail,
        id: 'codex',
        displayName: 'Codex',
        iconKey: 'codex',
        skillsDir: '/Users/me/.codex/skills',
      },
      agents: [
        { id: 'codex', displayName: 'Codex', iconKey: 'codex', enabled: true, skillsDir: '/Users/me/.codex/skills', version: null, latestVersion: null, installed: true, managedSkillCount: 1, unmanagedSkillCount: 1 } as AgentSummary,
      ],
      unmanaged: [codexItem],
    })
    const mismatch = new Error(
      "Unmanaged item 'unmanaged-codex-bird' does not belong to agent 'codex'.",
    )
    const deleteUnmanaged = vi.spyOn(skillApiV2, 'deleteUnmanagedAgentSkills').mockResolvedValue({
      deleted: 0,
      failures: [{ unmanagedId: 'unmanaged-codex-bird', error: mismatch.message }],
    })
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '未管理 1' }))
    fireEvent.click(screen.getByRole('button', { name: '删除 bird' }))

    const dialog = screen.getByRole('dialog', { name: '删除 Skill「bird」？' })
    expect(dialog).toHaveClass('sm2__modal--unmanaged-delete')
    fireEvent.click(within(dialog).getByRole('button', { name: '直接删除' }))

    expect(screen.queryByRole('dialog', { name: '删除 Skill「bird」？' })).not.toBeInTheDocument()
    await waitFor(() => expect(deleteUnmanaged).toHaveBeenCalledWith('codex', ['unmanaged-codex-bird'], LOCAL_RUNTIME_ENVIRONMENT_ID))
    expect(await screen.findByText(/该未管理 Skill 不属于 Agent「codex」，请重新扫描后重试/)).toBeInTheDocument()
    expect(useSkillStoreV2.getState().unmanaged).toContainEqual(codexItem)
  })

  it('deletes selected unmanaged agent skills in a batch after confirmation', async () => {
    useSkillStoreV2.setState({
      unmanaged: [
        ...useSkillStoreV2.getState().unmanaged,
        {
          id: 'unmanaged-2',
          agentId: 'claude-code',
          itemType: 'skill',
          path: '/c/skills/another-skill',
          inferredSkillId: 'another-skill',
          hash: null,
          reason: 'not_in_center_library',
        },
      ],
    })
    const deleteUnmanaged = vi.spyOn(skillApiV2, 'deleteUnmanagedAgentSkills').mockResolvedValue({ deleted: 2, failures: [] })
    const refreshAgentSkillView = vi.mocked(skillApiV2.refreshAgentSkillView)
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (3)'))
    fireEvent.click(screen.getByRole('button', { name: '未管理 2' }))
    fireEvent.click(screen.getByRole('button', { name: '批量管理' }))
    fireEvent.click(screen.getByRole('button', { name: '选择当前可接管' }))

    fireEvent.click(screen.getByRole('button', { name: '批量删除 2 个' }))
    const dialog = screen.getByRole('dialog', { name: '确认批量删除未管理 Skill？' })
    expect(dialog).toHaveTextContent('2 个未管理 Skill 将从当前 Agent 直接删除。')
    expect(deleteUnmanaged).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: '确认删除' }))
    expect(screen.queryByRole('dialog', { name: '确认批量删除未管理 Skill？' })).not.toBeInTheDocument()

    await waitFor(() => expect(deleteUnmanaged).toHaveBeenCalledWith('claude-code', ['unmanaged-1', 'unmanaged-2'], LOCAL_RUNTIME_ENVIRONMENT_ID))
    await waitFor(() => expect(refreshAgentSkillView).toHaveBeenCalledWith('claude-code'))
  })

  it('removes successful unmanaged batch items and keeps failed items selected', async () => {
    const failedItem: UnmanagedItemDto = {
      id: 'unmanaged-2',
      agentId: 'claude-code',
      itemType: 'skill',
      path: '/c/skills/another-skill',
      inferredSkillId: 'another-skill',
      hash: null,
      reason: 'not_in_center_library',
    }
    useSkillStoreV2.setState({
      unmanaged: [...useSkillStoreV2.getState().unmanaged, failedItem],
    })
    vi.spyOn(skillApiV2, 'deleteUnmanagedAgentSkills').mockResolvedValue({
      deleted: 1,
      failures: [{ unmanagedId: failedItem.id, error: 'permission denied' }],
    })
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (3)'))
    fireEvent.click(screen.getByRole('button', { name: '未管理 2' }))
    fireEvent.click(screen.getByRole('button', { name: '批量管理' }))
    fireEvent.click(screen.getByRole('button', { name: '选择当前可接管' }))
    fireEvent.click(screen.getByRole('button', { name: '批量删除 2 个' }))

    fireEvent.click(screen.getByRole('button', { name: '确认删除' }))

    await waitFor(() => expect(screen.queryByText('manual-skill')).not.toBeInTheDocument())
    expect(screen.getByText('another-skill')).toBeInTheDocument()
    expect(screen.getByLabelText('选择 another-skill')).toBeChecked()
    expect(useSkillStoreV2.getState().error).toContain('permission denied')
  })

  it('batch deletes shared unmanaged Skills with owner agents', async () => {
    useSkillStoreV2.setState({
      selectedAgentDetail: {
        ...agentDetail,
        id: 'codex',
        displayName: 'Codex',
        iconKey: 'codex',
        inheritsSharedSkills: true,
        inheritedManagedSkills: [],
        inheritedUnmanagedSkills: [
          makeSharedUnmanaged('shared-writing'),
          makeSharedUnmanaged('shared-review'),
        ],
      },
      selectedAgentId: 'codex',
      agents: [makeSidebarAgent('codex', 'Codex')],
      unmanaged: [],
    })
    const deleteUnmanaged = vi.spyOn(skillApiV2, 'deleteUnmanagedAgentSkills').mockResolvedValue({ deleted: 2, failures: [] })
    vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue(undefined)
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue(undefined)
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByRole('button', { name: 'Skills (3)' }))
    fireEvent.click(screen.getByRole('button', { name: '共享继承 2' }))
    fireEvent.click(screen.getByRole('button', { name: '未管理 2' }))
    fireEvent.click(screen.getByRole('button', { name: '批量管理' }))
    fireEvent.click(screen.getByRole('button', { name: '选择当前可接管' }))
    fireEvent.click(screen.getByRole('button', { name: '批量删除 2 个' }))

    const dialog = screen.getByRole('dialog', { name: '永久删除共享的未管理 Skill？' })
    fireEvent.click(within(dialog).getByRole('button', { name: '从共享目录删除' }))

    await waitFor(() => expect(deleteUnmanaged).toHaveBeenCalledWith('agents', ['raw-shared-writing', 'raw-shared-review'], LOCAL_RUNTIME_ENVIRONMENT_ID))
  })

  it('finishes confirmed unmanaged deletion on the original runtime for every owner', async () => {
    useSkillStoreV2.setState({
      selectedAgentDetail: {
        ...agentDetail,
        id: 'codex',
        displayName: 'Codex',
        iconKey: 'codex',
        inheritsSharedSkills: true,
        inheritedUnmanagedSkills: [
          makeSharedUnmanaged('shared-writing'),
          makeSharedUnmanaged('shared-review', { agentId: 'codex' }),
        ],
      },
      selectedAgentId: 'codex',
      agents: [makeSidebarAgent('codex', 'Codex')],
      unmanaged: [],
    })
    const ownerRuntimes: string[] = []
    let finishFirstOwner!: () => void
    const pendingFirstOwner = new Promise<{ deleted: number; failures: [] }>((resolve) => {
      finishFirstOwner = () => resolve({ deleted: 1, failures: [] })
    })
    const deleteUnmanaged = vi.spyOn(skillApiV2, 'deleteUnmanagedAgentSkills').mockImplementation(
      (owner, _ids, environmentId?: string) => {
        ownerRuntimes.push(environmentId ?? useRuntimeEnvironmentStore.getState().selectedEnvironmentId)
        return owner === 'agents' ? pendingFirstOwner : Promise.resolve({ deleted: 1, failures: [] })
      },
    )
    const refreshAgentSkillView = vi.mocked(skillApiV2.refreshAgentSkillView)
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByRole('button', { name: 'Skills (3)' }))
    fireEvent.click(screen.getByRole('button', { name: '共享继承 2' }))
    fireEvent.click(screen.getByRole('button', { name: '未管理 2' }))
    fireEvent.click(screen.getByRole('button', { name: '批量管理' }))
    fireEvent.click(screen.getByRole('button', { name: '选择当前可接管' }))
    fireEvent.click(screen.getByRole('button', { name: '批量删除 2 个' }))
    fireEvent.click(screen.getByRole('button', { name: '从共享目录删除' }))
    expect(deleteUnmanaged).toHaveBeenCalledTimes(1)

    await act(async () => {
      useRuntimeEnvironmentStore.getState().selectEnvironment('new-host')
      useSkillStoreV2.setState({ runtimeEnvironmentId: 'new-host', error: 'New host status' })
      finishFirstOwner()
    })

    expect(deleteUnmanaged.mock.calls.map(([owner, ids]) => [owner, ids])).toEqual([
      ['agents', ['raw-shared-writing']],
      ['codex', ['raw-shared-review']],
    ])
    expect(ownerRuntimes).toEqual([LOCAL_RUNTIME_ENVIRONMENT_ID, LOCAL_RUNTIME_ENVIRONMENT_ID])
    expect(refreshAgentSkillView).not.toHaveBeenCalled()
    expect(useSkillStoreV2.getState().error).toBe('New host status')
  })

  it('batch adopts shared unmanaged Skills through the standard flow with owner agents', async () => {
    useSkillStoreV2.setState({
      selectedAgentDetail: {
        ...agentDetail,
        id: 'codex',
        displayName: 'Codex',
        iconKey: 'codex',
        inheritsSharedSkills: true,
        inheritedManagedSkills: [],
        inheritedUnmanagedSkills: [
          makeSharedUnmanaged('shared-writing'),
          makeSharedUnmanaged('shared-review'),
        ],
      },
      selectedAgentId: 'codex',
      agents: [makeSidebarAgent('codex', 'Codex')],
      unmanaged: [],
    })
    const execute = vi.spyOn(skillApiV2, 'executeAdoptBatch').mockResolvedValue({
      items: [
        { unmanagedId: 'raw-shared-writing', skillId: 'shared-writing', error: null },
        { unmanagedId: 'raw-shared-review', skillId: 'shared-review', error: null },
      ],
      finalizationError: null,
    })
    vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue(undefined)
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue(undefined)
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByRole('button', { name: 'Skills (3)' }))
    fireEvent.click(screen.getByRole('button', { name: '共享继承 2' }))
    fireEvent.click(screen.getByRole('button', { name: '未管理 2' }))
    fireEvent.click(screen.getByRole('button', { name: '批量管理' }))
    fireEvent.click(screen.getByRole('button', { name: '选择当前可接管' }))
    fireEvent.click(screen.getByRole('button', { name: '接管到中心库' }))
    fireEvent.click(screen.getByRole('button', { name: '确认接管' }))

    await waitFor(() => expect(execute).toHaveBeenCalledWith([
      {
        agentId: 'agents',
        unmanagedId: 'raw-shared-writing',
        option: 'import_cleanup',
        renamedId: null,
      },
      {
        agentId: 'agents',
        unmanagedId: 'raw-shared-review',
        option: 'import_cleanup',
        renamedId: null,
      },
    ], LOCAL_RUNTIME_ENVIRONMENT_ID))
  })

  it('switches unmanaged skills into a peer tab and batch adopts them like agent sync', async () => {
    const execute = vi.spyOn(skillApiV2, 'executeAdoptBatch').mockResolvedValue({
      items: [{ unmanagedId: 'unmanaged-1', skillId: 'manual-skill', error: null }],
      finalizationError: null,
    })
    const listUnmanaged = vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '未管理 1' }))

    expect(screen.queryByLabelText('选择 manual-skill')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '批量管理' })).toBeInTheDocument()
    expect(screen.queryByText('选择当前可接管')).not.toBeInTheDocument()
    expect(screen.queryByText('已管理 Skills')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '批量管理' }))
    fireEvent.click(screen.getByText('选择当前可接管'))
    fireEvent.click(screen.getByRole('button', { name: '接管到中心库' }))

    expect(screen.getByRole('heading', { name: '批量接管 1 个 Skill' })).toBeInTheDocument()
    expect(screen.getByText('批量接管适用范围')).toBeInTheDocument()
    expect(screen.getByText(/需要你决定保留中心版本、覆盖中心库或重命名/)).toBeInTheDocument()
    expect(screen.getByText('是否将接管的 Skill 同步到技能包？')).toBeInTheDocument()
    expect(screen.getByRole('checkbox', { name: /同时同步到技能包/ })).not.toBeChecked()
    fireEvent.click(screen.getByRole('button', { name: '确认接管' }))

    await waitFor(() => {
      expect(execute).toHaveBeenCalledWith([{
        agentId: 'claude-code',
        unmanagedId: 'unmanaged-1',
        option: 'import_keep',
        renamedId: null,
      }], LOCAL_RUNTIME_ENVIRONMENT_ID)
    })
    expect(listUnmanaged).toHaveBeenCalled()
  })

  it('takes over every same-name center skill in one action and preserves local-only skills', async () => {
    const sameNameSkills = [
      {
        id: 'unmanaged-same',
        agentId: 'claude-code',
        itemType: 'skill' as const,
        path: '/c/skills/same-skill',
        inferredSkillId: 'same-skill',
        hash: 'same-hash',
        reason: 'same_name_as_center_skill',
      },
      {
        id: 'unmanaged-conflict',
        agentId: 'claude-code',
        itemType: 'skill' as const,
        path: '/c/skills/conflict-skill',
        inferredSkillId: 'conflict-skill',
        hash: 'agent-hash',
        reason: 'same_name_as_center_skill',
      },
    ]
    const localOnly = {
      id: 'unmanaged-local',
      agentId: 'claude-code',
      itemType: 'skill' as const,
      path: '/c/skills/local-only',
      inferredSkillId: 'local-only',
      hash: 'local-hash',
      reason: 'not_in_center_library',
    }
    useSkillStoreV2.setState({ unmanaged: [...sameNameSkills, localOnly] })
    const takeover = vi.spyOn(skillApiV2, 'takeoverCenterSkills').mockResolvedValue({
      items: sameNameSkills.map((item) => ({
        unmanagedId: item.id,
        skillId: item.inferredSkillId,
        error: null,
      })),
      finalizationError: null,
    })
    const refreshAgentSkillView = vi.mocked(skillApiV2.refreshAgentSkillView)
    refreshAgentSkillView.mockResolvedValueOnce({
      agentDetail,
      overview: { ...makeOverview(), agents: useSkillStoreV2.getState().agents },
      unmanaged: [localOnly],
    })
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (4)'))
    fireEvent.click(screen.getByRole('button', { name: '未管理 3' }))

    expect(screen.getAllByText('中心库已有同名 Skill').length).toBeGreaterThan(0)
    expect(screen.queryByText('same_name_as_center_skill')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '一键接管 2 个' }))

    expect(screen.getByRole('heading', { name: '接管中心库已有的 2 个 Skill？' })).toBeInTheDocument()
    expect(screen.getByText(/替换为指向中心库的软连接/)).toBeInTheDocument()
    expect(screen.getByText('其余 1 个未管理 Skill 不在中心库中，将保持不变。')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '以中心库为准接管' }))

    await waitFor(() => {
      expect(takeover).toHaveBeenCalledWith('claude-code', [
        'unmanaged-same',
        'unmanaged-conflict',
      ], LOCAL_RUNTIME_ENVIRONMENT_ID)
    })
    expect(refreshAgentSkillView).toHaveBeenCalledExactlyOnceWith('claude-code', LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(await screen.findByText('一键接管完成：已接管 2 个，失败 0 个；其他未管理 Skill 保持不变。')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '批量管理' })).toBeInTheDocument()
  })

  it('closes an unconfirmed one-click takeover when the runtime changes and returns', async () => {
    useSkillStoreV2.setState({
      unmanaged: [{
        ...makeSharedUnmanaged('same-skill', { agentId: 'claude-code' }),
        reason: 'same_name_as_center_skill',
      }],
    })
    const takeover = vi.spyOn(skillApiV2, 'takeoverCenterSkills')
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByRole('button', { name: 'Skills (2)' }))
    fireEvent.click(screen.getByRole('button', { name: '未管理 1' }))
    fireEvent.click(screen.getByRole('button', { name: '一键接管 1 个' }))
    expect(screen.getByRole('dialog', { name: '接管中心库已有的 1 个 Skill？' })).toBeInTheDocument()

    await act(async () => {
      useRuntimeEnvironmentStore.getState().selectEnvironment('new-host')
      useRuntimeEnvironmentStore.getState().selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    })

    expect(screen.queryByRole('dialog', { name: '接管中心库已有的 1 个 Skill？' })).not.toBeInTheDocument()
    expect(takeover).not.toHaveBeenCalled()
  })

  it.each(['takeover', 'refresh'] as const)('keeps another runtime intact when one-click takeover is pending in %s', async (phase) => {
    const item = {
      ...makeSharedUnmanaged('same-skill', { agentId: 'claude-code' }),
      reason: 'same_name_as_center_skill',
    }
    const remoteItem = makeSharedUnmanaged('remote-skill', { agentId: 'claude-code' })
    useSkillStoreV2.setState({ unmanaged: [item] })
    const result = {
      items: [{ unmanagedId: item.id, skillId: 'same-skill', error: null }],
      finalizationError: null,
    }
    let finishTakeover!: () => void
    const takeover = vi.spyOn(skillApiV2, 'takeoverCenterSkills').mockImplementation(() => {
      if (phase === 'refresh') return Promise.resolve(result)
      return new Promise((resolve) => { finishTakeover = () => resolve(result) })
    })
    const snapshot = { agentDetail, overview: makeOverview(), unmanaged: [] }
    let finishRefresh!: () => void
    const refreshAgentSkillView = vi.mocked(skillApiV2.refreshAgentSkillView)
    refreshAgentSkillView.mockImplementation(() => new Promise((resolve) => {
      finishRefresh = () => resolve(snapshot)
    }))
    const listUnmanaged = vi.spyOn(skillApiV2, 'listUnmanaged')
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByRole('button', { name: 'Skills (2)' }))
    fireEvent.click(screen.getByRole('button', { name: '未管理 1' }))
    fireEvent.click(screen.getByRole('button', { name: '一键接管 1 个' }))
    fireEvent.click(screen.getByRole('button', { name: '以中心库为准接管' }))
    expect(takeover).toHaveBeenCalledTimes(1)
    if (phase === 'refresh') await waitFor(() => expect(refreshAgentSkillView).toHaveBeenCalled())

    await act(async () => {
      useRuntimeEnvironmentStore.getState().selectEnvironment('new-host')
      useSkillStoreV2.setState({ runtimeEnvironmentId: 'new-host', unmanaged: [remoteItem], error: 'New host status' })
      if (phase === 'takeover') finishTakeover()
      else finishRefresh()
    })

    expect(useSkillStoreV2.getState().error).toBe('New host status')
    expect(useSkillStoreV2.getState().unmanaged).toEqual([remoteItem])
    expect(screen.queryByText(/一键接管完成/)).not.toBeInTheDocument()
    expect(takeover).toHaveBeenCalledWith('claude-code', [item.id], LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(listUnmanaged).not.toHaveBeenCalled()
    if (phase === 'takeover') expect(refreshAgentSkillView).not.toHaveBeenCalled()
    else expect(refreshAgentSkillView).toHaveBeenCalledExactlyOnceWith('claude-code', LOCAL_RUNTIME_ENVIRONMENT_ID)
  })

  it('explains which batch adoption conflicts require individual review', async () => {
    const conflicts = [
      {
        id: 'unmanaged-ant-skill-creator',
        agentId: 'claude-code',
        itemType: 'skill' as const,
        path: '/c/skills/ant-skill-creator',
        inferredSkillId: 'ant-skill-creator',
        hash: 'different-ant-hash',
        reason: 'same_name_as_center_skill',
      },
      {
        id: 'unmanaged-dws',
        agentId: 'claude-code',
        itemType: 'skill' as const,
        path: '/c/skills/dws',
        inferredSkillId: 'dws',
        hash: 'different-dws-hash',
        reason: 'same_name_as_center_skill',
      },
    ]
    useSkillStoreV2.setState({ unmanaged: conflicts })
    const unavailable = new Error(
      "Adopt option 'import_keep' is not allowed for 'conflict'. Re-run preview and choose one of the suggested actions.",
    )
    const execute = vi.spyOn(skillApiV2, 'executeAdoptBatch').mockResolvedValue({
      items: conflicts.map((item) => ({
        unmanagedId: item.id,
        skillId: null,
        error: unavailable.message,
      })),
      finalizationError: null,
    })
    vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue(conflicts)
    vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue(undefined)
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue(undefined)
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (3)'))
    fireEvent.click(screen.getByRole('button', { name: '未管理 2' }))
    fireEvent.click(screen.getByRole('button', { name: '批量管理' }))
    fireEvent.click(screen.getByText('选择当前可接管'))
    fireEvent.click(screen.getByRole('button', { name: '接管到中心库' }))

    expect(screen.getByText('批量接管适用范围')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '确认接管' }))

    await waitFor(() => expect(execute).toHaveBeenCalledTimes(1))
    expect(await screen.findByText(/以下 2 个 Skill 需要单独确认，批量接管已跳过：ant-skill-creator、dws/)).toBeInTheDocument()
    expect(screen.getByText(/分别点击对应卡片上的「接管」/)).toBeInTheDocument()
    expect(screen.getByText('批量接管完成：已接管 0 个，需单独确认 2 个，失败 0 个')).toBeInTheDocument()
    expect(screen.queryByText(/当前选择的接管方式已不可用/)).not.toBeInTheDocument()
  })

  it.each([false, true])('syncs batch-adopted skills into the original pack (switch during lookup: %s)', async (switchRuntime) => {
    useSkillStoreV2.setState({
      selectedAgentDetail: {
        ...agentDetail,
        availablePacks: [
          { id: 'agent-tools', name: 'Agent Tools', description: 'Daily agent tools', tags: [], memberCount: 1, appliedAgentCount: 0, healthy: true },
        ],
      },
      packs: [
        { id: 'agent-tools', name: 'Agent Tools', description: 'Daily agent tools', tags: [], memberCount: 1, appliedAgentCount: 0, healthy: true },
      ],
    })
    vi.spyOn(skillApiV2, 'executeAdoptBatch').mockResolvedValue({
      items: [{ unmanagedId: 'unmanaged-1', skillId: 'manual-skill', error: null }],
      finalizationError: null,
    })
    vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    const existingPack: SkillPackDetail = {
      id: 'agent-tools',
      name: 'Agent Tools',
      description: 'Daily agent tools',
      tags: [],
      members: [{ skillId: 'release-checklist', skillName: 'Release Checklist', required: true, sortOrder: 0, missing: false }],
      appliedAgents: [],
      createdAt: '2026-01-01T00:00:00Z',
      updatedAt: '2026-01-01T00:00:00Z',
    }
    let finishLookup!: (pack: SkillPackDetail) => void
    const pendingLookup = new Promise<SkillPackDetail>((resolve) => { finishLookup = resolve })
    const getPackDetail = vi.spyOn(skillApiV2, 'getPackDetail').mockReturnValue(switchRuntime ? pendingLookup : Promise.resolve(existingPack))
    const upsertPack = vi.spyOn(skillApiV2, 'upsertPack').mockResolvedValue({
      id: 'agent-tools',
      name: 'Agent Tools',
      description: 'Daily agent tools',
      tags: [],
      members: [
        { skillId: 'release-checklist', skillName: 'Release Checklist', required: true, sortOrder: 0, missing: false },
        { skillId: 'manual-skill', skillName: 'manual-skill', required: true, sortOrder: 1, missing: false },
      ],
      appliedAgents: [],
      createdAt: '2026-01-01T00:00:00Z',
      updatedAt: '2026-01-01T00:00:00Z',
    } as SkillPackDetail)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '未管理 1' }))
    fireEvent.click(screen.getByRole('button', { name: '批量管理' }))
    fireEvent.click(screen.getByText('选择当前可接管'))
    fireEvent.click(screen.getByRole('button', { name: '接管到中心库' }))
    fireEvent.click(screen.getByRole('checkbox', { name: /同时同步到技能包/ }))
    fireEvent.click(screen.getByRole('button', { name: '同步到已有技能包' }))
    fireEvent.change(screen.getByLabelText('目标技能包'), { target: { value: 'agent-tools' } })
    fireEvent.click(screen.getByRole('button', { name: '确认接管' }))

    if (switchRuntime) {
      await waitFor(() => expect(getPackDetail).toHaveBeenCalledTimes(1))
      await act(async () => {
        useRuntimeEnvironmentStore.getState().selectEnvironment('new-host')
        useSkillStoreV2.setState({ runtimeEnvironmentId: 'new-host', error: 'New host status' })
        finishLookup(existingPack)
      })
      expect(useSkillStoreV2.getState().error).toBe('New host status')
    }
    await waitFor(() => {
      expect(getPackDetail).toHaveBeenCalledWith('agent-tools', LOCAL_RUNTIME_ENVIRONMENT_ID)
      expect(upsertPack).toHaveBeenCalledWith({
        id: 'agent-tools',
        name: 'Agent Tools',
        description: 'Daily agent tools',
        tags: [],
        skillIds: ['release-checklist', 'manual-skill'],
      }, { environmentId: LOCAL_RUNTIME_ENVIRONMENT_ID })
    })
  })

  it.each([false, true])('creates a pack on the adoption runtime (switch during adoption: %s)', async (switchRuntime) => {
    const result = {
      items: [{ unmanagedId: 'unmanaged-1', skillId: 'manual-skill', error: null }],
      finalizationError: null,
    }
    let finishAdoption!: (value: typeof result) => void
    const pendingAdoption = new Promise<typeof result>((resolve) => { finishAdoption = resolve })
    const adopt = vi.spyOn(skillApiV2, 'executeAdoptBatch').mockReturnValue(switchRuntime ? pendingAdoption : Promise.resolve(result))
    vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    const upsertPack = vi.spyOn(skillApiV2, 'upsertPack').mockResolvedValue({
      id: 'pack-manual',
      name: 'Manual Pack',
      description: '',
      tags: [],
      members: [{ skillId: 'manual-skill', skillName: 'manual-skill', required: true, sortOrder: 0, missing: false }],
      appliedAgents: [],
      createdAt: '2026-01-01T00:00:00Z',
      updatedAt: '2026-01-01T00:00:00Z',
    } as SkillPackDetail)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '未管理 1' }))
    fireEvent.click(screen.getByRole('button', { name: '批量管理' }))
    fireEvent.click(screen.getByText('选择当前可接管'))
    fireEvent.click(screen.getByRole('button', { name: '接管到中心库' }))
    fireEvent.click(screen.getByRole('checkbox', { name: /同时同步到技能包/ }))
    fireEvent.click(screen.getByRole('button', { name: '新建技能包并同步' }))
    fireEvent.change(screen.getByLabelText('新技能包名称'), { target: { value: 'Manual Pack' } })
    fireEvent.click(screen.getByRole('button', { name: '确认接管' }))

    if (switchRuntime) {
      await waitFor(() => expect(adopt).toHaveBeenCalledTimes(1))
      await act(async () => {
        useRuntimeEnvironmentStore.getState().selectEnvironment('new-host')
        useSkillStoreV2.setState({ runtimeEnvironmentId: 'new-host', error: 'New host status' })
        finishAdoption(result)
      })
      expect(useSkillStoreV2.getState().error).toBe('New host status')
      expect(screen.queryByText(/批量接管完成/)).not.toBeInTheDocument()
    }
    await waitFor(() => {
      expect(upsertPack).toHaveBeenCalledWith({
        id: '',
        name: 'Manual Pack',
        description: '',
        tags: [],
        skillIds: ['manual-skill'],
      }, { environmentId: LOCAL_RUNTIME_ENVIRONMENT_ID })
    })
  })

  it('closes an unconfirmed batch Skill adoption when the runtime changes', async () => {
    const adopt = vi.spyOn(skillApiV2, 'executeAdoptBatch')
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '未管理 1' }))
    fireEvent.click(screen.getByRole('button', { name: '批量管理' }))
    fireEvent.click(screen.getByText('选择当前可接管'))
    fireEvent.click(screen.getByRole('button', { name: '接管到中心库' }))
    expect(screen.getByRole('button', { name: '确认接管' })).toBeInTheDocument()

    act(() => useRuntimeEnvironmentStore.getState().selectEnvironment('different-host'))

    expect(screen.queryByRole('button', { name: '确认接管' })).not.toBeInTheDocument()
    expect(adopt).not.toHaveBeenCalled()
  })

  it('localizes managed mode, direct claim, and unmanaged reason labels on the agent page', async () => {
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))

    expect(screen.getByText('软连接')).toBeInTheDocument()
    expect(screen.getAllByText('直接分发').length).toBeGreaterThan(0)
    expect(screen.getByText('正常')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '未管理 1' }))
    expect(screen.getByText('未在中心库')).toBeInTheDocument()
    expect(screen.queryByText('link')).not.toBeInTheDocument()
    expect(screen.queryByText('direct')).not.toBeInTheDocument()
    expect(screen.queryByText('ok')).not.toBeInTheDocument()
    expect(screen.queryByText('独立安装')).not.toBeInTheDocument()
    expect(screen.queryByText('not_in_center_library')).not.toBeInTheDocument()
  })

  it('localizes distribution preview mode labels', async () => {
    vi.spyOn(skillApiV2, 'previewDistribute').mockResolvedValueOnce({
      skillIds: ['release-checklist'],
      targetAgents: ['claude-code'],
      requestedMode: 'link',
      changes: [
        {
          skillId: 'release-checklist',
          agentId: 'claude-code',
          action: 'create',
          actualMode: 'link',
          targetPath: '/c/skills/release-checklist',
        },
      ],
      blockers: [],
      blockerDecisions: [],
    })
    const { DistributeDialog } = await import('../components/skills-v2/DistributeDialog')
    render(
      <DistributeDialog
        skill={makeSkill({ installedAgents: [] })}
        agents={[
          { id: 'claude-code', displayName: 'Claude Code', iconKey: 'claude-code', enabled: true, skillsDir: '/c', version: null, latestVersion: null, installed: true, managedSkillCount: 0, unmanagedSkillCount: 0 } as AgentSummary,
        ]}
        defaultMode="link"
        onClose={() => {}}
        onDone={() => {}}
      />,
    )

    expect(screen.getByText('推荐')).toBeInTheDocument()
    expect(screen.getByText('修改同步生效')).toBeInTheDocument()
    expect(screen.queryByText('中心库更新后自动生效')).not.toBeInTheDocument()
    fireEvent.click(screen.getByText('Claude Code'))
    fireEvent.click(screen.getByText('预览影响'))

    expect(await screen.findAllByText('软连接')).toHaveLength(2)
    expect(screen.queryByText('link')).not.toBeInTheDocument()
  })

  it('allows hook-installed agents to be selected even when the skill is already distributed', async () => {
    const previewDistribute = vi.spyOn(skillApiV2, 'previewDistribute').mockResolvedValueOnce({
      skillIds: ['release-checklist'],
      targetAgents: ['codex'],
      requestedMode: 'link',
      changes: [
        {
          skillId: 'release-checklist',
          agentId: 'codex',
          action: 'reinstall',
          actualMode: 'link',
          targetPath: '/codex/release-checklist',
          reason: 'Already managed - will refresh target from the center library.',
        },
      ],
      blockers: [],
      blockerDecisions: [],
    })
    const { DistributeDialog } = await import('../components/skills-v2/DistributeDialog')
    render(
      <DistributeDialog
        skill={makeSkill({
          installedAgents: [
            { agentId: 'codex', displayName: 'Codex', iconKey: 'codex', mode: 'link', status: 'ok' },
          ],
        })}
        agents={[
          { id: 'claude-code', displayName: 'Claude Code', iconKey: 'claude-code', enabled: true, skillsDir: '/c', version: null, latestVersion: null, installed: true, managedSkillCount: 0, unmanagedSkillCount: 0 } as AgentSummary,
          { id: 'codex', displayName: 'Codex', iconKey: 'codex', enabled: true, skillsDir: '/codex', version: null, latestVersion: null, installed: true, managedSkillCount: 0, unmanagedSkillCount: 0 } as AgentSummary,
          { id: 'gemini-cli', displayName: 'Gemini CLI', iconKey: 'gemini-cli', enabled: true, skillsDir: null, version: null, latestVersion: null, installed: false, managedSkillCount: 0, unmanagedSkillCount: 0 } as AgentSummary,
        ]}
        defaultMode="link"
        onClose={() => {}}
        onDone={() => {}}
      />,
    )

    expect(screen.getByText('Claude Code')).toBeInTheDocument()
    expect(screen.getByText('Codex')).toBeInTheDocument()
    expect(screen.queryByText('Gemini CLI')).not.toBeInTheDocument()
    expect(screen.getByText('0/2')).toBeInTheDocument()
    expect(screen.getByText('2 个可选')).toBeInTheDocument()
    expect(screen.getByText('已安装 · 将重新软连接')).toBeInTheDocument()
    expect(Array.from(document.body.querySelectorAll('.sm2-distribute__agent strong')).map((node) => node.textContent)).toEqual([
      'Codex',
      'Claude Code',
    ])

    const codexRow = screen.getByText('Codex').closest('label')
    expect(codexRow?.querySelector('input')).not.toBeDisabled()

    fireEvent.click(screen.getByText('Codex'))
    fireEvent.click(screen.getByText('预览影响'))

    await waitFor(() => {
      expect(previewDistribute).toHaveBeenCalledWith(['release-checklist'], ['codex'], 'link')
    })
  })

  it('localizes distribution change reasons in the confirmation preview', async () => {
    vi.spyOn(skillApiV2, 'previewDistribute').mockResolvedValueOnce({
      skillIds: ['release-checklist'],
      targetAgents: ['codex'],
      requestedMode: 'copy',
      changes: [
        {
          skillId: 'release-checklist',
          agentId: 'codex',
          action: 'convert',
          actualMode: 'copy',
          targetPath: '/codex/release-checklist',
          reason: 'Already managed as link — will convert to copy.',
        },
      ],
      blockers: [],
      blockerDecisions: [],
    })
    const { DistributeDialog } = await import('../components/skills-v2/DistributeDialog')
    render(
      <DistributeDialog
        skill={makeSkill({
          installedAgents: [
            { agentId: 'codex', displayName: 'Codex', iconKey: 'codex', mode: 'link', status: 'ok' },
          ],
        })}
        agents={[
          { id: 'codex', displayName: 'Codex', iconKey: 'codex', enabled: true, skillsDir: '/codex', version: null, latestVersion: null, installed: true, managedSkillCount: 0, unmanagedSkillCount: 0 } as AgentSummary,
        ]}
        defaultMode="copy"
        onClose={() => {}}
        onDone={() => {}}
      />,
    )

    fireEvent.click(screen.getByText('Codex'))
    fireEvent.click(screen.getByText('预览影响'))

    expect(await screen.findByText('已通过软连接管理，将转换为复制。')).toBeInTheDocument()
    expect(screen.queryByText('Already managed as link — will convert to copy.')).not.toBeInTheDocument()
  })

  it('shows busy feedback while executing the final distribution action', async () => {
    const distributionPreview: DistributionPreview = {
      skillIds: ['release-checklist'],
      targetAgents: ['codex'],
      requestedMode: 'link',
      changes: [
        {
          skillId: 'release-checklist',
          agentId: 'codex',
          action: 'reinstall',
          actualMode: 'link',
          targetPath: '/codex/release-checklist',
          reason: 'Already managed - will refresh target from the center library.',
        },
      ],
      blockers: [],
      blockerDecisions: [],
    }
    vi.spyOn(skillApiV2, 'previewDistribute').mockResolvedValueOnce(distributionPreview)
    let resolveExecute: (preview: DistributionPreview) => void = () => {}
    const executePromise = new Promise<typeof distributionPreview>((resolve) => {
      resolveExecute = resolve
    })
    const execute = vi.spyOn(skillApiV2, 'executeDistribute').mockReturnValue(executePromise)
    const { DistributeDialog } = await import('../components/skills-v2/DistributeDialog')
    render(
      <DistributeDialog
        skill={makeSkill({
          installedAgents: [
            { agentId: 'codex', displayName: 'Codex', iconKey: 'codex', mode: 'link', status: 'ok' },
          ],
        })}
        agents={[
          { id: 'codex', displayName: 'Codex', iconKey: 'codex', enabled: true, skillsDir: '/codex', version: null, latestVersion: null, installed: true, managedSkillCount: 0, unmanagedSkillCount: 0 } as AgentSummary,
        ]}
        defaultMode="link"
        onClose={() => {}}
        onDone={() => {}}
      />,
    )

    fireEvent.click(screen.getByText('Codex'))
    fireEvent.click(screen.getByText('预览影响'))
    fireEvent.click(await screen.findByRole('button', { name: '执行分发' }))

    await waitFor(() => expect(execute).toHaveBeenCalledTimes(1))
    const busyButton = screen.getByRole('button', { name: '处理中…' })
    expect(busyButton).toBeDisabled()
    expect(busyButton).toHaveAttribute('aria-busy', 'true')
    expect(busyButton).toHaveAttribute('data-busy', 'true')
    expect(busyButton.querySelector('.sm2__spinner')).not.toBeNull()
    expect(screen.getByText('正在分发 1 个目标')).toBeInTheDocument()
    const progress = screen.getByRole('progressbar', { name: '分发进度' })
    expect(progress).toHaveAttribute('aria-valuemin', '0')
    expect(progress).toHaveAttribute('aria-valuemax', '100')
    expect(progress).toHaveAttribute('aria-valuenow')

    resolveExecute(distributionPreview)
  })

  it('does not offer the shared .agents directory as a distribution target', async () => {
    const { DistributeDialog } = await import('../components/skills-v2/DistributeDialog')
    render(
      <DistributeDialog
        skill={makeSkill({ installedAgents: [] })}
        agents={[
          { id: 'agents', displayName: '.agents', iconKey: 'agents', enabled: true, skillsDir: '/Users/me/.agents/skills', version: null, latestVersion: null, installed: true, managedSkillCount: 0, unmanagedSkillCount: 0 } as AgentSummary,
          { id: 'claude-code', displayName: 'Claude Code', iconKey: 'claude-code', enabled: true, skillsDir: '/c', version: null, latestVersion: null, installed: true, managedSkillCount: 0, unmanagedSkillCount: 0 } as AgentSummary,
        ]}
        defaultMode="link"
        onClose={() => {}}
        onDone={() => {}}
      />,
    )

    expect(screen.queryByText('.agents')).not.toBeInTheDocument()
    expect(screen.getByText('Claude Code')).toBeInTheDocument()
    expect(screen.getByText('1 个可选')).toBeInTheDocument()
  })

  it('marks changed copy targets before redistributing them', async () => {
    const { DistributeDialog } = await import('../components/skills-v2/DistributeDialog')
    render(
      <DistributeDialog
        skill={makeSkill({
          installedAgents: [
            { agentId: 'codex', displayName: 'Codex', iconKey: 'codex', mode: 'copy', status: 'copy_modified' },
          ],
        })}
        agents={[
          { id: 'codex', displayName: 'Codex', iconKey: 'codex', enabled: true, skillsDir: '/codex', version: null, latestVersion: null, installed: true, managedSkillCount: 0, unmanagedSkillCount: 0 } as AgentSummary,
        ]}
        defaultMode="copy"
        onClose={() => {}}
        onDone={() => {}}
      />,
    )

    expect(screen.getByText('已安装 · 副本已修改 · 将重新复制')).toBeInTheDocument()
    expect(screen.getByText('1 个可选')).toBeInTheDocument()
    expect(screen.getByText('Codex').closest('label')?.querySelector('input')).not.toBeDisabled()
  })

  it('allows selecting fully installed agents when switching distribution mode', async () => {
    const previewDistribute = vi.spyOn(skillApiV2, 'previewDistribute').mockResolvedValueOnce({
      skillIds: ['release-checklist', 'db-debug'],
      targetAgents: ['codex'],
      requestedMode: 'link',
      changes: [
        {
          skillId: 'release-checklist',
          agentId: 'codex',
          action: 'convert',
          actualMode: 'link',
          targetPath: '/codex/release-checklist',
        },
      ],
      blockers: [],
      blockerDecisions: [],
    })
    const { DistributeDialog } = await import('../components/skills-v2/DistributeDialog')
    render(
      <DistributeDialog
        skills={[
          makeSkill({
            id: 'release-checklist',
            name: 'Release Checklist',
            installedAgents: [
              { agentId: 'codex', displayName: 'Codex', iconKey: 'codex', mode: 'copy', status: 'ok' },
            ],
          }),
          makeSkill({
            id: 'db-debug',
            name: 'Database Debugging',
            installedAgents: [
              { agentId: 'codex', displayName: 'Codex', iconKey: 'codex', mode: 'copy', status: 'ok' },
            ],
          }),
        ]}
        agents={[
          { id: 'codex', displayName: 'Codex', iconKey: 'codex', enabled: true, skillsDir: '/codex', version: null, latestVersion: null, installed: true, managedSkillCount: 0, unmanagedSkillCount: 0 } as AgentSummary,
        ]}
        defaultMode="link"
        onClose={() => {}}
        onDone={() => {}}
      />,
    )

    expect(screen.getByText('1 个可选')).toBeInTheDocument()
    const codexRow = screen.getByText('Codex').closest('label')
    expect(codexRow?.querySelector('input')).not.toBeDisabled()

    fireEvent.click(screen.getByText('Codex'))
    fireEvent.click(screen.getByText('预览影响'))

    await waitFor(() => {
      expect(previewDistribute).toHaveBeenCalledWith(['release-checklist', 'db-debug'], ['codex'], 'link')
    })
  })

  it('localizes distribution blocker reasons', async () => {
    vi.spyOn(skillApiV2, 'previewDistribute').mockResolvedValueOnce({
      skillIds: ['find-skills'],
      targetAgents: ['claude-code'],
      requestedMode: 'link',
      changes: [],
      blockers: [
        {
          skillId: 'find-skills',
          agentId: 'claude-code',
          reason: "An unmanaged 'find-skills' already exists at the target path. Adopt/overwrite/rename it first.",
          existingPath: '/c/skills/find-skills',
          existingPathKind: 'symlink',
          resolvedExistingPath: '/Users/mac/.skills-manager/skills/find-skills',
        },
      ],
      blockerDecisions: [],
    })
    vi.spyOn(skillApiV2, 'executeDistribute').mockRejectedValueOnce(
      "Target path '/Users/mac/.codex/skills/find-skills' must be a direct child of /Users/mac/.codex/skills.",
    )
    const openPath = vi.spyOn(skillApiV2, 'openPath').mockResolvedValue(undefined)
    const { DistributeDialog } = await import('../components/skills-v2/DistributeDialog')
    render(
      <DistributeDialog
        skill={{ ...makeSkill({ installedAgents: [] }), id: 'find-skills', name: 'find-skills' }}
        agents={[
          { id: 'claude-code', displayName: 'Claude Code', iconKey: 'claude-code', enabled: true, skillsDir: '/c', version: null, latestVersion: null, installed: true, managedSkillCount: 0, unmanagedSkillCount: 0 } as AgentSummary,
        ]}
        defaultMode="link"
        onClose={() => {}}
        onDone={() => {}}
      />,
    )

    fireEvent.click(screen.getByText('Claude Code'))
    fireEvent.click(screen.getByText('预览影响'))

    expect(await screen.findByText('目标路径已存在未管理的 Skill「find-skills」。请选择覆盖安装或忽略此目标。')).toBeInTheDocument()
    expect(screen.queryByText(/An unmanaged/)).not.toBeInTheDocument()
    expect(screen.getAllByText('软连接').length).toBeGreaterThan(1)
    expect(screen.getByText(/真实路径/)).toBeInTheDocument()
    expect(screen.getByText('/Users/mac/.skills-manager/skills/find-skills')).toBeInTheDocument()
    fireEvent.click(screen.getByText('打开文件夹'))
    expect(openPath).toHaveBeenCalledWith('/c/skills/find-skills')

    fireEvent.click(screen.getByText('覆盖安装'))
    fireEvent.click(screen.getByText('按选择执行'))
    expect(await screen.findByText("目标路径 '/Users/mac/.codex/skills/find-skills' 必须直接位于 /Users/mac/.codex/skills 下。")).toBeInTheDocument()
    expect(screen.queryByText(/must be a direct child/)).not.toBeInTheDocument()
  })

  it('shows localized skill target mode labels and explains resolved target paths', async () => {
    vi.spyOn(skillApiV2, 'getSkillDetail').mockResolvedValueOnce({
      ...makeSkill(),
      frontmatter: {},
      files: null,
      targets: agentDetail.skills.map((target) => ({
        ...target,
        resolvedTargetPath: '/center/skills/release-checklist',
      })),
      source: null,
    })
    const openPath = vi.spyOn(skillApiV2, 'openPath').mockResolvedValue(undefined)
    const { SkillDetailSlider } = await import('../components/skills-v2/SkillDetailSlider')
    render(<SkillDetailSlider skillId="release-checklist" open={true} onClose={() => {}} />)

    fireEvent.click(await screen.findByText('Agent (1)'))
    expect(await screen.findByText(/软连接 · 正常/)).toBeInTheDocument()
    expect(screen.getByText('直接分发')).toBeInTheDocument()
    expect(screen.getByText(/打开将跳转到真实路径/)).toBeInTheDocument()
    expect(screen.getByText('/center/skills/release-checklist')).toBeInTheDocument()
    fireEvent.click(screen.getByText('打开'))
    expect(openPath).toHaveBeenCalledWith('/c/skills/release-checklist')
  })

  it('shows a rescan action inside the empty unmanaged skills section', async () => {
    useSkillStoreV2.setState({ unmanaged: [] })
    const scan = vi.spyOn(skillApiV2, 'scanAgentInventory').mockResolvedValue({ agentId: 'claude-code', managed: 1, unmanaged: 1 })
    const scannedItem = makeSharedUnmanaged('scanned-item', { agentId: 'claude-code' })
    const refresh = vi.spyOn(skillApiV2, 'refreshAgentSkillView').mockResolvedValue({
      agentDetail,
      overview: makeOverview(),
      unmanaged: [scannedItem],
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    const { container } = render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (1)'))
    fireEvent.click(screen.getByRole('button', { name: '未管理 0' }))

    const empty = container.querySelector('.sm2__unmanaged-empty')
    expect(empty).not.toBeNull()
    const button = empty?.querySelector('button')
    expect(button).toHaveTextContent('重新扫描此 Agent')

    fireEvent.click(button!)
    await waitFor(() => expect(useSkillStoreV2.getState().unmanaged).toEqual([scannedItem]))
    expect(scan).toHaveBeenCalledExactlyOnceWith('claude-code', LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(refresh).toHaveBeenCalledExactlyOnceWith('claude-code', LOCAL_RUNTIME_ENVIRONMENT_ID)
  })

  it.each(['during scan', 'during inventory read', 'away and back', 'before scan failure'] as const)(
    'keeps the current runtime inventory after switching %s',
    async (scenario) => {
      const counts = { agentId: 'claude-code', managed: 1, unmanaged: 1 }
      let finishScan!: () => void
      let failScan!: (error: Error) => void
      const scanPromise = new Promise<typeof counts>((resolve, reject) => {
        finishScan = () => resolve(counts)
        failScan = reject
      })
      const scan = vi.spyOn(skillApiV2, 'scanAgentInventory').mockReturnValue(scanPromise)
      const previousRuntimeItem = makeSharedUnmanaged('old-scan', { agentId: 'claude-code' })
      let finishInventory!: () => void
      const inventoryPromise = new Promise<AgentSkillViewSnapshot>((resolve) => {
        finishInventory = () => resolve({
          agentDetail,
          overview: makeOverview(),
          unmanaged: [previousRuntimeItem],
        })
      })
      const refresh = vi.spyOn(skillApiV2, 'refreshAgentSkillView').mockReturnValue(inventoryPromise)
      vi.spyOn(skillApiV2, 'overview').mockResolvedValue(makeOverview())
      useSkillStoreV2.setState({ unmanaged: [] })

      const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
      render(<AgentManagementPage />)
      fireEvent.click(screen.getByRole('button', { name: '重新扫描此 Agent' }))
      expect(scan).toHaveBeenCalledTimes(1)
      const waitingForInventory = scenario === 'during inventory read' || scenario === 'away and back'
      if (waitingForInventory) {
        await act(async () => finishScan())
        expect(refresh).toHaveBeenCalledTimes(1)
      }

      const updatedItem = makeSharedUnmanaged('current-runtime-item', { agentId: 'claude-code' })
      await act(async () => {
        const runtime = useRuntimeEnvironmentStore.getState()
        runtime.selectEnvironment('other-host')
        if (scenario === 'away and back') runtime.selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
        const runtimeEnvironmentId = useRuntimeEnvironmentStore.getState().selectedEnvironmentId
        useSkillStoreV2.setState({ runtimeEnvironmentId, unmanaged: [updatedItem], error: 'Current runtime status' })
        if (scenario === 'before scan failure') failScan(new Error('Old scan failed'))
        else finishScan()
        finishInventory()
      })

      expect(useSkillStoreV2.getState().unmanaged).toEqual([updatedItem])
      expect(useSkillStoreV2.getState().error).toBe('Current runtime status')
      expect(scan).toHaveBeenCalledExactlyOnceWith('claude-code', LOCAL_RUNTIME_ENVIRONMENT_ID)
      if (waitingForInventory) expect(refresh).toHaveBeenCalledExactlyOnceWith('claude-code', LOCAL_RUNTIME_ENVIRONMENT_ID)
      else expect(refresh).not.toHaveBeenCalled()
    },
  )

  it('shows immediate busy feedback while preparing to adopt an unmanaged skill', async () => {
    let resolvePreview: (preview: AdoptPreview) => void = () => {}
    const previewPromise = new Promise<AdoptPreview>((resolve) => {
      resolvePreview = resolve
    })
    const preview = vi.spyOn(skillApiV2, 'previewAdopt').mockReturnValue(previewPromise)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '未管理 1' }))

    fireEvent.click(screen.getByRole('button', { name: '接管' }))

    await waitFor(() => expect(preview).toHaveBeenCalledWith('claude-code', 'unmanaged-1'))
    const busyButton = screen.getByRole('button', { name: '准备接管' })
    expect(busyButton).toBeDisabled()
    expect(busyButton).toHaveAttribute('aria-busy', 'true')

    resolvePreview({
      agentId: 'claude-code',
      unmanagedId: 'unmanaged-1',
      skillPath: '/c/skills/manual-skill',
      inferredSkillId: 'manual-skill',
      hash: 'manual-hash',
      centerHasSameId: false,
      canQuickAdopt: true,
      options: [
        { value: 'import_keep', label: 'Import to center, keep agent file as-is', destructive: false },
      ],
    })
  })

  it('hides unmanaged skills when the setting is disabled', async () => {
    const hiddenOverview = makeOverview()
    hiddenOverview.settings.showUnmanaged = false
    useSkillStoreV2.setState({
      settings: hiddenOverview.settings,
      unmanaged: [
        {
          id: 'unmanaged-hidden',
          agentId: 'claude-code',
          itemType: 'skill',
          path: '/c/skills/hidden-skill',
          inferredSkillId: 'hidden-skill',
          hash: null,
          reason: 'not_in_center_library',
        },
      ],
    })
    vi.spyOn(skillApiV2, 'overview').mockResolvedValue(hiddenOverview)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(screen.getByText('Skills (1)'))
    expect(screen.queryByText('未管理 Skills')).not.toBeInTheDocument()
    expect(screen.queryByText('hidden-skill')).not.toBeInTheDocument()
  })

  it('renders MCP, plugin, config, and health details for the selected agent', async () => {
    useSkillStoreV2.setState({
      selectedAgentDetail: {
        ...agentDetail,
        mcpServers: [
          { name: 'filesystem', command: 'npx', args: ['-y', '@modelcontextprotocol/server-filesystem'], valid: true, message: 'configured' },
          { name: 'broken', command: '', args: [], valid: false, message: 'missing command' },
        ],
        plugins: [
          { id: 'reviewer', name: 'Reviewer Tools', version: '1.2.3', enabled: true, source: 'claude-plugin' },
        ],
        health: [
          { kind: 'skills_dir_missing', message: 'Skills directory does not exist: /c/skills', severity: 'warning' },
        ],
      },
    })
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    expect(screen.getByText('Skills directory does not exist: /c/skills')).toBeInTheDocument()
    fireEvent.click(screen.getByText('MCP (2)'))
    expect(screen.getByText('filesystem')).toBeInTheDocument()
    expect(screen.getByText('broken')).toBeInTheDocument()
    expect(screen.getByText('missing command')).toBeInTheDocument()

    fireEvent.click(screen.getByText('Plugins (1)'))
    expect(screen.getByText('Reviewer Tools')).toBeInTheDocument()
    expect(screen.getByText('claude-plugin · v1.2.3')).toBeInTheDocument()

    fireEvent.click(screen.getByText('路径与设置'))
    expect(screen.getByText('/c')).toBeInTheDocument()
    expect(screen.getByText('/c/config.json')).toBeInTheDocument()
    expect(screen.getByText('/c/mcp.json')).toBeInTheDocument()
  })

  it('shows live program versions and config paths in the config tab', async () => {
    const liveDetail: AgentDetail = {
      ...agentDetail,
      version: null,
      latestVersion: null,
      configPath: '/Users/me/.claude/settings.json',
      pluginDir: '/Users/me/.claude/plugins/cache',
    }
    useSkillStoreV2.setState({
      selectedAgentDetail: liveDetail,
      selectedAgentId: 'claude-code',
    })
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([
      makeProgram({ installedVersion: '2.1.179', latestVersion: '2.1.179' }),
    ])
    vi.spyOn(skillApiV2, 'overview').mockResolvedValue(makeOverview())
    vi.spyOn(skillApiV2, 'getAgentDetail').mockResolvedValue(liveDetail)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    const { container } = render(<AgentManagementPage />)

    fireEvent.click(screen.getByText('路径与设置'))
    await waitFor(() => expect(container.querySelectorAll('.sm2__config-facts > div')[1]).toHaveTextContent('2.1.179'))
    expect(screen.getByText('/Users/me/.claude/settings.json')).toBeInTheDocument()
    expect(screen.getByText('/Users/me/.claude/plugins/cache')).toBeInTheDocument()
  })

  it('formats, validates, edits, and reveals an Agent config resource', async () => {
    const readConfig = vi.spyOn(skillApiV2, 'readAgentConfigFile').mockResolvedValue({
      path: '/c/config.json',
      content: '{"enabled":false,"threshold":9007199254740993}',
      revision: 'sha256:before',
    })
    const writeConfig = vi.spyOn(skillApiV2, 'writeAgentConfigFile').mockResolvedValue({
      path: '/c/config.json',
      content: '{\n  "enabled": true\n}\n',
      revision: 'sha256:after',
    })
    const revealPath = vi.spyOn(skillApiV2, 'revealPath').mockResolvedValue(undefined)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(screen.getByText('路径与设置'))
    const configRow = screen.getByText('配置文件').closest('.sm2__config-resource')
    expect(configRow).not.toBeNull()

    fireEvent.click(within(configRow as HTMLElement).getByRole('button', { name: '编辑' }))
    const editor = await screen.findByLabelText('编辑配置文件内容')
    expect(readConfig).toHaveBeenCalledWith('claude-code', '/c/config.json', LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(editor).toHaveValue('{\n  "enabled": false,\n  "threshold": 9007199254740993\n}\n')
    expect(screen.getByText('JSON 语法正确')).toBeInTheDocument()
    expect(within(configRow as HTMLElement).queryByRole('button', { name: /复制/ })).not.toBeInTheDocument()

    fireEvent.change(editor, { target: { value: '{"enabled":' } })
    expect(screen.getByText('JSON 语法有误')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '保存更改' })).toBeDisabled()

    fireEvent.change(editor, { target: { value: '{"enabled":true}' } })
    fireEvent.click(screen.getByRole('button', { name: '格式化 JSON' }))
    expect(editor).toHaveValue('{\n  "enabled": true\n}\n')
    fireEvent.click(screen.getByRole('button', { name: '保存更改' }))
    const dialog = screen.getByRole('dialog')
    expect(await within(dialog).findByText('配置已保存，原文件已备份')).toBeInTheDocument()
    expect(writeConfig).toHaveBeenCalledWith(
      'claude-code',
      '/c/config.json',
      '{\n  "enabled": true\n}\n',
      'sha256:before',
      LOCAL_RUNTIME_ENVIRONMENT_ID,
    )
    fireEvent.click(within(dialog).getByRole('button', { name: '关闭' }))

    fireEvent.click(within(configRow as HTMLElement).getByRole('button', { name: '定位' }))
    await waitFor(() => expect(revealPath).toHaveBeenCalledWith('/c/config.json'))
  })

  it('keeps Agent config edits typed during a save and saves them with the new revision', async () => {
    const path = '/c/config.json'
    vi.spyOn(skillApiV2, 'readAgentConfigFile').mockResolvedValue({ path, content: '{"count":0}', revision: 'initial' })
    let saved!: (document: { path: string; content: string; revision: string }) => void
    const write = vi.spyOn(skillApiV2, 'writeAgentConfigFile')
      .mockReturnValueOnce(new Promise((resolve) => { saved = resolve }))
      .mockResolvedValueOnce({ path, content: '{"count":2}', revision: 'second' })
      .mockRejectedValueOnce('Configuration write failed temporarily')
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('路径与设置'))
    const row = screen.getByText('配置文件').closest('.sm2__config-resource') as HTMLElement
    fireEvent.click(within(row).getByRole('button', { name: '编辑' }))
    const editor = await screen.findByLabelText('编辑配置文件内容')

    fireEvent.change(editor, { target: { value: '{"count":1}' } })
    fireEvent.click(screen.getByRole('button', { name: '保存更改' }))
    fireEvent.change(editor, { target: { value: '{"count":2}' } })
    await act(async () => saved({ path, content: '{"count":1}', revision: 'first' }))
    expect(editor).toHaveValue('{"count":2}')
    const save = screen.getByRole('button', { name: '保存更改' })
    expect(save).toBeEnabled()
    fireEvent.click(save)
    await waitFor(() => expect(write).toHaveBeenNthCalledWith(
      2, 'claude-code', path, '{"count":2}', 'first', LOCAL_RUNTIME_ENVIRONMENT_ID,
    ))
    await within(screen.getByRole('dialog')).findByText('配置已保存，原文件已备份')

    fireEvent.change(editor, { target: { value: '{"count":3}' } })
    fireEvent.click(save)
    expect(await within(screen.getByRole('dialog')).findByRole('alert')).toHaveTextContent('Configuration write failed temporarily')
    expect(editor).toHaveValue('{"count":3}')
    expect(save).toBeEnabled()
  })

  it('ignores an Agent config save response after changing environments', async () => {
    const path = '/c/config.json'
    vi.spyOn(skillApiV2, 'readAgentConfigFile').mockResolvedValue({ path, content: '{"enabled":false}', revision: 'initial' })
    let saved!: (document: { path: string; content: string; revision: string }) => void
    const write = vi.spyOn(skillApiV2, 'writeAgentConfigFile')
      .mockReturnValueOnce(new Promise((resolve) => { saved = resolve }))
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('路径与设置'))
    const row = screen.getByText('配置文件').closest('.sm2__config-resource') as HTMLElement
    fireEvent.click(within(row).getByRole('button', { name: '编辑' }))
    const editor = await screen.findByLabelText('编辑配置文件内容')
    fireEvent.change(editor, { target: { value: '{"enabled":true}' } })
    fireEvent.click(screen.getByRole('button', { name: '保存更改' }))

    const { selectEnvironment } = useRuntimeEnvironmentStore.getState()
    act(() => {
      selectEnvironment('different-host')
      selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    })
    await act(async () => saved({ path, content: '{"enabled":true}', revision: 'saved' }))
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
    expect(screen.queryByText('配置已保存，原文件已备份')).not.toBeInTheDocument()
    expect(write).toHaveBeenCalledTimes(1)
  })

  it('shows hook bridge command details and open actions', async () => {
    vi.spyOn(tauriApi, 'getAllHookStatus').mockResolvedValue([
      {
        toolId: 'claude-code',
        adapterId: 'claude-code',
        profileId: 'claude-code',
        name: 'claude-code',
        displayName: 'Claude Code',
        installed: true,
        installStatus: 'installed',
        configPath: '/Users/me/.claude/settings.json',
        configDir: '/Users/me/.claude',
        status: 'Installed',
        supportsEventSelection: true,
        bridgeCommand: '/Users/me/Library/Application Support/com.vibetunnel.agentbro/agentbro-bridge --source claude-code',
        bridgePath: '/Users/me/Library/Application Support/com.vibetunnel.agentbro/agentbro-bridge',
        events: [
          {
            name: 'PreToolUse',
            category: 'approvals',
            categoryTitle: '审批',
            categorySubtitle: '工具调用审批与权限请求，可能需要用户回应',
            timeout: 5,
            enabled: true,
          },
        ],
        enabledEventNames: ['PreToolUse'],
      },
    ])
    const openPath = vi.spyOn(skillApiV2, 'openPath').mockResolvedValue(undefined)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(screen.getByText('Hooks'))
    expect(await screen.findByText('桥接命令')).toBeInTheDocument()
    expect(screen.getByText(/agentbro-bridge --source claude-code/)).toBeInTheDocument()

    fireEvent.click(screen.getByText('打开配置'))
    expect(openPath).toHaveBeenCalledWith('/Users/me/.claude/settings.json')
    fireEvent.click(screen.getByText('打开脚本'))
    expect(openPath).toHaveBeenCalledWith('/Users/me/Library/Application Support/com.vibetunnel.agentbro/agentbro-bridge')
  })

  it('does not read the local Hook settings from Agent Management on an SSH host', async () => {
    const localStatus = vi.spyOn(tauriApi, 'getAllHookStatus').mockResolvedValue([{
      toolId: 'claude-code',
      name: 'claude-code',
      displayName: 'Local Claude',
      installed: true,
      status: 'Installed',
      configPath: '/local-machine/claude/settings.json',
    }])
    useRuntimeEnvironmentStore.getState().selectEnvironment('build-host')
    useSkillStoreV2.setState({ runtimeEnvironmentId: 'build-host' })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByRole('button', { name: 'Hooks' }))
    await act(async () => {})

    expect(localStatus).not.toHaveBeenCalled()
    expect(screen.queryByText('/local-machine/claude/settings.json')).not.toBeInTheDocument()
  })

  it('installs and removes a Hook on the SSH host shown in Agent Management', async () => {
    const remoteStatus = vi.spyOn(agentApi, 'remoteHookStatus')
      .mockResolvedValueOnce('notInstalled')
      .mockResolvedValueOnce('installed')
      .mockResolvedValueOnce('notInstalled')
    const install = vi.spyOn(agentApi, 'installHook').mockResolvedValue()
    const uninstall = vi.spyOn(agentApi, 'uninstallHook').mockResolvedValue()
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true)
    useRuntimeEnvironmentStore.getState().selectEnvironment('build-host')
    useSkillStoreV2.setState({ runtimeEnvironmentId: 'build-host' })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByRole('button', { name: 'Hooks' }))
    expect(await screen.findByText(i18n.t('skills.runtimeEnvironment.currentLabel', { name: 'build-host' }))).toBeInTheDocument()
    fireEvent.click(await screen.findByRole('button', { name: i18n.t('settings.installHooks') }))
    expect(install).toHaveBeenCalledExactlyOnceWith('claude-code', 'build-host')
    fireEvent.click(await screen.findByRole('button', { name: i18n.t('settings.uninstallHooks') }))
    expect(confirm).toHaveBeenCalledWith(expect.stringContaining('build-host'))
    expect(uninstall).toHaveBeenCalledExactlyOnceWith('claude-code', 'build-host')
    expect(await screen.findByRole('button', { name: i18n.t('settings.installHooks') })).toBeEnabled()
    expect(remoteStatus.mock.calls).toEqual(Array.from({ length: 3 }, () => ['claude-code', 'build-host']))
  })

  it('does not reload Hook status for an abandoned SSH host after install finishes', async () => {
    const remoteStatus = vi.spyOn(agentApi, 'remoteHookStatus')
      .mockResolvedValueOnce('needsReinstall')
      .mockResolvedValueOnce('settingsCorrupted')
    let finishInstall!: () => void
    const install = vi.spyOn(agentApi, 'installHook').mockReturnValue(new Promise((resolve) => { finishInstall = resolve }))
    useRuntimeEnvironmentStore.getState().selectEnvironment('build-host')
    useSkillStoreV2.setState({ runtimeEnvironmentId: 'build-host' })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByRole('button', { name: 'Hooks' }))
    fireEvent.click(await screen.findByRole('button', { name: i18n.t('settings.reinstall') }))
    expect(install).toHaveBeenCalledExactlyOnceWith('claude-code', 'build-host')

    await act(async () => {
      useRuntimeEnvironmentStore.getState().selectEnvironment('gpu-host')
      useSkillStoreV2.setState({ runtimeEnvironmentId: 'gpu-host' })
    })
    expect(await screen.findByText(i18n.t('settings.hookSettingsCorrupted'))).toBeInTheDocument()
    await act(async () => finishInstall())

    expect(remoteStatus.mock.calls).toEqual([['claude-code', 'build-host'], ['claude-code', 'gpu-host']])
    expect(screen.getByText(i18n.t('settings.hookSettingsCorrupted'))).toBeInTheDocument()
  })

  it('shows SSH connection errors without marking the remote Hook uninstalled', async () => {
    vi.spyOn(agentApi, 'remoteHookStatus').mockRejectedValue(new Error('SSH connection refused'))
    useRuntimeEnvironmentStore.getState().selectEnvironment('offline-host')
    useSkillStoreV2.setState({ runtimeEnvironmentId: 'offline-host' })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByRole('button', { name: 'Hooks' }))

    expect(await screen.findByRole('alert')).toHaveTextContent('SSH connection refused')
    expect(screen.queryByRole('button', { name: i18n.t('settings.installHooks') })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: i18n.t('settings.detectNow') })).toBeEnabled()
  })

  it('routes custom Claude Code hook actions through its engine instance', async () => {
    const customDetail: AgentDetail = {
      ...agentDetail,
      id: 'custom-codefuse',
      displayName: 'CodeFuse Claude Code',
      skillsDir: '/Users/me/.codefuse/engine/cc/skills',
      configPath: '/Users/me/.codefuse/engine/cc/settings.json',
      mcpConfigPath: '/Users/me/.codefuse/engine/cc/settings.json',
      pluginDir: '/Users/me/.codefuse/engine/cc/plugins/cache',
      skills: [],
    }
    useSkillStoreV2.setState({
      selectedAgentId: customDetail.id,
      selectedAgentDetail: customDetail,
      agents: [
        {
          id: customDetail.id,
          displayName: customDetail.displayName,
          iconKey: 'claude-code',
          enabled: true,
          skillsDir: customDetail.skillsDir,
          version: null,
          latestVersion: null,
          installed: true,
          managedSkillCount: 0,
          unmanagedSkillCount: 0,
        } as AgentSummary,
      ],
      unmanaged: [],
    })
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([
      makeProgram({
        id: customDetail.id,
        displayName: customDetail.displayName,
        icon: 'claude-code',
        packageManager: 'custom',
        configDir: '/Users/me/.codefuse/engine/cc',
        skillsDir: customDetail.skillsDir,
        isCustom: true,
      }),
    ])
    vi.spyOn(tauriApi, 'getAllHookStatus').mockResolvedValue([
      {
        toolId: 'engine:custom-codefuse',
        adapterId: 'claude-code',
        profileId: 'claude-code',
        name: 'claude-code',
        displayName: customDetail.displayName,
        installed: true,
        installStatus: 'installed',
        configPath: customDetail.configPath!,
        configDir: '/Users/me/.codefuse/engine/cc',
        status: 'Available',
        isCustom: true,
        customId: customDetail.id,
      },
    ])
    const uninstallHook = vi.spyOn(tauriApi, 'uninstallAgentHook').mockResolvedValue(undefined)
    vi.spyOn(window, 'confirm').mockReturnValue(true)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(screen.getByText('Hooks'))
    expect(await screen.findByText(customDetail.configPath!)).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '卸载 Hook' }))

    await waitFor(() => expect(uninstallHook).toHaveBeenCalledWith('engine:custom-codefuse'))
  })

  it.each([
    { id: 'cursor-cli', name: 'Cursor CLI', otherTool: 'cursor', otherAdapter: 'cursor', configPath: '~/.cursor/settings.json' },
    { id: 'claude-code', name: 'Claude Code', otherTool: 'engine:team', otherAdapter: 'claude-code', configPath: '~/.claude/settings.json' },
  ])('uses the exact $name Hook profile even when another entry appears first', async ({ id, name, otherTool, otherAdapter, configPath }) => {
    const agent = makeSidebarAgent(id, name)
    useSkillStoreV2.setState({
      agents: [agent],
      selectedAgentId: id,
      selectedAgentDetail: { ...agentDetail, ...agent, configPath },
    })
    vi.spyOn(tauriApi, 'getAllHookStatus').mockResolvedValue([
      {
        toolId: otherTool,
        adapterId: otherAdapter,
        name: otherAdapter,
        displayName: 'Other profile',
        installed: true,
        configPath,
        status: 'Installed',
        isCustom: otherTool.startsWith('engine:'),
        customId: otherTool.startsWith('engine:') ? 'team' : undefined,
      },
      {
        toolId: id,
        adapterId: id,
        name: id,
        displayName: name,
        installed: true,
        configPath,
        status: 'Installed',
      },
    ])
    const uninstall = vi.spyOn(tauriApi, 'uninstallAgentHook').mockResolvedValue()
    vi.spyOn(window, 'confirm').mockReturnValue(true)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByRole('button', { name: 'Hooks' }))
    const button = await screen.findByRole('button', { name: '卸载 Hook' })
    await act(async () => fireEvent.click(button))

    expect(uninstall).toHaveBeenCalledExactlyOnceWith(id)
  })

  it('does not show a sibling built-in Hook when the selected profile has no status', async () => {
    const agent = makeSidebarAgent('cursor-cli', 'Cursor CLI')
    const configPath = '~/.cursor/settings.json'
    useSkillStoreV2.setState({
      agents: [agent],
      selectedAgentId: agent.id,
      selectedAgentDetail: { ...agentDetail, ...agent, configPath },
    })
    vi.spyOn(tauriApi, 'getAllHookStatus').mockResolvedValue([{
      toolId: 'cursor',
      adapterId: 'cursor',
      name: 'cursor',
      displayName: 'Cursor',
      installed: true,
      configPath,
      status: 'Installed',
    }])

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByRole('button', { name: 'Hooks' }))

    expect(await screen.findByText('该 Agent 暂不支持 Hook 接入')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '卸载 Hook' })).not.toBeInTheDocument()
  })

  it('resolves the custom Hook for the same config file with an expanded home path', async () => {
    const agent = makeSidebarAgent('custom-registered', 'Registered Agent')
    useSkillStoreV2.setState({
      agents: [agent],
      selectedAgentId: agent.id,
      selectedAgentDetail: { ...agentDetail, ...agent, configPath: '/Profiles/owner/registered/settings.json' },
    })
    vi.spyOn(tauriApi, 'getAllHookStatus').mockResolvedValue([
      {
        toolId: 'custom:other', customId: 'other', name: 'claude-code', displayName: agent.displayName,
        installed: true, status: 'Installed', isCustom: true, configPath: '~/other/settings.json',
      },
      {
        toolId: 'custom:matching', customId: 'matching', name: 'claude-code', displayName: 'Matching',
        installed: true, status: 'Installed', isCustom: true, configPath: '~/registered/settings.json',
      },
    ])
    const uninstall = vi.spyOn(tauriApi, 'uninstallAgentHook').mockResolvedValue()
    vi.spyOn(window, 'confirm').mockReturnValue(true)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByRole('button', { name: 'Hooks' }))
    const button = await screen.findByRole('button', { name: '卸载 Hook' })
    await act(async () => fireEvent.click(button))

    expect(uninstall).toHaveBeenCalledExactlyOnceWith('custom:matching')
  })

  it('uses the install action when the selected agent is not installed', async () => {
    const codexDetail: AgentDetail = {
      ...agentDetail,
      id: 'codex',
      displayName: 'Codex',
      iconKey: 'codex',
      version: null,
      latestVersion: null,
      skillsDir: '/codex',
      skills: [],
    }
    useSkillStoreV2.setState({
      selectedAgentId: 'codex',
      selectedAgentDetail: codexDetail,
      agents: [
        { id: 'codex', displayName: 'Codex', iconKey: 'codex', enabled: true, skillsDir: '/codex', version: null, latestVersion: null, installed: false, managedSkillCount: 0, unmanagedSkillCount: 0 } as AgentSummary,
      ],
      unmanaged: [],
    })
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([makeProgram({ id: 'codex', status: 'notInstalled', installCommand: 'npm install -g @openai/codex' })])
    const install = vi.spyOn(agentApi, 'install').mockResolvedValue(undefined)
    vi.spyOn(skillApiV2, 'overview').mockResolvedValue(makeOverview())
    vi.spyOn(skillApiV2, 'getAgentDetail').mockResolvedValue(codexDetail)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(await screen.findByText('安装此 Agent'))
    await waitFor(() => expect(install).toHaveBeenCalledWith('codex', LOCAL_RUNTIME_ENVIRONMENT_ID))
  })

  it('opens the download page when an uninstalled app has no install command', async () => {
    const cursorDetail: AgentDetail = {
      ...agentDetail,
      id: 'cursor',
      displayName: 'Cursor',
      iconKey: 'cursor',
      version: null,
      latestVersion: null,
      skillsDir: '/cursor',
      skills: [],
    }
    useSkillStoreV2.setState({
      selectedAgentId: 'cursor',
      selectedAgentDetail: cursorDetail,
      agents: [
        { id: 'cursor', displayName: 'Cursor', iconKey: 'cursor', enabled: true, skillsDir: '/cursor', version: null, latestVersion: null, installed: false, managedSkillCount: 0, unmanagedSkillCount: 0 } as AgentSummary,
      ],
      unmanaged: [],
    })
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([
      makeProgram({
        id: 'cursor',
        displayName: 'Cursor',
        icon: 'cursor',
        kind: 'app',
        status: 'notInstalled',
        installCommand: null,
        downloadUrl: 'https://cursor.com',
      }),
    ])
    const openDownload = vi.spyOn(agentApi, 'openDownload').mockResolvedValue(undefined)
    vi.spyOn(skillApiV2, 'overview').mockResolvedValue(makeOverview())
    vi.spyOn(skillApiV2, 'getAgentDetail').mockResolvedValue(cursorDetail)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    const { container } = render(<AgentManagementPage />)

    const primaryButton = await waitFor(() => {
      const button = container.querySelector<HTMLButtonElement>('.sm2__agent-hero .sm2__btn--primary')
      expect(button).toBeTruthy()
      expect(button).toBeEnabled()
      return button!
    })
    fireEvent.click(primaryButton)

    await waitFor(() => expect(openDownload).toHaveBeenCalledWith('cursor'))
  })

  it('keeps an agent uninstalled when program metadata is missing', async () => {
    const cursorDetail: AgentDetail = {
      ...agentDetail,
      id: 'cursor',
      displayName: 'Cursor',
      iconKey: 'cursor',
      version: null,
      latestVersion: null,
      skillsDir: '/cursor',
      skills: [],
    }
    useSkillStoreV2.setState({
      selectedAgentId: 'cursor',
      selectedAgentDetail: cursorDetail,
      agents: [
        { id: 'cursor', displayName: 'Cursor', iconKey: 'cursor', enabled: true, skillsDir: '/cursor', version: null, latestVersion: null, installed: false, managedSkillCount: 0, unmanagedSkillCount: 0 } as AgentSummary,
      ],
      unmanaged: [],
    })
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([])
    vi.spyOn(skillApiV2, 'overview').mockResolvedValue(makeOverview())
    vi.spyOn(skillApiV2, 'getAgentDetail').mockResolvedValue(cursorDetail)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    const { container } = render(<AgentManagementPage />)

    await waitFor(() => {
      expect(container.querySelector('.sm2__agent-version-pill')).toHaveTextContent('未安装')
    })
    const primaryButton = container.querySelector<HTMLButtonElement>('.sm2__agent-hero .sm2__btn--primary')
    expect(primaryButton).toHaveTextContent('打开安装页')
    expect(primaryButton).toBeDisabled()
  })

  it('rescans Agent installation state when refreshing the overview', async () => {
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([makeProgram()])
    const refresh = vi.spyOn(useSkillStoreV2.getState(), 'refresh').mockResolvedValue(undefined)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(await screen.findByRole('button', { name: '刷新总览' }))
    await waitFor(() => expect(refresh).toHaveBeenCalled())
  })

  it('uses the update action when a newer agent version is available', async () => {
    const claudeDetail: AgentDetail = {
      ...agentDetail,
      version: '1.0.0',
      latestVersion: '1.1.0',
    }
    useSkillStoreV2.setState({
      selectedAgentId: 'claude-code',
      selectedAgentDetail: claudeDetail,
      agents: [
        { id: 'claude-code', displayName: 'Claude Code', iconKey: 'claude-code', enabled: true, skillsDir: '/c', version: '1.0.0', latestVersion: '1.1.0', installed: true, managedSkillCount: 1, unmanagedSkillCount: 0 } as AgentSummary,
      ],
      unmanaged: [],
    })
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([makeProgram({ status: 'updateAvailable', installedVersion: '1.0.0', latestVersion: '1.1.0' })])
    const update = vi.spyOn(agentApi, 'update').mockResolvedValue(undefined)
    vi.spyOn(skillApiV2, 'overview').mockResolvedValue(makeOverview())
    vi.spyOn(skillApiV2, 'getAgentDetail').mockResolvedValue(claudeDetail)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(await screen.findByText('更新此 Agent'))
    await waitFor(() => expect(update).toHaveBeenCalledWith('claude-code', LOCAL_RUNTIME_ENVIRONMENT_ID))
  })

  it('uninstalls a supported agent from the page header after confirmation', async () => {
    useSkillStoreV2.setState({ unmanaged: [] })
    vi.spyOn(agentApi, 'refresh')
      .mockResolvedValueOnce([makeProgram()])
      .mockResolvedValue([makeProgram({ status: 'notInstalled', binaryPath: null })])
    const uninstall = vi.spyOn(agentApi, 'uninstall').mockResolvedValue(undefined)
    vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    vi.spyOn(skillApiV2, 'overview').mockResolvedValue(makeOverview())
    vi.spyOn(skillApiV2, 'getAgentDetail').mockResolvedValue(agentDetail)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(await screen.findByRole('button', { name: '卸载 Agent' }))
    expect(screen.getByRole('dialog', { name: '卸载 Agent「Claude Code」' })).toBeInTheDocument()
    expect(screen.getByText('npm uninstall -g @anthropic-ai/claude-code')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '确认卸载' }))

    await waitFor(() => expect(uninstall).toHaveBeenCalledWith('claude-code', LOCAL_RUNTIME_ENVIRONMENT_ID))
    expect(await screen.findByText('Agent「Claude Code」已卸载，已清理 1 个 Skills')).toBeInTheDocument()
  })

  it('keeps an uninstall limited to the Skills shown before another Skill was discovered', async () => {
    const reviewed = makeSharedUnmanaged('reviewed', { agentId: 'claude-code' })
    const discovered = makeSharedUnmanaged('discovered', { agentId: 'claude-code' })
    useSkillStoreV2.setState({ unmanaged: [reviewed] })
    vi.spyOn(agentApi, 'refresh')
      .mockResolvedValueOnce([makeProgram()])
      .mockResolvedValue([makeProgram({ status: 'notInstalled', binaryPath: null })])
    vi.spyOn(agentApi, 'uninstall').mockResolvedValue(undefined)
    vi.spyOn(skillApiV2, 'deleteSkillTargetDistributions').mockResolvedValue({ deleted: 1, failures: [] })
    const removeUnmanaged = vi.spyOn(skillApiV2, 'deleteUnmanagedAgentSkill').mockResolvedValue(undefined)
    vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([discovered])
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockImplementation(async (force) => {
      if (force) useSkillStoreV2.setState({ agents: [makeSidebarAgent('claude-code', 'Claude Code', { unmanaged: 1 })] })
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(await screen.findByRole('button', { name: '卸载 Agent' }))
    act(() => useSkillStoreV2.setState({ unmanaged: [reviewed, discovered] }))
    const reviewedCount = within(screen.getByRole('dialog')).getByText('未管理 Skills').parentElement?.textContent
    fireEvent.click(screen.getByRole('button', { name: '确认卸载' }))

    expect(await screen.findByText(/Agent「Claude Code」已卸载/)).toBeInTheDocument()
    expect(removeUnmanaged).toHaveBeenCalledExactlyOnceWith('claude-code', reviewed.id, LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(reviewedCount).toBe('1未管理 Skills')
    expect(useSkillStoreV2.getState().unmanaged).toEqual([discovered])
    expect(useSkillStoreV2.getState().agents[0]?.unmanagedSkillCount).toBe(1)
  })

  it.each([
    { change: 'a new installation', reviewed: makeProgram({ status: 'notInstalled', binaryPath: null }), current: makeProgram() },
    { change: 'a replaced launcher', reviewed: makeProgram({ binaryPath: '/previous/claude' }), current: makeProgram() },
    { change: 'an additional Hook', reviewed: makeProgram(), current: makeProgram({ hooksInstalled: true }) },
  ])('requires a fresh cleanup review after detecting $change', async ({ reviewed, current }) => {
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([reviewed])
    const uninstall = vi.spyOn(agentApi, 'uninstall').mockResolvedValue(undefined)
    const removeManaged = vi.spyOn(skillApiV2, 'deleteSkillTargetDistributions').mockResolvedValue({ deleted: 1, failures: [] })
    const removeUnmanaged = vi.spyOn(skillApiV2, 'deleteUnmanagedAgentSkill').mockResolvedValue(undefined)
    const uninstallHook = vi.spyOn(agentApi, 'uninstallHook').mockResolvedValue(undefined)
    vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue()
    vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue()

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    const cleanupOnly = reviewed.status === 'notInstalled'
    fireEvent.click(await screen.findByRole('button', { name: cleanupOnly ? '清理残留' : '卸载 Agent' }))
    act(() => useSkillStoreV2.setState({
      agentProgramSnapshot: {
        environmentId: LOCAL_RUNTIME_ENVIRONMENT_ID,
        selectionRevision: useRuntimeEnvironmentStore.getState().selectionRevision,
        programs: { [current.id]: current },
      },
    }))
    fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: /确认(卸载|清理)/ }))

    expect(uninstall).not.toHaveBeenCalled()
    expect(removeManaged).not.toHaveBeenCalled()
    expect(removeUnmanaged).not.toHaveBeenCalled()
    expect(uninstallHook).not.toHaveBeenCalled()
    expect(await screen.findByRole('alert')).toHaveTextContent('Claude Code 的程序或 Hook 状态已变化')
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '卸载 Agent' })).toBeEnabled()
  })

  it('keeps Agent resources when the native package command is already busy', async () => {
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([makeProgram({ hooksInstalled: true })])
    const uninstall = vi.spyOn(agentApi, 'uninstall').mockRejectedValue('agent_package_operation_in_progress')
    const removeManaged = vi.spyOn(skillApiV2, 'deleteSkillTargetDistributions').mockResolvedValue({ deleted: 1, failures: [] })
    const removeUnmanaged = vi.spyOn(skillApiV2, 'deleteUnmanagedAgentSkill').mockResolvedValue(undefined)
    const uninstallHook = vi.spyOn(agentApi, 'uninstallHook').mockResolvedValue(undefined)
    vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue()
    vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue()

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(await screen.findByRole('button', { name: '卸载 Agent' }))
    fireEvent.click(screen.getByRole('button', { name: '确认卸载' }))

    const alert = await screen.findByRole('alert')
    expect(uninstall).toHaveBeenCalledExactlyOnceWith('claude-code', LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(removeManaged).not.toHaveBeenCalled()
    expect(removeUnmanaged).not.toHaveBeenCalled()
    expect(uninstallHook).not.toHaveBeenCalled()
    expect(alert).toHaveTextContent('另一个 Agent 程序操作正在运行')
    expect(screen.getByRole('button', { name: '卸载 Agent' })).toBeEnabled()
  })

  it('cleans AgentBro resources while keeping an installed app without an uninstall command', async () => {
    useSkillStoreV2.setState({
      unmanaged: [makeSharedUnmanaged('builtin', { agentId: 'claude-code', readOnly: true })],
    })
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([makeProgram({ kind: 'app', uninstallCommand: null })])
    const uninstall = vi.spyOn(agentApi, 'uninstall').mockResolvedValue(undefined)
    const remove = vi.spyOn(skillApiV2, 'deleteSkillTargetDistributions').mockResolvedValue({ deleted: 1, failures: [] })
    const removeUnmanaged = vi.spyOn(skillApiV2, 'deleteUnmanagedAgentSkill').mockResolvedValue(undefined)
    vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue()
    const loadDetail = vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue()
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(await screen.findByRole('button', { name: '清理残留' }))
    expect(screen.getByRole('dialog')).not.toHaveTextContent('未管理 Skills')
    fireEvent.click(screen.getByRole('button', { name: '确认清理' }))

    expect(await screen.findByText(/已清理 Claude Code 的 AgentBro 资源/)).toHaveTextContent('已清理 1 个 Skills')
    expect(uninstall).not.toHaveBeenCalled()
    expect(removeUnmanaged).not.toHaveBeenCalled()
    expect(remove).toHaveBeenCalledExactlyOnceWith(agentDetail.skills.map((skill) => skill.id), LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(loadDetail).toHaveBeenCalledWith('claude-code', true)
    expect(useSkillStoreV2.getState().agents.find((agent) => agent.id === 'claude-code')?.installed).toBe(true)
    expect(useSkillStoreV2.getState().selectedAgentId).toBe('claude-code')
  })

  it.each([true, false])('keeps the Agent selected without verified removal, detected=%s', async (detected) => {
    const refresh = vi.spyOn(agentApi, 'refresh').mockResolvedValueOnce([makeProgram()])
    if (detected) refresh.mockResolvedValueOnce([makeProgram()])
    else refresh.mockRejectedValueOnce('Detection failed')
    const uninstall = vi.spyOn(agentApi, 'uninstall').mockResolvedValue(undefined)
    vi.spyOn(skillApiV2, 'deleteSkillTargetDistributions').mockResolvedValue({ deleted: 1, failures: [] })
    vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue()
    const loadDetail = vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue()
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(await screen.findByRole('button', { name: '卸载 Agent' }))
    fireEvent.click(screen.getByRole('button', { name: '确认卸载' }))

    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent(
      detected ? '仍检测到 Claude Code' : '无法确认 Claude Code 是否已卸载',
    ))
    expect(uninstall).toHaveBeenCalledExactlyOnceWith('claude-code', LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(loadDetail).toHaveBeenCalledWith('claude-code', true)
    expect(useSkillStoreV2.getState().agents.find((agent) => agent.id === 'claude-code')?.installed).toBe(true)
    expect(useSkillStoreV2.getState().selectedAgentId).toBe('claude-code')
    expect(screen.queryByText(/已卸载，/)).not.toBeInTheDocument()
  })

  it('finishes an uninstall on the confirmed runtime without changing the newly selected host', async () => {
    const localSkill = makeSharedUnmanaged('local-only', { agentId: 'claude-code' })
    const remoteSkill = makeSharedUnmanaged('remote-only', { agentId: 'claude-code' })
    useSkillStoreV2.setState({ unmanaged: [localSkill] })
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([makeProgram({ hooksInstalled: true })])
    let finishProgramUninstall!: () => void
    const pendingProgramUninstall = new Promise<void>((resolve) => { finishProgramUninstall = resolve })
    const uninstall = vi.spyOn(agentApi, 'uninstall').mockReturnValue(pendingProgramUninstall)
    const deleteManaged = vi.spyOn(skillApiV2, 'deleteSkillTargetDistributions').mockResolvedValue({ deleted: 1, failures: [] })
    const deleteUnmanaged = vi.spyOn(skillApiV2, 'deleteUnmanagedAgentSkill').mockResolvedValue(undefined)
    const uninstallHook = vi.spyOn(agentApi, 'uninstallHook').mockResolvedValue(undefined)
    const listUnmanaged = vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue(undefined)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(await screen.findByRole('button', { name: '卸载 Agent' }))
    fireEvent.click(screen.getByRole('button', { name: '确认卸载' }))
    expect(uninstall).toHaveBeenCalledTimes(1)

    const remoteAgent = makeSidebarAgent('claude-code', 'Remote Claude')
    await act(async () => {
      useRuntimeEnvironmentStore.getState().selectEnvironment('remote-runtime')
      useSkillStoreV2.setState({ runtimeEnvironmentId: 'remote-runtime', unmanaged: [remoteSkill], agents: [remoteAgent] })
      finishProgramUninstall()
    })

    expect(uninstall).toHaveBeenCalledWith('claude-code', LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(deleteManaged).toHaveBeenCalledExactlyOnceWith(agentDetail.skills.map((skill) => skill.id), LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(deleteUnmanaged).toHaveBeenCalledExactlyOnceWith('claude-code', localSkill.id, LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(uninstallHook).toHaveBeenCalledExactlyOnceWith('claude-code', LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(listUnmanaged).toHaveBeenCalledExactlyOnceWith(LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(useSkillStoreV2.getState().agents).toEqual([remoteAgent])
    expect(useSkillStoreV2.getState().unmanaged).toEqual([remoteSkill])
    expect(screen.queryByText(/已卸载/)).not.toBeInTheDocument()
  })

  it('removes an uninstalled Agent from the sidebar and selects the next detected installation', async () => {
    const codexDetail: AgentDetail = {
      ...agentDetail,
      id: 'codex',
      displayName: 'Codex',
      iconKey: 'codex',
      skillsDir: '/Users/me/.codex/skills',
      skills: [],
    }
    useSkillStoreV2.setState({
      activeTab: 'agents',
      selectedAgentId: 'claude-code',
      selectedAgentDetail: agentDetail,
      agents: [
        makeSidebarAgent('claude-code', 'Claude Code', { managed: 1 }),
        makeSidebarAgent('gemini', 'Gemini'),
        { ...makeSidebarAgent('codex', 'Codex'), installed: false },
      ],
      unmanaged: [],
    })
    const codexProgram = makeProgram({ id: 'codex', displayName: 'Codex' })
    const geminiProgram = makeProgram({ id: 'gemini', displayName: 'Gemini', status: 'notInstalled', binaryPath: null })
    vi.spyOn(agentApi, 'refresh')
      .mockResolvedValueOnce([makeProgram(), codexProgram, geminiProgram])
      .mockResolvedValue([makeProgram({ status: 'notInstalled', binaryPath: null }), codexProgram, geminiProgram])
    vi.spyOn(agentApi, 'uninstall').mockResolvedValue(undefined)
    vi.spyOn(skillApiV2, 'deleteSkillTargetDistributions').mockResolvedValue({ deleted: 1, failures: [] })
    vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    vi.spyOn(skillApiV2, 'getAgentDetail').mockImplementation(async (agentId) => (
      agentId === 'codex' ? codexDetail : agentDetail
    ))
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue(undefined)

    const { SettingsSidebar } = await import('../components/settings/SettingsSidebar')
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    const { container } = render(
      <>
        <SettingsSidebar
          activeSection="skill-manager-v2"
          activeIslandView="overview"
          activeMonitorView="overview"
          collapsed={false}
          onCollapsedChange={() => {}}
          onSelect={() => {}}
          onIslandViewChange={() => {}}
          onMonitorViewChange={() => {}}
        />
        <AgentManagementPage />
      </>,
    )

    expect(container.querySelector('[data-agent-id="claude-code"]')).not.toBeNull()
    fireEvent.click(await screen.findByRole('button', { name: '卸载 Agent' }))
    fireEvent.click(screen.getByRole('button', { name: '确认卸载' }))

    await waitFor(() => {
      expect(container.querySelector('[data-agent-id="claude-code"]')).toBeNull()
      expect(container.querySelector('[data-agent-id="codex"]')).not.toBeNull()
      expect(useSkillStoreV2.getState().selectedAgentId).toBe('codex')
    })
  })

  it('offers safe Trash removal for a standalone macOS agent app', async () => {
    const kiroDetail: AgentDetail = {
      ...agentDetail,
      id: 'kiro',
      displayName: 'Kiro',
      iconKey: 'kiro',
      skillsDir: '/Users/me/.kiro/skills',
      skills: [],
    }
    useSkillStoreV2.setState({
      selectedAgentId: 'kiro',
      selectedAgentDetail: kiroDetail,
      agents: [
        { id: 'kiro', displayName: 'Kiro', iconKey: 'kiro', enabled: true, skillsDir: '/Users/me/.kiro/skills', version: null, latestVersion: null, installed: true, managedSkillCount: 0, unmanagedSkillCount: 0 } as AgentSummary,
      ],
      unmanaged: [],
    })
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([
      makeProgram({
        id: 'kiro',
        displayName: 'Kiro',
        icon: 'kiro',
        kind: 'app',
        packageManager: 'app',
        packageName: null,
        binaryPath: '/Applications/Kiro.app/Contents/MacOS/Kiro',
        appPath: '/Applications/Kiro.app',
        uninstallCommand: 'Move application to Trash',
      }),
    ])

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(await screen.findByRole('button', { name: '卸载 Agent' }))
    expect(screen.getByRole('dialog', { name: '卸载 Agent「Kiro」' })).toHaveTextContent('程序移到废纸篓')
    expect(screen.getByText('移到废纸篓：/Applications/Kiro.app')).toBeInTheDocument()
  })

  it('offers the official npm uninstall command for GitHub Copilot CLI', async () => {
    const copilotDetail: AgentDetail = {
      ...agentDetail,
      id: 'copilot',
      displayName: 'Copilot',
      iconKey: 'copilot',
      skillsDir: '/Users/me/.copilot/skills',
      skills: [],
    }
    useSkillStoreV2.setState({
      selectedAgentId: 'copilot',
      selectedAgentDetail: copilotDetail,
      agents: [makeSidebarAgent('copilot', 'Copilot')],
      unmanaged: [],
    })
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([
      makeProgram({
        id: 'copilot',
        displayName: 'GitHub Copilot',
        icon: 'copilot',
        packageName: '@github/copilot',
        binaryPath: '/opt/homebrew/bin/copilot',
        configDir: '/Users/me/.copilot',
        downloadUrl: 'https://docs.github.com/copilot/how-tos/set-up/install-copilot-cli',
        installCommand: 'npm install -g @github/copilot',
        updateCommand: 'npm install -g @github/copilot@latest',
        uninstallCommand: 'npm uninstall -g @github/copilot',
        skillsDir: '/Users/me/.copilot/skills',
      }),
    ])

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(await screen.findByRole('button', { name: '卸载 Agent' }))
    expect(screen.getByRole('dialog', { name: '卸载 Agent「Copilot」' })).toBeInTheDocument()
    expect(screen.getByText('npm uninstall -g @github/copilot')).toBeInTheDocument()
  })

  it('does not offer uninstall for Aider when only its config directory remains', async () => {
    const aiderDetail: AgentDetail = {
      ...agentDetail,
      id: 'aider',
      displayName: 'Aider',
      iconKey: 'aider',
      skillsDir: '/Users/me/.aider/skills',
      skills: [],
    }
    useSkillStoreV2.setState({
      selectedAgentId: 'aider',
      selectedAgentDetail: aiderDetail,
      agents: [
        { id: 'aider', displayName: 'Aider', iconKey: 'aider', enabled: true, skillsDir: '/Users/me/.aider/skills', version: null, latestVersion: null, installed: true, managedSkillCount: 0, unmanagedSkillCount: 0 } as AgentSummary,
      ],
      unmanaged: [],
    })
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([
      makeProgram({
        id: 'aider',
        displayName: 'Aider',
        icon: 'aider',
        status: 'notInstalled',
        packageManager: 'uv',
        packageName: 'aider-chat',
        binaryPath: null,
        configDir: '/Users/me/.aider',
        installCommand: 'uv tool install --force --python python3.12 --with pip aider-chat@latest',
        updateCommand: 'uv tool install --force --python python3.12 --with pip aider-chat@latest',
        uninstallCommand: 'uv tool uninstall aider-chat',
      }),
    ])

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    expect(await screen.findByText('当前版本 未安装')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '卸载 Agent' })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '安装此 Agent' })).toBeInTheDocument()
  })

  it('cleans managed and unmanaged Skills when the Agent program is already absent', async () => {
    const unmanagedItem = {
      id: 'unmanaged-aider',
      agentId: 'aider',
      itemType: 'agent_skill' as const,
      path: '/Users/me/.aider/skills/local-only',
      inferredSkillId: 'local-only',
      hash: 'local-hash',
      reason: 'not_in_center_library',
    }
    const aiderDetail: AgentDetail = {
      ...agentDetail,
      id: 'aider',
      displayName: 'Aider',
      iconKey: 'aider',
      skillsDir: '/Users/me/.aider/skills',
      skills: agentDetail.skills.map((skill) => ({
        ...skill,
        id: 'target-aider',
        agentId: 'aider',
        targetPath: '/Users/me/.aider/skills/release-checklist',
      })),
    }
    useSkillStoreV2.setState({
      selectedAgentId: 'aider',
      selectedAgentDetail: aiderDetail,
      agents: [
        { id: 'aider', displayName: 'Aider', iconKey: 'aider', enabled: true, skillsDir: '/Users/me/.aider/skills', version: null, latestVersion: null, installed: true, managedSkillCount: 1, unmanagedSkillCount: 1 } as AgentSummary,
      ],
      unmanaged: [unmanagedItem],
    })
    const program = makeProgram({
      id: 'aider',
      displayName: 'Aider',
      icon: 'aider',
      status: 'notInstalled',
      packageManager: 'uv',
      packageName: 'aider-chat',
      binaryPath: null,
      configDir: '/Users/me/.aider',
      installCommand: 'uv tool install --force --python python3.12 --with pip aider-chat@latest',
      updateCommand: 'uv tool install --force --python python3.12 --with pip aider-chat@latest',
      uninstallCommand: 'uv tool uninstall aider-chat',
      hooksInstalled: true,
    })
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([program])
    const uninstall = vi.spyOn(agentApi, 'uninstall').mockResolvedValue(undefined)
    const uninstallHook = vi.spyOn(agentApi, 'uninstallHook').mockResolvedValue(undefined)
    const deleteManaged = vi.spyOn(skillApiV2, 'deleteSkillTargetDistributions').mockResolvedValue({ deleted: 1, failures: [] })
    let unmanagedPresent = true
    const deleteUnmanaged = vi.spyOn(skillApiV2, 'deleteUnmanagedAgentSkill').mockImplementation(async () => {
      unmanagedPresent = false
    })
    vi.spyOn(skillApiV2, 'listUnmanaged').mockImplementation(async () => unmanagedPresent ? [unmanagedItem] : [])
    vi.spyOn(skillApiV2, 'overview').mockResolvedValue(makeOverview())
    vi.spyOn(skillApiV2, 'getAgentDetail').mockResolvedValue({ ...aiderDetail, skills: [] })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(await screen.findByRole('button', { name: '清理残留' }))
    const dialog = screen.getByRole('dialog', { name: '清理「Aider」的 AgentBro 资源' })
    expect(dialog).toHaveTextContent('未安装，仅清理残留')
    expect(dialog).toHaveTextContent('1已管理 Skills')
    expect(dialog).toHaveTextContent('1未管理 Skills')
    fireEvent.click(screen.getByRole('button', { name: '确认清理' }))

    await waitFor(() => expect(deleteManaged).toHaveBeenCalledWith(['target-aider'], LOCAL_RUNTIME_ENVIRONMENT_ID))
    expect(deleteUnmanaged).toHaveBeenCalledWith('aider', 'unmanaged-aider', LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(uninstallHook).toHaveBeenCalledWith('aider', LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(uninstall).not.toHaveBeenCalled()
    expect(await screen.findByText(/已清理 Aider 的 AgentBro 资源/)).toHaveTextContent('已清理 2 个 Skills')
  })

  it('SkillPackPage renders the redesigned pack workspace', async () => {
    const { SkillPackPage } = await import('../components/skills-v2/SkillPackPage')
    const { container } = render(<SkillPackPage />)
    expect(container.querySelector('.sm2__pack-layout')).not.toBeNull()
    expect(container.querySelector('.sm2__pack-sidebar')).not.toBeNull()
    expect(container.querySelector('.sm2__pack-canvas')).not.toBeNull()
    expect(container.querySelector('.sm2__pack-dashboard')).toBeNull()
    expect(screen.getByLabelText('技能包概览')).toBeInTheDocument()
    expect(container.querySelector('.sm2__rail')).toBeNull()
  })

  async function renderPackPage(overrides: Partial<SkillPackDetail> = {}) {
    const skill = makeSkill()
    const pack: SkillPackDetail = {
      id: 'default',
      name: 'Review pack',
      description: '',
      tags: [],
      members: [{ skillId: skill.id, skillName: skill.name, required: true, sortOrder: 0, missing: false }],
      appliedAgents: [],
      createdAt: '',
      updatedAt: '',
      ...overrides,
    }
    const agents = useSkillStoreV2.getState().agents
    const overview = {
      ...makeOverview(),
      agents: [...agents, { ...agents[0], id: 'codex', displayName: 'Codex', iconKey: 'codex' }],
      skills: [skill],
      packs: [{ id: pack.id, name: pack.name, description: '', tags: [], memberCount: 1, appliedAgentCount: 0, healthy: true }],
    }
    useSkillStoreV2.setState({
      ...overview,
      overview,
      selectedPackId: pack.id,
      selectedPackDetail: pack,
      lastOverviewLoadedAt: Date.now(),
    })
    vi.spyOn(skillApiV2, 'overview').mockResolvedValue(overview)
    vi.spyOn(skillApiV2, 'getPackDetail').mockResolvedValue(pack)
    const { SkillPackPage } = await import('../components/skills-v2/SkillPackPage')
    render(<SkillPackPage />)
    return { pack, skill, overview }
  }

  async function openPackApplication() {
    const { pack, skill } = await renderPackPage()
    fireEvent.click(screen.getByRole('button', { name: '应用到 Agent' }))
    return { pack, skill, dialog: within(screen.getByRole('dialog', { name: '应用「Review pack」' })) }
  }

  it('reviews a shared GitHub Pack and keeps its source through recheck and confirmation', async () => {
    const { pack, skill } = await renderPackPage()
    const link = 'https://github.com/example/workflows/blob/main/team.agentbro-pack.json'
    const sourceUrl = 'https://raw.githubusercontent.com/example/workflows/main/team.agentbro-pack.json'
    const fingerprint = 'a'.repeat(64)
    const manifest: SkillPackManifest = {
      format: 'agentbro-skill-pack', version: 2, name: 'Shared Pack', description: '',
      tags: ['review'], skillIds: [skill.id], skillFingerprints: { [skill.id]: fingerprint },
    }
    const read = vi.spyOn(skillApiV2, 'readPackManifestLink')
      .mockRejectedValueOnce('Could not open the public GitHub file (HTTP 404).')
      .mockResolvedValueOnce({ manifest, sourceUrl })
    const reviewed = [{ skillId: skill.id, missing: false, fingerprint }]
    const fingerprints = vi.spyOn(skillApiV2, 'getPackFingerprints').mockResolvedValue(reviewed)
    const create = vi.spyOn(skillApiV2, 'importReviewedPack').mockResolvedValue({ ...pack, id: 'linked', name: manifest.name })

    fireEvent.click(screen.getByRole('button', { name: '从链接导入' }))
    const inputDialog = within(screen.getByRole('dialog', { name: '从链接导入' }))
    const input = inputDialog.getByLabelText('GitHub 链接')
    const review = inputDialog.getByRole('button', { name: '预览技能包' })
    expect(review).toBeDisabled()
    fireEvent.change(input, { target: { value: link } })
    fireEvent.click(review)
    expect(await inputDialog.findByRole('alert')).toHaveTextContent('HTTP 404')
    expect(input).toHaveValue(link)
    fireEvent.keyDown(input, { key: 'Enter' })

    const preview = within(await screen.findByRole('dialog', { name: '导入技能包' }))
    expect(preview.getByText(manifest.name)).toBeInTheDocument()
    expect(preview.getByText(sourceUrl)).toBeInTheDocument()
    expect(fingerprints).toHaveBeenCalledWith([skill.id], LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(create).not.toHaveBeenCalled()
    fireEvent.click(preview.getByRole('button', { name: '重新比对技能' }))
    await waitFor(() => expect(preview.getByRole('button', { name: '创建技能包' })).toBeEnabled())
    expect(preview.getByText(sourceUrl)).toBeInTheDocument()
    expect(read).toHaveBeenCalledTimes(2)
    fireEvent.click(preview.getByRole('button', { name: '创建技能包' }))
    await waitFor(() => expect(create).toHaveBeenCalledWith(manifest, reviewed, false, LOCAL_RUNTIME_ENVIRONMENT_ID))
  })

  it('discards a shared Pack download after changing the selected runtime', async () => {
    const { skill } = await renderPackPage()
    const sourceUrl = 'https://raw.githubusercontent.com/example/repo/main/pack.json'
    const manifest: SkillPackManifest = {
      format: 'agentbro-skill-pack', version: 2, name: 'Old host Pack', description: '',
      tags: [], skillIds: [skill.id],
    }
    let finish!: (value: { manifest: SkillPackManifest; sourceUrl: string }) => void
    const read = vi.spyOn(skillApiV2, 'readPackManifestLink')
      .mockReturnValue(new Promise((resolve) => { finish = resolve }))
    const fingerprints = vi.spyOn(skillApiV2, 'getPackFingerprints')
    fireEvent.click(screen.getByRole('button', { name: '从链接导入' }))
    const dialog = within(screen.getByRole('dialog', { name: '从链接导入' }))
    fireEvent.change(dialog.getByLabelText('GitHub 链接'), { target: { value: sourceUrl } })
    fireEvent.click(dialog.getByRole('button', { name: '预览技能包' }))
    expect(read).toHaveBeenCalledWith(sourceUrl)

    const { selectEnvironment } = useRuntimeEnvironmentStore.getState()
    act(() => {
      selectEnvironment('other-host')
      selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    })
    await act(async () => finish({ manifest, sourceUrl }))
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
    expect(fingerprints).not.toHaveBeenCalled()
  })

  it('exports the Skill Pack definition and its reviewed contents', async () => {
    const { pack, skill } = await renderPackPage()
    act(() => useSkillStoreV2.setState({
      selectedPackDetail: {
        ...pack,
        description: 'Review before publishing',
        tags: ['team'],
        appliedAgents: [{ packId: pack.id, packName: pack.name, memberCount: 1, agentId: 'claude-code' }],
      },
    }))
    const destination = 'C:/Team Files/技能包.agentbro-pack.json'
    vi.mocked(saveDialog).mockResolvedValueOnce(destination)
    const fingerprint = 'a'.repeat(64)
    const fingerprints = vi.spyOn(skillApiV2, 'getPackFingerprints').mockResolvedValue([
      { skillId: skill.id, missing: false, fingerprint },
    ])
    const write = vi.spyOn(skillApiV2, 'writePackManifest').mockResolvedValue(undefined)
    fireEvent.click(screen.getByLabelText('更多操作'))
    fireEvent.click(screen.getByRole('menuitem', { name: '导出技能包' }))

    expect(await screen.findByText('技能包已导出。')).toBeInTheDocument()
    expect(fingerprints).toHaveBeenCalledWith([skill.id], LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(write).toHaveBeenCalledWith(destination, {
      format: 'agentbro-skill-pack',
      version: 2,
      name: pack.name,
      description: 'Review before publishing',
      tags: ['team'],
      skillIds: [skill.id],
      skillFingerprints: { [skill.id]: fingerprint },
    })
  })

  it('exports pinned GitHub sources only when explicitly requested', async () => {
    const { skill } = await renderPackPage()
    const file = '/Team/pinned.agentbro-pack.json'
    const source = { repository: 'example/workflows', commit: 'b'.repeat(40), skillPath: 'review/SKILL.md' }
    vi.mocked(saveDialog).mockResolvedValueOnce(file)
    const fingerprints = vi.spyOn(skillApiV2, 'getPackFingerprints').mockResolvedValue([
      { skillId: skill.id, missing: false, fingerprint: 'a'.repeat(64), source },
    ])
    const write = vi.spyOn(skillApiV2, 'writePackManifest').mockResolvedValue(undefined)
    fireEvent.click(screen.getByLabelText('更多操作'))
    fireEvent.click(screen.getByRole('menuitem', { name: '导出并包含 GitHub 来源' }))
    await waitFor(() => expect(write).toHaveBeenCalledWith(file, expect.objectContaining({
      version: 3,
      skillSources: { [skill.id]: source },
    })))
    expect(fingerprints).toHaveBeenCalledWith([skill.id], LOCAL_RUNTIME_ENVIRONMENT_ID, true)
  })

  it('reviews a pinned source, reports a failed restore, and rechecks before creating the Pack', async () => {
    const { pack, skill } = await renderPackPage()
    const fingerprint = 'a'.repeat(64)
    const manifest: SkillPackManifest = {
      format: 'agentbro-skill-pack',
      version: 3,
      name: 'Pinned pack',
      description: '',
      tags: [],
      skillIds: [skill.id],
      skillFingerprints: { [skill.id]: fingerprint },
      skillSources: {
        [skill.id]: { repository: 'example/workflows', commit: 'b'.repeat(40), skillPath: 'review/SKILL.md' },
      },
    }
    const reviewed = [{ skillId: skill.id, missing: false, fingerprint }]
    vi.mocked(openDialog).mockResolvedValueOnce('/Team/pinned.json')
    vi.spyOn(skillApiV2, 'readPackManifest').mockResolvedValue(manifest)
    const review = vi.spyOn(skillApiV2, 'getPackFingerprints')
      .mockResolvedValueOnce([{ skillId: skill.id, missing: true, fingerprint: null }])
      .mockResolvedValueOnce(reviewed)
    const restore = vi.spyOn(skillApiV2, 'restorePackMember')
      .mockRejectedValueOnce('Skill content does not match the recorded fingerprint.')
      .mockResolvedValueOnce(undefined)
    const create = vi.spyOn(skillApiV2, 'importReviewedPack')
      .mockResolvedValue({ ...pack, id: 'restored-pack', name: manifest.name })

    fireEvent.click(screen.getByRole('button', { name: '导入技能包' }))
    const dialog = within(await screen.findByRole('dialog', { name: '导入技能包' }))
    const restoreButton = dialog.getByRole('button', { name: `恢复 ${skill.id}` })
    expect(dialog.getByText(/example\/workflows.*review\/SKILL\.md/)).toBeInTheDocument()
    expect(dialog.getByRole('button', { name: '创建技能包' })).toBeDisabled()
    fireEvent.click(restoreButton)
    expect(await dialog.findByRole('alert')).toHaveTextContent('does not match the recorded fingerprint')
    expect(restore).toHaveBeenCalledWith(manifest, skill.id, LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(create).not.toHaveBeenCalled()

    fireEvent.click(restoreButton)
    await waitFor(() => expect(review).toHaveBeenCalledTimes(2))
    const confirm = await dialog.findByRole('button', { name: '创建技能包' })
    expect(confirm).toBeEnabled()
    fireEvent.click(confirm)
    await waitFor(() => expect(create).toHaveBeenCalledWith(manifest, reviewed, false, LOCAL_RUNTIME_ENVIRONMENT_ID))
  })

  it('leaves a pending Skill Pack restore bound to its original SSH host', async () => {
    const { skill } = await renderPackPage()
    const { selectEnvironment } = useRuntimeEnvironmentStore.getState()
    act(() => {
      selectEnvironment('pack-ssh-host')
      useSkillStoreV2.setState({ runtimeEnvironmentId: 'pack-ssh-host' })
    })
    const manifest: SkillPackManifest = {
      format: 'agentbro-skill-pack',
      version: 3,
      name: 'Remote pinned pack',
      description: '',
      tags: [],
      skillIds: [skill.id],
      skillFingerprints: { [skill.id]: 'a'.repeat(64) },
      skillSources: {
        [skill.id]: { repository: 'example/workflows', commit: 'b'.repeat(40), skillPath: 'review/SKILL.md' },
      },
    }
    vi.mocked(openDialog).mockResolvedValueOnce('/Team/pinned.json')
    vi.spyOn(skillApiV2, 'readPackManifest').mockResolvedValue(manifest)
    const review = vi.spyOn(skillApiV2, 'getPackFingerprints')
      .mockResolvedValue([{ skillId: skill.id, missing: true, fingerprint: null }])
    let finish!: () => void
    const restore = vi.spyOn(skillApiV2, 'restorePackMember')
      .mockReturnValue(new Promise<void>((resolve) => { finish = resolve }))

    fireEvent.click(screen.getByRole('button', { name: '导入技能包' }))
    const dialog = within(await screen.findByRole('dialog', { name: '导入技能包' }))
    fireEvent.click(dialog.getByRole('button', { name: `恢复 ${skill.id}` }))
    expect(restore).toHaveBeenCalledWith(manifest, skill.id, 'pack-ssh-host')
    act(() => {
      selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
      useSkillStoreV2.setState({ runtimeEnvironmentId: LOCAL_RUNTIME_ENVIRONMENT_ID })
    })
    await act(async () => finish())
    expect(review).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('dialog', { name: '导入技能包' })).not.toBeInTheDocument()
  })

  it('previews missing Skill Pack members and imports a new pack in manifest order', async () => {
    const { pack, skill, overview } = await renderPackPage()
    const manifest: SkillPackManifest = {
      format: 'agentbro-skill-pack',
      version: 1,
      name: '团队审阅',
      description: '团队的审阅流程',
      tags: ['team'],
      skillIds: ['review-notes', skill.id],
    }
    const source = '/Users/me/Team Workspace/团队审阅.agentbro-pack.json'
    vi.mocked(openDialog).mockResolvedValueOnce(source)
    const read = vi.spyOn(skillApiV2, 'readPackManifest').mockResolvedValue(manifest)
    const upsert = vi.spyOn(skillApiV2, 'upsertPack').mockResolvedValue({
      ...pack,
      id: 'pack-imported',
      name: manifest.name,
      description: manifest.description,
      tags: manifest.tags,
      members: [
        { skillId: 'review-notes', skillName: 'Review Notes', required: true, sortOrder: 0, missing: false },
        { ...pack.members[0], sortOrder: 1 },
      ],
    })
    fireEvent.click(screen.getByRole('button', { name: '导入技能包' }))
    const preview = within(await screen.findByRole('dialog', { name: '导入技能包' }))
    expect(read).toHaveBeenCalledWith(source)
    expect(preview.getByText(manifest.description)).toBeInTheDocument()
    expect(preview.getByText('缺失')).toBeInTheDocument()
    expect(preview.getByRole('button', { name: '创建技能包' })).toBeDisabled()
    expect(upsert).not.toHaveBeenCalled()

    const updatedSkills = [skill, makeSkill({ id: 'review-notes' })]
    vi.mocked(skillApiV2.overview).mockResolvedValue({ ...overview, skills: updatedSkills })
    act(() => useSkillStoreV2.setState({ skills: updatedSkills }))
    fireEvent.click(preview.getByRole('button', { name: '创建技能包' }))

    expect(await screen.findByText('已将「团队审阅」导入为新技能包。')).toBeInTheDocument()
    expect(upsert).toHaveBeenCalledWith({
      id: '',
      name: manifest.name,
      description: manifest.description,
      tags: manifest.tags,
      skillIds: ['review-notes', skill.id],
    }, { deferSync: true })
    expect(useSkillStoreV2.getState().selectedPackId).toBe('pack-imported')
  })

  it('discards a Skill Pack import review after switching runtimes', async () => {
    const { skill } = await renderPackPage()
    vi.mocked(openDialog).mockResolvedValueOnce('/Team/workflow.json')
    vi.spyOn(skillApiV2, 'readPackManifest').mockResolvedValue({
      format: 'agentbro-skill-pack',
      version: 1,
      name: 'Portable pack',
      description: '',
      tags: [],
      skillIds: [skill.id],
    })
    const upsert = vi.spyOn(skillApiV2, 'upsertPack')
    fireEvent.click(screen.getByRole('button', { name: '导入技能包' }))
    const confirm = within(await screen.findByRole('dialog', { name: '导入技能包' }))
      .getByRole('button', { name: '创建技能包' })
    const { selectEnvironment } = useRuntimeEnvironmentStore.getState()
    act(() => {
      selectEnvironment('other-machine')
      selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    })
    fireEvent.click(confirm)

    expect(upsert).not.toHaveBeenCalled()
    expect(screen.queryByRole('dialog', { name: '导入技能包' })).not.toBeInTheDocument()
  })

  it('requires fresh confirmation when the reviewed Skill Pack copy changed', async () => {
    const { pack, skill } = await renderPackPage()
    const manifest: SkillPackManifest = {
      format: 'agentbro-skill-pack',
      version: 2,
      name: 'Shared review pack',
      description: '',
      tags: [],
      skillIds: [skill.id],
      skillFingerprints: { [skill.id]: 'a'.repeat(64) },
    }
    const firstReview = [{ skillId: skill.id, missing: false, fingerprint: 'b'.repeat(64) }]
    const secondReview = [{ skillId: skill.id, missing: false, fingerprint: 'c'.repeat(64) }]
    vi.mocked(openDialog).mockResolvedValueOnce('/Team/shared.json')
    vi.spyOn(skillApiV2, 'readPackManifest').mockResolvedValue(manifest)
    vi.spyOn(skillApiV2, 'getPackFingerprints')
      .mockResolvedValueOnce(firstReview)
      .mockResolvedValueOnce(secondReview)
    const saved = vi.spyOn(skillApiV2, 'importReviewedPack')
      .mockRejectedValueOnce('The selected library changed since review.')
      .mockResolvedValueOnce({ ...pack, id: 'pack-shared', name: manifest.name })

    fireEvent.click(screen.getByRole('button', { name: '导入技能包' }))
    const preview = within(await screen.findByRole('dialog', { name: '导入技能包' }))
    const confirm = preview.getByRole('button', { name: '创建技能包' })
    const useCopy = preview.getByRole('checkbox', { name: /使用当前库中的技能/ })
    expect(preview.getByText('内容不同')).toBeInTheDocument()
    expect(confirm).toBeDisabled()
    fireEvent.click(useCopy)
    fireEvent.click(confirm)
    expect(await preview.findByRole('alert')).toHaveTextContent('changed since review')
    expect(saved).toHaveBeenCalledWith(manifest, firstReview, true, LOCAL_RUNTIME_ENVIRONMENT_ID)

    fireEvent.click(preview.getByRole('button', { name: '重新比对技能' }))
    await waitFor(() => expect(useCopy).not.toBeChecked())
    expect(confirm).toBeDisabled()
    fireEvent.click(useCopy)
    fireEvent.click(confirm)
    expect(await screen.findByText('已将「Shared review pack」导入为新技能包。')).toBeInTheDocument()
    expect(saved).toHaveBeenLastCalledWith(manifest, secondReview, true, LOCAL_RUNTIME_ENVIRONMENT_ID)
  })

  it('discards a late pack preview when the selected Agent and mode change', async () => {
    const { pack, skill, dialog } = await openPackApplication()
    let resolvePreview!: (value: DistributionPreview) => void
    const pending = new Promise<DistributionPreview>((resolve) => { resolvePreview = resolve })
    const older: DistributionPreview = {
      skillIds: [skill.id],
      targetAgents: ['claude-code'],
      requestedMode: 'link',
      changes: [],
      blockers: [],
      blockerDecisions: [],
    }
    const latest: DistributionPreview = {
      ...older,
      targetAgents: ['codex'],
      requestedMode: 'copy',
      changes: [{
        skillId: skill.id,
        agentId: 'codex',
        action: 'reinstall',
        actualMode: 'copy',
        targetPath: 'C:\\Users\\Tester\\.codex\\skills\\example',
        targetFingerprint: 'directory:reviewed-version',
      }],
    }
    const preview = vi.spyOn(skillApiV2, 'previewApplyPack').mockReturnValueOnce(pending).mockResolvedValue(latest)
    const execute = vi.spyOn(skillApiV2, 'executeApplyPack').mockResolvedValue(latest)
    fireEvent.click(dialog.getByRole('button', { name: /Claude Code/ }))
    fireEvent.click(dialog.getByRole('button', { name: '预览影响' }))
    expect(preview).toHaveBeenCalledWith(pack.id, ['claude-code'], 'link')

    fireEvent.click(dialog.getByRole('button', { name: /Claude Code/ }))
    fireEvent.click(dialog.getByRole('button', { name: /Codex/ }))
    fireEvent.change(dialog.getByLabelText('分发方式'), { target: { value: 'copy' } })
    await act(async () => { resolvePreview(older) })

    expect(screen.queryByRole('dialog', { name: '确认应用预览' })).not.toBeInTheDocument()
    expect(execute).not.toHaveBeenCalled()
    fireEvent.click(dialog.getByRole('button', { name: '预览影响' }))
    expect(preview).toHaveBeenLastCalledWith(pack.id, ['codex'], 'copy')
    const review = await screen.findByRole('dialog', { name: '确认应用预览' })
    expect(review).toHaveTextContent('Codex')
    expect(review).toHaveTextContent('复制')
    fireEvent.click(within(review).getByRole('button', { name: '执行应用' }))
    await waitFor(() => expect(execute).toHaveBeenCalledWith(pack.id, ['codex'], 'copy', [], latest))
  })

  it.each([false, true])('invalidates a pack review after changing runtime (returned: %s)', async (returnToLocal) => {
    const { skill, dialog } = await openPackApplication()
    const prepared: DistributionPreview = {
      skillIds: [skill.id],
      targetAgents: ['claude-code'],
      requestedMode: 'link',
      changes: [],
      blockers: [],
      blockerDecisions: [],
    }
    vi.spyOn(skillApiV2, 'previewApplyPack').mockResolvedValue(prepared)
    const execute = vi.spyOn(skillApiV2, 'executeApplyPack').mockResolvedValue(prepared)
    fireEvent.click(dialog.getByRole('button', { name: /Claude Code/ }))
    fireEvent.click(dialog.getByRole('button', { name: '预览影响' }))
    const confirm = await screen.findByRole('button', { name: '执行应用' })
    const { selectEnvironment } = useRuntimeEnvironmentStore.getState()
    try {
      act(() => {
        selectEnvironment('other-test-machine')
        if (returnToLocal) selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
      })
      fireEvent.click(confirm)

      expect(execute).not.toHaveBeenCalled()
      expect(screen.queryByRole('button', { name: '执行应用' })).not.toBeInTheDocument()
    } finally {
      act(() => selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID))
    }
  })

  it('keeps the current pack detail when the previous selection responds late', async () => {
    const slowPack: SkillPackDetail = {
      id: 'slow-pack', name: 'Slow Pack', description: '', tags: [], members: [], appliedAgents: [],
      createdAt: '2026-01-01T00:00:00Z', updatedAt: '2026-01-01T00:00:00Z',
    }
    const fastPack: SkillPackDetail = { ...slowPack, id: 'fast-pack', name: 'Fast Pack' }
    let resolveSlow!: (detail: SkillPackDetail) => void
    const pending = new Promise<SkillPackDetail>((resolve) => { resolveSlow = resolve })
    const getDetail = vi.spyOn(skillApiV2, 'getPackDetail').mockImplementation((id) =>
      id === slowPack.id ? pending : Promise.resolve(fastPack),
    )
    const packs = [slowPack, fastPack].map(({ id, name, tags, description }) => ({
      id, name, tags, description, memberCount: 0, appliedAgentCount: 0, healthy: true,
    }))
    vi.spyOn(skillApiV2, 'overview').mockResolvedValue({ ...makeOverview(), packs })
    useSkillStoreV2.setState({ packs, selectedPackId: fastPack.id, selectedPackDetail: fastPack })

    const { SkillPackPage } = await import('../components/skills-v2/SkillPackPage')
    render(<SkillPackPage />)
    fireEvent.click(screen.getByRole('button', { name: /^Slow Pack/ }))
    expect(getDetail).toHaveBeenCalledWith(slowPack.id, LOCAL_RUNTIME_ENVIRONMENT_ID)
    fireEvent.click(screen.getByRole('button', { name: /^Fast Pack/ }))
    expect(await screen.findByRole('heading', { name: fastPack.name })).toBeInTheDocument()

    await act(async () => { resolveSlow(slowPack) })

    expect(useSkillStoreV2.getState().selectedPackId).toBe(fastPack.id)
    expect(screen.getByRole('heading', { name: fastPack.name })).toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: slowPack.name })).not.toBeInTheDocument()
  })

  it('opens the pack builder in a dialog instead of replacing the canvas', async () => {
    const { SkillPackPage } = await import('../components/skills-v2/SkillPackPage')
    const { container } = render(<SkillPackPage />)

    fireEvent.click(container.querySelector('.sm2__header .sm2__btn--primary')!)

    expect(document.body.querySelector('.sm2__pack-builder-modal')).not.toBeNull()
    expect(document.body.querySelector('.sm2__pack-builder-modal .sm2__skill-picker2')).not.toBeNull()
    expect(document.body.querySelector('.sm2__pack-builder-rail')).toBeNull()
    expect(document.body.querySelector('.sm2__pack-builder-footer')).not.toBeNull()
    expect(container.querySelector('.sm2__pack-canvas .sm2__pack-builder2')).toBeNull()
  })

  it('keeps custom pack creation focused on name and members only', async () => {
    const { SkillPackPage } = await import('../components/skills-v2/SkillPackPage')
    const { container } = render(<SkillPackPage />)

    fireEvent.click(container.querySelector('.sm2__header .sm2__btn--primary')!)

    expect(screen.getByLabelText('技能包名称')).toBeInTheDocument()
    expect(screen.queryByText('描述')).not.toBeInTheDocument()
    expect(screen.queryByText('标签')).not.toBeInTheDocument()
  })

  it.each(['edit', 'duplicate'] as const)('preserves a Skill Pack description and tags on %s', async (mode) => {
    const { pack, skill } = await renderPackPage({
      id: 'shared-workflow',
      description: 'Use this pack for deployment reviews. 部署检查。',
      tags: ['release', '团队'],
    })
    const save = vi.spyOn(skillApiV2, 'upsertPack').mockResolvedValue({
      ...pack,
      id: mode === 'edit' ? pack.id : 'copy-of-shared-workflow',
      name: 'Reviewed workflow',
    })

    if (mode === 'edit') {
      fireEvent.click(screen.getByRole('button', { name: '编辑' }))
    } else {
      fireEvent.click(screen.getByLabelText('更多操作'))
      fireEvent.click(screen.getByRole('menuitem', { name: '复制技能包' }))
    }
    const builder = within(screen.getByRole('dialog', { name: mode === 'edit' ? '编辑技能包' : '复制技能包' }))
    fireEvent.change(builder.getByLabelText('技能包名称'), { target: { value: 'Reviewed workflow' } })
    await act(async () => {
      fireEvent.click(builder.getByRole('button', { name: mode === 'edit' ? '保存更改' : '创建技能包' }))
    })

    expect(save).toHaveBeenCalledExactlyOnceWith({
      id: mode === 'edit' ? pack.id : '',
      name: 'Reviewed workflow',
      description: pack.description,
      tags: ['release', '团队'],
      skillIds: [skill.id],
    }, { deferSync: true, environmentId: LOCAL_RUNTIME_ENVIRONMENT_ID })
  })

  it('keeps pack builder skill choices visible and removable', async () => {
    const { SkillPackPage } = await import('../components/skills-v2/SkillPackPage')
    const { container } = render(<SkillPackPage />)

    fireEvent.click(container.querySelector('.sm2__header .sm2__btn--primary')!)
    fireEvent.click(screen.getByRole('button', { name: '添加 Release Checklist 到技能包' }))

    expect(screen.getByRole('button', { name: '从技能包移除 Release Checklist' })).toHaveAttribute('aria-pressed', 'true')
    expect(screen.getByRole('status', { name: '已选择 Skill 数量' })).toHaveTextContent('1')

    fireEvent.click(screen.getByRole('button', { name: '移除 Release Checklist' }))
    expect(screen.getByRole('button', { name: '添加 Release Checklist 到技能包' })).toHaveAttribute('aria-pressed', 'false')
    expect(screen.getByRole('status', { name: '已选择 Skill 数量' })).toHaveTextContent('0')
  })

  it('shows other skill pack memberships in the detail and editor member lists', async () => {
    const currentPack: SkillPackDetail = {
      id: 'daily-pack',
      name: '日常包',
      description: '',
      tags: [],
      members: [
        { skillId: 'release-checklist', skillName: 'Release Checklist', required: true, sortOrder: 0, missing: false },
      ],
      appliedAgents: [],
      createdAt: '',
      updatedAt: '',
    }
    const releasePack: SkillPackDetail = {
      ...currentPack,
      id: 'release-pack',
      name: '发布包',
    }
    const packs = [
      { id: currentPack.id, name: currentPack.name, description: '', tags: [], memberCount: 1, appliedAgentCount: 0, healthy: true },
      { id: releasePack.id, name: releasePack.name, description: '', tags: [], memberCount: 1, appliedAgentCount: 0, healthy: true },
    ]
    useSkillStoreV2.setState({
      overview: { ...makeOverview(), skills: [makeSkill()], packs },
      skills: [makeSkill()],
      packs,
      selectedPackId: currentPack.id,
      selectedPackDetail: currentPack,
      lastOverviewLoadedAt: Date.now(),
    })
    const getPackDetail = vi.spyOn(skillApiV2, 'getPackDetail').mockImplementation(async (packId) => (
      packId === releasePack.id ? releasePack : currentPack
    ))

    const { SkillPackPage } = await import('../components/skills-v2/SkillPackPage')
    render(<SkillPackPage />)

    expect(await screen.findByLabelText('还属于技能包：发布包')).toBeInTheDocument()
    expect(getPackDetail).toHaveBeenCalledWith('release-pack')

    fireEvent.click(screen.getByRole('button', { name: '编辑' }))
    const dialog = screen.getByRole('dialog', { name: '编辑技能包' })
    expect(within(dialog).getAllByLabelText('还属于技能包：发布包')).toHaveLength(2)
  })

  it('removes an applied pack member and forces Agent sync when saving', async () => {
    const pack: SkillPackDetail = {
      id: 'daily-pack',
      name: 'Daily Pack',
      description: '',
      tags: [],
      members: [
        { skillId: 'release-checklist', skillName: 'Release Checklist', required: true, sortOrder: 0, missing: false },
      ],
      appliedAgents: [
        { packId: 'daily-pack', packName: 'Daily Pack', memberCount: 1, agentId: 'codex', displayName: 'Codex', syncStatus: 'synced' },
      ],
      syncStatus: 'synced',
      pendingSyncCount: 0,
      failedSyncCount: 0,
      createdAt: '',
      updatedAt: '',
    }
    const pendingPack: SkillPackDetail = {
      ...pack,
      members: [],
      revision: 2,
      syncStatus: 'pending',
      pendingSyncCount: 1,
    }
    useSkillStoreV2.setState({
      skills: [makeSkill()],
      packs: [
        { id: pack.id, name: pack.name, description: '', tags: [], memberCount: 1, appliedAgentCount: 1, healthy: true },
      ],
      selectedPackId: pack.id,
      selectedPackDetail: pack,
      settings: {
        centerPath: '~/.agents/skills',
        sqlitePath: '~/.agentbro/skill-manager.db',
        defaultDistributeMode: 'link',
        linkFailPolicy: 'ask',
        startupScan: true,
        showUnmanaged: true,
        autoSyncSkillPacks: false,
      },
      lastOverviewLoadedAt: Date.now(),
    })
    const upsertPack = vi.spyOn(skillApiV2, 'upsertPack').mockResolvedValue(pendingPack)
    const syncPack = vi.spyOn(skillApiV2, 'syncPackToAgents').mockImplementation(() => new Promise(() => {}))

    const { SkillPackPage } = await import('../components/skills-v2/SkillPackPage')
    render(<SkillPackPage />)
    fireEvent.click(screen.getByRole('button', { name: '编辑' }))
    fireEvent.click(screen.getByRole('button', { name: '移除 Release Checklist' }))

    expect(screen.getByRole('button', { name: '添加 Release Checklist 到技能包' })).toHaveAttribute('aria-pressed', 'false')
    expect(screen.queryByText(/已应用技能包中的成员需要/)).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '保存更改' }))

    await waitFor(() => expect(upsertPack).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'daily-pack', skillIds: [] }),
      { deferSync: true, environmentId: LOCAL_RUNTIME_ENVIRONMENT_ID },
    ))
    await waitFor(() => expect(syncPack).toHaveBeenCalledWith('daily-pack', [], LOCAL_RUNTIME_ENVIRONMENT_ID))
    expect(screen.getByText('“Daily Pack”已保存，正在后台同步到 Agent…')).toBeInTheDocument()
  })

  it('closes the pack builder after the record is saved while Agent sync continues in the background', async () => {
    const pack: SkillPackDetail = {
      id: 'daily-pack',
      name: 'Daily Pack',
      description: '',
      tags: [],
      members: [
        { skillId: 'release-checklist', skillName: 'Release Checklist', required: true, sortOrder: 0, missing: false },
      ],
      appliedAgents: [
        { packId: 'daily-pack', packName: 'Daily Pack', memberCount: 1, agentId: 'codex', displayName: 'Codex', syncStatus: 'synced' },
      ],
      syncStatus: 'synced',
      pendingSyncCount: 0,
      failedSyncCount: 0,
      createdAt: '',
      updatedAt: '',
    }
    const pendingPack: SkillPackDetail = {
      ...pack,
      members: [
        ...pack.members,
        { skillId: 'pireel', skillName: 'pireel', required: true, sortOrder: 1, missing: false },
      ],
      revision: 2,
      syncStatus: 'pending',
      pendingSyncCount: 1,
    }
    useSkillStoreV2.setState({
      skills: [makeSkill(), makeSkill({ id: 'pireel', name: 'pireel', installedAgents: [] })],
      packs: [
        { id: pack.id, name: pack.name, description: '', tags: [], memberCount: 1, appliedAgentCount: 1, healthy: true },
      ],
      selectedPackId: pack.id,
      selectedPackDetail: pack,
      settings: {
        centerPath: '~/.agents/skills',
        sqlitePath: '~/.agentbro/skill-manager.db',
        defaultDistributeMode: 'link',
        linkFailPolicy: 'ask',
        startupScan: true,
        showUnmanaged: true,
        autoSyncSkillPacks: true,
      },
      lastOverviewLoadedAt: Date.now(),
    })
    const upsertPack = vi.spyOn(skillApiV2, 'upsertPack').mockResolvedValue(pendingPack)
    const syncPack = vi.spyOn(skillApiV2, 'syncPackToAgents').mockImplementation(() => new Promise(() => {}))

    const { SkillPackPage } = await import('../components/skills-v2/SkillPackPage')
    render(<SkillPackPage />)
    fireEvent.click(screen.getByRole('button', { name: '编辑' }))
    fireEvent.click(screen.getByRole('button', { name: '添加 pireel 到技能包' }))
    fireEvent.click(screen.getByRole('button', { name: '保存更改' }))

    await waitFor(() => expect(upsertPack).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'daily-pack', skillIds: ['release-checklist', 'pireel'] }),
      { deferSync: true, environmentId: LOCAL_RUNTIME_ENVIRONMENT_ID },
    ))
    await waitFor(() => expect(document.body.querySelector('.sm2__pack-builder-modal')).toBeNull())
    expect(syncPack).toHaveBeenCalledWith('daily-pack', [], LOCAL_RUNTIME_ENVIRONMENT_ID)
    expect(screen.getByText('“Daily Pack”已保存，正在后台同步到 Agent…')).toBeInTheDocument()
  })

  it('discards a Skill Pack save response after switching away and back', async () => {
    const { pack, skill } = await renderPackPage()
    let resolveSave!: (pack: SkillPackDetail) => void
    const pending = new Promise<SkillPackDetail>((resolve) => { resolveSave = resolve })
    const upsert = vi.spyOn(skillApiV2, 'upsertPack').mockReturnValue(pending)
    const sync = vi.spyOn(skillApiV2, 'syncPackToAgents').mockReturnValue(new Promise(() => {}))
    fireEvent.click(screen.getByRole('button', { name: /新建技能包/ }))
    const builder = within(screen.getByRole('dialog', { name: '创建技能包' }))
    fireEvent.change(builder.getByLabelText('技能包名称'), { target: { value: 'Saved elsewhere' } })
    fireEvent.click(builder.getByRole('button', { name: `添加 ${skill.name} 到技能包` }))
    fireEvent.click(builder.getByRole('button', { name: '创建技能包' }))
    expect(upsert).toHaveBeenCalledTimes(1)

    const { selectEnvironment } = useRuntimeEnvironmentStore.getState()
    act(() => {
      selectEnvironment('other-machine')
      selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    })
    await act(async () => {
      resolveSave({ ...pack, id: 'saved-elsewhere', name: 'Saved elsewhere', pendingSyncCount: 1, syncStatus: 'pending' })
    })

    expect(sync).not.toHaveBeenCalled()
    expect(useSkillStoreV2.getState().selectedPackId).toBe(pack.id)
    expect(screen.queryByRole('dialog', { name: '创建技能包' })).not.toBeInTheDocument()
  })

  it('discards a failed Skill Pack sync response after switching away and back', async () => {
    const { pack } = await renderPackPage({ id: 'sync-pack', pendingSyncCount: 1, syncStatus: 'pending' })
    type SyncResult = Awaited<ReturnType<typeof skillApiV2.syncPackToAgents>>
    let resolveSync!: (result: SyncResult) => void
    const pending = new Promise<SyncResult>((resolve) => { resolveSync = resolve })
    const sync = vi.spyOn(skillApiV2, 'syncPackToAgents').mockReturnValue(pending)
    const preview = vi.spyOn(skillApiV2, 'previewApplyPack')
    fireEvent.click(screen.getByRole('button', { name: '同步全部' }))
    expect(sync).toHaveBeenCalledTimes(1)

    const { selectEnvironment } = useRuntimeEnvironmentStore.getState()
    act(() => {
      selectEnvironment('other-machine')
      selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    })
    await act(async () => {
      resolveSync({
        packId: pack.id, packName: pack.name, revision: 1, status: 'failed',
        agents: [{ agentId: 'codex', displayName: 'Codex', status: 'failed', error: 'A target already exists.' }],
      })
    })

    expect(preview).not.toHaveBeenCalled()
    expect(screen.queryByText(/个 Agent 同步失败/)).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '同步全部' })).toBeEnabled()
  })

  it('keeps the newly selected Skill Pack after the previous sync finishes', async () => {
    const { pack, overview } = await renderPackPage({ id: 'sync-pack', pendingSyncCount: 1, syncStatus: 'pending' })
    const other = { ...pack, id: 'other-pack', name: 'Other Pack' }
    const packs = [...overview.packs, { ...overview.packs[0], id: other.id, name: other.name }]
    act(() => useSkillStoreV2.setState({ packs }))
    vi.mocked(skillApiV2.overview).mockResolvedValue({ ...overview, packs })
    vi.mocked(skillApiV2.getPackDetail).mockImplementation(async (id) => id === other.id ? other : pack)
    type SyncResult = Awaited<ReturnType<typeof skillApiV2.syncPackToAgents>>
    let resolveSync!: (result: SyncResult) => void
    const pending = new Promise<SyncResult>((resolve) => { resolveSync = resolve })
    const sync = vi.spyOn(skillApiV2, 'syncPackToAgents').mockReturnValue(pending)
    fireEvent.click(screen.getByRole('button', { name: '同步全部' }))
    expect(sync).toHaveBeenCalledWith(pack.id, [], LOCAL_RUNTIME_ENVIRONMENT_ID)
    fireEvent.click(screen.getByRole('button', { name: /^Other Pack/ }))
    expect(await screen.findByRole('heading', { name: other.name })).toBeInTheDocument()

    await act(async () => {
      resolveSync({ packId: pack.id, packName: pack.name, revision: 1, status: 'synced', agents: [] })
    })

    expect(useSkillStoreV2.getState().selectedPackId).toBe(other.id)
    expect(screen.getByRole('heading', { name: other.name })).toBeInTheDocument()
    expect(screen.queryByText('已同步 0 个 Agent。')).not.toBeInTheDocument()
  })

  it('filters pack builder skill choices by source', async () => {
    useSkillStoreV2.setState({
      skills: [
        makeSkill(),
        makeSkill({
          id: 'repo-helper',
          name: 'Repo Helper',
          description: 'GitHub imported helper',
          sourceType: 'github',
          sourceUri: 'github:owner/repo/repo-helper',
          installedAgents: [],
        }),
      ],
    })
    const { SkillPackPage } = await import('../components/skills-v2/SkillPackPage')
    const { container } = render(<SkillPackPage />)

    fireEvent.click(container.querySelector('.sm2__header .sm2__btn--primary')!)
    fireEvent.click(screen.getByRole('button', { name: 'GitHub 1' }))

    expect(screen.getByRole('button', { name: '添加 Repo Helper 到技能包' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '添加 Release Checklist 到技能包' })).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '全选当前' }))
    expect(screen.getByRole('status', { name: '已选择 Skill 数量' })).toHaveTextContent('1')
    expect(screen.getByRole('button', { name: '移除 Repo Helper' })).toBeInTheDocument()
  })

  it('treats the built-in full pack as read-only in the pack workspace', async () => {
    const defaultPack: SkillPackDetail = {
      id: 'default',
      name: '全量技能包',
      description: '中心库全部 Skills。无需维护成员，应用时按当前中心库全量分发。',
      tags: [],
      members: [
        { skillId: 'release-checklist', skillName: 'Release Checklist', required: true, sortOrder: 0, missing: false },
      ],
      appliedAgents: [],
      createdAt: '',
      updatedAt: '',
    }
    useSkillStoreV2.setState({
      packs: [
        { id: 'default', name: '全量技能包', description: defaultPack.description, tags: [], memberCount: 1, appliedAgentCount: 0, healthy: true },
      ],
      selectedPackId: 'default',
      selectedPackDetail: defaultPack,
    })

    const { SkillPackPage } = await import('../components/skills-v2/SkillPackPage')
    render(<SkillPackPage />)

    expect(screen.getAllByText('全量技能包').length).toBeGreaterThan(1)
    expect(screen.getByText(/系统内置入口/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '应用到 Agent' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '编辑' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '复制' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '删除' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '移除' })).not.toBeInTheDocument()
  })

  it('opens the shared Skill detail slider from a pack member', async () => {
    const pack: SkillPackDetail = {
      id: 'daily-pack',
      name: 'Daily Pack',
      description: 'Daily tools',
      tags: [],
      members: [
        { skillId: 'release-checklist', skillName: 'Release Checklist', required: true, sortOrder: 0, missing: false },
      ],
      appliedAgents: [],
      createdAt: '',
      updatedAt: '',
    }
    useSkillStoreV2.setState({
      packs: [
        { id: pack.id, name: pack.name, description: pack.description, tags: [], memberCount: 1, appliedAgentCount: 0, healthy: true },
      ],
      selectedPackId: pack.id,
      selectedPackDetail: pack,
    })
    const getSkillDetail = vi.spyOn(skillApiV2, 'getSkillDetail').mockResolvedValue({
      ...makeSkill(),
      frontmatter: {},
      files: null,
      targets: [],
      source: null,
    })

    const { SkillPackPage } = await import('../components/skills-v2/SkillPackPage')
    render(<SkillPackPage />)

    fireEvent.click(screen.getByRole('button', { name: '查看 Skill 详情 Release Checklist' }))

    await waitFor(() => expect(getSkillDetail).toHaveBeenCalledWith('release-checklist'))
    expect(document.body.querySelector('.sm2__slideover--skill-detail')).not.toBeNull()
  })

  it.each([false, true])('scopes the pack sync blocker dialog, switch runtime=%s', async (switchRuntime) => {
    const pack: SkillPackDetail = {
      id: 'pack-ant',
      name: '蚂蚁Skill',
      description: '',
      tags: [],
      members: [
        { skillId: 'antcode-skill', skillName: 'antcode-skill', required: true, sortOrder: 0, missing: false },
      ],
      appliedAgents: [
        {
          packId: 'pack-ant',
          packName: '蚂蚁Skill',
          memberCount: 1,
          agentId: 'codex',
          displayName: 'Codex',
          syncStatus: 'failed',
          syncError: '1 blocker(s) need manual resolution before syncing.',
        },
      ],
      pendingSyncCount: 0,
      failedSyncCount: 1,
      syncStatus: 'failed',
      createdAt: '2026-01-01T00:00:00Z',
      updatedAt: '2026-01-01T00:00:00Z',
    }
    const overview = {
      ...makeOverview(),
      packs: [
        { id: 'pack-ant', name: '蚂蚁Skill', description: '', tags: [], memberCount: 1, appliedAgentCount: 1, healthy: true, pendingSyncCount: 0, failedSyncCount: 1, syncStatus: 'failed' },
      ],
      agents: [
        { id: 'codex', displayName: 'Codex', iconKey: 'codex', enabled: true, skillsDir: '/Users/me/.codex/skills', version: null, latestVersion: null, installed: true, managedSkillCount: 0, unmanagedSkillCount: 1 } as AgentSummary,
      ],
    }
    vi.spyOn(skillApiV2, 'overview').mockResolvedValue(overview)
    vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    vi.spyOn(skillApiV2, 'getPackDetail').mockResolvedValue(pack)
    useSkillStoreV2.setState({
      overview,
      packs: [
        { id: 'pack-ant', name: '蚂蚁Skill', description: '', tags: [], memberCount: 1, appliedAgentCount: 1, healthy: true, pendingSyncCount: 0, failedSyncCount: 1, syncStatus: 'failed' },
      ],
      selectedPackId: 'pack-ant',
      selectedPackDetail: pack,
      agents: [
        { id: 'codex', displayName: 'Codex', iconKey: 'codex', enabled: true, skillsDir: '/Users/me/.codex/skills', version: null, latestVersion: null, installed: true, managedSkillCount: 0, unmanagedSkillCount: 1 } as AgentSummary,
      ],
      settings: {
        centerPath: '~/.agentbro/skills',
        sqlitePath: '~/.agentbro/skill-manager.db',
        defaultDistributeMode: 'link',
        linkFailPolicy: 'ask',
        startupScan: true,
        showUnmanaged: true,
      },
      lastOverviewLoadedAt: Date.now(),
    })
    vi.spyOn(skillApiV2, 'syncPackToAgents').mockResolvedValue({
      packId: 'pack-ant',
      packName: '蚂蚁Skill',
      revision: 2,
      status: 'failed',
      agents: [
        { agentId: 'codex', displayName: 'Codex', status: 'failed', error: '1 blocker(s) need manual resolution before syncing.' },
      ],
    })
    const preview: DistributionPreview = {
      skillIds: ['antcode-skill'],
      targetAgents: ['codex'],
      requestedMode: 'link',
      changes: [],
      blockers: [
        {
          skillId: 'antcode-skill',
          agentId: 'codex',
          reason: "An unmanaged 'antcode-skill' already exists at the target path. Adopt/overwrite/rename it first.",
          existingPath: '/Users/me/.codex/skills/antcode-skill',
          existingPathKind: 'directory',
          resolvedExistingPath: null,
        },
      ],
      blockerDecisions: [],
    }
    const previewApply = vi.spyOn(skillApiV2, 'previewApplyPack').mockResolvedValue(preview)
    const execute = vi.spyOn(skillApiV2, 'executeApplyPack').mockResolvedValue({ ...preview, blockers: [] })

    const { SkillPackPage } = await import('../components/skills-v2/SkillPackPage')
    render(<SkillPackPage />)

    fireEvent.click(screen.getByRole('button', { name: '同步' }))

    expect(await screen.findByText('确认同步冲突')).toBeInTheDocument()
    expect(screen.getAllByText(/未接管的同名 Skill/).length).toBeGreaterThan(0)
    expect(screen.getByText(/antcode-skill\/codex/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '覆盖安装' })).toBeInTheDocument()
    expect(previewApply).toHaveBeenCalledWith(pack.id, ['codex'], 'link', LOCAL_RUNTIME_ENVIRONMENT_ID)
    fireEvent.click(screen.getByRole('button', { name: '忽略此目标' }))
    const confirm = screen.getByRole('button', { name: '按选择同步' })
    if (switchRuntime) {
      const { selectEnvironment } = useRuntimeEnvironmentStore.getState()
      act(() => {
        selectEnvironment('other-machine')
        selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
      })
      fireEvent.click(confirm)
      expect(execute).not.toHaveBeenCalled()
      expect(screen.queryByText('确认同步冲突')).not.toBeInTheDocument()
      return
    }

    fireEvent.click(confirm)
    expect(execute).toHaveBeenCalledWith(
      pack.id, ['codex'], 'link', [{ skillId: 'antcode-skill', agentId: 'codex', action: 'skip' }],
      preview, LOCAL_RUNTIME_ENVIRONMENT_ID,
    )
    await waitFor(() => expect(screen.queryByText('确认同步冲突')).not.toBeInTheDocument())
  })

  it.each([false, true])('drops a Pack deletion review after changing runtimes, already visible=%s', async (visible) => {
    const { pack } = await renderPackPage({ id: 'review-pack' })
    const preview = {
      packId: pack.id, packName: pack.name, appliedAgents: [], affectedTargets: [], removable: true, warnings: [],
    }
    let resolveReview!: (value: typeof preview) => void
    const pending = new Promise<typeof preview>((resolve) => { resolveReview = resolve })
    vi.spyOn(skillApiV2, 'previewDeletePack').mockReturnValue(pending)
    const remove = vi.spyOn(skillApiV2, 'deletePack').mockResolvedValue(undefined)
    fireEvent.click(screen.getByLabelText('更多操作'))
    fireEvent.click(screen.getByRole('menuitem', { name: '删除技能包' }))

    if (visible) await act(async () => { resolveReview(preview) })
    const confirm = screen.queryByRole('button', { name: '删除' })
    if (visible) expect(confirm).toBeInTheDocument()
    const { selectEnvironment } = useRuntimeEnvironmentStore.getState()
    act(() => {
      selectEnvironment('other-machine')
      selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    })
    if (!visible) await act(async () => { resolveReview(preview) })
    if (confirm) fireEvent.click(confirm)

    expect(remove).not.toHaveBeenCalled()
    expect(screen.queryByRole('dialog', { name: `删除技能包「${pack.name}」` })).not.toBeInTheDocument()
    expect(useSkillStoreV2.getState().selectedPackId).toBe(pack.id)
  })

  it.each([false, true])('preserves the selected Pack after a member removal, switch runtime=%s', async (switchRuntime) => {
    const { pack, skill, overview } = await renderPackPage({ id: 'removal-pack' })
    const other = { ...pack, id: 'other-pack', name: 'Other Pack' }
    const packs = [...overview.packs, { ...overview.packs[0], id: other.id, name: other.name }]
    act(() => useSkillStoreV2.setState({ packs }))
    vi.mocked(skillApiV2.overview).mockResolvedValue({ ...overview, packs })
    vi.mocked(skillApiV2.getPackDetail).mockImplementation(async (id) => id === other.id ? other : pack)
    vi.spyOn(skillApiV2, 'previewRemoveSkillFromPack').mockResolvedValue({
      packId: pack.id, packName: pack.name, skillId: skill.id, skillName: skill.name,
      affectedTargets: [], appliedAgentCount: 0, canKeepStandalone: true, canRemoveTargets: true,
    })
    let finishRemoval!: () => void
    const pending = new Promise<void>((resolve) => { finishRemoval = resolve })
    const remove = vi.spyOn(skillApiV2, 'removeSkillFromPack').mockReturnValue(pending)
    fireEvent.click(screen.getByRole('button', { name: '移除' }))
    fireEvent.click(await screen.findByRole('button', { name: switchRuntime ? '同步从 Agent 移除' : '保留为直接分发' }))
    expect(remove).toHaveBeenCalledWith(pack.id, skill.id, switchRuntime, LOCAL_RUNTIME_ENVIRONMENT_ID)

    if (switchRuntime) {
      const { selectEnvironment } = useRuntimeEnvironmentStore.getState()
      act(() => {
        selectEnvironment('other-machine')
        selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
      })
    }
    fireEvent.click(screen.getByRole('button', { name: /^Other Pack/ }))
    expect(await screen.findByRole('heading', { name: other.name })).toBeInTheDocument()
    await act(async () => { finishRemoval() })

    expect(useSkillStoreV2.getState().selectedPackId).toBe(other.id)
    expect(screen.getByRole('heading', { name: other.name })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '保留为直接分发' })).not.toBeInTheDocument()
  })

  it('refreshes the selected agent detail after revoking a pack from the pack workspace', async () => {
    const appliedPack = { packId: 'default', packName: '全量技能包', memberCount: 1, agentId: 'claude-code', displayName: 'Claude Code' }
    const defaultPack: SkillPackDetail = {
      id: 'default',
      name: '全量技能包',
      description: '中心库全部 Skills。无需维护成员，应用时按当前中心库全量分发。',
      tags: [],
      members: [
        { skillId: 'release-checklist', skillName: 'Release Checklist', required: true, sortOrder: 0, missing: false },
      ],
      appliedAgents: [appliedPack],
      createdAt: '',
      updatedAt: '',
    }
    const refreshedPack = { ...defaultPack, appliedAgents: [] }
    const refreshedAgent = {
      ...agentDetail,
      appliedPacks: [],
      availablePacks: [
        { id: 'default', name: '全量技能包', description: '', tags: [], memberCount: 1, appliedAgentCount: 0, healthy: true },
      ],
    }
    vi.spyOn(skillApiV2, 'overview').mockResolvedValue(makeOverview())
    vi.spyOn(skillApiV2, 'previewRemovePackFromAgent').mockResolvedValue({
      packId: 'default',
      packName: '全量技能包',
      agentId: 'claude-code',
      displayName: 'Claude Code',
      affectedTargets: [],
      willRemoveTargets: 1,
      willPreserveTargets: 0,
    })
    vi.spyOn(skillApiV2, 'removePackFromAgent').mockResolvedValue({
      packId: 'default',
      agentId: 'claude-code',
      removedClaims: 1,
      removedTargets: 1,
      preservedTargets: 0,
    })
    vi.spyOn(skillApiV2, 'getPackDetail').mockResolvedValue(refreshedPack)
    const loadAgentDetail = vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockImplementation(async () => {
      useSkillStoreV2.setState({ selectedAgentDetail: refreshedAgent })
    })
    useSkillStoreV2.setState({
      packs: [
        { id: 'default', name: '全量技能包', description: defaultPack.description, tags: [], memberCount: 1, appliedAgentCount: 1, healthy: true },
      ],
      selectedPackId: 'default',
      selectedPackDetail: defaultPack,
      selectedAgentId: 'claude-code',
      selectedAgentDetail: {
        ...agentDetail,
        appliedPacks: [appliedPack],
      },
    })

    const { SkillPackPage } = await import('../components/skills-v2/SkillPackPage')
    render(<SkillPackPage />)

    fireEvent.click(screen.getByRole('tab', { name: /已应用 Agent/ }))
    fireEvent.click(screen.getByRole('button', { name: '撤销' }))
    fireEvent.click(await screen.findByRole('button', { name: '撤销技能包' }))

    await waitFor(() => expect(loadAgentDetail).toHaveBeenCalledWith('claude-code', true))
    expect(useSkillStoreV2.getState().selectedAgentDetail?.appliedPacks).toEqual([])
  })

  it('shows pack toggles in the Agent Skills tab', async () => {
    useSkillStoreV2.setState({
      selectedAgentDetail: {
        ...agentDetail,
        availablePacks: [
          { id: 'default', name: '全量技能包', description: '', tags: [], memberCount: 73, appliedAgentCount: 0, healthy: true },
          { id: 'pack-review', name: 'Code Review', description: '', tags: [], memberCount: 3, appliedAgentCount: 0, healthy: true },
        ],
      },
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)

    fireEvent.click(screen.getByText('Skills (2)'))

    expect(screen.getByRole('button', { name: '应用 全量技能包' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '应用 Code Review' })).toBeInTheDocument()
    expect(screen.getByLabelText('已应用 0 个，共 2 个技能包')).toHaveTextContent('0/ 2 已应用')
  })

  it('keeps the full pack first and groups other applied packs before unapplied packs', async () => {
    useSkillStoreV2.setState({
      selectedAgentDetail: {
        ...agentDetail,
        appliedPacks: [
          { packId: 'pack-applied', packName: 'Applied Pack', memberCount: 2, agentId: 'claude-code', displayName: 'Claude Code' },
          { packId: 'pack-applied-only', packName: 'Applied Only Pack', memberCount: 1, agentId: 'claude-code', displayName: 'Claude Code' },
        ],
        availablePacks: [
          { id: 'default', name: '全量技能包', description: '', tags: [], memberCount: 73, appliedAgentCount: 0, healthy: true },
          { id: 'pack-unapplied-first', name: 'Unapplied First', description: '', tags: [], memberCount: 3, appliedAgentCount: 0, healthy: true },
          { id: 'pack-applied', name: 'Applied Pack', description: '', tags: [], memberCount: 2, appliedAgentCount: 1, healthy: true },
          { id: 'pack-unapplied-last', name: 'Unapplied Last', description: '', tags: [], memberCount: 4, appliedAgentCount: 0, healthy: true },
        ],
      },
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))

    const packButtons = screen.getByLabelText('技能包应用').querySelectorAll<HTMLButtonElement>('.sm2__agent-pack-toggle')
    expect(Array.from(packButtons, (button) => button.getAttribute('aria-label'))).toEqual([
      '应用 全量技能包',
      '取消应用 Applied Pack',
      '取消应用 Applied Only Pack',
      '应用 Unapplied First',
      '应用 Unapplied Last',
    ])
    expect(screen.getByLabelText('已应用 2 个，共 5 个技能包')).toHaveTextContent('2/ 5 已应用')
  })

  it('applies an unapplied pack from the Agent Skills tab without a preview round trip', async () => {
    const preview: DistributionPreview = {
      skillIds: ['release-checklist'],
      targetAgents: ['claude-code'],
      requestedMode: 'link',
      changes: [],
      blockers: [],
      blockerDecisions: [],
    }
    const previewApply = vi.spyOn(skillApiV2, 'previewApplyPack')
    const executeApply = vi.spyOn(skillApiV2, 'executeApplyPack').mockResolvedValue(preview)
    const loadAgentDetail = vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue(undefined)
    const loadOverview = vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue(undefined)
    useSkillStoreV2.setState({
      selectedAgentDetail: {
        ...agentDetail,
        availablePacks: [
          { id: 'default', name: '全量技能包', description: '', tags: [], memberCount: 73, appliedAgentCount: 0, healthy: true },
        ],
      },
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))

    const applyButton = screen.getByRole('button', { name: '应用 全量技能包' })
    expect(applyButton).not.toHaveClass('sm2__agent-pack-toggle--applied')

    fireEvent.click(applyButton)

    await waitFor(() => expect(executeApply).toHaveBeenCalledWith('default', ['claude-code'], 'link', [], undefined, LOCAL_RUNTIME_ENVIRONMENT_ID))
    expect(previewApply).not.toHaveBeenCalled()
    expect(loadAgentDetail).toHaveBeenCalledWith('claude-code', true)
    expect(loadOverview).toHaveBeenCalledWith(true)
  })

  it('removes an applied pack from the Agent Skills tab without a preview round trip', async () => {
    const previewRemove = vi.spyOn(skillApiV2, 'previewRemovePackFromAgent')
    const removePack = vi.spyOn(skillApiV2, 'removePackFromAgent').mockResolvedValue({
      packId: 'default',
      agentId: 'claude-code',
      removedClaims: 1,
      removedTargets: 1,
      preservedTargets: 0,
    })
    const loadAgentDetail = vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue(undefined)
    const loadOverview = vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue(undefined)
    useSkillStoreV2.setState({
      selectedAgentDetail: {
        ...agentDetail,
        appliedPacks: [
          { packId: 'default', packName: '全量技能包', memberCount: 73, agentId: 'claude-code', displayName: 'Claude Code' },
        ],
        availablePacks: [
          { id: 'default', name: '全量技能包', description: '', tags: [], memberCount: 73, appliedAgentCount: 1, healthy: true },
        ],
      },
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '取消应用 全量技能包' }))

    await waitFor(() => expect(removePack).toHaveBeenCalledWith('default', 'claude-code', LOCAL_RUNTIME_ENVIRONMENT_ID))
    expect(previewRemove).not.toHaveBeenCalled()
    expect(loadAgentDetail).toHaveBeenCalledWith('claude-code', true)
    expect(loadOverview).toHaveBeenCalledWith(true)
  })

  it('clears stale page errors after a successful pack toggle', async () => {
    vi.spyOn(skillApiV2, 'previewRemovePackFromAgent')
    vi.spyOn(skillApiV2, 'removePackFromAgent').mockResolvedValue({
      packId: 'default',
      agentId: 'claude-code',
      removedClaims: 1,
      removedTargets: 1,
      preservedTargets: 0,
    })
    vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue(undefined)
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue(undefined)
    useSkillStoreV2.setState({
      error: 'Query returned no rows',
      selectedAgentDetail: {
        ...agentDetail,
        appliedPacks: [
          { packId: 'default', packName: '全量技能包', memberCount: 73, agentId: 'claude-code', displayName: 'Claude Code' },
        ],
        availablePacks: [
          { id: 'default', name: '全量技能包', description: '', tags: [], memberCount: 73, appliedAgentCount: 1, healthy: true },
        ],
      },
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    expect(screen.getByText('Query returned no rows')).toBeInTheDocument()

    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '取消应用 全量技能包' }))

    await waitFor(() => expect(screen.queryByText('Query returned no rows')).not.toBeInTheDocument())
  })

  it.each([false, true])('ignores a late Agent pack apply error after leaving its runtime (returned: %s)', async (returnToLocal) => {
    let failApply!: (error: Error) => void
    const pending = new Promise<DistributionPreview>((_resolve, reject) => { failApply = reject })
    const apply = vi.spyOn(skillApiV2, 'executeApplyPack').mockReturnValueOnce(pending)
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue(undefined)
    useSkillStoreV2.setState({
      selectedAgentDetail: {
        ...agentDetail,
        availablePacks: [
          { id: 'default', name: '全量技能包', description: '', tags: [], memberCount: 1, appliedAgentCount: 0, healthy: true },
        ],
      },
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '应用 全量技能包' }))
    expect(apply).toHaveBeenCalledTimes(1)

    await act(async () => {
      const runtime = useRuntimeEnvironmentStore.getState()
      runtime.selectEnvironment('other-host')
      if (returnToLocal) runtime.selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
      useSkillStoreV2.setState({
        runtimeEnvironmentId: returnToLocal ? LOCAL_RUNTIME_ENVIRONMENT_ID : 'other-host',
        error: 'Current runtime status',
      })
      failApply(new Error('Original runtime pack apply failed'))
    })

    expect(useSkillStoreV2.getState().error).toBe('Current runtime status')
    expect(screen.queryByRole('status', { name: '技能包应用进度' })).not.toBeInTheDocument()
  })

  it('ignores a completed Agent pack revoke after leaving its runtime', async () => {
    const result = { packId: 'default', agentId: 'claude-code', removedClaims: 1, removedTargets: 1, preservedTargets: 0 }
    let finishRevoke!: (value: typeof result) => void
    const pending = new Promise<typeof result>((resolve) => { finishRevoke = resolve })
    const revoke = vi.spyOn(skillApiV2, 'removePackFromAgent').mockReturnValueOnce(pending)
    const store = useSkillStoreV2.getState()
    const loadDetail = vi.spyOn(store, 'loadAgentDetail').mockResolvedValue(undefined)
    const loadOverview = vi.spyOn(store, 'loadOverview').mockResolvedValue(undefined)
    useSkillStoreV2.setState({
      selectedAgentDetail: {
        ...agentDetail,
        appliedPacks: [
          { packId: 'default', packName: '全量技能包', memberCount: 1, agentId: 'claude-code', displayName: 'Claude Code' },
        ],
      },
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '取消应用 全量技能包' }))
    expect(revoke).toHaveBeenCalledTimes(1)
    loadDetail.mockClear()
    loadOverview.mockClear()

    await act(async () => {
      useRuntimeEnvironmentStore.getState().selectEnvironment('other-host')
      useSkillStoreV2.setState({ runtimeEnvironmentId: 'other-host', error: 'Current runtime status' })
      finishRevoke(result)
    })

    expect(loadDetail).not.toHaveBeenCalled()
    expect(loadOverview).not.toHaveBeenCalled()
    expect(useSkillStoreV2.getState().error).toBe('Current runtime status')
    expect(screen.queryByText('技能包已取消应用')).not.toBeInTheDocument()
  })

  it.each([false, true])('applies a reviewed Agent pack only in the original runtime view (switched: %s)', async (switched) => {
    const resolved: DistributionPreview = {
      skillIds: ['release-checklist'], targetAgents: ['claude-code'], requestedMode: 'link',
      changes: [], blockers: [], blockerDecisions: [],
    }
    const review: DistributionPreview = {
      ...resolved,
      blockers: [{
        skillId: 'release-checklist', agentId: 'claude-code', reason: 'Conflicting Skill',
        existingPath: '/c/skills/release-checklist',
      }],
    }
    const execute = vi.spyOn(skillApiV2, 'executeApplyPack').mockResolvedValueOnce(review).mockResolvedValueOnce(resolved)
    const store = useSkillStoreV2.getState()
    vi.spyOn(store, 'loadAgentDetail').mockResolvedValue(undefined)
    vi.spyOn(store, 'loadOverview').mockResolvedValue(undefined)
    useSkillStoreV2.setState({
      selectedAgentDetail: {
        ...agentDetail,
        availablePacks: [
          { id: 'default', name: '全量技能包', description: '', tags: [], memberCount: 1, appliedAgentCount: 0, healthy: true },
        ],
      },
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '应用 全量技能包' }))
    const dialog = await screen.findByRole('dialog', { name: '应用「全量技能包」' })
    fireEvent.click(within(dialog).getByRole('button', { name: '忽略此目标' }))
    const confirm = within(dialog).getByRole('button', { name: '按选择执行' })

    if (switched) {
      await act(async () => {
        const runtime = useRuntimeEnvironmentStore.getState()
        runtime.selectEnvironment('other-host')
        runtime.selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
      })
      expect(dialog).not.toBeInTheDocument()
      fireEvent.click(confirm)
      expect(execute).toHaveBeenCalledTimes(1)
      expect(screen.queryByRole('status', { name: '技能包应用进度' })).not.toBeInTheDocument()
    } else {
      fireEvent.click(confirm)
      await waitFor(() => expect(execute).toHaveBeenNthCalledWith(
        2, 'default', ['claude-code'], 'link',
        [{ skillId: 'release-checklist', agentId: 'claude-code', action: 'skip' }],
        review, LOCAL_RUNTIME_ENVIRONMENT_ID,
      ))
      await waitFor(() => expect(dialog).not.toBeInTheDocument())
    }
  })

  it('shows a floating progress toast immediately while applying a pack', async () => {
    const preview: DistributionPreview = {
      skillIds: ['release-checklist'],
      targetAgents: ['claude-code'],
      requestedMode: 'link',
      changes: [],
      blockers: [],
      blockerDecisions: [],
    }
    let resolveExecute: (value: DistributionPreview) => void = () => {}
    const executePromise = new Promise<DistributionPreview>((resolve) => {
      resolveExecute = resolve
    })
    vi.spyOn(skillApiV2, 'previewApplyPack')
    vi.spyOn(skillApiV2, 'executeApplyPack').mockReturnValue(executePromise)
    vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue(undefined)
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue(undefined)
    useSkillStoreV2.setState({
      selectedAgentDetail: {
        ...agentDetail,
        availablePacks: [
          { id: 'default', name: '全量技能包', description: '', tags: [], memberCount: 73, appliedAgentCount: 0, healthy: true },
        ],
      },
    })

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '应用 全量技能包' }))

    const progressToast = await screen.findByRole('status', { name: '技能包应用进度' })
    expect(progressToast).toHaveClass('sm2__pack-apply-progress--floating')
    expect(progressToast).toHaveTextContent('正在应用')
    expect(progressToast).toHaveTextContent('全量技能包')
    expect(screen.queryByText('确认应用「全量技能包」？')).not.toBeInTheDocument()
    expect(skillApiV2.previewApplyPack).not.toHaveBeenCalled()
    expect(screen.getByRole('progressbar', { name: '技能包应用进度条' })).toHaveAttribute('aria-valuenow')

    resolveExecute(preview)

    await waitFor(() => expect(progressToast).toHaveTextContent('已应用'))
    expect(screen.getByRole('progressbar', { name: '技能包应用进度条' })).toHaveAttribute('aria-valuenow', '100')
  })

  it('auto dismisses the completed pack progress toast', async () => {
    vi.useFakeTimers()
    const preview: DistributionPreview = {
      skillIds: ['release-checklist'],
      targetAgents: ['claude-code'],
      requestedMode: 'link',
      changes: [],
      blockers: [],
      blockerDecisions: [],
    }
    vi.spyOn(skillApiV2, 'executeApplyPack').mockResolvedValue(preview)
    vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue(undefined)
    vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue(undefined)
    useSkillStoreV2.setState({
      selectedAgentDetail: {
        ...agentDetail,
        availablePacks: [
          { id: 'default', name: '全量技能包', description: '', tags: [], memberCount: 73, appliedAgentCount: 0, healthy: true },
        ],
      },
    })

    try {
      const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
      render(<AgentManagementPage />)
      fireEvent.click(screen.getByText('Skills (2)'))
      fireEvent.click(screen.getByRole('button', { name: '应用 全量技能包' }))

      await act(async () => {
        await Promise.resolve()
        await Promise.resolve()
        await Promise.resolve()
      })
      expect(screen.getByRole('status', { name: '技能包应用进度' })).toHaveTextContent('已应用')

      await act(async () => {
        vi.advanceTimersByTime(2500)
      })

      expect(screen.queryByRole('status', { name: '技能包应用进度' })).not.toBeInTheDocument()
    } finally {
      vi.useRealTimers()
    }
  })
})

function makeProgram(overrides: Partial<AgentProgramInfo> = {}): AgentProgramInfo {
  return {
    id: 'claude-code',
    displayName: 'Claude Code',
    icon: 'claude-code',
    kind: 'cli',
    status: 'installed',
    packageManager: 'npm',
    packageName: '@anthropic-ai/claude-code',
    installedVersion: null,
    latestVersion: null,
    binaryPath: '/usr/local/bin/claude',
    configDir: '/Users/me/.claude',
    appPath: null,
    downloadUrl: 'https://docs.anthropic.com/en/docs/claude-code',
    installCommand: 'npm install -g @anthropic-ai/claude-code',
    updateCommand: 'npm install -g @anthropic-ai/claude-code@latest',
    uninstallCommand: 'npm uninstall -g @anthropic-ai/claude-code',
    hooksInstalled: false,
    skillsDir: '/Users/me/.claude/skills',
    isCustom: false,
    ...overrides,
  }
}

function makeOverview() {
  return {
    metrics: { centerSkillCount: 0, targetCount: 0, unmanagedCount: 0, issueCount: 0 },
    skills: [],
    agents: [],
    packs: [],
    issues: [],
    settings: {
      centerPath: '~/.agents/skills',
      sqlitePath: '~/.agentbro/skill-manager.db',
      defaultDistributeMode: 'link' as const,
      linkFailPolicy: 'ask' as const,
      startupScan: true,
      showUnmanaged: true,
    },
  }
}

describe('Skill manager settings page', () => {
  beforeEach(() => {
    cleanup()
    useRuntimeEnvironmentStore.setState({ selectedEnvironmentId: LOCAL_RUNTIME_ENVIRONMENT_ID })
    useSkillStoreV2.setState({
      runtimeEnvironmentId: LOCAL_RUNTIME_ENVIRONMENT_ID,
      settings: {
        centerPath: '/Users/mac/.agentbro/skills',
        sqlitePath: '/Users/mac/.agentbro/skill-manager/skill-manager.db',
        defaultDistributeMode: 'link',
        linkFailPolicy: 'ask',
        startupScan: true,
        showUnmanaged: true,
      },
      error: null,
    })
  })

  it('shows a database load error and can retry after it becomes available', async () => {
    const { loadOverview, loadProjects } = useSkillStoreV2.getInitialState()
    useSkillStoreV2.setState({ settings: null, overview: null, loadOverview, loadProjects })
    const overview = vi.spyOn(skillApiV2, 'overview')
      .mockRejectedValueOnce('Could not initialize Skill Manager database: database is locked')
      .mockResolvedValueOnce(makeOverview())
    const unmanaged = vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    const projects = vi.spyOn(skillApiV2, 'listProjects').mockResolvedValue([])
    const { SettingsPageV2 } = await import('../components/skills-v2/SettingsPageV2')

    try {
      render(<SettingsPageV2 />)

      expect(await screen.findByRole('alert')).toHaveTextContent('database is locked')
      fireEvent.click(screen.getByRole('button', { name: i18n.t('settings.retry') }))

      expect(await screen.findByRole('button', { name: i18n.t('skills.revealSqlite') })).toBeInTheDocument()
      expect(overview).toHaveBeenCalledTimes(2)
      expect(screen.queryByRole('alert')).not.toBeInTheDocument()
    } finally {
      overview.mockRestore()
      unmanaged.mockRestore()
      projects.mockRestore()
    }
  })

  it('reveals the SQLite database in the file manager and shows feedback', async () => {
    const revealPath = vi.spyOn(skillApiV2, 'revealPath').mockResolvedValue(undefined)
    const { SettingsPageV2 } = await import('../components/skills-v2/SettingsPageV2')

    render(<SettingsPageV2 />)
    fireEvent.click(screen.getByRole('button', { name: i18n.t('skills.revealSqlite') }))

    await waitFor(() => expect(revealPath).toHaveBeenCalledWith('/Users/mac/.agentbro/skill-manager/skill-manager.db'))
    expect(await screen.findByText(i18n.t('skills.sqliteRevealed'))).toBeInTheDocument()
  })

  it('does not expose a center library path editor', async () => {
    const { SettingsPageV2 } = await import('../components/skills-v2/SettingsPageV2')

    render(<SettingsPageV2 />)

    expect(screen.queryByText('中心库路径')).not.toBeInTheDocument()
    expect(screen.queryByText('默认 ~/.agentbro/skills。修改后下次刷新生效。')).not.toBeInTheDocument()
  })
})

describe('Diagnosis workbench', () => {
  beforeEach(() => {
    cleanup()
    vi.restoreAllMocks()
    useSkillStoreV2.setState({
      issues: [],
      unmanaged: [],
      busyAction: null,
      error: null,
    })
  })

  it('summarizes diagnosis status with plain-language issue groups and safe-fix feedback', async () => {
    const initialIssues = [
      {
        id: 'snapshot-stale',
        issueType: 'snapshot_stale',
        severity: 'warning' as const,
        fixKind: 'auto' as const,
        title: 'JSON snapshot is out of date',
        detail: 'The center library changed since the snapshot was last written.',
        entityType: 'snapshot' as const,
        entityId: null,
        actions: [{ id: 'fix:snapshot_stale', label: 'Refresh snapshot', destructive: false }],
      },
      {
        id: 'agent-unmanaged-codex-bird',
        issueType: 'agent_unmanaged',
        severity: 'info' as const,
        fixKind: 'info' as const,
        title: 'Unmanaged skill in Codex',
        detail: '/Users/mac/.codex/skills/bird — reason: same_name_as_center_skill',
        entityType: 'target' as const,
        entityId: 'codex-bird',
        actions: [],
      },
      {
        id: 'copy-modified',
        issueType: 'copy_modified',
        severity: 'error' as const,
        fixKind: 'confirm' as const,
        title: 'Copy was modified locally',
        detail: "'/Users/mac/.codex/skills/release-checklist' differs from the center snapshot.",
        entityType: 'target' as const,
        entityId: 'target-1',
        actions: [{ id: 'fix:copy_modified', label: 'Push to center', destructive: true }],
      },
    ]
    const afterFixIssues = initialIssues.slice(1)
    const listDiagnosisIssues = vi.spyOn(skillApiV2, 'listDiagnosisIssues').mockResolvedValue(initialIssues)
    const runDiagnosis = vi.spyOn(skillApiV2, 'runDiagnosis').mockResolvedValue(afterFixIssues)
    vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    vi.spyOn(skillApiV2, 'executeSafeFixes').mockResolvedValue(1)

    const { DiagnosisPage } = await import('../components/skills-v2/DiagnosisPage')
    render(<DiagnosisPage />)

    expect(await screen.findByText('Skill 状态需要整理')).toBeInTheDocument()
    expect(listDiagnosisIssues).toHaveBeenCalledTimes(1)
    expect(runDiagnosis).not.toHaveBeenCalled()
    expect(screen.getAllByText('1 项').length).toBeGreaterThan(0)
    expect(screen.getAllByText('可安全修复').length).toBeGreaterThan(0)
    expect(screen.getAllByText('需要你确认').length).toBeGreaterThan(0)
    expect(screen.getAllByText('仅提示').length).toBeGreaterThan(0)
    expect(screen.getByText('未接管的 Skill')).toBeInTheDocument()
    expect(screen.getByText(/本地已有同名 Skill/)).toBeInTheDocument()
    expect(screen.queryByText(/same_name_as_center_skill/)).not.toBeInTheDocument()
    expect(screen.getByText(/不会删除你的 Skill 内容/)).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '修复安全项' }))

    expect(await screen.findByText('已处理 1 项安全问题，还剩 2 项需要查看。')).toBeInTheDocument()
    expect(runDiagnosis).toHaveBeenCalledTimes(1)
  })
})
