import { describe, it, expect, beforeEach, vi } from 'vitest'
import { render, screen, fireEvent, cleanup, waitFor } from '@testing-library/react'
import '@testing-library/jest-dom/vitest'
import { AgentIconBadge } from '../components/skills-v2/AgentIconBadge'
import { useSkillStoreV2 } from '../stores/skillStoreV2'
import { skillApiV2 } from '../services/skillApiV2'
import { agentApi, type AgentProgramInfo } from '../services/agentApi'
import * as tauriApi from '../services/tauriApi'
import { open as openShell } from '@tauri-apps/plugin-shell'
import i18n from '../i18n'
import type { SkillSummary, AgentSummary, AgentDetail, AgentSkillInventoryAgent } from '../services/skillApiV2'

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
      loading: false,
      error: null,
      initialized: true,
      selectedSkillId: null,
      selectedSkillDetail: null,
    })
  })

  it('renders both skills as cards by default', async () => {
    const { SkillLibraryPage } = await import('../components/skills-v2/SkillLibraryPage')
    render(<SkillLibraryPage />)
    expect(screen.getByText('Release Checklist')).toBeInTheDocument()
    expect(screen.getByText('Database Debugging')).toBeInTheDocument()
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

    fireEvent.click(screen.getByRole('button', { name: '批量分发' }))
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

    const preserve = await screen.findByRole('button', { name: '保留 Agent 副本' })
    const removeAll = screen.getByRole('button', { name: '移除 Agent 安装' })
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
})

describe('Agent sync local agent chips', () => {
  beforeEach(() => {
    cleanup()
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

    expect(await screen.findByLabelText('选择 Agent')).toHaveTextContent('Claude Code · 1 可接管')
    expect(screen.getByText('Claude Code')).toBeInTheDocument()
    expect(screen.queryByText('DeepSeek')).not.toBeInTheDocument()
    expect(container.querySelector('.sm2__agent-sync-statline')).toHaveTextContent('1 Agent')
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
    expect(container.querySelector('.sm2__agent-sync-statline')).toHaveTextContent('2 Agent')
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
    finishFirstAdopt()
  })

  it('batch adopts shared .agents skills as center symlinks', async () => {
    const { skillApiV2 } = await import('../services/skillApiV2')
    const inventory: AgentSkillInventoryAgent[] = [
      {
        agentId: 'agents',
        displayName: '.agents',
        iconKey: 'agents',
        skillsDir: '/Users/me/.agents/skills',
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

    expect(await screen.findByLabelText('选择 Agent')).toHaveTextContent('.agents · 1 可接管')
    fireEvent.click(screen.getByText('选择当前可接管'))
    fireEvent.click(screen.getByText('接管到中心库'))

    await waitFor(() => {
      expect(execute).toHaveBeenCalledWith('agents', 'shared-local-alpha', 'import_link')
    })
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
    vi.spyOn(skillApiV2, 'listAgentSkillInventory').mockResolvedValue(inventory)
    const execute = vi.spyOn(skillApiV2, 'executeAdopt').mockResolvedValue('alpha')
    execute.mockClear()

    const { AgentSyncPanel } = await import('../components/skills-v2/InstallView')
    render(<AgentSyncPanel onDone={() => {}} />)

    fireEvent.click(await screen.findByText('一键整理'))

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

    fireEvent.click(await screen.findByText('一键整理'))
    fireEvent.click(await screen.findByLabelText('保留现有文件'))
    fireEvent.click(screen.getByText('开始整理'))

    await waitFor(() => {
      expect(execute).toHaveBeenCalledWith('claude-code', 'local-alpha', 'import_keep', null)
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

    fireEvent.click(screen.getByText('文件'))
    const filePreview = document.body.querySelector('.sm2__markdown--file')
    expect(filePreview).toHaveClass('selectable')
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

  it('Agent skills default to cards and can switch to list view', async () => {
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    const { container } = render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))
    expect(container.querySelector('.sm2__agent-skill-grid')).not.toBeNull()
    expect(container.querySelector('.sm2__agent-skill-card')).not.toBeNull()
    fireEvent.click(screen.getByText('列表'))
    expect(container.querySelector('.sm2__agent-skill-list')).not.toBeNull()
  })

  it('localizes managed mode, direct claim, and unmanaged reason labels on the agent page', async () => {
    const { AgentManagementPage } = await import('../components/skills-v2/AgentManagementPage')
    render(<AgentManagementPage />)
    fireEvent.click(screen.getByText('Skills (2)'))

    expect(screen.getByText('软连接')).toBeInTheDocument()
    expect(screen.getAllByText('直接分发').length).toBeGreaterThan(0)
    expect(screen.getByText('正常')).toBeInTheDocument()
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

  it('only offers hook-installed agents that do not already have the skill', async () => {
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
    expect(screen.getByText('0/1')).toBeInTheDocument()
    expect(screen.getByText('1 个可选')).toBeInTheDocument()
    expect(screen.getByText('已安装 · 软连接')).toBeInTheDocument()
    expect(Array.from(document.body.querySelectorAll('.sm2-distribute__agent strong')).map((node) => node.textContent)).toEqual([
      'Codex',
      'Claude Code',
    ])

    const codexRow = screen.getByText('Codex').closest('label')
    expect(codexRow?.querySelector('input')).toBeDisabled()

    fireEvent.click(screen.getByText('Codex'))
    expect(screen.getByText('预览影响').closest('button')).toBeDisabled()
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

    const empty = container.querySelector('.sm2__unmanaged-empty')
    expect(empty).not.toBeNull()
    const button = empty?.querySelector('button')
    expect(button).toHaveTextContent('重新扫描此 Agent')

    fireEvent.click(button!)
    await waitFor(() => expect(scan).toHaveBeenCalledWith('claude-code'))
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

  it('SkillPackPage renders the redesigned pack workspace', async () => {
    const { SkillPackPage } = await import('../components/skills-v2/SkillPackPage')
    const { container } = render(<SkillPackPage />)
    expect(container.querySelector('.sm2__pack-layout')).not.toBeNull()
    expect(container.querySelector('.sm2__pack-sidebar')).not.toBeNull()
    expect(container.querySelector('.sm2__pack-canvas')).not.toBeNull()
    expect(container.querySelector('.sm2__rail')).toBeNull()
  })

  it('opens the pack builder in a dialog instead of replacing the canvas', async () => {
    const { SkillPackPage } = await import('../components/skills-v2/SkillPackPage')
    const { container } = render(<SkillPackPage />)

    fireEvent.click(container.querySelector('.sm2__header .sm2__btn--primary')!)

    expect(document.body.querySelector('.sm2__pack-builder-modal')).not.toBeNull()
    expect(document.body.querySelector('.sm2__pack-builder-modal .sm2__skill-picker2')).not.toBeNull()
    expect(container.querySelector('.sm2__pack-canvas .sm2__pack-builder2')).toBeNull()
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
