import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, screen, fireEvent, cleanup, waitFor, act, within } from '@testing-library/react'
import '@testing-library/jest-dom/vitest'
import { AgentIconBadge } from '../components/skills-v2/AgentIconBadge'
import { useSkillStoreV2 } from '../stores/skillStoreV2'
import { useSessionStore } from '../stores/sessionStore'
import { skillApiV2 } from '../services/skillApiV2'
import { agentApi, type AgentProgramInfo } from '../services/agentApi'
import * as tauriApi from '../services/tauriApi'
import { open as openShell } from '@tauri-apps/plugin-shell'
import i18n from '../i18n'
import type { SkillSummary, AgentSummary, AgentDetail, AgentSkillInventoryAgent, AdoptPreview, DistributionPreview, MoveDirectSkillToPackPreview, SkillPackDetail, SkillDetail, SkillTargetDetail } from '../services/skillApiV2'
import type { AgentType, SessionState } from '../types/agent'

// SkillManagerShell imports pages that call skillApiV2 at mount; we stub the api
// so tests run without the Tauri runtime.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(undefined) }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn().mockResolvedValue(null) }))
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
      )
    })

    await screen.findAllByText('algorithmic-art')
    fireEvent.click(screen.getByText('安装所选 (2)'))
    await waitFor(() => {
      expect(install).toHaveBeenCalledWith(
        'https://github.com/acme/private-skills',
        expect.any(Array),
        'github-test-token',
      )
    })
  })
})

describe('Local skill import', () => {
  beforeEach(() => {
    cleanup()
    vi.restoreAllMocks()
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
})

describe('Agent sync local agent chips', () => {
  beforeEach(() => {
    cleanup()
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
    vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValue(inventory)
    vi.spyOn(skillApiV2, 'executeAdopt').mockImplementationOnce(
      () => new Promise<string>((resolve) => {
        finishFirstAdopt = () => resolve('')
      }),
    )

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    render(<AgentSyncPanel onDone={() => {}} />)

    fireEvent.click(await screen.findByText('选择当前可接管'))
    fireEvent.click(screen.getByText('接管到中心库'))

    expect(await screen.findByRole('status')).toHaveTextContent('正在接管 1 / 2')
    expect(screen.getByRole('status')).toHaveTextContent('alpha')
    expect(screen.getByRole('status')).toHaveClass('sm2__agent-sync-progress--floating')
    expect(screen.getByLabelText('选择 Agent')).not.toBeDisabled()
    fireEvent.click(screen.getByText('beta'))
    expect(document.body.querySelector('.sm2__slideover-title')).toHaveTextContent('beta')
    finishFirstAdopt()
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
    useSkillStoreV2.setState({
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
  })

  it('SkillDetailSlider renders when open', async () => {
    const { SkillDetailSlider } = await import('../components/skills-v2/SkillDetailSlider')
    const { container } = render(<SkillDetailSlider skillId="release-checklist" open={true} onClose={() => {}} onDistribute={() => {}} onDelete={() => {}} />)
    // portal renders the slide-over
    expect(document.body.querySelector('.sm2__slideover')).not.toBeNull()
    expect(container).toBeTruthy()
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
    }))
    expect(loadOverview).toHaveBeenCalledWith(true)
    expect(screen.queryByRole('dialog', { name: /添加 Claude Code 实例/ })).not.toBeInTheDocument()
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

    await waitFor(() => expect(removeCustom).toHaveBeenCalledWith('custom-antcc'))
    expect(loadOverview).toHaveBeenCalledWith(true)
    expect(useSkillStoreV2.getState().selectedAgentId).toBeNull()
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
    expect(container.querySelector('.sm2__agent-skill-card')).not.toBeNull()
    fireEvent.click(screen.getByText('列表'))
    expect(container.querySelector('.sm2__agent-skill-list')).not.toBeNull()
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
    expect(screen.getByRole('button', { name: '已管理 1' })).toBeInTheDocument()

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

  it('enters managed skill multi-select only after a batch action', async () => {
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))

    expect(screen.queryByLabelText('选择 release-checklist')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '批量选择' }))

    expect(screen.getByLabelText('选择 release-checklist')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '批量删除 0 个' })).toBeInTheDocument()
  })

  it('moves a directly distributed Skill into a selected skill pack', async () => {
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
        skillIds: ['frontend-design', 'source-check'],
        targetAgents: ['claude-code'],
        requestedMode: 'link',
        changes: [],
        blockers: [],
        blockerDecisions: [],
      },
    }
    const previewMove = vi.spyOn(skillApiV2, 'previewMoveDirectSkillToPack').mockResolvedValue(movePreview)
    const executeMove = vi.spyOn(skillApiV2, 'moveDirectSkillToPack').mockResolvedValue(movePreview)
    vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    const loadAgentDetail = vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue(undefined)
    const loadOverview = vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue(undefined)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '归入技能包 release-checklist' }))

    expect(screen.getByRole('heading', { name: '将直接分发归入技能包' })).toBeInTheDocument()
    await waitFor(() => expect(previewMove).toHaveBeenCalledWith('target-1', 'daily-pack'))
    expect(await screen.findByText('加入技能包成员')).toBeInTheDocument()
    expect(screen.getByText(/其余 2 个成员生效/)).toBeInTheDocument()
    expect(screen.getByText('切换为技能包控制')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '移动并交由技能包管理' }))
    await waitFor(() => expect(executeMove).toHaveBeenCalledWith('target-1', 'daily-pack', []))
    expect(loadAgentDetail).toHaveBeenCalledWith('claude-code', true)
    expect(loadOverview).toHaveBeenCalledWith(true)
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
    await waitFor(() => expect(previewMove).toHaveBeenCalledWith('target-1', 'nice-try'))
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
    const loadAgentDetail = vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue(undefined)
    const loadOverview = vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue(undefined)
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))

    fireEvent.click(screen.getByRole('button', { name: '删除 release-checklist' }))
    await waitFor(() => expect(deleteTargets).toHaveBeenCalledWith(['target-1']))

    fireEvent.click(screen.getByRole('button', { name: '批量选择' }))
    fireEvent.click(screen.getByLabelText('选择 release-checklist'))
    fireEvent.click(screen.getByRole('button', { name: '批量删除 1 个' }))
    expect(screen.getByRole('heading', { name: '确认批量删除 Skill？' })).toBeInTheDocument()
    expect(screen.getByText('1个SKILL 将从当前Agent直接删除，您后续仍旧可以从中心库安装')).toBeInTheDocument()
    expect(deleteTargets).toHaveBeenCalledTimes(1)

    fireEvent.click(screen.getByRole('button', { name: '确认删除' }))
    await waitFor(() => expect(deleteTargets).toHaveBeenCalledTimes(2))
    expect(loadAgentDetail).toHaveBeenCalledWith('claude-code', true)
    expect(loadOverview).toHaveBeenCalledWith(true)
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

  it('deletes an unmanaged agent skill from the skill card after confirmation', async () => {
    const deleteUnmanaged = vi.spyOn(skillApiV2, 'deleteUnmanagedAgentSkill').mockResolvedValue(undefined)
    vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    const loadAgentDetail = vi.spyOn(useSkillStoreV2.getState(), 'loadAgentDetail').mockResolvedValue(undefined)
    const loadOverview = vi.spyOn(useSkillStoreV2.getState(), 'loadOverview').mockResolvedValue(undefined)
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '未管理 1' }))

    fireEvent.click(screen.getByRole('button', { name: '删除 manual-skill' }))
    const dialog = screen.getByRole('dialog', { name: '删除 Skill「manual-skill」？' })
    expect(dialog).toBeInTheDocument()
    expect(dialog).toHaveTextContent('/c/skills/manual-skill')
    fireEvent.click(screen.getByRole('button', { name: '直接删除' }))

    await waitFor(() => expect(deleteUnmanaged).toHaveBeenCalledWith('claude-code', 'unmanaged-1'))
    expect(loadAgentDetail).toHaveBeenCalledWith('claude-code', true)
    expect(loadOverview).toHaveBeenCalledWith(true)
  })

  it('uses the shared owner and keeps deletion errors visible inside the confirmation dialog', async () => {
    const sharedItem = {
      id: 'unmanaged-shared-bird',
      agentId: 'agents',
      itemType: 'skill' as const,
      path: '/Users/me/.agents/skills/bird',
      inferredSkillId: 'bird',
      hash: null,
      reason: 'shared_agents_directory',
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
      unmanaged: [sharedItem],
    })
    const mismatch = new Error(
      "Unmanaged item 'unmanaged-shared-bird' does not belong to agent 'agents'.",
    )
    const deleteUnmanaged = vi.spyOn(skillApiV2, 'deleteUnmanagedAgentSkill').mockRejectedValue(mismatch)
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    fireEvent.click(screen.getByRole('button', { name: '未管理 1' }))
    fireEvent.click(screen.getByRole('button', { name: '删除 bird' }))

    const dialog = screen.getByRole('dialog', { name: '删除 Skill「bird」？' })
    expect(dialog).toHaveClass('sm2__modal--unmanaged-delete')
    fireEvent.click(within(dialog).getByRole('button', { name: '直接删除' }))

    await waitFor(() => expect(deleteUnmanaged).toHaveBeenCalledWith('agents', 'unmanaged-shared-bird'))
    expect(await within(dialog).findByText('该未管理 Skill 不属于 Agent「agents」，请重新扫描后重试。')).toBeInTheDocument()
    expect(useSkillStoreV2.getState().error).toBeNull()
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
      }])
    })
    expect(listUnmanaged).toHaveBeenCalled()
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

  it('syncs successfully batch-adopted unmanaged skills into an existing skill pack', async () => {
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
    vi.spyOn(skillApiV2, 'getPackDetail').mockResolvedValue({
      id: 'agent-tools',
      name: 'Agent Tools',
      description: 'Daily agent tools',
      tags: [],
      members: [{ skillId: 'release-checklist', skillName: 'Release Checklist', required: true, sortOrder: 0, missing: false }],
      appliedAgents: [],
      createdAt: '2026-01-01T00:00:00Z',
      updatedAt: '2026-01-01T00:00:00Z',
    } as SkillPackDetail)
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

    await waitFor(() => {
      expect(upsertPack).toHaveBeenCalledWith({
        id: 'agent-tools',
        name: 'Agent Tools',
        description: 'Daily agent tools',
        tags: [],
        skillIds: ['release-checklist', 'manual-skill'],
      })
    })
  })

  it('creates a skill pack from successfully batch-adopted unmanaged skills', async () => {
    vi.spyOn(skillApiV2, 'executeAdoptBatch').mockResolvedValue({
      items: [{ unmanagedId: 'unmanaged-1', skillId: 'manual-skill', error: null }],
      finalizationError: null,
    })
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

    await waitFor(() => {
      expect(upsertPack).toHaveBeenCalledWith({
        id: '',
        name: 'Manual Pack',
        description: '',
        tags: [],
        skillIds: ['manual-skill'],
      })
    })
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

    expect(await screen.findByText("目标路径已存在未管理的 Skill 'find-skills'。请先接管、覆盖或重命名。")).toBeInTheDocument()
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
    const scan = vi.spyOn(skillApiV2, 'scanAgentInventory').mockResolvedValue({ agentId: 'claude-code', managed: 1, unmanaged: 0 })
    vi.spyOn(skillApiV2, 'listUnmanaged').mockResolvedValue([])
    vi.spyOn(skillApiV2, 'overview').mockResolvedValue(makeOverview())
    vi.spyOn(skillApiV2, 'getAgentDetail').mockResolvedValue(agentDetail)

    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    const { container } = render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (1)'))
    fireEvent.click(screen.getByRole('button', { name: '未管理 0' }))

    const empty = container.querySelector('.sm2__unmanaged-empty')
    expect(empty).not.toBeNull()
    const button = empty?.querySelector('button')
    expect(button).toHaveTextContent('重新扫描此 Agent')

    fireEvent.click(button!)
    await waitFor(() => expect(scan).toHaveBeenCalledWith('claude-code'))
  })

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
    await waitFor(() => expect(container.querySelectorAll('.sm2__object-row--path')[1]).toHaveTextContent('2.1.179'))
    expect(screen.getByText('/Users/me/.claude/settings.json')).toBeInTheDocument()
    expect(screen.getByText('/Users/me/.claude/plugins/cache')).toBeInTheDocument()
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
    await waitFor(() => expect(install).toHaveBeenCalledWith('codex'))
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
    await waitFor(() => expect(update).toHaveBeenCalledWith('claude-code'))
  })

  it('uninstalls a supported agent from the page header after confirmation', async () => {
    useSkillStoreV2.setState({ unmanaged: [] })
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([makeProgram()])
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

    await waitFor(() => expect(uninstall).toHaveBeenCalledWith('claude-code'))
    expect(await screen.findByText('Agent「Claude Code」已卸载，已清理 1 个 Skills')).toBeInTheDocument()
  })

  it('removes an uninstalled Agent from the installed sidebar and selects the next one', async () => {
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
        makeSidebarAgent('codex', 'Codex'),
      ],
      unmanaged: [],
    })
    vi.spyOn(agentApi, 'refresh').mockResolvedValue([makeProgram()])
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

    fireEvent.click(await screen.findByRole('button', { name: '卸载 Agent' }))
    const dialog = screen.getByRole('dialog', { name: '卸载 Agent「Aider」' })
    expect(dialog).toHaveTextContent('未安装，仅清理残留')
    expect(dialog).toHaveTextContent('1已管理 Skills')
    expect(dialog).toHaveTextContent('1未管理 Skills')
    fireEvent.click(screen.getByRole('button', { name: '确认卸载' }))

    await waitFor(() => expect(deleteManaged).toHaveBeenCalledWith(['target-aider']))
    expect(deleteUnmanaged).toHaveBeenCalledWith('aider', 'unmanaged-aider')
    expect(uninstallHook).toHaveBeenCalledWith('aider')
    expect(uninstall).not.toHaveBeenCalled()
    expect(await screen.findByText('Agent「Aider」已卸载，已清理 2 个 Skills')).toBeInTheDocument()
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
      { deferSync: true },
    ))
    await waitFor(() => expect(syncPack).toHaveBeenCalledWith('daily-pack', []))
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
      { deferSync: true },
    ))
    await waitFor(() => expect(document.body.querySelector('.sm2__pack-builder-modal')).toBeNull())
    expect(syncPack).toHaveBeenCalledWith('daily-pack', [])
    expect(screen.getByText('“Daily Pack”已保存，正在后台同步到 Agent…')).toBeInTheDocument()
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

  it('opens a blocker resolution dialog when pack sync hits an unmanaged same-name skill', async () => {
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
    vi.spyOn(skillApiV2, 'previewApplyPack').mockResolvedValue({
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
    })

    const { SkillPackPage } = await import('../components/skills-v2/SkillPackPage')
    render(<SkillPackPage />)

    fireEvent.click(screen.getByRole('button', { name: '同步' }))

    expect(await screen.findByText('确认同步冲突')).toBeInTheDocument()
    expect(screen.getAllByText(/未接管的同名 Skill/).length).toBeGreaterThan(0)
    expect(screen.getByText(/antcode-skill\/codex/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '覆盖安装' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '忽略此目标' })).toBeInTheDocument()
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

    await waitFor(() => expect(executeApply).toHaveBeenCalledWith('default', ['claude-code'], 'link'))
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

    await waitFor(() => expect(removePack).toHaveBeenCalledWith('default', 'claude-code'))
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
    useSkillStoreV2.setState({
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

  it('reveals the SQLite database in Finder and shows feedback', async () => {
    const revealPath = vi.spyOn(skillApiV2, 'revealPath').mockResolvedValue(undefined)
    const { SettingsPageV2 } = await import('../components/skills-v2/SettingsPageV2')

    render(<SettingsPageV2 />)
    fireEvent.click(screen.getByText('在 Finder 中显示 SQLite'))

    await waitFor(() => expect(revealPath).toHaveBeenCalledWith('/Users/mac/.agentbro/skill-manager/skill-manager.db'))
    expect(screen.getByText('已在 Finder 中定位 SQLite')).toBeInTheDocument()
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
