import { create } from 'zustand'
import type {
  SkillManagerOverview,
  SkillManagerSettings,
  SkillSummary,
  SkillDetail,
  AgentSummary,
  AgentDetail,
  AgentSkillViewSnapshot,
  SkillPackSummary,
  SkillPackDetail,
  DiagnosisIssue,
  UnmanagedItemDto,
  DistributionPreview,
  ProjectSummary,
  ProjectDetail,
  MarketplaceBatchProgress,
} from '../services/skillApiV2'
import { skillApiV2 } from '../services/skillApiV2'
import { LOCAL_RUNTIME_ENVIRONMENT_ID, useRuntimeEnvironmentStore } from './runtimeEnvironmentStore'

export type SkillManagerTab = 'library' | 'install' | 'packs' | 'projects' | 'agents' | 'diagnostics' | 'settings'
export type SkillInstallTab = 'official' | 'agent' | 'local' | 'git'
export type SkillViewMode = 'cards' | 'list'
export type MarketplaceInstallItemStatus = 'queued' | 'installing' | 'success' | 'failed' | 'cancelled'

export interface MarketplaceInstallItemState {
  id: string
  name: string
  status: MarketplaceInstallItemStatus
  message?: string
}

export interface MarketplaceInstallTaskResult {
  successCount: number
  failedCount: number
  cancelled: boolean
  packName?: string
  completionError?: string
  sourceError?: string
}

export interface MarketplaceInstallTask {
  jobId: string
  source: string
  startedAt: number
  phase: MarketplaceBatchProgress['phase']
  busy: boolean
  cancelRequested: boolean
  items: Record<string, MarketplaceInstallItemState>
  result: MarketplaceInstallTaskResult | null
  error: string | null
}

const OVERVIEW_CACHE_TTL_MS = 60_000

export interface SkillFilters {
  query: string
  source: string
  status: string
  type: string
}

interface SkillV2State {
  runtimeEnvironmentId: string
  activeTab: SkillManagerTab
  activeInstallTab: SkillInstallTab
  viewMode: SkillViewMode
  overview: SkillManagerOverview | null
  settings: SkillManagerSettings | null
  skills: SkillSummary[]
  selectedSkillId: string | null
  selectedSkillDetail: SkillDetail | null
  selectedPackId: string | null
  selectedPackDetail: SkillPackDetail | null
  selectedAgentId: string | null
  agentSelectionRevision: number
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
  marketplaceInstallTask: MarketplaceInstallTask | null
  customAgentDialogRequest: number
}

interface SkillV2Actions {
  switchRuntimeEnvironment: (id: string) => Promise<void>
  init: () => Promise<void>
  refresh: () => Promise<void>
  loadOverview: (force?: boolean) => Promise<void>
  setTab: (tab: SkillManagerTab) => void
  setInstallTab: (tab: SkillInstallTab) => void
  setViewMode: (mode: SkillViewMode) => void
  setFilter: <K extends keyof SkillFilters>(key: K, value: SkillFilters[K]) => void
  selectSkill: (id: string | null) => Promise<void>
  selectPack: (id: string | null) => Promise<void>
  selectAgent: (id: string | null) => Promise<void>
  loadAgentDetail: (agentId: string, force?: boolean) => Promise<void>
  applyAgentSkillViewSnapshot: (expectedRuntimeEnvironmentId: string, agentId: string, snapshot: AgentSkillViewSnapshot) => boolean
  removeAgentSkillItems: (expectedRuntimeEnvironmentId: string, agentId: string, managedTargetIds: string[], unmanagedIds: string[]) => boolean
  loadProjects: (force?: boolean) => Promise<void>
  refreshProject: (projectId: string | null) => Promise<void>
  addProject: (rootPath: string) => Promise<void>
  removeProject: (projectId: string) => Promise<void>
  selectProject: (id: string | null, force?: boolean) => Promise<void>
  scanProject: (projectId: string) => Promise<void>
  loadDiagnosisIssues: () => Promise<void>
  runDiagnosis: () => Promise<void>
  updateSettings: (patch: Partial<SkillManagerSettings>) => Promise<void>
  setBusy: (action: string | null) => void
  setError: (err: string | null) => void
  setLastPreview: (p: DistributionPreview | null) => void
  beginMarketplaceInstallTask: (jobId: string, source: string, items: Array<{ id: string; name: string }>) => void
  updateMarketplaceInstallProgress: (progress: MarketplaceBatchProgress) => void
  updateMarketplaceInstallItem: (itemId: string, patch: Partial<MarketplaceInstallItemState>) => void
  setMarketplaceInstallPhase: (phase: MarketplaceBatchProgress['phase']) => void
  finishMarketplaceInstallTask: (result: MarketplaceInstallTaskResult) => void
  cancelMarketplaceInstallTask: () => Promise<boolean>
  dismissMarketplaceInstallTask: () => void
  requestCustomAgentDialog: () => void
}

let marketplaceProgressListenerStarted = false
let latestAgentDetailRequest = 0
let latestProjectListRequest = 0
let latestProjectDetailRequest = 0
let projectSelectionRevision = 0
let skillRuntimeGeneration = 0
let pendingInitialization: { generation: number; task: Promise<void> } | null = null

function isProjectRuntime(id: string) {
  return useSkillStoreV2.getState().runtimeEnvironmentId === id
    && useRuntimeEnvironmentStore.getState().selectedEnvironmentId === id
}

export function captureProjectView(projectId: string | null) {
  const { runtimeEnvironmentId } = useSkillStoreV2.getState()
  const { selectionRevision } = useRuntimeEnvironmentStore.getState()
  const projectRevision = projectSelectionRevision
  const isRuntimeCurrent = () => isProjectRuntime(runtimeEnvironmentId)
    && useRuntimeEnvironmentStore.getState().selectionRevision === selectionRevision
  return {
    isRuntimeCurrent,
    isSelected: () => isRuntimeCurrent()
      && projectSelectionRevision === projectRevision
      && useSkillStoreV2.getState().selectedProjectId === projectId,
  }
}

function ensureMarketplaceProgressListener() {
  if (marketplaceProgressListenerStarted) return
  marketplaceProgressListenerStarted = true
  void skillApiV2.onMarketplaceBatchProgress((progress) => {
    useSkillStoreV2.getState().updateMarketplaceInstallProgress(progress)
  }).catch(() => {
    marketplaceProgressListenerStarted = false
  })
}

export const useSkillStoreV2 = create<SkillV2State & SkillV2Actions>((set, get) => ({
  runtimeEnvironmentId: LOCAL_RUNTIME_ENVIRONMENT_ID,
  activeTab: 'library',
  activeInstallTab: 'official',
  viewMode: 'cards',
  overview: null,
  settings: null,
  skills: [],
  selectedSkillId: null,
  selectedSkillDetail: null,
  selectedPackId: null,
  selectedPackDetail: null,
  selectedAgentId: null,
  agentSelectionRevision: 0,
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
  marketplaceInstallTask: null,
  customAgentDialogRequest: 0,

  switchRuntimeEnvironment: async (id) => {
    if (get().runtimeEnvironmentId === id) {
      await get().init()
      return
    }
    skillRuntimeGeneration += 1
    latestAgentDetailRequest += 1
    latestProjectListRequest += 1
    latestProjectDetailRequest += 1
    projectSelectionRevision += 1
    set({
      runtimeEnvironmentId: id,
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
      error: null,
      busyAction: null,
      lastPreview: null,
      initialized: false,
      agentDetailLoading: false,
      projectDetailLoading: false,
      startupScanInFlight: false,
      lastOverviewLoadedAt: 0,
    })
    await get().init()
  },

  init: async () => {
    const runtimeEnvironmentId = get().runtimeEnvironmentId
    const generation = skillRuntimeGeneration
    const isCurrent = () => generation === skillRuntimeGeneration
      && get().runtimeEnvironmentId === runtimeEnvironmentId
      && useRuntimeEnvironmentStore.getState().selectedEnvironmentId === runtimeEnvironmentId
    if (!isCurrent()) return
    // Page entry should be cheap: bootstrap only ensures DB/dirs are usable,
    // then reads cached SQLite state. Startup scanning runs in the background
    // so opening the library is not blocked by walking every Agent skill dir.
    if (get().initialized) {
      if (!get().overview) await get().loadOverview(true)
      return
    }
    if (pendingInitialization?.generation === generation) return pendingInitialization.task
    const task = Promise.resolve().then(async () => {
      if (!isCurrent()) return
      set({ loading: true, error: null })
      try {
        await skillApiV2.bootstrap()
        if (!isCurrent()) return
        await get().loadOverview(true)
        if (!isCurrent()) return
        await get().loadProjects(true)
        const settings = get().settings
        if (!isCurrent() || !settings) return
        set({ initialized: true })

        if (settings.startupScan && !get().startupScanInFlight) {
          set({ startupScanInFlight: true, busyAction: get().busyAction ?? 'startupScan' })
          void (async () => {
            try {
              await skillApiV2.init()
              if (isCurrent()) await get().loadOverview(true)
            } catch (e) {
              if (isCurrent()) set({ error: String(e) })
            } finally {
              if (isCurrent()) {
                set((s) => ({
                  startupScanInFlight: false,
                  busyAction: s.busyAction === 'startupScan' ? null : s.busyAction,
                }))
              }
            }
          })()
        }
      } catch (e) {
        if (isCurrent()) set({ error: String(e) })
      } finally {
        if (isCurrent()) set({ loading: false })
      }
    })
    const initialization = { generation, task }
    pendingInitialization = initialization
    try {
      await task
    } finally {
      if (pendingInitialization === initialization) pendingInitialization = null
    }
  },
  refresh: async () => {
    const runtimeEnvironmentId = get().runtimeEnvironmentId
    set({ loading: true, error: null })
    try {
      const overview = await skillApiV2.refreshOverview()
      const unmanaged = await skillApiV2.listUnmanaged()
      if (get().runtimeEnvironmentId !== runtimeEnvironmentId) return
      set({
        overview,
        skills: overview.skills,
        agents: overview.agents,
        packs: overview.packs,
        issues: overview.issues,
        unmanaged,
        settings: overview.settings,
        lastOverviewLoadedAt: Date.now(),
      })
      await get().loadProjects(true)
      set({ initialized: true })
    } catch (e) {
      set({ error: String(e) })
    } finally {
      if (get().runtimeEnvironmentId === runtimeEnvironmentId) {
        set({ loading: false })
      }
    }
  },
  loadOverview: async (force = false) => {
    const runtimeEnvironmentId = get().runtimeEnvironmentId
    const selection = useRuntimeEnvironmentStore.getState()
    const isCurrent = () => {
      const current = useRuntimeEnvironmentStore.getState()
      return get().runtimeEnvironmentId === runtimeEnvironmentId
        && current.selectedEnvironmentId === selection.selectedEnvironmentId
        && current.selectionRevision === selection.selectionRevision
    }
    const now = Date.now()
    if (!force && get().overview && now - get().lastOverviewLoadedAt < OVERVIEW_CACHE_TTL_MS) {
      return
    }
    try {
      const overview = await skillApiV2.overview()
      if (!isCurrent()) return
      const unmanaged = await skillApiV2.listUnmanaged()
      if (!isCurrent()) return
      set({
        overview,
        skills: overview.skills,
        agents: overview.agents,
        packs: overview.packs,
        issues: overview.issues,
        unmanaged,
        settings: overview.settings,
        lastOverviewLoadedAt: Date.now(),
      })
      void get().loadProjects()
    } catch (e) {
      if (isCurrent()) set({ error: String(e) })
    }
  },
  setTab: (tab) => set({ activeTab: tab }),
  requestCustomAgentDialog: () => set((state) => ({
    customAgentDialogRequest: state.customAgentDialogRequest + 1,
  })),
  setInstallTab: (tab) => set({ activeInstallTab: tab }),
  setViewMode: (mode) => set({ viewMode: mode }),
  setFilter: (key, value) =>
    set((s) => ({ filters: { ...s.filters, [key]: value } })),
  selectSkill: async (id) => {
    const runtimeEnvironmentId = get().runtimeEnvironmentId
    set({ selectedSkillId: id, selectedSkillDetail: null })
    if (!id) return
    try {
      const detail = await skillApiV2.getSkillDetail(id)
      if (get().runtimeEnvironmentId !== runtimeEnvironmentId) return
      set({ selectedSkillDetail: detail })
    } catch (e) {
      set({ error: String(e) })
    }
  },
  selectPack: async (id) => {
    const runtimeEnvironmentId = get().runtimeEnvironmentId
    set({ selectedPackId: id, selectedPackDetail: null })
    if (!id) return
    try {
      const detail = await skillApiV2.getPackDetail(id)
      if (get().runtimeEnvironmentId !== runtimeEnvironmentId) return
      set({ selectedPackDetail: detail })
    } catch (e) {
      set({ error: String(e) })
    }
  },
  selectAgent: async (id) => {
    const { selectedAgentId, selectedAgentDetail, agentDetailLoading, agentSelectionRevision } = get()
    if (id && selectedAgentId === id && selectedAgentDetail && !agentDetailLoading) {
      return
    }
    latestAgentDetailRequest += 1
    set({
      selectedAgentId: id,
      agentSelectionRevision: selectedAgentId !== id ? agentSelectionRevision + 1 : agentSelectionRevision,
      selectedAgentDetail: null,
      agentDetailLoading: !!id,
    })
    if (id) await get().loadAgentDetail(id)
    else set({ selectedAgentDetail: null })
  },
  loadAgentDetail: async (agentId, force = false) => {
    const { runtimeEnvironmentId, selectedAgentId, selectedAgentDetail, agentDetailLoading } = get()
    if (selectedAgentId !== agentId || (!force && selectedAgentDetail?.id === agentId && !agentDetailLoading)) return

    const requestId = ++latestAgentDetailRequest
    const isCurrentRequest = () => {
      const current = get()
      return requestId === latestAgentDetailRequest
        && current.runtimeEnvironmentId === runtimeEnvironmentId
        && current.selectedAgentId === agentId
    }

    set({ agentDetailLoading: true })
    try {
      const detail = await skillApiV2.getAgentDetail(agentId)
      if (isCurrentRequest()) {
        set({ selectedAgentDetail: detail, agentDetailLoading: false })
      }
    } catch (e) {
      if (isCurrentRequest()) {
        set({ error: String(e), agentDetailLoading: false })
      }
    }
  },
  applyAgentSkillViewSnapshot: (expectedRuntimeEnvironmentId, agentId, snapshot) => {
    if (
      get().runtimeEnvironmentId !== expectedRuntimeEnvironmentId
      || get().selectedAgentId !== agentId
      || snapshot.agentDetail.id !== agentId
    ) {
      return false
    }
    latestAgentDetailRequest += 1
    const overview = snapshot.overview
    set({
      selectedAgentDetail: snapshot.agentDetail,
      agentDetailLoading: false,
      overview,
      skills: overview.skills,
      agents: overview.agents,
      packs: overview.packs,
      issues: overview.issues,
      unmanaged: snapshot.unmanaged,
      settings: overview.settings,
      lastOverviewLoadedAt: Date.now(),
    })
    return true
  },
  removeAgentSkillItems: (expectedRuntimeEnvironmentId, agentId, managedTargetIds, unmanagedIds) => {
    const current = get()
    const detail = current.selectedAgentDetail
    if (
      current.runtimeEnvironmentId !== expectedRuntimeEnvironmentId
      || current.selectedAgentId !== agentId
      || detail?.id !== agentId
    ) {
      return false
    }

    const managedIds = new Set(managedTargetIds)
    const unmanagedIdSet = new Set(unmanagedIds)
    const managedTargets = [...detail.skills, ...detail.inheritedManagedSkills]
      .filter((target) => managedIds.has(target.id))
    const unmanagedItems = [...current.unmanaged, ...detail.inheritedUnmanagedSkills]
      .filter((item, index, items) => unmanagedIdSet.has(item.id) && items.findIndex((candidate) => candidate.id === item.id) === index)
    const removedTargetKeys = new Set(managedTargets.map((target) => `${target.skillId}\0${target.agentId}`))
    const managedCounts = new Map<string, number>()
    const unmanagedCounts = new Map<string, number>()
    managedTargets.forEach((target) => managedCounts.set(target.agentId, (managedCounts.get(target.agentId) || 0) + 1))
    unmanagedItems.forEach((item) => {
      if (item.agentId) unmanagedCounts.set(item.agentId, (unmanagedCounts.get(item.agentId) || 0) + 1)
    })

    const overview = current.overview
      ? {
          ...current.overview,
          metrics: {
            ...current.overview.metrics,
            targetCount: Math.max(0, current.overview.metrics.targetCount - managedTargets.length),
            unmanagedCount: Math.max(0, current.overview.metrics.unmanagedCount - unmanagedItems.length),
          },
          skills: current.overview.skills.map((skill) => ({
            ...skill,
            installedAgents: skill.installedAgents.filter((installed) => !removedTargetKeys.has(`${skill.id}\0${installed.agentId}`)),
          })),
          agents: current.overview.agents.map((agent) => ({
            ...agent,
            managedSkillCount: Math.max(0, agent.managedSkillCount - (managedCounts.get(agent.id) || 0)),
            unmanagedSkillCount: Math.max(0, agent.unmanagedSkillCount - (unmanagedCounts.get(agent.id) || 0)),
          })),
        }
      : null
    const skills = overview?.skills ?? current.skills.map((skill) => ({
      ...skill,
      installedAgents: skill.installedAgents.filter((installed) => !removedTargetKeys.has(`${skill.id}\0${installed.agentId}`)),
    }))
    const agents = overview?.agents ?? current.agents.map((agent) => ({
      ...agent,
      managedSkillCount: Math.max(0, agent.managedSkillCount - (managedCounts.get(agent.id) || 0)),
      unmanagedSkillCount: Math.max(0, agent.unmanagedSkillCount - (unmanagedCounts.get(agent.id) || 0)),
    }))

    latestAgentDetailRequest += 1
    set({
      agentDetailLoading: false,
      selectedAgentDetail: {
        ...detail,
        skills: detail.skills.filter((target) => !managedIds.has(target.id)),
        inheritedManagedSkills: detail.inheritedManagedSkills.filter((target) => !managedIds.has(target.id)),
        inheritedUnmanagedSkills: detail.inheritedUnmanagedSkills.filter((item) => !unmanagedIdSet.has(item.id)),
      },
      overview,
      skills,
      agents,
      unmanaged: current.unmanaged.filter((item) => !unmanagedIdSet.has(item.id)),
    })
    return true
  },
  loadProjects: async () => {
    const runtimeEnvironmentId = get().runtimeEnvironmentId
    if (!isProjectRuntime(runtimeEnvironmentId)) return
    const requestId = ++latestProjectListRequest
    const isCurrent = () => requestId === latestProjectListRequest && isProjectRuntime(runtimeEnvironmentId)
    try {
      const projects = await skillApiV2.listProjects()
      if (!isCurrent()) return
      const selectedProjectId = get().selectedProjectId
      if (selectedProjectId && !projects.some((project) => project.id === selectedProjectId)) {
        latestProjectDetailRequest += 1
        projectSelectionRevision += 1
        set({ projects, selectedProjectId: null, selectedProjectDetail: null, projectDetailLoading: false })
      } else {
        set({ projects })
      }
    } catch (e) {
      if (isCurrent()) set({ error: String(e) })
    }
  },
  refreshProject: async (projectId) => {
    const view = captureProjectView(projectId)
    await get().loadProjects()
    if (projectId && view.isRuntimeCurrent() && get().selectedProjectId === projectId) {
      await get().selectProject(projectId, true)
    }
  },
  addProject: async (rootPath) => {
    const view = captureProjectView(get().selectedProjectId)
    if (!view.isRuntimeCurrent()) return
    set({ error: null })
    try {
      const detail = await skillApiV2.addProject(rootPath)
      if (!view.isRuntimeCurrent()) return
      await get().loadProjects()
      if (view.isSelected() && get().projects.some((project) => project.id === detail.id)) {
        await get().selectProject(detail.id, true)
      }
    } catch (e) {
      if (view.isSelected()) set({ error: String(e) })
    }
  },
  removeProject: async (projectId) => {
    const view = captureProjectView(projectId)
    if (!view.isSelected()) return
    try {
      await skillApiV2.removeProject(projectId)
      if (view.isRuntimeCurrent()) await get().loadProjects()
    } catch (e) {
      if (view.isSelected()) set({ error: String(e) })
    }
  },
  selectProject: async (id, force = false) => {
    const runtimeEnvironmentId = get().runtimeEnvironmentId
    if (!isProjectRuntime(runtimeEnvironmentId)) return
    const current = get()
    const sameProject = current.selectedProjectId === id
    if (!force && sameProject && (current.selectedProjectDetail || current.projectDetailLoading)) return
    if (!sameProject) projectSelectionRevision += 1
    const requestId = ++latestProjectDetailRequest
    const isCurrent = () => requestId === latestProjectDetailRequest
      && get().selectedProjectId === id
      && isProjectRuntime(runtimeEnvironmentId)
    set({
      selectedProjectId: id,
      selectedProjectDetail: sameProject ? current.selectedProjectDetail : null,
      projectDetailLoading: id !== null,
      error: sameProject ? current.error : null,
    })
    if (!id) return
    try {
      const detail = await skillApiV2.getProjectDetail(id)
      if (isCurrent()) set({ selectedProjectDetail: detail })
    } catch (e) {
      if (isCurrent()) set({ error: String(e) })
    } finally {
      if (isCurrent()) set({ projectDetailLoading: false })
    }
  },
  scanProject: async (projectId) => {
    const view = captureProjectView(projectId)
    if (!view.isRuntimeCurrent()) return
    if (view.isSelected()) set({ error: null })
    try {
      await skillApiV2.scanProject(projectId)
      if (view.isRuntimeCurrent()) await get().refreshProject(projectId)
    } catch (e) {
      if (view.isSelected()) set({ error: String(e) })
    }
  },
  loadDiagnosisIssues: async () => {
    const runtimeEnvironmentId = get().runtimeEnvironmentId
    try {
      const issues = await skillApiV2.listDiagnosisIssues()
      if (get().runtimeEnvironmentId !== runtimeEnvironmentId) return
      set({ issues })
    } catch (e) {
      set({ error: String(e) })
    }
  },
  runDiagnosis: async () => {
    const runtimeEnvironmentId = get().runtimeEnvironmentId
    set({ busyAction: 'diagnosis' })
    try {
      const issues = await skillApiV2.runDiagnosis()
      const unmanaged = await skillApiV2.listUnmanaged()
      if (get().runtimeEnvironmentId !== runtimeEnvironmentId) return
      set({ issues, unmanaged, lastOverviewLoadedAt: 0 })
    } catch (e) {
      set({ error: String(e) })
    } finally {
      if (get().runtimeEnvironmentId === runtimeEnvironmentId) {
        set({ busyAction: null })
      }
    }
  },
  updateSettings: async (patch) => {
    const runtimeEnvironmentId = get().runtimeEnvironmentId
    try {
      const next = await skillApiV2.updateSettings(patch)
      if (get().runtimeEnvironmentId !== runtimeEnvironmentId) return
      set({ settings: next })
    } catch (e) {
      set({ error: String(e) })
    }
  },
  setBusy: (action) => set({ busyAction: action }),
  setError: (err) => set({ error: err }),
  setLastPreview: (p) => set({ lastPreview: p }),
  beginMarketplaceInstallTask: (jobId, source, items) => {
    ensureMarketplaceProgressListener()
    set({
      marketplaceInstallTask: {
        jobId,
        source,
        startedAt: Date.now(),
        phase: 'preparing',
        busy: true,
        cancelRequested: false,
        items: Object.fromEntries(items.map((item) => [item.id, {
          ...item,
          status: 'queued' as const,
        }])),
        result: null,
        error: null,
      },
    })
  },
  updateMarketplaceInstallProgress: (progress) => set((state) => {
    const task = state.marketplaceInstallTask
    if (!task || task.jobId !== progress.jobId) return state
    let items = task.items
    if (progress.itemId && ['installing', 'success', 'failed'].includes(progress.phase)) {
      const current = items[progress.itemId]
      if (current) {
        items = {
          ...items,
          [progress.itemId]: {
            ...current,
            status: progress.phase === 'installing'
              ? 'installing'
              : progress.phase === 'success'
                ? 'success'
                : 'failed',
            message: progress.message || current.message,
          },
        }
      }
    }
    return {
      marketplaceInstallTask: {
        ...task,
        phase: progress.phase,
        items,
        error: (progress.phase === 'failed' || progress.phase === 'source_failed') && !progress.itemId
          ? progress.message || task.error
          : task.error,
      },
    }
  }),
  updateMarketplaceInstallItem: (itemId, patch) => set((state) => {
    const task = state.marketplaceInstallTask
    const current = task?.items[itemId]
    if (!task || !current) return state
    return {
      marketplaceInstallTask: {
        ...task,
        items: {
          ...task.items,
          [itemId]: { ...current, ...patch, id: current.id, name: current.name },
        },
      },
    }
  }),
  setMarketplaceInstallPhase: (phase) => set((state) => state.marketplaceInstallTask ? {
    marketplaceInstallTask: { ...state.marketplaceInstallTask, phase },
  } : state),
  finishMarketplaceInstallTask: (result) => set((state) => state.marketplaceInstallTask ? {
    marketplaceInstallTask: {
      ...state.marketplaceInstallTask,
      phase: result.cancelled ? 'cancelled' : result.sourceError ? 'source_failed' : result.failedCount > 0 || result.completionError ? 'failed' : 'completed',
      busy: false,
      cancelRequested: false,
      result,
      error: result.sourceError || result.completionError || state.marketplaceInstallTask.error,
    },
  } : state),
  cancelMarketplaceInstallTask: async () => {
    const task = get().marketplaceInstallTask
    if (!task?.busy || task.phase === 'organizing') return false
    const previousPhase = task.phase
    set({ marketplaceInstallTask: { ...task, phase: 'cancelling', cancelRequested: true, error: null } })
    try {
      for (let attempt = 0; attempt < 5; attempt += 1) {
        if (await skillApiV2.cancelMarketplaceSkillBatch(task.jobId)) return true
        await new Promise((resolve) => setTimeout(resolve, 80))
      }
      set((state) => state.marketplaceInstallTask?.jobId === task.jobId ? {
        marketplaceInstallTask: {
          ...state.marketplaceInstallTask,
          phase: previousPhase,
          cancelRequested: false,
          error: 'cancel-not-accepted',
        },
      } : state)
      return false
    } catch (error) {
      set((state) => state.marketplaceInstallTask?.jobId === task.jobId ? {
        marketplaceInstallTask: {
          ...state.marketplaceInstallTask,
          phase: previousPhase,
          cancelRequested: false,
          error: String(error),
        },
      } : state)
      return false
    }
  },
  dismissMarketplaceInstallTask: () => set((state) => (
    state.marketplaceInstallTask?.busy ? state : { marketplaceInstallTask: null }
  )),
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
