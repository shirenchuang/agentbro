import { describe, it, expect, beforeEach, vi } from 'vitest'
import { render, screen, fireEvent, cleanup } from '@testing-library/react'
import '@testing-library/jest-dom/vitest'
import { AgentIconBadge } from '../components/skills-v2/AgentIconBadge'
import { useSkillStoreV2 } from '../stores/skillStoreV2'
import type { SkillSummary, AgentSummary } from '../services/skillApiV2'

// SkillManagerShell imports pages that call skillApiV2 at mount; we stub the api
// so tests run without the Tauri runtime.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(undefined) }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn().mockResolvedValue(null) }))

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
    useSkillStoreV2.setState({
      activeTab: 'library',
      viewMode: 'cards',
      filters: { query: '', source: '', status: '', type: '' },
      skills: [
        makeSkill(),
        makeSkill({ id: 'db-debug', name: 'Database Debugging', status: 'copyDiverged' }),
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
      loading: false,
      error: null,
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

  it('selecting a skill calls store selectSkill', async () => {
    const selectSpy = vi.spyOn(useSkillStoreV2.getState(), 'selectSkill')
    const { SkillLibraryPage } = await import('../components/skills-v2/SkillLibraryPage')
    render(<SkillLibraryPage />)
    fireEvent.click(screen.getByText('Release Checklist'))
    expect(selectSpy).toHaveBeenCalledWith('release-checklist')
  })
})
