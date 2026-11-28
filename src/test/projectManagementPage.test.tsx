import { beforeEach, describe, expect, it, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import '@testing-library/jest-dom/vitest'
import { ProjectManagementPage } from '../components/skills-v2/ProjectManagementPage'
import { SkillManagerShell } from '../components/skills-v2/SkillManagerShell'
import { SettingsSidebar } from '../components/settings/SettingsSidebar'
import { skillApiV2 } from '../services/skillApiV2'
import * as tauriApi from '../services/tauriApi'
import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog'
import * as tauriPath from '@tauri-apps/api/path'
import { PhysicalPosition } from '@tauri-apps/api/dpi'
import { getCurrentWebview, type Webview } from '@tauri-apps/api/webview'
import { LOCAL_RUNTIME_ENVIRONMENT_ID, useRuntimeEnvironmentStore } from '../stores/runtimeEnvironmentStore'
import { useSkillStoreV2 } from '../stores/skillStoreV2'
import i18n from '../i18n'
import type { AddCenterSkillPreview, ProjectCandidate, ProjectDetail, ProjectDiscovery, SkillManagerOverview, SkillSummary } from '../services/skillApiV2'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(undefined) }))
vi.mock('@tauri-apps/plugin-shell', () => ({ open: vi.fn().mockResolvedValue(undefined) }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(), save: vi.fn() }))
vi.mock('@tauri-apps/api/path', { spy: true })
vi.mock('@tauri-apps/api/webview', { spy: true })

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

function candidate(path: string, overrides: Partial<ProjectCandidate> = {}): ProjectCandidate {
  return { path, name: 'Example repo', sources: ['codex', 'claude-code'], lastUsedAt: null, directoryAvailable: true, registeredProjectId: null, git: null, ...overrides }
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

  it('imports a folder chosen from the native picker and opens its overview', async () => {
    await i18n.changeLanguage('en')
    vi.spyOn(tauriApi, 'isTauri').mockReturnValue(true)
    const project = { ...alternateProject(), rootPath: 'C:\\Users\\me\\Second project' }
    useSkillStoreV2.setState({ projects: [] })
    vi.mocked(skillApiV2.listProjects).mockResolvedValueOnce([]).mockResolvedValue([project])
    vi.mocked(skillApiV2.getProjectDetail).mockResolvedValue(project)
    vi.mocked(openDialog).mockResolvedValue(project.rootPath)
    const add = vi.spyOn(skillApiV2, 'addProject').mockResolvedValue(project)
    render(<ProjectManagementPage />)

    fireEvent.click(await screen.findByRole('button', { name: 'Choose project folder' }))
    expect(await screen.findByRole('heading', { name: project.name })).toBeInTheDocument()
    expect(openDialog).toHaveBeenCalledWith({ directory: true, multiple: false })
    expect(add).toHaveBeenCalledExactlyOnceWith(project.rootPath)
  })

  it('keeps a manually supplied path when adding that directory fails', async () => {
    await i18n.changeLanguage('en')
    vi.spyOn(skillApiV2, 'addProject').mockRejectedValue(new Error('Project directory was moved'))
    render(<ProjectManagementPage />)
    const path = screen.getByRole('textbox', { name: 'Project folder path' })
    fireEvent.change(path, { target: { value: '/moved/project' } })
    fireEvent.click(screen.getByRole('button', { name: 'Add' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Project directory was moved')
    expect(path).toHaveValue('/moved/project')
  })

  it('accepts a native drop within the project import controls only', async () => {
    await i18n.changeLanguage('en')
    vi.spyOn(tauriApi, 'isTauri').mockReturnValue(true)
    const unlisten = vi.fn()
    const onDragDropEvent = vi.fn<Webview['onDragDropEvent']>().mockResolvedValue(unlisten)
    vi.mocked(getCurrentWebview, { partial: true }).mockReturnValueOnce({ onDragDropEvent })
    const view = render(<ProjectManagementPage />)
    const input = screen.getByRole('textbox', { name: 'Drop a folder here or paste its path' })
    const zone = input.closest('.sm2__project-import')!
    vi.spyOn(zone, 'getBoundingClientRect').mockReturnValue(new DOMRect(20, 30, 200, 100))
    await waitFor(() => expect(onDragDropEvent).toHaveBeenCalledOnce())
    const [drop] = onDragDropEvent.mock.calls[0]
    const scale = window.devicePixelRatio
    act(() => drop({ event: 'tauri://drag-drop', id: 1, payload: { type: 'drop', paths: ['/outside'], position: new PhysicalPosition(10 * scale, 10 * scale) } }))
    expect(input).toHaveValue('')
    act(() => drop({ event: 'tauri://drag-drop', id: 1, payload: { type: 'drop', paths: ['/path/linked worktree'], position: new PhysicalPosition(30 * scale, 40 * scale) } }))
    expect(input).toHaveValue('/path/linked worktree')
    view.unmount()
    expect(unlisten).toHaveBeenCalledOnce()
  })

  it('adds only checked recent projects and keeps per-folder batch failures available for retry', async () => {
    await i18n.changeLanguage('en')
    vi.spyOn(tauriApi, 'isTauri').mockReturnValue(true)
    const current = makeProject()
    const added = alternateProject()
    const failed = '/Users/me/moved'
    vi.spyOn(skillApiV2, 'discoverProjects').mockResolvedValue({
      candidates: [
        candidate(current.rootPath, { name: current.name, registeredProjectId: current.id }),
        candidate(added.rootPath, { name: added.name, git: { worktree: added.rootPath, commonDirectory: '/repo/.git', branch: 'topic', commit: 'abc' } }),
        candidate(failed, { name: 'Moved later' }),
        candidate('/not-found', { name: 'Missing now', directoryAvailable: false }),
      ],
      unavailableSources: ['antigravity'],
    })
    vi.mocked(skillApiV2.listProjects).mockResolvedValueOnce([current]).mockResolvedValue([added, current])
    vi.mocked(skillApiV2.getProjectDetail).mockImplementation(async (id) => id === added.id ? added : current)
    const add = vi.spyOn(skillApiV2, 'addProject').mockResolvedValueOnce(added).mockRejectedValueOnce(new Error('Folder moved'))
    render(<ProjectManagementPage />)
    fireEvent.click(await screen.findByRole('button', { name: 'Recent projects' }))
    const dialog = await screen.findByRole('dialog', { name: 'Recent projects' })
    expect(await within(dialog).findByText(/History could not be read from Antigravity/)).toBeInTheDocument()
    expect(within(dialog).getByRole('checkbox', { name: /agentbro/ })).toBeDisabled()
    expect(within(dialog).getByRole('checkbox', { name: /Missing now/ })).toBeDisabled()
    expect(within(dialog).getByText('/repo/.git')).toBeInTheDocument()
    fireEvent.click(within(dialog).getByRole('checkbox', { name: /Second project/ }))
    fireEvent.click(within(dialog).getByRole('checkbox', { name: /Moved later/ }))
    fireEvent.click(within(dialog).getByRole('button', { name: 'Add selected (2)' }))
    expect(await within(dialog).findByRole('alert')).toHaveTextContent('Folder moved')
    expect(add.mock.calls).toEqual([[added.rootPath], [failed]])
    expect(within(dialog).getByRole('checkbox', { name: /Second project/ })).toBeDisabled()
    expect(within(dialog).getByRole('checkbox', { name: /Moved later/ })).toBeChecked()
  })

  it('drops a discovery result when switching away from the local runtime', async () => {
    await i18n.changeLanguage('en')
    vi.spyOn(tauriApi, 'isTauri').mockReturnValue(true)
    const pending = deferred<ProjectDiscovery>()
    vi.spyOn(skillApiV2, 'discoverProjects').mockReturnValue(pending.promise)
    render(<ProjectManagementPage />)
    fireEvent.click(await screen.findByRole('button', { name: 'Recent projects' }))
    expect(await screen.findByRole('dialog')).toBeInTheDocument()
    await act(async () => {
      useRuntimeEnvironmentStore.getState().selectEnvironment('other-machine')
      pending.resolve({ candidates: [candidate('/local/private-repo')], unavailableSources: [] })
    })
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
    expect(screen.queryByText('/local/private-repo')).not.toBeInTheDocument()
  })

  it('shows the error when the project directory cannot be opened', async () => {
    await i18n.changeLanguage('zh')
    const openPath = vi.spyOn(skillApiV2, 'openPath').mockRejectedValue(new Error('Project path no longer exists'))
    render(<ProjectManagementPage />)

    fireEvent.click(await screen.findByRole('button', { name: '打开' }))
    expect(openPath).toHaveBeenCalledWith('/Users/me/agentbro')
    expect(await screen.findByText(/Project path no longer exists/)).toBeInTheDocument()
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

  it('reviews selected ZCode transcripts and maps the same preview to export and Codex import', async () => {
    await i18n.changeLanguage('en')
    vi.spyOn(tauriApi, 'isTauri').mockReturnValue(true)
    vi.mocked(openDialog).mockResolvedValue('/projects/target')
    vi.mocked(saveDialog).mockResolvedValue('/exports/handoff.zip')
    const sessions = [
      { id: 'saved', title: 'Finish the fix', cwd: '/Users/me/agentbro', archived: false, updatedAt: null, directoryExists: true },
      { id: 'older', title: 'Prior approach', cwd: '/Users/me/agentbro', archived: true, updatedAt: null, directoryExists: true },
    ]
    const selection = { projectId: 'project-1', sessionIds: ['saved', 'older'], targetDirectory: '/projects/target', direction: 'zcodeToCodex' }
    vi.spyOn(skillApiV2, 'listProjectConversations').mockResolvedValue(sessions)
    const review = vi.spyOn(skillApiV2, 'previewProjectConversationMigration').mockResolvedValue({
      revision: 'verified-content',
      bundle: {
        projectRoot: '/Users/me/agentbro',
        targetDirectory: '/projects/target',
        conversations: [
          { ...sessions[0], readIssue: null, git: null, messages: [
            { id: 'q', role: 'user', timestamp: null, text: 'Apply the confirmed fix.' },
            { id: 'a', role: 'assistant', timestamp: null, text: 'The change is ready to test.' },
          ] },
          { ...sessions[1], readIssue: 'empty', git: null, messages: [] },
        ],
      },
    })
    const save = vi.spyOn(skillApiV2, 'exportProjectConversationHandoff').mockResolvedValue()
    const migrate = vi.spyOn(skillApiV2, 'migrateProjectConversations').mockResolvedValue([{ sourceId: 'saved', targetId: 'codex-fork', reused: true }])
    const open = vi.spyOn(skillApiV2, 'openMigratedCodexThread').mockResolvedValue()
    render(<ProjectManagementPage />)

    fireEvent.click(await screen.findByRole('button', { name: 'Conversations' }))
    expect(await screen.findByRole('checkbox', { name: /Prior approach.*Archived/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Preview migration' })).toBeDisabled()
    fireEvent.click(screen.getByRole('button', { name: 'Select all' }))
    fireEvent.click(screen.getByRole('button', { name: 'Choose folder' }))
    await screen.findByText('/projects/target')
    fireEvent.click(screen.getByRole('button', { name: 'Preview migration' }))
    const dialog = await screen.findByRole('dialog', { name: 'Preview migration' })
    expect(review).toHaveBeenCalledWith(selection)
    expect(within(dialog).getByText('Apply the confirmed fix.')).toBeInTheDocument()
    expect(within(dialog).getByText('The change is ready to test.')).toBeInTheDocument()
    expect(within(dialog).getByText('No readable messages for this conversation.')).toBeInTheDocument()
    expect(migrate).not.toHaveBeenCalled()
    fireEvent.click(within(dialog).getByRole('button', { name: 'Save handoff' }))
    await screen.findByText('Handoff saved.')
    expect(save).toHaveBeenCalledWith(selection, 'verified-content', '/exports/handoff.zip')

    fireEvent.click(screen.getByRole('button', { name: 'Preview migration' }))
    const nextDialog = await screen.findByRole('dialog', { name: 'Preview migration' })
    fireEvent.click(within(nextDialog).getByRole('button', { name: 'Migrate to Codex' }))
    expect(await screen.findByText('Already migrated')).toBeInTheDocument()
    expect(migrate).toHaveBeenCalledExactlyOnceWith(selection, 'verified-content')
    fireEvent.click(screen.getByRole('button', { name: 'Open in Codex' }))
    expect(open).toHaveBeenCalledExactlyOnceWith('codex-fork')
  })

  it('switches to Codex source and distinguishes continued messages in the ZCode review', async () => {
    await i18n.changeLanguage('en')
    vi.spyOn(tauriApi, 'isTauri').mockReturnValue(true)
    const archived = { id: 'codex-fork', title: 'Imported conversation', cwd: '/Users/me/agentbro', archived: true, updatedAt: null, directoryExists: true }
    const list = vi.spyOn(skillApiV2, 'listProjectConversations')
      .mockImplementation(async (_id, direction) => direction === 'codexToZcode' ? [archived] : [])
    const preview = vi.spyOn(skillApiV2, 'previewProjectConversationMigration').mockResolvedValue({
      revision: 'reverse-preview',
      bundle: {
        projectRoot: '/Users/me/agentbro',
        targetDirectory: '/Users/me/agentbro',
        conversations: [{
          ...archived, readIssue: 'partial', git: null,
          returnContext: { originalAgent: 'zcode', originalSessionId: 'sess-source', matchingMessages: 1 },
          messages: [
            { id: 'original', role: 'user', timestamp: null, text: 'Original message' },
            { id: 'continued', role: 'assistant', timestamp: null, text: 'Work continued in Codex' },
          ],
        }],
      },
    })
    const migrate = vi.spyOn(skillApiV2, 'migrateProjectConversations').mockResolvedValue([{ sourceId: 'codex-fork', targetId: 'sess-restored', reused: false }])
    render(<ProjectManagementPage />)
    fireEvent.click(await screen.findByRole('button', { name: 'Conversations' }))
    fireEvent.change(screen.getByRole('combobox', { name: 'Migration direction' }), { target: { value: 'codexToZcode' } })
    fireEvent.click(await screen.findByRole('checkbox', { name: /Imported conversation/ }))
    expect(list).toHaveBeenCalledWith('project-1', 'codexToZcode')
    fireEvent.click(screen.getByRole('button', { name: 'Preview migration' }))
    const dialog = await screen.findByRole('dialog', { name: 'Preview migration' })
    expect(within(dialog).getByText(/sess-source/)).toBeInTheDocument()
    expect(within(dialog).getByText('Work continued in Codex')).toBeInTheDocument()
    expect(within(dialog).getByText('Messages after the original history')).toBeInTheDocument()
    expect(within(dialog).getByText(/Some content was unavailable/)).toBeInTheDocument()
    fireEvent.click(within(dialog).getByRole('button', { name: 'Migrate to ZCode' }))
    await screen.findByText('sess-restored')
    const selection = { projectId: 'project-1', direction: 'codexToZcode', sessionIds: ['codex-fork'], targetDirectory: '/Users/me/agentbro' }
    expect(preview).toHaveBeenCalledExactlyOnceWith(selection)
    expect(migrate).toHaveBeenCalledExactlyOnceWith(selection, 'reverse-preview')
  })

  it('copies the native Claude Code resume command for the mapped project', async () => {
    await i18n.changeLanguage('en')
    vi.spyOn(tauriApi, 'isTauri').mockReturnValue(true)
    const directory = '/projects/Claude workspace'
    vi.mocked(openDialog).mockResolvedValue(directory)
    const session = { id: 'codex-source', title: 'Review the build', cwd: '/Users/me/agentbro', archived: false, updatedAt: null, directoryExists: true }
    vi.spyOn(skillApiV2, 'listProjectConversations').mockResolvedValue([session])
    vi.spyOn(skillApiV2, 'previewProjectConversationMigration').mockResolvedValue({
      revision: 'claude-preview',
      bundle: {
        projectRoot: session.cwd, targetDirectory: directory,
        conversations: [{ ...session, readIssue: null, git: null, messages: [
          { id: 'q', role: 'user', timestamp: null, text: 'Review the build.' },
        ] }],
      },
    })
    const id = '1dee7df9-9ae0-408c-a410-c1a5e1e0b754'
    const migrate = vi.spyOn(skillApiV2, 'migrateProjectConversations').mockResolvedValue([{ sourceId: session.id, targetId: id, reused: false }])
    const writeText = vi.fn().mockResolvedValue(undefined)
    const originalClipboard = Object.getOwnPropertyDescriptor(navigator, 'clipboard')
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } })
    try {
      render(<ProjectManagementPage />)
      fireEvent.click(await screen.findByRole('button', { name: 'Conversations' }))
      fireEvent.change(screen.getByRole('combobox', { name: 'Migration direction' }), { target: { value: 'codexToClaudeCode' } })
      fireEvent.click(await screen.findByRole('checkbox', { name: /Review the build/ }))
      fireEvent.click(screen.getByRole('button', { name: 'Choose folder' }))
      await screen.findByText(directory)
      fireEvent.click(screen.getByRole('button', { name: 'Preview migration' }))
      const dialog = await screen.findByRole('dialog', { name: 'Preview migration' })
      fireEvent.click(within(dialog).getByRole('button', { name: 'Migrate to Claude Code' }))
      fireEvent.click(await screen.findByRole('button', { name: 'Copy resume command' }))
      expect(migrate).toHaveBeenCalledExactlyOnceWith({ projectId: 'project-1', direction: 'codexToClaudeCode', sessionIds: [session.id], targetDirectory: directory }, 'claude-preview')
      expect(writeText).toHaveBeenCalledExactlyOnceWith(`claude --resume ${id}`)
      expect(await screen.findByText('Resume command copied.')).toBeInTheDocument()
    } finally {
      if (originalClipboard) Object.defineProperty(navigator, 'clipboard', originalClipboard)
      else Reflect.deleteProperty(navigator, 'clipboard')
    }
  })

  it('saves the reviewed Antigravity handoff Markdown in the mapped workspace', async () => {
    await i18n.changeLanguage('en')
    vi.spyOn(tauriApi, 'isTauri').mockReturnValue(true)
    const directory = '/projects/Antigravity workspace'
    const destination = `${directory}/conversation-handoff.md`
    vi.mocked(openDialog).mockResolvedValue(directory)
    vi.mocked(saveDialog).mockResolvedValue(destination)
    vi.mocked(tauriPath.join).mockResolvedValueOnce(destination)
    const session = { id: 'codex-source', title: 'Review the build', cwd: '/Users/me/agentbro', archived: false, updatedAt: null, directoryExists: true }
    vi.spyOn(skillApiV2, 'listProjectConversations').mockResolvedValue([session])
    vi.spyOn(skillApiV2, 'previewProjectConversationMigration').mockResolvedValue({
      revision: 'handoff-review',
      bundle: {
        projectRoot: session.cwd, targetDirectory: directory,
        conversations: [{ ...session, readIssue: null, git: null, messages: [
          { id: 'q', role: 'user', timestamp: null, text: 'Review the build.' },
        ] }],
      },
    })
    const exportHandoff = vi.spyOn(skillApiV2, 'exportProjectConversationHandoff').mockResolvedValue()
    const migrate = vi.spyOn(skillApiV2, 'migrateProjectConversations')
    render(<ProjectManagementPage />)
    fireEvent.click(await screen.findByRole('button', { name: 'Conversations' }))
    fireEvent.change(screen.getByRole('combobox', { name: 'Migration direction' }), { target: { value: 'codexToAntigravity' } })
    fireEvent.click(await screen.findByRole('checkbox', { name: /Review the build/ }))
    fireEvent.click(screen.getByRole('button', { name: 'Choose folder' }))
    await screen.findByText(directory)
    fireEvent.click(screen.getByRole('button', { name: 'Preview migration' }))
    const dialog = await screen.findByRole('dialog', { name: 'Preview migration' })
    expect(within(dialog).getByText(/attach the handoff Markdown/)).toBeInTheDocument()
    expect(within(dialog).getByRole('button', { name: 'Save handoff' })).toBeEnabled()
    fireEvent.click(within(dialog).getByRole('button', { name: 'Save Markdown for Antigravity' }))
    await screen.findByText('Handoff saved.')
    expect(saveDialog).toHaveBeenCalledWith({ defaultPath: destination, filters: [{ name: 'Markdown', extensions: ['md'] }] })
    expect(exportHandoff).toHaveBeenCalledExactlyOnceWith({ projectId: 'project-1', direction: 'codexToAntigravity', sessionIds: [session.id], targetDirectory: directory }, 'handoff-review', destination)
    expect(migrate).not.toHaveBeenCalled()
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

  it('checks installed Skills against the current lock without making them restorable', async () => {
    await i18n.changeLanguage('en')
    const project = makeProject()
    vi.mocked(skillApiV2.getProjectDetail).mockResolvedValue({
      ...project,
      skillLock: {
        version: 2,
        revision: 'verify-digest',
        error: null,
        skills: ['matching', 'edited', 'incomplete'].map((name) => ({
          name, source: 'example/skills', reference: 'locked-commit', status: 'existing',
        })),
      },
    })
    const pending = deferred<Record<string, 'verified' | 'modified' | 'unverified'>>()
    const verify = vi.spyOn(skillApiV2, 'verifyProjectSkillLock').mockReturnValue(pending.promise)
    const restore = vi.spyOn(skillApiV2, 'restoreProjectSkillLock')
    render(<ProjectManagementPage />)

    fireEvent.click(await screen.findByRole('button', { name: 'Verify installed Skills' }))
    expect(verify).toHaveBeenCalledWith(project.id, 'verify-digest')
    expect(screen.getByRole('button', { name: 'Verifying…' })).toBeDisabled()
    await act(async () => {
      pending.resolve({ matching: 'verified', edited: 'modified', incomplete: 'unverified' })
    })

    expect(screen.getByText('Matches lock file')).toBeInTheDocument()
    expect(screen.getByText('Differs from lock file')).toBeInTheDocument()
    expect(screen.getByText('Unable to verify')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /Restore selected/ })).not.toBeInTheDocument()
    expect(restore).not.toHaveBeenCalled()
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

  it.each([
    ['Skill', '中心库 Skill', '安装 Skill', 'installCenterSkillsToProject'],
    ['Pack', '技能包', '应用技能包', 'installSkillPackToProject'],
  ] as const)('uses the visible replacement after the selected %s disappears', async (kind, label, button, method) => {
    const overview = makeOverview(makeSkill())
    const removedId = 'removed-choice'
    const isSkill = kind === 'Skill'
    const initialSkill = overview.skills[0]
    const initialPack = overview.packs[0]
    const replacementId = isSkill ? initialSkill.id : initialPack.id
    const project = makeProject()
    const operation = vi.spyOn(skillApiV2, method).mockResolvedValue(project)
    render(<ProjectManagementPage />)
    await screen.findByRole('heading', { name: project.name })
    act(() => useSkillStoreV2.setState({
      skills: [initialSkill, { ...initialSkill, id: removedId, name: 'Removed Skill' }],
      packs: [initialPack, { ...initialPack, id: removedId, name: 'Removed Pack' }],
    }))
    const select = screen.getByLabelText(label)
    fireEvent.change(select, { target: { value: removedId } })
    expect(select).toHaveValue(removedId)

    act(() => useSkillStoreV2.setState(isSkill ? { skills: [initialSkill] } : { packs: [initialPack] }))
    expect(select).toHaveValue(replacementId)
    const install = screen.getByRole('button', { name: button })
    await act(async () => { fireEvent.click(install) })

    expect(operation).toHaveBeenCalledExactlyOnceWith(project.id, 'codex', isSkill ? [replacementId] : replacementId, 'link')
    act(() => useSkillStoreV2.setState(isSkill ? { skills: [] } : { packs: [] }))
    expect(install).toBeDisabled()
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
