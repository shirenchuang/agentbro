import { invoke } from '@tauri-apps/api/core'

// ── Skill Manager v2 DTO types ────────────────────────────────────

export interface SkillManagerMetrics {
  centerSkillCount: number
  targetCount: number
  unmanagedCount: number
  issueCount: number
}

export interface SkillManagerSettings {
  centerPath: string
  sqlitePath: string
  defaultDistributeMode: 'link' | 'copy'
  linkFailPolicy: 'ask' | 'copy'
  startupScan: boolean
  showUnmanaged: boolean
}

export interface InstalledAgentRef {
  agentId: string
  displayName: string
  iconKey: string
  mode: 'link' | 'copy'
  status: string
}

export interface SkillSummary {
  id: string
  name: string
  description: string
  skillType: string
  sourceType: string
  sourceUri: string | null
  centerPath: string
  currentHash: string
  status: 'ok' | 'updateAvailable' | 'conflict' | 'unmanaged' | 'copyDiverged' | string
  installedAgents: InstalledAgentRef[]
}

export interface TargetClaim {
  id: string
  claimType: 'direct' | 'pack'
  packId: string | null
  packName: string | null
  createdAt: string
}

export interface SkillSourceDetail {
  sourceType: string
  sourceUri: string | null
  sourceRef: string | null
  importedFromAgent: string | null
  importedFromPath: string | null
  installedVia: string
  createdAt: string
  updatedAt: string
}

export interface SkillTargetDetail {
  id: string
  agentId: string
  targetPath: string
  installMode: 'link' | 'copy'
  actualMode: 'link' | 'copy'
  sourceHash: string
  currentHash: string | null
  status: string
  createdAt: string
  updatedAt: string
  claims: TargetClaim[]
}

export interface FileTreeNode {
  name: string
  nodeType: 'file' | 'dir'
  path: string
  children: FileTreeNode[] | null
}

export interface SkillDetail extends SkillSummary {
  frontmatter: Record<string, string>
  files: FileTreeNode | null
  targets: SkillTargetDetail[]
  source: SkillSourceDetail | null
}

export interface AgentSummary {
  id: string
  displayName: string
  iconKey: string
  enabled: boolean
  skillsDir: string | null
  version: string | null
  latestVersion: string | null
  installed: boolean
  managedSkillCount: number
  unmanagedSkillCount: number
}

export interface AgentSummaryLite {
  id: string
  displayName: string
  iconKey: string
}

export interface SkillPackSummary {
  id: string
  name: string
  description: string
  tags: string[]
  memberCount: number
  appliedAgentCount: number
  healthy: boolean
}

export interface PackMember {
  skillId: string
  skillName: string
  required: boolean
  sortOrder: number
  missing: boolean
}

export interface AppliedPackSummary {
  packId: string
  packName: string
  memberCount: number
}

export interface SkillPackDetail {
  id: string
  name: string
  description: string
  tags: string[]
  members: PackMember[]
  appliedAgents: AppliedPackSummary[]
  createdAt: string
  updatedAt: string
}

export interface McpServerStatus {
  name: string
  command: string
  args: string[]
  valid: boolean
  message: string
}

export interface PluginStatus {
  id: string
  name: string
  version: string | null
  enabled: boolean
  source: string | null
}

export interface AgentHealthIssue {
  kind: string
  message: string
  severity: string
}

export interface AgentDetail {
  id: string
  displayName: string
  iconKey: string
  version: string | null
  latestVersion: string | null
  skillsDir: string | null
  configPath: string | null
  mcpConfigPath: string | null
  pluginDir: string | null
  skills: SkillTargetDetail[]
  appliedPacks: AppliedPackSummary[]
  availablePacks: SkillPackSummary[]
  mcpServers: McpServerStatus[]
  plugins: PluginStatus[]
  health: AgentHealthIssue[]
}

export interface SkillManagerOverview {
  metrics: SkillManagerMetrics
  skills: SkillSummary[]
  agents: AgentSummary[]
  packs: SkillPackSummary[]
  issues: DiagnosisIssue[]
  settings: SkillManagerSettings
}

export interface ConflictBlocker {
  skillId: string
  agentId: string
  reason: string
  existingPath: string | null
}

export interface DistributionChange {
  skillId: string
  agentId: string
  action: 'create' | 'reuse' | 'appendClaim' | 'skip' | 'blocked' | string
  actualMode?: 'link' | 'copy'
  reason?: string
  targetPath: string
}

export interface DistributionPreview {
  skillIds: string[]
  targetAgents: string[]
  requestedMode: 'link' | 'copy'
  changes: DistributionChange[]
  blockers: ConflictBlocker[]
}

export interface AddCenterSkillInput {
  sourcePath: string
  sourceType: string
  sourceUri?: string | null
  importedFromAgent?: string | null
  importedFromPath?: string | null
  multi?: boolean
}

export interface AddCenterSkillCandidate {
  skillId: string
  proposedSkillId: string
  name: string
  description: string
  sourceDir: string
  hash: string
  action: 'create' | 'update' | 'blocked_same_name_diff_source' | string
  existingSourceType: string | null
  reason: string | null
}

export interface AddCenterSkillPreview {
  candidates: AddCenterSkillCandidate[]
  blockers: AddCenterSkillCandidate[]
  centerPath: string
}

export interface AddCenterSkillDecision {
  skillId: string
  proposedSkillId?: string | null
  resolution: 'create' | 'update' | 'skip'
}

export interface AddCenterSkillResult {
  skillIds: string[]
  updated: string[]
  skipped: string[]
}

export interface AffectedTarget {
  targetId: string
  agentId: string
  displayName: string
  targetPath: string
  mode: string
  claimCount: number
}

export interface DeleteCenterSkillPreview {
  skillId: string
  affectedTargets: AffectedTarget[]
  removable: boolean
  warnings: string[]
}

export interface UnmanagedItemDto {
  id: string
  itemType: string
  agentId: string | null
  path: string
  inferredSkillId: string | null
  hash: string | null
  reason: string
}

export interface DiagnosisAction {
  id: string
  label: string
  destructive: boolean
}

export interface DiagnosisIssue {
  id: string
  issueType: string
  severity: 'info' | 'warning' | 'error'
  fixKind: 'auto' | 'confirm' | 'manual' | 'info'
  title: string
  detail: string
  entityType: 'skill' | 'target' | 'pack' | 'agent' | 'mcp' | 'plugin' | 'snapshot'
  entityId: string | null
  actions: DiagnosisAction[]
}

export interface AdoptOption {
  value: string
  label: string
  destructive: boolean
}

export interface AdoptPreview {
  agentId: string
  unmanagedId: string
  skillPath: string
  inferredSkillId: string
  hash: string
  centerHasSameId: boolean
  canQuickAdopt: boolean
  options: AdoptOption[]
}

export interface CopySyncPreview {
  targetId: string
  skillId: string
  targetPath: string
  sourceHash: string
  centerHash: string
  copyHash: string
  state: 'ok' | 'copy_outdated' | 'copy_modified' | 'copy_diverged' | string
  suggested: 'none' | 'center_over_agent' | 'agent_over_center' | 'manual' | string
}

export interface RevokeResult {
  packId: string
  agentId: string
  removedClaims: number
  removedTargets: number
  preservedTargets: number
}

export interface UpsertPackInput {
  id: string
  name: string
  description: string
  tags: string[]
  skillIds: string[]
}

const isTauri = '__TAURI_INTERNALS__' in window

export const skillApiV2 = {
  init: () => (isTauri ? invoke<void>('skill_manager_init') : Promise.resolve()),
  overview: () =>
    isTauri
      ? invoke<SkillManagerOverview>('skill_manager_overview')
      : Promise.resolve(demoOverview()),
  refresh: () => (isTauri ? invoke<void>('skill_manager_refresh') : Promise.resolve()),
  getSettings: () =>
    isTauri
      ? invoke<SkillManagerSettings>('skill_manager_settings')
      : Promise.resolve({
          centerPath: '~/.agentbro/skills',
          sqlitePath: '~/.agentbro/skill-manager.db',
          defaultDistributeMode: 'link' as const,
          linkFailPolicy: 'ask' as const,
          startupScan: true,
          showUnmanaged: true,
        }),
  updateSettings: (patch: Partial<SkillManagerSettings>) =>
    isTauri
      ? invoke<SkillManagerSettings>('skill_manager_update_settings', {
          update: {
            centerPath: patch.centerPath ?? null,
            sqlitePath: patch.sqlitePath ?? null,
            defaultDistributeMode: patch.defaultDistributeMode ?? null,
            linkFailPolicy: patch.linkFailPolicy ?? null,
            startupScan: patch.startupScan ?? null,
            showUnmanaged: patch.showUnmanaged ?? null,
          },
        })
      : Promise.resolve({} as SkillManagerSettings),

  listCenterSkills: () =>
    isTauri ? invoke<SkillSummary[]>('list_center_skills_v2') : Promise.resolve([]),
  getSkillDetail: (skillId: string) =>
    isTauri ? invoke<SkillDetail>('get_skill_detail_v2', { skillId }) : Promise.resolve(null as unknown as SkillDetail),

  previewAddCenterSkill: (input: AddCenterSkillInput) =>
    isTauri
      ? invoke<AddCenterSkillPreview>('preview_add_center_skill', { input })
      : Promise.resolve({ candidates: [], blockers: [], centerPath: '' }),
  executeAddCenterSkill: (input: AddCenterSkillInput, decisions: AddCenterSkillDecision[]) =>
    isTauri
      ? invoke<AddCenterSkillResult>('execute_add_center_skill', { input, decisions })
      : Promise.resolve({ skillIds: [], updated: [], skipped: [] }),

  previewDeleteCenterSkill: (skillId: string) =>
    isTauri
      ? invoke<DeleteCenterSkillPreview>('preview_delete_center_skill', { skillId })
      : Promise.resolve({ skillId, affectedTargets: [], removable: true, warnings: [] }),
  executeDeleteCenterSkill: (skillId: string, removeLinked: boolean) =>
    isTauri
      ? invoke<void>('execute_delete_center_skill', { skillId, removeLinked })
      : Promise.resolve(),

  previewDistribute: (skillIds: string[], targetAgents: string[], requestedMode: 'link' | 'copy') =>
    isTauri
      ? invoke<DistributionPreview>('preview_distribute_skill', { skillIds, targetAgents, requestedMode })
      : Promise.resolve({ skillIds, targetAgents, requestedMode, changes: [], blockers: [] }),
  executeDistribute: (preview: DistributionPreview) =>
    isTauri ? invoke<DistributionPreview>('execute_distribute_skill', { preview }) : Promise.resolve(preview),

  scanAgentInventory: (agentId: string) =>
    isTauri ? invoke<{ agentId: string; managed: number; unmanaged: number }>('scan_agent_inventory', { agentId }) : Promise.resolve({ agentId, managed: 0, unmanaged: 0 }),

  previewAdopt: (agentId: string, unmanagedId: string) =>
    isTauri ? invoke<AdoptPreview>('preview_adopt_agent_skill', { agentId, unmanagedId }) : Promise.resolve(null as unknown as AdoptPreview),
  executeAdopt: (agentId: string, unmanagedId: string, option: string, renamedId?: string | null) =>
    isTauri ? invoke<string>('execute_adopt_agent_skill', { agentId, unmanagedId, option, renamedId: renamedId ?? null }) : Promise.resolve(''),

  previewSyncCopy: (targetId: string) =>
    isTauri ? invoke<CopySyncPreview>('preview_sync_copy_target', { targetId }) : Promise.resolve(null as unknown as CopySyncPreview),
  executeSyncCopy: (targetId: string, action: string) =>
    isTauri ? invoke<CopySyncPreview>('execute_sync_copy_target', { targetId, action }) : Promise.resolve(null as unknown as CopySyncPreview),

  listPacks: () => (isTauri ? invoke<SkillPackSummary[]>('list_skill_packs_v2') : Promise.resolve([])),
  getPackDetail: (packId: string) =>
    isTauri ? invoke<SkillPackDetail>('get_skill_pack_detail', { packId }) : Promise.resolve(null as unknown as SkillPackDetail),
  upsertPack: (pack: UpsertPackInput) =>
    isTauri ? invoke<SkillPackDetail>('execute_upsert_skill_pack', { pack }) : Promise.resolve(null as unknown as SkillPackDetail),
  deletePack: (packId: string) =>
    isTauri ? invoke<void>('execute_delete_skill_pack', { packId }) : Promise.resolve(),
  previewApplyPack: (packId: string, targetAgents: string[], requestedMode: 'link' | 'copy') =>
    isTauri ? invoke<DistributionPreview>('preview_apply_skill_pack', { packId, targetAgents, requestedMode }) : Promise.resolve({ skillIds: [], targetAgents, requestedMode, changes: [], blockers: [] }),
  executeApplyPack: (packId: string, targetAgents: string[], requestedMode: 'link' | 'copy') =>
    isTauri ? invoke<DistributionPreview>('execute_apply_skill_pack', { packId, targetAgents, requestedMode }) : Promise.resolve({ skillIds: [], targetAgents, requestedMode, changes: [], blockers: [] }),
  removePackFromAgent: (packId: string, agentId: string) =>
    isTauri ? invoke<RevokeResult>('execute_remove_skill_pack_from_agent', { packId, agentId }) : Promise.resolve({ packId, agentId, removedClaims: 0, removedTargets: 0, preservedTargets: 0 }),
  removeSkillFromPack: (packId: string, skillId: string, alsoRemoveTargets: boolean) =>
    isTauri ? invoke<void>('execute_remove_skill_from_pack', { packId, skillId, alsoRemoveTargets }) : Promise.resolve(),

  listAgents: () => (isTauri ? invoke<AgentSummary[]>('list_managed_agents_v2') : Promise.resolve([])),
  getAgentDetail: (agentId: string) =>
    isTauri ? invoke<AgentDetail>('get_agent_detail_v2', { agentId }) : Promise.resolve(null as unknown as AgentDetail),
  listUnmanaged: () => (isTauri ? invoke<UnmanagedItemDto[]>('list_unmanaged_v2') : Promise.resolve([])),

  runDiagnosis: () => (isTauri ? invoke<DiagnosisIssue[]>('run_skill_manager_diagnosis') : Promise.resolve([])),
  listDiagnosisIssues: () => (isTauri ? invoke<DiagnosisIssue[]>('list_diagnosis_issues') : Promise.resolve([])),
  previewFixIssue: (issueType: string, entityId: string) =>
    isTauri ? invoke<{ issue: DiagnosisIssue | null; destructive: boolean }>('preview_fix_diagnosis_issue', { issueType, entityId }) : Promise.resolve({ issue: null, destructive: false }),
  executeFixIssue: (issueType: string, entityId: string) =>
    isTauri ? invoke<void>('execute_fix_diagnosis_issue', { issueType, entityId }) : Promise.resolve(),
  executeSafeFixes: () => (isTauri ? invoke<number>('execute_safe_fixes') : Promise.resolve(0)),

  exportSnapshot: () => (isTauri ? invoke<string>('skill_manager_export_snapshot') : Promise.resolve('')),
  openPath: (path: string) => (isTauri ? invoke<void>('open_skill_path', { path }) : Promise.resolve()),
}

function demoOverview(): SkillManagerOverview {
  return {
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
}
