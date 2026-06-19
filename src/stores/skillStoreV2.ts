import { create } from 'zustand'
import type {
  SkillManagerOverview,
  SkillManagerSettings,
  SkillSummary,
  SkillDetail,
  AgentSummary,
  AgentDetail,
  SkillPackSummary,
  SkillPackDetail,
  DiagnosisIssue,
  UnmanagedItemDto,
  DistributionPreview,
  ProjectSummary,
  ProjectDetail,
} from '../services/skillApiV2'
import { skillApiV2 } from '../services/skillApiV2'

export type SkillManagerTab = 'library' | 'install' | 'packs' | 'projects' | 'agents' | 'diagnostics' | 'settings'
export type SkillViewMode = 'cards' | 'list'

const OVERVIEW_CACHE_TTL_MS = 60_000

export interface SkillFilters {
  query: string
  source: string
  status: string
  type: string
}

interface SkillV2State {
  activeTab: SkillManagerTab
  viewMode: SkillViewMode
  overview: SkillManagerOverview | null
  settings: SkillManagerSettings | null
  skills: SkillSummary[]
  selectedSkillId: string | null
  selectedSkillDetail: SkillDetail | null
  selectedPackId: string | null
  selectedPackDetail: SkillPackDetail | null
  selectedAgentId: string | null
  selectedAgentDetail: AgentDetail | null
  selectedProjectId: string | null
  selectedProjectDetail: ProjectDetail | null
  agents: AgentSummary[]
  packs: SkillPackSummary[]
  projects: ProjectSummary[]
  issues: DiagnosisIssue[]
  unmanaged: UnmanagedItemDto[]
  filters: SkillFilters
  loading: boolean
  error: string | null
  busyAction: string | null
  lastPreview: DistributionPreview | null
  initialized: boolean
  agentDetailLoading: boolean
  projectDetailLoading: boolean
  startupScanInFlight: boolean
  lastOverviewLoadedAt: number
}

interface SkillV2Actions {
  init: () => Promise<void>
  refresh: () => Promise<void>
  loadOverview: (force?: boolean) => Promise<void>
  setTab: (tab: SkillManagerTab) => void
  setViewMode: (mode: SkillViewMode) => void
  setFilter: <K extends keyof SkillFilters>(key: K, value: SkillFilters[K]) => void
  selectSkill: (id: string | null) => Promise<void>
  selectPack: (id: string | null) => Promise<void>
  selectAgent: (id: string | null) => Promise<void>
  loadAgentDetail: (agentId: string, force?: boolean) => Promise<void>
  loadProjects: (force?: boolean) => Promise<void>
  addProject: (rootPath: string) => Promise<void>
  removeProject: (projectId: string) => Promise<void>
  selectProject: (id: string | null) => Promise<void>
  scanProject: (projectId: string) => Promise<void>
  loadDiagnosisIssues: () => Promise<void>
  runDiagnosis: () => Promise<void>
  updateSettings: (patch: Partial<SkillManagerSettings>) => Promise<void>
  setBusy: (action: string | null) => void
  setError: (err: string | null) => void
  setLastPreview: (p: DistributionPreview | null) => void
}

export const useSkillStoreV2 = create<SkillV2State & SkillV2Actions>((set, get) => ({
  activeTab: 'library',
  viewMode: 'cards',
  overview: null,
  settings: null,
  skills: [],
  selectedSkillId: null,
  selectedSkillDetail: null,
  selectedPackId: null,
  selectedPackDetail: null,
  selectedAgentId: null,
  selectedAgentDetail: null,
  selectedProjectId: null,
  selectedProjectDetail: null,
  agents: [],
  packs: [],
  projects: [],
  issues: [],
  unmanaged: [],
  filters: { query: '', source: '', status: '', type: '' },
  loading: false,
  error: null,
  busyAction: null,
  lastPreview: null,
  initialized: false,
  agentDetailLoading: false,
  projectDetailLoading: false,
  startupScanInFlight: false,
  lastOverviewLoadedAt: 0,

  init: async () => {
    // Page entry should be cheap: bootstrap only ensures DB/dirs are usable,
    // then reads cached SQLite state. Startup scanning runs in the background
    // so opening the library is not blocked by walking every Agent skill dir.
    if (get().initialized) {
      if (!get().overview) await get().loadOverview(true)
      return
    }
    set({ loading: true, error: null })
    try {
      await skillApiV2.bootstrap()
      await get().loadOverview(true)
      await get().loadProjects(true)
      set({ initialized: true })

      if (get().settings?.startupScan && !get().startupScanInFlight) {
        set({ startupScanInFlight: true, busyAction: get().busyAction ?? 'startupScan' })
        void (async () => {
          try {
            await skillApiV2.init()
            await get().loadOverview(true)
          } catch (e) {
            set({ error: String(e) })
          } finally {
            set((s) => ({
              startupScanInFlight: false,
              busyAction: s.busyAction === 'startupScan' ? null : s.busyAction,
            }))
          }
        })()
      }
    } catch (e) {
      set({ error: String(e) })
    } finally {
      set({ loading: false })
    }
  },
  refresh: async () => {
    set({ loading: true, error: null })
    try {
      const overview = await skillApiV2.refreshOverview()
      set({
        overview,
        skills: overview.skills,
        agents: overview.agents,
        packs: overview.packs,
        issues: overview.issues,
        settings: overview.settings,
        lastOverviewLoadedAt: Date.now(),
      })
      await get().loadProjects(true)
      set({ initialized: true })
    } catch (e) {
      set({ error: String(e) })
    } finally {
      set({ loading: false })
    }
  },
  loadOverview: async (force = false) => {
    const now = Date.now()
    if (!force && get().overview && now - get().lastOverviewLoadedAt < OVERVIEW_CACHE_TTL_MS) {
      return
    }
    try {
      const overview = await skillApiV2.overview()
      set({
        overview,
        skills: overview.skills,
        agents: overview.agents,
        packs: overview.packs,
        issues: overview.issues,
        settings: overview.settings,
        lastOverviewLoadedAt: Date.now(),
        initialized: true,
      })
      void get().loadProjects()
    } catch (e) {
      set({ error: String(e) })
    }
  },
  setTab: (tab) => set({ activeTab: tab }),
  setViewMode: (mode) => set({ viewMode: mode }),
  setFilter: (key, value) =>
    set((s) => ({ filters: { ...s.filters, [key]: value } })),
  selectSkill: async (id) => {
    set({ selectedSkillId: id, selectedSkillDetail: null })
    if (!id) return
    try {
      const detail = await skillApiV2.getSkillDetail(id)
      set({ selectedSkillDetail: detail })
    } catch (e) {
      set({ error: String(e) })
    }
  },
  selectPack: async (id) => {
    set({ selectedPackId: id, selectedPackDetail: null })
    if (!id) return
    try {
      const detail = await skillApiV2.getPackDetail(id)
      set({ selectedPackDetail: detail })
    } catch (e) {
      set({ error: String(e) })
    }
  },
  selectAgent: async (id) => {
    if (id && get().selectedAgentId === id && get().selectedAgentDetail && !get().agentDetailLoading) {
      return
    }
    set({ selectedAgentId: id, selectedAgentDetail: null, agentDetailLoading: !!id })
    if (id) await get().loadAgentDetail(id)
    else set({ selectedAgentDetail: null })
  },
  loadAgentDetail: async (agentId, force = false) => {
    if (!force && get().selectedAgentId === agentId && get().selectedAgentDetail && !get().agentDetailLoading) {
      return
    }
    set({ agentDetailLoading: true })
    try {
      const detail = await skillApiV2.getAgentDetail(agentId)
      set({ selectedAgentDetail: detail })
    } catch (e) {
      set({ error: String(e) })
    } finally {
      set({ agentDetailLoading: false })
    }
  },
  loadProjects: async () => {
    try {
      const projects = await skillApiV2.listProjects()
      set({ projects })
      const selectedProjectId = get().selectedProjectId
      if (selectedProjectId && !projects.some((project) => project.id === selectedProjectId)) {
        set({ selectedProjectId: null, selectedProjectDetail: null })
      }
    } catch (e) {
      set({ error: String(e) })
    }
  },
  addProject: async (rootPath) => {
    set({ projectDetailLoading: true, error: null })
    try {
      const detail = await skillApiV2.addProject(rootPath)
      const projects = await skillApiV2.listProjects()
      set({
        projects,
        selectedProjectId: detail.id,
        selectedProjectDetail: detail,
      })
    } catch (e) {
      set({ error: String(e) })
    } finally {
      set({ projectDetailLoading: false })
    }
  },
  removeProject: async (projectId) => {
    try {
      await skillApiV2.removeProject(projectId)
      const projects = await skillApiV2.listProjects()
      set((s) => ({
        projects,
        selectedProjectId: s.selectedProjectId === projectId ? null : s.selectedProjectId,
        selectedProjectDetail: s.selectedProjectId === projectId ? null : s.selectedProjectDetail,
      }))
    } catch (e) {
      set({ error: String(e) })
    }
  },
  selectProject: async (id) => {
    if (id && get().selectedProjectId === id && get().selectedProjectDetail && !get().projectDetailLoading) {
      return
    }
    set({ selectedProjectId: id, selectedProjectDetail: null, projectDetailLoading: !!id })
    if (!id) {
      set({ selectedProjectDetail: null, projectDetailLoading: false })
      return
    }
    try {
      const detail = await skillApiV2.getProjectDetail(id)
      set({ selectedProjectDetail: detail })
    } catch (e) {
      set({ error: String(e) })
    } finally {
      set({ projectDetailLoading: false })
    }
  },
  scanProject: async (projectId) => {
    set({ projectDetailLoading: true, error: null })
    try {
      const detail = await skillApiV2.scanProject(projectId)
      const projects = await skillApiV2.listProjects()
      set({
        projects,
        selectedProjectId: detail.id,
        selectedProjectDetail: detail,
      })
    } catch (e) {
      set({ error: String(e) })
    } finally {
      set({ projectDetailLoading: false })
    }
  },
  loadDiagnosisIssues: async () => {
    try {
      const issues = await skillApiV2.listDiagnosisIssues()
      set({ issues })
    } catch (e) {
      set({ error: String(e) })
    }
  },
  runDiagnosis: async () => {
    set({ busyAction: 'diagnosis' })
    try {
      const issues = await skillApiV2.runDiagnosis()
      const unmanaged = await skillApiV2.listUnmanaged()
      set({ issues, unmanaged, lastOverviewLoadedAt: 0 })
    } catch (e) {
      set({ error: String(e) })
    } finally {
      set({ busyAction: null })
    }
  },
  updateSettings: async (patch) => {
    try {
      const next = await skillApiV2.updateSettings(patch)
      set({ settings: next })
    } catch (e) {
      set({ error: String(e) })
    }
  },
  setBusy: (action) => set({ busyAction: action }),
  setError: (err) => set({ error: err }),
  setLastPreview: (p) => set({ lastPreview: p }),
}))

export function filteredSkills(state: SkillV2State): SkillSummary[] {
  const { skills, filters } = state
  const q = filters.query.trim().toLowerCase()
  return skills.filter((s) => {
    if (q) {
      const haystack = [
        s.name,
        s.description,
        s.sourceType,
        s.status,
        skillStatusSearchLabel(s.status),
        s.installedAgents.map((a) => a.displayName).join(' '),
        s.installedAgents.map((a) => a.status).join(' '),
        s.installedAgents.map((a) => targetStatusSearchLabel(a.status)).join(' '),
        hasChangedCopyInstall(s) ? 'diff 副本分叉 副本变更 副本已修改 已修改' : '',
      ]
        .join(' ')
        .toLowerCase()
      if (!haystack.includes(q)) return false
    }
    if (filters.status && s.status !== filters.status) return false
    if (filters.source && s.sourceType !== filters.source) return false
    if (filters.type && s.skillType !== filters.type) return false
    return true
  })
}

function hasChangedCopyInstall(skill: SkillSummary): boolean {
  return skill.installedAgents.some((agent) => (
    agent.mode === 'copy'
    && ['copy_modified', 'copy_diverged', 'copy_outdated', 'copyDiverged'].includes(agent.status)
  ))
}

function skillStatusSearchLabel(status: string): string {
  const labels: Record<string, string> = {
    ok: '正常',
    conflict: '冲突',
    copyDiverged: '副本分叉',
    updateAvailable: '可更新',
    unmanaged: '未管理',
  }
  return labels[status] || status
}

function targetStatusSearchLabel(status: string): string {
  const labels: Record<string, string> = {
    ok: '正常',
    conflict: '冲突',
    copy_outdated: '可更新 副本可更新',
    copy_modified: '已修改 副本已修改 副本分叉',
    copy_diverged: '已分叉 副本分叉',
    copyDiverged: '副本分叉',
    broken_link: '坏链接',
    missing: '失效',
  }
  return labels[status] || status
}
