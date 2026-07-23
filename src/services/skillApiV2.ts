import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { isTauri as isTauriRuntime } from './tauriApi'

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
  autoSyncSkillPacks?: boolean
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
  skillId: string
  agentId: string
  targetPath: string
  resolvedTargetPath: string | null
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
  centerResolvedPath?: string | null
  frontmatter: Record<string, string>
  files: FileTreeNode | null
  targets: SkillTargetDetail[]
  source: SkillSourceDetail | null
}

export interface SkillExplanation {
  skillId: string
  lang: string
  model: string
  text: string
  cachedAt: string
  fromCache: boolean
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
  revision?: number
  syncStatus?: 'synced' | 'pending' | 'syncing' | 'failed' | 'partial' | string
  pendingSyncCount?: number
  failedSyncCount?: number
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
  agentId?: string | null
  displayName?: string | null
  iconKey?: string | null
  packRevision?: number
  syncedRevision?: number
  syncStatus?: 'synced' | 'pending' | 'syncing' | 'failed' | 'partial' | string
  syncError?: string | null
}

export interface SkillPackDetail {
  id: string
  name: string
  description: string
  tags: string[]
  members: PackMember[]
  appliedAgents: AppliedPackSummary[]
  revision?: number
  syncStatus?: 'synced' | 'pending' | 'syncing' | 'failed' | 'partial' | string
  pendingSyncCount?: number
  failedSyncCount?: number
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
  agentDir?: string | null
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

export interface SkillPackPickerData {
  agents: AgentSummary[]
  packs: SkillPackSummary[]
  appliedByAgent: Record<string, string[]>
  defaultDistributeMode: 'link' | 'copy'
}

export interface ProjectSummary {
  id: string
  name: string
  rootPath: string
  createdAt: string
  updatedAt: string
  lastScannedAt: string | null
  detectedAgentCount: number
  skillCount: number
  mcpCount: number
  pluginCount: number
  instructionCount: number
  issueCount: number
}

export interface ProjectSkillItem {
  id: string
  name: string
  description: string
  agentId: string
  path: string
  hash: string
  status: 'projectOnly' | 'centerSynced' | 'centerDiff' | string
  importable: boolean
}

export interface ProjectInstructionFile {
  agentId: string
  path: string
  exists: boolean
  bytes: number | null
}

export interface ProjectHealthIssue {
  agentId: string | null
  kind: string
  message: string
  severity: 'info' | 'warning' | 'error' | string
}

export interface ProjectAgentDetail {
  agentId: string
  displayName: string
  iconKey: string
  skillsDirs: string[]
  configPaths: string[]
  mcpConfigPaths: string[]
  pluginConfigPaths: string[]
  skills: ProjectSkillItem[]
  mcpServers: McpServerStatus[]
  plugins: PluginStatus[]
  health: ProjectHealthIssue[]
}

export interface ProjectDetail extends ProjectSummary {
  agents: ProjectAgentDetail[]
  instructions: ProjectInstructionFile[]
  health: ProjectHealthIssue[]
}

export interface ConflictBlocker {
  skillId: string
  agentId: string
  reason: string
  existingPath: string | null
  existingPathKind?: 'symlink' | 'directory' | 'file' | 'broken_symlink' | 'missing' | string | null
  resolvedExistingPath?: string | null
}

export interface DistributionChange {
  skillId: string
  agentId: string
  action: 'create' | 'reuse' | 'appendClaim' | 'skip' | 'blocked' | 'overwrite' | string
  actualMode?: 'link' | 'copy'
  reason?: string
  targetPath: string
}

export interface DistributionBlockerDecision {
  skillId: string
  agentId: string
  action: 'overwrite' | 'agent_over_center' | 'skip'
}

export interface DistributionPreview {
  skillIds: string[]
  targetAgents: string[]
  requestedMode: 'link' | 'copy'
  changes: DistributionChange[]
  blockers: ConflictBlocker[]
  blockerDecisions: DistributionBlockerDecision[]
}

export interface AddCenterSkillInput {
  sourcePath: string
  sourceType: string
  sourceUri?: string | null
  importedFromAgent?: string | null
  importedFromPath?: string | null
  multi?: boolean
  importMode?: 'copy' | 'link'
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
  unchangedCount?: number
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

export interface MarketplaceBatchSkillInput {
  itemId: string
  skillId: string
  sourceUri: string
}

export interface MarketplaceBatchItemResult {
  itemId: string
  skillId: string
  success: boolean
  error: string | null
}

export interface MarketplaceBatchInstallResult {
  items: MarketplaceBatchItemResult[]
  cancelled: boolean
}

export interface MarketplaceBatchProgress {
  jobId: string
  phase: 'cloning' | 'installing' | 'success' | 'failed' | 'completed' | 'cancelled' | string
  itemId: string | null
  completed: number
  total: number
  message: string | null
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
  skillIds?: string[]
  affectedTargets: AffectedTarget[]
  removable: boolean
  warnings: string[]
}

export interface DeleteSkillPackPreview {
  packId: string
  packName: string
  appliedAgents: string[]
  affectedTargets: AffectedTarget[]
  removable: boolean
  warnings: string[]
}

export interface DeleteSkillTargetDistributionFailure {
  targetId: string
  error: string
}

export interface DeleteSkillTargetDistributionsResult {
  deleted: number
  failures: DeleteSkillTargetDistributionFailure[]
}

export interface RemovePackFromAgentPreview {
  packId: string
  packName: string
  agentId: string
  displayName: string
  affectedTargets: AffectedTarget[]
  willRemoveTargets: number
  willPreserveTargets: number
}

export interface RemoveSkillFromPackPreview {
  packId: string
  packName: string
  skillId: string
  skillName: string
  affectedTargets: AffectedTarget[]
  appliedAgentCount: number
  canKeepStandalone: boolean
  canRemoveTargets: boolean
}

export interface MoveDirectSkillToPackPreview {
  targetId: string
  skillId: string
  skillName: string
  agentId: string
  displayName: string
  packId: string
  packName: string
  alreadyMember: boolean
  alreadyApplied: boolean
  willAddToPack: boolean
  otherMemberCount: number
  distribution: DistributionPreview
}

export interface SkillPackSyncAgentResult {
  agentId: string
  displayName: string
  status: 'synced' | 'failed' | string
  error: string | null
}

export interface SkillPackSyncResult {
  packId: string
  packName: string
  revision: number
  status: 'synced' | 'pending' | 'failed' | 'partial' | string
  agents: SkillPackSyncAgentResult[]
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

export interface AgentSkillInventoryItem {
  id: string
  agentId: string
  skillId: string
  name: string
  path: string
  managed: boolean
  canImport: boolean
  status: string
  statusLabel: string
  reason: string | null
  targetId: string | null
  actualMode: string | null
  hash: string | null
}

export interface AgentSkillInventoryAgent {
  agentId: string
  displayName: string
  iconKey: string
  skillsDir: string | null
  installed: boolean
  managedCount: number
  unmanagedCount: number
  importableCount: number
  items: AgentSkillInventoryItem[]
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

export interface CopyTargetDiffFile {
  path: string
  changeType: 'modified' | 'copy_added' | 'copy_removed' | string
  centerContent: string | null
  copyContent: string | null
}

export interface CopyTargetDiffPreview {
  targetId: string
  skillId: string
  targetPath: string
  centerPath: string
  state: 'ok' | 'copy_outdated' | 'copy_modified' | 'copy_diverged' | string
  files: CopyTargetDiffFile[]
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

export interface GitHubRepoPreview {
  repo: {
    owner: string
    repo: string
    branch: string
    normalizedUrl: string
  }
  skills: Array<{
    sourcePath: string
    skillId: string
    skillName: string
    description: string | null
    rootDirectory: string
    skillDirectoryName: string
    downloadUrl: string
    conflict: {
      existingSkillId: string
      existingName: string
      existingCanonicalPath: string | null
      proposedSkillId: string
      proposedName: string
    } | null
  }>
}

export interface GitHubSkillImportSelection {
  sourcePath: string
  resolution: 'overwrite' | 'skip' | 'rename'
  renamedSkillId?: string | null
}

export interface GitHubRepoImportResult {
  repo: GitHubRepoPreview['repo']
  importedSkills: Array<{
    sourcePath: string
    originalSkillId: string
    importedSkillId: string
    skillName: string
    targetDirectory: string
    resolution: string
  }>
  skippedSkills: string[]
}

export interface MarketplaceSkill {
  id: string
  registryId: string
  name: string
  description: string | null
  source: string | null
  installCount: number | null
  downloadUrl: string
  webUrl: string | null
  isInstalled: boolean
  syncedAt: string
  cacheUpdatedAt: string | null
}

export interface MarketplaceSkillDetail {
  description: string | null
  githubUrl: string | null
  installCommand: string | null
  webUrl: string | null
}

export const skillApiV2 = {
  bootstrap: () => (isTauriRuntime() ? invoke<void>('skill_manager_bootstrap') : Promise.resolve()),
  init: () => (isTauriRuntime() ? invoke<void>('skill_manager_init') : Promise.resolve()),
  overview: () =>
    isTauriRuntime()
      ? invoke<SkillManagerOverview>('skill_manager_overview')
      : Promise.resolve(demoOverview()),
  getSkillPackPickerData: (): Promise<SkillPackPickerData> => {
    if (isTauriRuntime()) return invoke<SkillPackPickerData>('skill_pack_picker_data')
    const overview = demoOverview()
    return Promise.resolve({
      agents: overview.agents,
      packs: overview.packs,
      appliedByAgent: {} as Record<string, string[]>,
      defaultDistributeMode: overview.settings.defaultDistributeMode,
    })
  },
  refresh: () => (isTauriRuntime() ? invoke<void>('skill_manager_refresh') : Promise.resolve()),
  refreshOverview: () =>
    isTauriRuntime()
      ? invoke<SkillManagerOverview>('skill_manager_refresh_overview')
      : Promise.resolve(demoOverview()),
  getSettings: () =>
    isTauriRuntime()
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
    isTauriRuntime()
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
    isTauriRuntime() ? invoke<SkillSummary[]>('list_center_skills_v2') : Promise.resolve([]),
  getSkillDetail: (skillId: string) =>
    isTauriRuntime() ? invoke<SkillDetail>('get_skill_detail_v2', { skillId }) : Promise.resolve(null as unknown as SkillDetail),
  readFileTree: (skillPath: string) =>
    isTauriRuntime()
      ? invoke<FileTreeNode>('read_skill_files', { skillPath })
      : Promise.resolve({
          name: skillPath.split(/[\\/]+/).filter(Boolean).pop() || 'skill',
          nodeType: 'dir' as const,
          path: skillPath,
          children: [
            {
              name: 'SKILL.md',
              nodeType: 'file' as const,
              path: `${skillPath}/SKILL.md`,
              children: null,
            },
          ],
        }),
  readFileContent: (filePath: string) =>
    isTauriRuntime() ? invoke<string>('read_skill_file_content', { filePath }) : Promise.resolve(''),
  getSkillExplanation: (skillId: string, lang: string) =>
    isTauriRuntime() ? invoke<SkillExplanation | null>('get_skill_explanation_cmd', { skillId, lang }) : Promise.resolve(null),
  generateSkillExplanation: (skillId: string, skillPath: string, lang: string, refresh = false) =>
    isTauriRuntime()
      ? invoke<SkillExplanation>('generate_skill_explanation_cmd', { skillId, skillPath, lang, refresh })
      : Promise.resolve({
          skillId,
          lang,
          model: 'demo',
          text: '解释生成功能需要在 Tauri 应用中使用。',
          cachedAt: new Date().toISOString(),
          fromCache: false,
        }),

  previewAddCenterSkill: (input: AddCenterSkillInput) =>
    isTauriRuntime()
      ? invoke<AddCenterSkillPreview>('preview_add_center_skill', { input })
      : Promise.resolve({ candidates: [], blockers: [], unchangedCount: 0, centerPath: '' }),
  executeAddCenterSkill: (input: AddCenterSkillInput, decisions: AddCenterSkillDecision[]) =>
    isTauriRuntime()
      ? invoke<AddCenterSkillResult>('execute_add_center_skill', { input, decisions })
      : Promise.resolve({ skillIds: [], updated: [], skipped: [] }),
  executeMarketplaceSkillBatch: (jobId: string, repoSource: string, skills: MarketplaceBatchSkillInput[]) =>
    isTauriRuntime()
      ? invoke<MarketplaceBatchInstallResult>('execute_marketplace_skill_batch', { jobId, repoSource, skills })
      : Promise.resolve({
          items: skills.map((skill) => ({ itemId: skill.itemId, skillId: skill.skillId, success: true, error: null })),
          cancelled: false,
        }),
  cancelMarketplaceSkillBatch: (jobId: string) =>
    isTauriRuntime()
      ? invoke<boolean>('cancel_marketplace_skill_batch', { jobId })
      : Promise.resolve(true),
  onMarketplaceBatchProgress: (handler: (progress: MarketplaceBatchProgress) => void): Promise<UnlistenFn> =>
    isTauriRuntime()
      ? listen<MarketplaceBatchProgress>('marketplace-skill-batch-progress', (event) => handler(event.payload))
      : Promise.resolve(() => {}),
  previewGitHubRepoImport: (repoUrl: string, githubToken?: string) =>
    isTauriRuntime()
      ? invoke<GitHubRepoPreview>('preview_github_repo_import', {
          repoUrl,
          githubToken: githubToken?.trim() || null,
        })
      : Promise.resolve({ repo: { owner: '', repo: '', branch: 'HEAD', normalizedUrl: repoUrl }, skills: [] }),
  importGitHubRepoSkills: (repoUrl: string, selections: GitHubSkillImportSelection[], githubToken?: string) =>
    isTauriRuntime()
      ? invoke<GitHubRepoImportResult>('import_github_repo_skills', {
          repoUrl,
          selections,
          githubToken: githubToken?.trim() || null,
        })
      : Promise.resolve({
          repo: { owner: '', repo: '', branch: 'HEAD', normalizedUrl: repoUrl },
          importedSkills: [],
          skippedSkills: selections.filter(item => item.resolution === 'skip').map(item => item.sourcePath),
        }),

  previewDeleteCenterSkill: (skillId: string) =>
    isTauriRuntime()
      ? invoke<DeleteCenterSkillPreview>('preview_delete_center_skill', { skillId })
      : Promise.resolve({ skillId, skillIds: [skillId], affectedTargets: [], removable: true, warnings: [] }),
  executeDeleteCenterSkill: (skillId: string, removeLinked: boolean) =>
    isTauriRuntime()
      ? invoke<void>('execute_delete_center_skill', { skillId, removeLinked })
      : Promise.resolve(),
  previewDeleteCenterSkills: (skillIds: string[]) =>
    isTauriRuntime()
      ? invoke<DeleteCenterSkillPreview>('preview_delete_center_skills', { skillIds })
      : Promise.resolve({ skillId: skillIds[0] ?? '', skillIds, affectedTargets: [], removable: true, warnings: [] }),
  executeDeleteCenterSkills: (skillIds: string[], removeLinked: boolean) =>
    isTauriRuntime()
      ? invoke<void>('execute_delete_center_skills', { skillIds, removeLinked })
      : Promise.resolve(),

  previewDistribute: (skillIds: string[], targetAgents: string[], requestedMode: 'link' | 'copy') =>
    isTauriRuntime()
      ? invoke<DistributionPreview>('preview_distribute_skill', { skillIds, targetAgents, requestedMode })
      : Promise.resolve({ skillIds, targetAgents, requestedMode, changes: [], blockers: [], blockerDecisions: [] }),
  executeDistribute: (preview: DistributionPreview) =>
    isTauriRuntime() ? invoke<DistributionPreview>('execute_distribute_skill', { preview }) : Promise.resolve(preview),

  scanAgentInventory: (agentId: string) =>
    isTauriRuntime() ? invoke<{ agentId: string; managed: number; unmanaged: number }>('scan_agent_inventory', { agentId }) : Promise.resolve({ agentId, managed: 0, unmanaged: 0 }),

  previewAdopt: (agentId: string, unmanagedId: string) =>
    isTauriRuntime() ? invoke<AdoptPreview>('preview_adopt_agent_skill', { agentId, unmanagedId }) : Promise.resolve(null as unknown as AdoptPreview),
  executeAdopt: (agentId: string, unmanagedId: string, option: string, renamedId?: string | null) =>
    isTauriRuntime() ? invoke<string>('execute_adopt_agent_skill', { agentId, unmanagedId, option, renamedId: renamedId ?? null }) : Promise.resolve(''),
  deleteUnmanagedAgentSkill: (agentId: string, unmanagedId: string) =>
    isTauriRuntime() ? invoke<void>('delete_unmanaged_agent_skill', { agentId, unmanagedId }) : Promise.resolve(),

  previewSyncCopy: (targetId: string) =>
    isTauriRuntime() ? invoke<CopySyncPreview>('preview_sync_copy_target', { targetId }) : Promise.resolve(null as unknown as CopySyncPreview),
  previewCopyTargetDiff: (targetId: string) =>
    isTauriRuntime() ? invoke<CopyTargetDiffPreview>('preview_copy_target_diff', { targetId }) : Promise.resolve(null as unknown as CopyTargetDiffPreview),
  executeSyncCopy: (targetId: string, action: string) =>
    isTauriRuntime() ? invoke<CopySyncPreview>('execute_sync_copy_target', { targetId, action }) : Promise.resolve(null as unknown as CopySyncPreview),
  deleteSkillTargetDistribution: (targetId: string) =>
    isTauriRuntime() ? invoke<void>('delete_skill_target_distribution', { targetId }) : Promise.resolve(),
  deleteSkillTargetDistributions: (targetIds: string[]) =>
    isTauriRuntime()
      ? invoke<DeleteSkillTargetDistributionsResult>('delete_skill_target_distributions', { targetIds })
      : Promise.resolve({ deleted: targetIds.length, failures: [] }),

  listPacks: () => (isTauriRuntime() ? invoke<SkillPackSummary[]>('list_skill_packs_v2') : Promise.resolve([])),
  getPackDetail: (packId: string) =>
    isTauriRuntime() ? invoke<SkillPackDetail>('get_skill_pack_detail', { packId }) : Promise.resolve(null as unknown as SkillPackDetail),
  upsertPack: (pack: UpsertPackInput, options: { deferSync?: boolean } = {}) =>
    isTauriRuntime()
      ? invoke<SkillPackDetail>('execute_upsert_skill_pack', { pack, deferSync: options.deferSync ?? false })
      : Promise.resolve(null as unknown as SkillPackDetail),
  previewDeletePack: (packId: string) =>
    isTauriRuntime() ? invoke<DeleteSkillPackPreview>('preview_delete_skill_pack', { packId }) : Promise.resolve({ packId, packName: packId, appliedAgents: [], affectedTargets: [], removable: true, warnings: [] }),
  deletePack: (packId: string) =>
    isTauriRuntime() ? invoke<void>('execute_delete_skill_pack', { packId }) : Promise.resolve(),
  previewApplyPack: (packId: string, targetAgents: string[], requestedMode: 'link' | 'copy') =>
    isTauriRuntime() ? invoke<DistributionPreview>('preview_apply_skill_pack', { packId, targetAgents, requestedMode }) : Promise.resolve({ skillIds: [], targetAgents, requestedMode, changes: [], blockers: [], blockerDecisions: [] }),
  executeApplyPack: (packId: string, targetAgents: string[], requestedMode: 'link' | 'copy', blockerDecisions: DistributionBlockerDecision[] = []) =>
    isTauriRuntime() ? invoke<DistributionPreview>('execute_apply_skill_pack', { packId, targetAgents, requestedMode, blockerDecisions }) : Promise.resolve({ skillIds: [], targetAgents, requestedMode, changes: [], blockers: [], blockerDecisions }),
  syncPackToAgents: (packId: string, targetAgents: string[] = []) =>
    isTauriRuntime() ? invoke<SkillPackSyncResult>('execute_sync_skill_pack_to_agents', { packId, targetAgents }) : Promise.resolve({ packId, packName: packId, revision: 1, status: 'synced', agents: [] }),
  previewRemovePackFromAgent: (packId: string, agentId: string) =>
    isTauriRuntime() ? invoke<RemovePackFromAgentPreview>('preview_remove_skill_pack_from_agent', { packId, agentId }) : Promise.resolve({ packId, packName: packId, agentId, displayName: agentId, affectedTargets: [], willRemoveTargets: 0, willPreserveTargets: 0 }),
  removePackFromAgent: (packId: string, agentId: string) =>
    isTauriRuntime() ? invoke<RevokeResult>('execute_remove_skill_pack_from_agent', { packId, agentId }) : Promise.resolve({ packId, agentId, removedClaims: 0, removedTargets: 0, preservedTargets: 0 }),
  previewRemoveSkillFromPack: (packId: string, skillId: string) =>
    isTauriRuntime() ? invoke<RemoveSkillFromPackPreview>('preview_remove_skill_from_pack', { packId, skillId }) : Promise.resolve({ packId, packName: packId, skillId, skillName: skillId, affectedTargets: [], appliedAgentCount: 0, canKeepStandalone: true, canRemoveTargets: true }),
  removeSkillFromPack: (packId: string, skillId: string, alsoRemoveTargets: boolean) =>
    isTauriRuntime() ? invoke<void>('execute_remove_skill_from_pack', { packId, skillId, alsoRemoveTargets }) : Promise.resolve(),
  previewMoveDirectSkillToPack: (targetId: string, packId: string) =>
    isTauriRuntime()
      ? invoke<MoveDirectSkillToPackPreview>('preview_move_direct_skill_to_pack', { targetId, packId })
      : Promise.resolve(null as unknown as MoveDirectSkillToPackPreview),
  moveDirectSkillToPack: (targetId: string, packId: string, blockerDecisions: DistributionBlockerDecision[] = []) =>
    isTauriRuntime()
      ? invoke<MoveDirectSkillToPackPreview>('execute_move_direct_skill_to_pack', { targetId, packId, blockerDecisions })
      : Promise.resolve(null as unknown as MoveDirectSkillToPackPreview),

  listAgents: () => (isTauriRuntime() ? invoke<AgentSummary[]>('list_managed_agents_v2') : Promise.resolve([])),
  getAgentDetail: (agentId: string) =>
    isTauriRuntime() ? invoke<AgentDetail>('get_agent_detail_v2', { agentId }) : Promise.resolve(null as unknown as AgentDetail),
  listUnmanaged: () => (isTauriRuntime() ? invoke<UnmanagedItemDto[]>('list_unmanaged_v2') : Promise.resolve([])),
  listAgentSkillInventory: () =>
    isTauriRuntime() ? invoke<AgentSkillInventoryAgent[]>('list_agent_skill_inventory_v2') : Promise.resolve(demoAgentInventory()),
  listProjects: () =>
    isTauriRuntime() ? invoke<ProjectSummary[]>('list_skill_projects_v2') : Promise.resolve(demoProjectSummaries()),
  addProject: (rootPath: string) =>
    isTauriRuntime() ? invoke<ProjectDetail>('add_skill_project_v2', { rootPath }) : Promise.resolve(demoProjectDetail(rootPath)),
  removeProject: (projectId: string) =>
    isTauriRuntime() ? invoke<void>('remove_skill_project_v2', { projectId }) : Promise.resolve(),
  getProjectDetail: (projectId: string) =>
    isTauriRuntime() ? invoke<ProjectDetail>('get_skill_project_detail_v2', { projectId }) : Promise.resolve(demoProjectDetail(projectId)),
  scanProject: (projectId: string) =>
    isTauriRuntime() ? invoke<ProjectDetail>('scan_skill_project_v2', { projectId }) : Promise.resolve(demoProjectDetail(projectId)),
  installCenterSkillsToProject: (projectId: string, agentId: string, skillIds: string[], requestedMode: 'link' | 'copy') =>
    isTauriRuntime()
      ? invoke<ProjectDetail>('install_center_skills_to_project_v2', { projectId, agentId, skillIds, requestedMode })
      : Promise.resolve(demoProjectDetail(projectId)),
  installSkillPackToProject: (projectId: string, agentId: string, packId: string, requestedMode: 'link' | 'copy') =>
    isTauriRuntime()
      ? invoke<ProjectDetail>('install_skill_pack_to_project_v2', { projectId, agentId, packId, requestedMode })
      : Promise.resolve(demoProjectDetail(projectId)),

  runDiagnosis: () => (isTauriRuntime() ? invoke<DiagnosisIssue[]>('run_skill_manager_diagnosis') : Promise.resolve([])),
  listDiagnosisIssues: () => (isTauriRuntime() ? invoke<DiagnosisIssue[]>('list_diagnosis_issues') : Promise.resolve([])),
  previewFixIssue: (issueType: string, entityId: string) =>
    isTauriRuntime() ? invoke<{ issue: DiagnosisIssue | null; destructive: boolean }>('preview_fix_diagnosis_issue', { issueType, entityId }) : Promise.resolve({ issue: null, destructive: false }),
  executeFixIssue: (issueType: string, entityId: string) =>
    isTauriRuntime() ? invoke<void>('execute_fix_diagnosis_issue', { issueType, entityId }) : Promise.resolve(),
  executeSafeFixes: () => (isTauriRuntime() ? invoke<number>('execute_safe_fixes') : Promise.resolve(0)),

  exportSnapshot: () => (isTauriRuntime() ? invoke<string>('skill_manager_export_snapshot') : Promise.resolve('')),
  openPath: (path: string) => (isTauriRuntime() ? invoke<void>('open_skill_path', { path }) : Promise.resolve()),
  revealPath: (path: string) => (isTauriRuntime() ? invoke<void>('reveal_skill_path', { path }) : Promise.resolve()),
  searchMarketplaceSkills: (registryId?: string | null, query?: string | null, board?: string | null) =>
    isTauriRuntime()
      ? invoke<MarketplaceSkill[]>('search_marketplace_skills', { registryId: registryId ?? null, query: query ?? null, board: board ?? null })
      : fetchSkillsShMarketplace(registryId, query, board),
  fetchMarketplaceSkillDetail: (source: string, skillId: string) =>
    isTauriRuntime()
      ? invoke<MarketplaceSkillDetail>('fetch_marketplace_skill_detail', { source, skillId })
      : Promise.resolve({ description: null, githubUrl: null, installCommand: null, webUrl: `https://skills.sh/${source}/${skillId}` }),
}

interface SkillsShSearchSkill {
  id?: string
  skillId?: string
  skill_id?: string
  name?: string
  source?: string
  installs?: number
}

interface SkillsShSearchResponse {
  skills?: SkillsShSearchSkill[]
}

async function fetchSkillsShMarketplace(
  registryId?: string | null,
  query?: string | null,
  board?: string | null,
): Promise<MarketplaceSkill[]> {
  const wantsSkillsSh = !registryId || ['skills-sh', 'skills.sh', 'skillssh'].includes(registryId)
  if (!wantsSkillsSh || typeof fetch !== 'function') return []

  try {
    const url = new URL('https://skills.sh/api/search')
    const queryText = query?.trim() || (board === 'hot' ? 'popular' : board === 'trending' ? 'trending' : 'skill')
    url.searchParams.set('q', queryText)
    url.searchParams.set('limit', '200')
    const response = await fetch(url.toString())
    if (!response.ok) return fallbackSkillsShMarketplace(query, board)

    const value = (await response.json()) as SkillsShSearchResponse | SkillsShSearchSkill[]
    const skills = Array.isArray(value) ? value : value.skills ?? []
    const now = new Date().toISOString()
    const mapped = skills
      .map((skill) => toMarketplaceSkill(skill, now))
      .filter((skill): skill is MarketplaceSkill => Boolean(skill))
    return mapped.length > 0 ? mapped : fallbackSkillsShMarketplace(query, board)
  } catch {
    return fallbackSkillsShMarketplace(query, board)
  }
}

function toMarketplaceSkill(skill: SkillsShSearchSkill, syncedAt: string): MarketplaceSkill | null {
  const source = skill.source?.trim()
  const skillId = (skill.skillId ?? skill.skill_id ?? '').trim()
  if (!source || !skillId) return null

  const id = skill.id?.trim()
    ? `skillssh:${skill.id.trim().replace(/\//g, '@')}`
    : `skillssh:${source.replace(/\//g, '@')}@${skillId.replace(/\//g, '@')}`
  const installs = typeof skill.installs === 'number' ? skill.installs : 0
  return {
    id,
    registryId: 'skills-sh',
    name: skill.name?.trim() || skillId,
    description: installs > 0 ? `skills.sh · ${installs} installs · ${source}` : `skills.sh · ${source}`,
    source,
    installCount: installs > 0 ? installs : null,
    downloadUrl: `skillssh:${source}/${skillId}`,
    webUrl: `https://skills.sh/${source}/${skillId}`,
    isInstalled: false,
    syncedAt,
    cacheUpdatedAt: syncedAt,
  }
}

function fallbackSkillsShMarketplace(query?: string | null, board?: string | null): MarketplaceSkill[] {
  const now = new Date().toISOString()
  const rows: SkillsShSearchSkill[] = [
    { id: 'vercel-labs/skills/find-skills', skillId: 'find-skills', name: 'find-skills', source: 'vercel-labs/skills', installs: 2_006_831 },
    { id: 'anthropics/skills/frontend-design', skillId: 'frontend-design', name: 'frontend-design', source: 'anthropics/skills', installs: 541_500 },
    { id: 'vercel-labs/agent-skills/vercel-react-best-practices', skillId: 'vercel-react-best-practices', name: 'vercel-react-best-practices', source: 'vercel-labs/agent-skills', installs: 474_400 },
    { id: 'vercel-labs/agent-browser/agent-browser', skillId: 'agent-browser', name: 'agent-browser', source: 'vercel-labs/agent-browser', installs: 447_200 },
    { id: 'microsoft/azure-skills/microsoft-foundry', skillId: 'microsoft-foundry', name: 'microsoft-foundry', source: 'microsoft/azure-skills', installs: 389_800 },
    { id: 'vercel-labs/agent-skills/web-design-guidelines', skillId: 'web-design-guidelines', name: 'web-design-guidelines', source: 'vercel-labs/agent-skills', installs: 388_900 },
    { id: 'microsoft/azure-skills/azure-ai', skillId: 'azure-ai', name: 'azure-ai', source: 'microsoft/azure-skills', installs: 387_400 },
    { id: 'microsoft/azure-skills/azure-deploy', skillId: 'azure-deploy', name: 'azure-deploy', source: 'microsoft/azure-skills', installs: 387_000 },
    { id: 'microsoft/azure-skills/azure-diagnostics', skillId: 'azure-diagnostics', name: 'azure-diagnostics', source: 'microsoft/azure-skills', installs: 386_900 },
  ]
  const q = query?.trim().toLowerCase()
  const filtered = q
    ? rows.filter((row) => [row.name, row.source, row.skillId].filter(Boolean).join(' ').toLowerCase().includes(q))
    : rows
  const ordered = board === 'trending'
    ? [...filtered].sort((a, b) => (b.name || '').localeCompare(a.name || ''))
    : board === 'hot'
      ? [...filtered].sort((a, b) => (b.installs || 0) - (a.installs || 0)).slice(1)
      : filtered
  return ordered
    .map((skill) => toMarketplaceSkill(skill, now))
    .filter((skill): skill is MarketplaceSkill => Boolean(skill))
}

function demoAgentInventory(): AgentSkillInventoryAgent[] {
  return [
    {
      agentId: 'claude-code',
      displayName: 'Claude Code',
      iconKey: 'claude-code',
      skillsDir: '~/.claude/skills',
      installed: true,
      managedCount: 1,
      unmanagedCount: 2,
      importableCount: 1,
      items: [
        {
          id: 'demo-managed-release',
          agentId: 'claude-code',
          skillId: 'release-checklist',
          name: 'release-checklist',
          path: '~/.claude/skills/release-checklist',
          managed: true,
          canImport: false,
          status: 'ok',
          statusLabel: '已管理',
          reason: null,
          targetId: 'demo-target-1',
          actualMode: 'link',
          hash: null,
        },
        {
          id: 'demo-unmanaged-article',
          agentId: 'claude-code',
          skillId: 'article-writer',
          name: 'article-writer',
          path: '~/.claude/skills/article-writer',
          managed: false,
          canImport: true,
          status: 'unmanaged',
          statusLabel: '未管理',
          reason: 'not_in_center_library',
          targetId: null,
          actualMode: null,
          hash: 'demo-hash',
        },
        {
          id: 'demo-conflict',
          agentId: 'claude-code',
          skillId: 'frontend-design',
          name: 'frontend-design',
          path: '~/.claude/skills/frontend-design',
          managed: false,
          canImport: false,
          status: 'conflict',
          statusLabel: '未管理 · 同名冲突',
          reason: 'center_library_conflict',
          targetId: null,
          actualMode: null,
          hash: 'demo-conflict-hash',
        },
      ],
    },
    {
      agentId: 'codex',
      displayName: 'Codex',
      iconKey: 'codex',
      skillsDir: '~/.codex/skills',
      installed: true,
      managedCount: 0,
      unmanagedCount: 1,
      importableCount: 1,
      items: [
        {
          id: 'demo-codex-browser',
          agentId: 'codex',
          skillId: 'browser-control',
          name: 'browser-control',
          path: '~/.codex/skills/browser-control',
          managed: false,
          canImport: true,
          status: 'unmanaged',
          statusLabel: '未管理',
          reason: 'not_in_center_library',
          targetId: null,
          actualMode: null,
          hash: 'demo-browser-hash',
        },
      ],
    },
  ]
}

function demoProjectSummaries(): ProjectSummary[] {
  const detail = demoProjectDetail('/Users/me/project')
  return [{
    id: detail.id,
    name: detail.name,
    rootPath: detail.rootPath,
    createdAt: detail.createdAt,
    updatedAt: detail.updatedAt,
    lastScannedAt: detail.lastScannedAt,
    detectedAgentCount: detail.detectedAgentCount,
    skillCount: detail.skillCount,
    mcpCount: detail.mcpCount,
    pluginCount: detail.pluginCount,
    instructionCount: detail.instructionCount,
    issueCount: detail.issueCount,
  }]
}

function demoProjectDetail(rootPath: string): ProjectDetail {
  const now = new Date().toISOString()
  return {
    id: 'project-demo',
    name: rootPath.split(/[\\/]+/).filter(Boolean).pop() || 'project',
    rootPath,
    createdAt: now,
    updatedAt: now,
    lastScannedAt: now,
    detectedAgentCount: 2,
    skillCount: 3,
    mcpCount: 2,
    pluginCount: 1,
    instructionCount: 2,
    issueCount: 1,
    agents: [
      {
        agentId: 'codex',
        displayName: 'Codex',
        iconKey: 'codex',
        skillsDirs: [`${rootPath}/.agents/skills`],
        configPaths: [`${rootPath}/.codex/config.toml`],
        mcpConfigPaths: [`${rootPath}/.codex/config.toml`],
        pluginConfigPaths: [`${rootPath}/.codex/config.toml`],
        skills: [
          {
            id: 'release-review',
            name: 'release-review',
            description: 'Project-only release review workflow',
            agentId: 'codex',
            path: `${rootPath}/.agents/skills/release-review`,
            hash: 'demo-project-hash',
            status: 'projectOnly',
            importable: true,
          },
          {
            id: 'frontend-design',
            name: 'frontend-design',
            description: 'Design workflow synced with center library',
            agentId: 'codex',
            path: `${rootPath}/.agents/skills/frontend-design`,
            hash: 'demo-center-hash',
            status: 'centerSynced',
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
      {
        agentId: 'claude-code',
        displayName: 'Claude Code',
        iconKey: 'claude-code',
        skillsDirs: [`${rootPath}/.claude/skills`],
        configPaths: [`${rootPath}/.claude/settings.json`],
        mcpConfigPaths: [`${rootPath}/.mcp.json`],
        pluginConfigPaths: [`${rootPath}/.claude/settings.json`],
        skills: [
          {
            id: 'docs-editor',
            name: 'docs-editor',
            description: 'Project documentation workflow differs from center',
            agentId: 'claude-code',
            path: `${rootPath}/.claude/skills/docs-editor`,
            hash: 'demo-diff-hash',
            status: 'centerDiff',
            importable: true,
          },
        ],
        mcpServers: [
          { name: 'filesystem', command: 'npx', args: ['-y', '@modelcontextprotocol/server-filesystem'], valid: true, message: 'configured' },
        ],
        plugins: [],
        health: [{ agentId: 'claude-code', kind: 'center_diff', message: 'docs-editor differs from center library', severity: 'warning' }],
      },
    ],
    instructions: [
      { agentId: 'codex', path: `${rootPath}/AGENTS.md`, exists: true, bytes: 2400 },
      { agentId: 'claude-code', path: `${rootPath}/.claude/CLAUDE.md`, exists: true, bytes: 3200 },
    ],
    health: [],
  }
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
      autoSyncSkillPacks: true,
    },
  }
}
