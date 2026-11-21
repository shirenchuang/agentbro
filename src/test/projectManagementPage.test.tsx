import { beforeEach, describe, expect, it, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import '@testing-library/jest-dom/vitest'
import { ProjectManagementPage } from '../components/skills-v2/ProjectManagementPage'
import { SkillManagerShell } from '../components/skills-v2/SkillManagerShell'
import { SettingsSidebar } from '../components/settings/SettingsSidebar'
import { skillApiV2 } from '../services/skillApiV2'
import { LOCAL_RUNTIME_ENVIRONMENT_ID, useRuntimeEnvironmentStore } from '../stores/runtimeEnvironmentStore'
import { useSkillStoreV2 } from '../stores/skillStoreV2'
import i18n from '../i18n'
import type { AddCenterSkillPreview, ProjectDetail, SkillManagerOverview, SkillSummary } from '../services/skillApiV2'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(undefined) }))
vi.mock('@tauri-apps/plugin-shell', () => ({ open: vi.fn().mockResolvedValue(undefined) }))

function makeSkill(): SkillSummary {
  return {
    id: 'release-checklist',
    name: 'Release Checklist',
    description: 'Release workflow',
    skillType: 'skill',
    sourceType: 'local_folder',
    sourceUri: null,
    centerPath: '/center/release-checklist',
    currentHash: 'hash-center',
    status: 'ok',
    installedAgents: [],
  }
}

function makeOverview(skill: SkillSummary): SkillManagerOverview {
  return {
    metrics: { centerSkillCount: 1, targetCount: 0, unmanagedCount: 0, issueCount: 0 },
    skills: [skill],
    agents: [],
    packs: [
      { id: 'default', name: '全量技能包', description: '', tags: [], memberCount: 1, appliedAgentCount: 0, healthy: true },
    ],
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
}

function makeProject(): ProjectDetail {
  return {
    id: 'project-1',
    name: 'agentbro',
    rootPath: '/Users/me/agentbro',
    createdAt: '2026-06-19T00:00:00Z',
    updatedAt: '2026-06-19T00:00:00Z',
    lastScannedAt: '2026-06-19T00:00:00Z',
    detectedAgentCount: 1,
    skillCount: 1,
    mcpCount: 1,
    pluginCount: 1,
    instructionCount: 1,
    issueCount: 0,
    agents: [
      {
        agentId: 'codex',
        displayName: 'Codex',
        iconKey: 'codex',
        skillsDirs: ['/Users/me/agentbro/.agents/skills'],
        configPaths: ['/Users/me/agentbro/.codex/config.toml'],
        mcpConfigPaths: ['/Users/me/agentbro/.codex/config.toml'],
        pluginConfigPaths: ['/Users/me/agentbro/.codex/config.toml'],
        skills: [
          {
            id: 'project-review',
            name: 'project-review',
            description: 'Project workflow',
            agentId: 'codex',
            path: '/Users/me/agentbro/.agents/skills/project-review',
            hash: 'hash-project',
            status: 'projectOnly',
            importable: true,
          },
        ],
        mcpServers: [
          { name: 'context7', command: 'npx', args: ['-y', '@upstash/context7-mcp'], valid: true, message: 'configured' },
        ],
        plugins: [
          { id: 'documents@openai-primary-runtime', name: 'documents@openai-primary-runtime', version: null, enabled: true, source: 'project-config' },
        ],
        health: [],
      },
    ],
    instructions: [
      { agentId: 'codex', path: '/Users/me/agentbro/AGENTS.md', exists: true, bytes: 2048 },
    ],
    health: [],
  }
}

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (error: Error) => void
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise
    reject = rejectPromise
  })
  return { promise, resolve, reject }
}

function alternateProject(): ProjectDetail {
  return { ...makeProject(), id: 'project-2', name: 'Second project', rootPath: '/Users/me/second' }
}

describe('ProjectManagementPage', () => {
  beforeEach(() => {
    cleanup()
    vi.restoreAllMocks()
    useRuntimeEnvironmentStore.getState().selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    const skill = makeSkill()
    const project = makeProject()
    useSkillStoreV2.setState({
      runtimeEnvironmentId: LOCAL_RUNTIME_ENVIRONMENT_ID,
      activeTab: 'projects',
      skills: [skill],
      packs: makeOverview(skill).packs,
      projects: [project],
      selectedProjectId: null,
      selectedProjectDetail: null,
      projectDetailLoading: false,
      loading: false,
      error: null,
      settings: makeOverview(skill).settings,
      overview: makeOverview(skill),
      initialized: true,
    })
    vi.spyOn(skillApiV2, 'overview').mockResolvedValue(makeOverview(skill))
    vi.spyOn(skillApiV2, 'listProjects').mockResolvedValue([project])
    vi.spyOn(skillApiV2, 'getProjectDetail').mockResolvedValue(project)
  })

  it('renders project resources and installs a center skill into the project', async () => {
    const install = vi.spyOn(skillApiV2, 'installCenterSkillsToProject').mockResolvedValue(makeProject())
    render(<ProjectManagementPage />)

    await waitFor(() => expect(screen.getByText('agentbro')).toBeInTheDocument())

    fireEvent.click(screen.getByText('Skills (1)'))
    expect(screen.getByText('project-review')).toBeInTheDocument()

    fireEvent.click(screen.getByText('MCP (1)'))
    expect(screen.getByText('context7')).toBeInTheDocument()

    fireEvent.click(screen.getByText('Plugins (1)'))
    expect(screen.getAllByText('documents@openai-primary-runtime').length).toBeGreaterThan(0)

    fireEvent.click(screen.getByRole('button', { name: '安装 Skill' }))

    await waitFor(() => {
      expect(install).toHaveBeenCalledWith('project-1', 'codex', ['release-checklist'], 'link')
    })
  })

  it('previews a project lock and restores only checked Skills with per-Skill feedback', async () => {
    await i18n.changeLanguage('en')
    useSkillStoreV2.setState({ activeTab: 'library' })
    const project = makeProject()
    const skillLock = {
      version: 2,
      revision: 'preview-digest',
      error: null,
      skills: [
        { name: 'missing-skill', source: 'example/skills', reference: 'locked-commit', status: 'ready' },
        { name: 'mismatch', source: 'example/skills', reference: 'main', status: 'ready' },
        { name: 'other-source', source: 'package', reference: null, status: 'unsupported' },
        { name: 'present', source: 'example/skills', reference: 'main', status: 'existing' },
        { name: 'unselected', source: 'example/skills', reference: null, status: 'ready' },
      ],
    } satisfies NonNullable<ProjectDetail['skillLock']>
    vi.mocked(skillApiV2.getProjectDetail)
      .mockResolvedValueOnce({ ...project, skillLock })
      .mockResolvedValue({
        ...project,
        skillLock: {
          ...skillLock,
          skills: skillLock.skills.map((skill) => skill.name === 'missing-skill'
            ? { ...skill, status: 'existing' }
            : skill),
        },
      })
    const restore = vi.spyOn(skillApiV2, 'restoreProjectSkillLock').mockResolvedValue([
      { name: 'missing-skill', error: null },
      { name: 'mismatch', error: 'Downloaded Skill content does not match the lock file' },
    ])
    render(
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
        <SkillManagerShell />
      </>,
    )
    fireEvent.click(screen.getByRole('button', { name: 'Projects' }))

    expect(await screen.findByText('example/skills · locked-commit')).toBeInTheDocument()
    expect(screen.getByText('Unsupported source or version')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Restore selected (0)' })).toBeDisabled()
    expect(restore).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('checkbox', { name: 'missing-skill' }))
    fireEvent.click(screen.getByRole('checkbox', { name: 'mismatch' }))
    fireEvent.click(screen.getByRole('button', { name: 'Restore selected (2)' }))

    await waitFor(() => expect(restore).toHaveBeenCalledWith('project-1', 'preview-digest', ['missing-skill', 'mismatch']))
    expect(await screen.findByText('Restored')).toBeInTheDocument()
    expect(screen.getByText('Downloaded Skill content does not match the lock file')).toBeInTheDocument()
    expect(screen.queryByRole('checkbox', { name: 'missing-skill' })).not.toBeInTheDocument()
  })

  it('keeps the selected project detail when an earlier selection finishes late', async () => {
    const first = makeProject()
    const second = alternateProject()
    const pendingFirst = deferred<ProjectDetail>()
    vi.mocked(skillApiV2.listProjects).mockResolvedValue([first, second])
    vi.mocked(skillApiV2.getProjectDetail)
      .mockReturnValueOnce(pendingFirst.promise)
      .mockResolvedValue(second)
    render(<ProjectManagementPage />)

    fireEvent.click(await screen.findByRole('button', { name: /Second project/ }))
    expect(await screen.findByRole('heading', { name: second.name })).toBeInTheDocument()

    await act(async () => { pendingFirst.resolve(first) })

    expect(screen.getByRole('heading', { name: second.name })).toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: first.name })).not.toBeInTheDocument()
  })

  it('keeps the new project loading when the previous project request fails', async () => {
    const first = makeProject()
    const second = alternateProject()
    const pendingFirst = deferred<ProjectDetail>()
    const pendingSecond = deferred<ProjectDetail>()
    vi.mocked(skillApiV2.listProjects).mockResolvedValue([first, second])
    vi.mocked(skillApiV2.getProjectDetail).mockImplementation((id) =>
      id === first.id ? pendingFirst.promise : pendingSecond.promise,
    )
    render(<ProjectManagementPage />)
    fireEvent.click(await screen.findByRole('button', { name: /Second project/ }))

    await act(async () => { pendingFirst.reject(new Error('Previous project disappeared')) })

    expect(screen.getByText('加载项目详情…')).toBeInTheDocument()
    expect(screen.queryByText(/Previous project disappeared/)).not.toBeInTheDocument()
    await act(async () => { pendingSecond.resolve(second) })
    expect(screen.getByRole('heading', { name: second.name })).toBeInTheDocument()
  })

  it.each([
    ['重新扫描', 'scanProject'],
    ['安装 Skill', 'installCenterSkillsToProject'],
    ['应用技能包', 'installSkillPackToProject'],
  ] as const)('keeps the selected project when %s finishes in another project', async (button, method) => {
    const first = makeProject()
    const second = alternateProject()
    const pending = deferred<ProjectDetail>()
    const operation = vi.spyOn(skillApiV2, method).mockReturnValue(pending.promise)
    vi.mocked(skillApiV2.listProjects).mockResolvedValue([first, second])
    vi.mocked(skillApiV2.getProjectDetail).mockImplementation(async (id) => id === first.id ? first : second)
    render(<ProjectManagementPage />)
    await screen.findByRole('heading', { name: first.name })

    fireEvent.click(screen.getByRole('button', { name: button }))
    expect(operation.mock.calls[0]?.[0]).toBe(first.id)
    fireEvent.click(screen.getByRole('button', { name: /Second project/ }))
    await screen.findByRole('heading', { name: second.name })
    await act(async () => { pending.resolve(first) })

    expect(screen.getByRole('heading', { name: second.name })).toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: first.name })).not.toBeInTheDocument()
  })

  it('reloads the selected project detail when refreshing projects', async () => {
    const project = makeProject()
    vi.mocked(skillApiV2.getProjectDetail)
      .mockResolvedValueOnce(project)
      .mockResolvedValue({ ...project, name: 'Updated project' })
    render(<ProjectManagementPage />)
    await screen.findByRole('heading', { name: project.name })

    fireEvent.click(screen.getByRole('button', { name: '刷新项目' }))

    expect(await screen.findByRole('heading', { name: 'Updated project' })).toBeInTheDocument()
    expect(skillApiV2.getProjectDetail).toHaveBeenCalledTimes(2)
    expect(skillApiV2.getProjectDetail).toHaveBeenLastCalledWith(project.id)
  })

  it('keeps a project discovered by refresh when an earlier project list arrives late', async () => {
    const first = makeProject()
    const second = alternateProject()
    const pending = deferred<ProjectDetail[]>()
    vi.mocked(skillApiV2.listProjects).mockReturnValueOnce(pending.promise).mockResolvedValue([first, second])
    vi.mocked(skillApiV2.getProjectDetail).mockImplementation(async (id) => id === first.id ? first : second)
    render(<ProjectManagementPage />)
    await screen.findByRole('heading', { name: first.name })
    fireEvent.click(screen.getByRole('button', { name: '刷新项目' }))
    fireEvent.click(await screen.findByRole('button', { name: /Second project/ }))
    await screen.findByRole('heading', { name: second.name })

    await act(async () => { pending.resolve([first]) })

    expect(screen.getByRole('button', { name: /Second project/ })).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: second.name })).toBeInTheDocument()
  })

  it.each(['switch', 'switch back'] as const)('discards an import preview after a runtime %s', async (transition) => {
    const pending = deferred<AddCenterSkillPreview>()
    const preview = vi.spyOn(skillApiV2, 'previewAddCenterSkill').mockReturnValue(pending.promise)
    const execute = vi.spyOn(skillApiV2, 'executeAddCenterSkill').mockResolvedValue({
      skillIds: [], updated: [], skipped: [],
    })
    render(<ProjectManagementPage />)
    fireEvent.click(await screen.findByRole('button', { name: 'Skills (1)' }))
    fireEvent.click(screen.getByRole('button', { name: '导入中心库' }))
    expect(preview).toHaveBeenCalledTimes(1)
    try {
      await act(async () => {
        const runtime = useRuntimeEnvironmentStore.getState()
        runtime.selectEnvironment('other-machine')
        if (transition === 'switch back') runtime.selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
        pending.resolve({ candidates: [], blockers: [], centerPath: '/center' })
      })

      expect(execute).not.toHaveBeenCalled()
    } finally {
      useRuntimeEnvironmentStore.getState().selectEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)
    }
  })

  it('imports from the original project when only the project selection changes', async () => {
    const first = makeProject()
    const second = alternateProject()
    const pending = deferred<AddCenterSkillPreview>()
    const sourceDir = first.agents[0].skills[0].path
    vi.mocked(skillApiV2.listProjects).mockResolvedValue([first, second])
    vi.mocked(skillApiV2.getProjectDetail).mockImplementation(async (id) => id === first.id ? first : second)
    vi.spyOn(skillApiV2, 'previewAddCenterSkill').mockReturnValue(pending.promise)
    const execute = vi.spyOn(skillApiV2, 'executeAddCenterSkill').mockResolvedValue({
      skillIds: ['project-review'], updated: [], skipped: [],
    })
    const scan = vi.spyOn(skillApiV2, 'scanProject').mockResolvedValue(first)
    render(<ProjectManagementPage />)
    fireEvent.click(await screen.findByRole('button', { name: 'Skills (1)' }))
    fireEvent.click(screen.getByRole('button', { name: '导入中心库' }))
    fireEvent.click(screen.getByRole('button', { name: /Second project/ }))
    await screen.findByRole('heading', { name: second.name })

    await act(async () => {
      pending.resolve({
        blockers: [],
        centerPath: '/center',
        candidates: [{
          skillId: 'project-review',
          proposedSkillId: 'project-review',
          name: 'Project Review',
          description: '',
          sourceDir,
          hash: 'hash-project',
          action: 'create',
          existingSourceType: null,
          reason: null,
        }],
      })
    })

    expect(execute).toHaveBeenCalledWith(expect.objectContaining({ sourcePath: sourceDir }), [
      { skillId: 'project-review', proposedSkillId: 'project-review', resolution: 'create' },
    ])
    expect(scan).toHaveBeenCalledWith(first.id)
    expect(screen.getByRole('heading', { name: second.name })).toBeInTheDocument()
  })
})
