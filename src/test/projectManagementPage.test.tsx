import { beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import '@testing-library/jest-dom/vitest'
import { ProjectManagementPage } from '../components/skills-v2/ProjectManagementPage'
import { SkillManagerShell } from '../components/skills-v2/SkillManagerShell'
import { SettingsSidebar } from '../components/settings/SettingsSidebar'
import { skillApiV2 } from '../services/skillApiV2'
import { useSkillStoreV2 } from '../stores/skillStoreV2'
import i18n from '../i18n'
import type { ProjectDetail, SkillManagerOverview, SkillSummary } from '../services/skillApiV2'

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

describe('ProjectManagementPage', () => {
  beforeEach(() => {
    cleanup()
    vi.restoreAllMocks()
    const skill = makeSkill()
    const project = makeProject()
    useSkillStoreV2.setState({
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
})
