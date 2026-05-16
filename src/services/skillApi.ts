import { invoke } from '@tauri-apps/api/core'

export interface AgentSkillState {
  agent: string
  installPath: string
  linkTarget?: string | null
  installMode: 'direct' | 'symlink'
  enabled: boolean
}

export interface ScannedSkill {
  id: string
  name: string
  description: string
  skillType: 'skill' | 'plugin' | 'mcp'
  icon: string | null
  source: 'island' | 'local'
  originUrl: string | null
  hasUpdate: boolean
  filePath: string
  fileSize: number
  modifiedAt: number
  agents: AgentSkillState[]
  frontmatter: Record<string, string>
}

export interface CentralSkillBundle {
  name: string
  path: string
  skillCount: number
  linkedAgentCount: number
  skillIds: string[]
}

export interface CentralDeletePreview {
  path: string
  skillIds: string[]
  linkedInstallPaths: string[]
  removablePaths: string[]
  warnings: string[]
}

export interface DiscoveredSkill {
  id: string
  name: string
  description: string
  filePath: string
  dirPath: string
  projectPath: string
  projectName: string
  sourceKind: string
}

export interface SkillExplanation {
  skillId: string
  lang: string
  model: string
  text: string
  cachedAt: string
  fromCache: boolean
}

export interface GitHubSkillPreview {
  sourcePath: string
  name: string
  description: string
  directoryName: string
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

export interface SkillPack {
  id: string
  name: string
  description: string
  skills: string[]
  targetAgents: string[]
}

export interface SkillCollection {
  id: string
  name: string
  description: string
  skills: string[]
  createdAt: string
  updatedAt: string
}

export interface ScanRoot {
  path: string
  enabled: boolean
  label: string
}

export interface ObsidianVault {
  id: string
  name: string
  path: string
  skillCount: number
}

export interface SyncConfig {
  method: string
  githubRepo: string | null
  githubToken: string | null
  lastSyncAt: string | null
  autoSync: boolean
}

export interface FileTreeNode {
  name: string
  nodeType: 'file' | 'dir'
  path: string
  children: FileTreeNode[] | null
}

export interface SyncResult {
  success: boolean
  message: string
  conflicts: { skillId: string; localModified: string; remoteModified: string }[]
}

export interface ConflictResolution {
  skillId: string
  action: 'keep_local' | 'use_remote' | 'keep_both'
}

export interface SyncPreview {
  toCopy: number
  toSkip: number
  toUpdate: number
  details: string[]
}

export interface TargetConfig {
  agent: string
  installMode: 'direct' | 'symlink'
}

export interface McpServerConfig {
  name: string
  command: string
  args: string[]
  env: Record<string, string>
}

export interface McpValidationResult {
  valid: boolean
  message: string
  warnings: string[]
}

export interface PluginInstallRequest {
  source: string
  agent: string
}

export interface MarketplaceSource {
  id: string
  name: string
  url: string
  enabled: boolean
}

export interface SkillRegistry {
  id: string
  name: string
  sourceType: string
  url: string
  isBuiltin: boolean
  isEnabled: boolean
  lastSynced: string | null
  lastAttemptedSync: string | null
  lastSyncStatus: string
  lastSyncError: string | null
  cacheUpdatedAt: string | null
  cacheExpiresAt: string | null
  etag: string | null
  lastModified: string | null
  createdAt: string
}

export interface RegistrySyncOptions {
  forceRefresh: boolean
}

export interface MarketplaceSkill {
  id: string
  registryId: string
  name: string
  description: string | null
  downloadUrl: string
  isInstalled: boolean
  syncedAt: string
  cacheUpdatedAt: string | null
}

export interface MarketplaceItem {
  id: string
  name: string
  description: string
  category: 'skill' | 'plugin' | 'mcp'
  sourceType: 'github' | 'url'
  source: string
  subPath?: string | null
  author: string
  accent: string
  mcp?: {
    command: string
    args: string[]
    env: Record<string, string>
  } | null
  plugin?: {
    agents: string[]
  } | null
}

const isTauri = '__TAURI_INTERNALS__' in window

export const skillApi = {
  scanAll: () => isTauri
    ? invoke<Record<string, ScannedSkill[]>>('scan_all_skills')
    : Promise.resolve({}),

  scanAgent: (agent: string) => isTauri
    ? invoke<ScannedSkill[]>('scan_agent_skills', { agent })
    : Promise.resolve([]),

  getCentralSkillBundles: () => isTauri
    ? invoke<CentralSkillBundle[]>('get_central_skill_bundles')
    : Promise.resolve([]),

  getCentralSkillBundleDetail: (bundleName: string) => isTauri
    ? invoke<ScannedSkill[]>('get_central_skill_bundle_detail', { bundleName })
    : Promise.resolve([]),

  previewDeleteCentralSkillBundle: (bundleName: string) => isTauri
    ? invoke<CentralDeletePreview>('preview_delete_central_skill_bundle', { bundleName })
    : Promise.resolve({ path: '', skillIds: [], linkedInstallPaths: [], removablePaths: [], warnings: [] }),

  deleteCentralSkillBundle: (bundleName: string, removeLinked = false) => isTauri
    ? invoke('delete_central_skill_bundle', { bundleName, removeLinked })
    : Promise.resolve(),

  previewDeleteCentralSkill: (skillPath: string) => isTauri
    ? invoke<CentralDeletePreview>('preview_delete_central_skill', { skillPath })
    : Promise.resolve({ path: skillPath, skillIds: [], linkedInstallPaths: [], removablePaths: [skillPath], warnings: [] }),

  deleteCentralSkill: (skillPath: string, removeLinked = false) => isTauri
    ? invoke('delete_central_skill', { skillPath, removeLinked })
    : Promise.resolve(),

  discoverProjectSkills: (roots: string[]) => isTauri
    ? invoke<DiscoveredSkill[]>('discover_project_skills_cmd', { roots })
    : Promise.resolve([]),

  discoverEnabledProjectSkills: () => isTauri
    ? invoke<DiscoveredSkill[]>('discover_enabled_project_skills_cmd')
    : Promise.resolve([]),

  getDiscoveredSkills: () => isTauri
    ? invoke<DiscoveredSkill[]>('get_discovered_skills_cmd')
    : Promise.resolve([]),

  clearDiscoveredSkills: () => isTauri
    ? invoke('clear_discovered_skills_cmd')
    : Promise.resolve(),

  stopProjectScan: () => isTauri
    ? invoke('stop_project_scan')
    : Promise.resolve(),

  getScanRoots: () => isTauri
    ? invoke<ScanRoot[]>('get_scan_roots_cmd')
    : Promise.resolve([
      { path: '~/code', enabled: true, label: 'code' },
      { path: '~/projects', enabled: true, label: 'projects' },
      { path: '~/workspace', enabled: true, label: 'workspace' },
    ]),

  setScanRoots: (roots: ScanRoot[]) => isTauri
    ? invoke('set_scan_roots_cmd', { roots })
    : Promise.resolve(),

  setScanRootEnabled: (path: string, enabled: boolean) => isTauri
    ? invoke('set_scan_root_enabled_cmd', { path, enabled })
    : Promise.resolve(),

  getObsidianVaults: () => isTauri
    ? invoke<ObsidianVault[]>('get_obsidian_vaults_cmd')
    : Promise.resolve([]),

  getObsidianVaultSkills: (vaultPath: string) => isTauri
    ? invoke<DiscoveredSkill[]>('get_obsidian_vault_skills_cmd', { vaultPath })
    : Promise.resolve([]),

  install: (source: string, targets: TargetConfig[], mode: 'direct' | 'symlink') => isTauri
    ? invoke('install_skill_cmd', { source, targets, mode })
    : Promise.resolve(),

  batchImportDiscoveredSkills: (skillsToImport: DiscoveredSkill[], targetAgents: string[], mode: 'direct' | 'symlink') => isTauri
    ? invoke<string[]>('batch_import_discovered_skills_cmd', { skillsToImport, targetAgents, mode })
    : Promise.resolve(skillsToImport.map(skill => skill.name)),

  previewGitHubSkills: (source: string) => isTauri
    ? invoke<GitHubSkillPreview[]>('preview_github_skills_cmd', { source })
    : Promise.resolve([]),

  previewGitHubRepoImport: (repoUrl: string) => isTauri
    ? invoke<GitHubRepoPreview>('preview_github_repo_import', { repoUrl })
    : Promise.resolve({ repo: { owner: '', repo: '', branch: 'HEAD', normalizedUrl: repoUrl }, skills: [] }),

  importGitHubRepoSkills: (repoUrl: string, selections: GitHubSkillImportSelection[]) => isTauri
    ? invoke<GitHubRepoImportResult>('import_github_repo_skills', { repoUrl, selections })
    : Promise.resolve({ repo: { owner: '', repo: '', branch: 'HEAD', normalizedUrl: repoUrl }, importedSkills: [], skippedSkills: selections.filter(item => item.resolution === 'skip').map(item => item.sourcePath) }),

  installPlugin: (request: PluginInstallRequest) => isTauri
    ? invoke<string>('install_plugin_cmd', { request })
    : Promise.resolve(''),

  uninstall: (skillPath: string) => isTauri
    ? invoke('uninstall_skill_cmd', { skillPath })
    : Promise.resolve(),

  upsertMcpServer: (agent: string, server: McpServerConfig) => isTauri
    ? invoke('upsert_mcp_server_cmd', { agent, server })
    : Promise.resolve(),

  removeMcpServer: (agent: string, serverName: string) => isTauri
    ? invoke('remove_mcp_server_cmd', { agent, serverName })
    : Promise.resolve(),

  validateMcpServer: (agent: string, serverName: string) => isTauri
    ? invoke<McpValidationResult>('validate_mcp_server_cmd', { agent, serverName })
    : Promise.resolve({ valid: true, message: '', warnings: [] }),

  toggle: (skillId: string, agent: string, enabled: boolean) => isTauri
    ? invoke('toggle_skill_cmd', { skillId, agent, enabled })
    : Promise.resolve(),

  readFileTree: (skillPath: string) => isTauri
    ? invoke<FileTreeNode>('read_skill_files', { skillPath })
    : Promise.resolve({ name: '', nodeType: 'dir' as const, path: '', children: [] }),

  readFileContent: (filePath: string) => isTauri
    ? invoke<string>('read_skill_file_content', { filePath })
    : Promise.resolve(''),

  getSkillExplanation: (skillId: string, lang: string) => isTauri
    ? invoke<SkillExplanation | null>('get_skill_explanation_cmd', { skillId, lang })
    : Promise.resolve(null),

  generateSkillExplanation: (skillId: string, skillPath: string, lang: string, refresh = false) => isTauri
    ? invoke<SkillExplanation>('generate_skill_explanation_cmd', { skillId, skillPath, lang, refresh })
    : Promise.resolve({
      skillId,
      lang,
      model: 'browser-demo',
      text: 'AI explanation requires the Tauri desktop runtime.',
      cachedAt: new Date().toISOString(),
      fromCache: false,
    }),

  openPath: (path: string) => isTauri
    ? invoke('open_system_path', { path })
    : Promise.resolve(),

  listPacks: () => isTauri
    ? invoke<SkillPack[]>('list_packs_cmd')
    : Promise.resolve([]),

  createPack: (pack: SkillPack) => isTauri
    ? invoke('create_pack_cmd', { pack })
    : Promise.resolve(),

  updatePack: (pack: SkillPack) => isTauri
    ? invoke('update_pack_cmd', { pack })
    : Promise.resolve(),

  deletePack: (id: string) => isTauri
    ? invoke('delete_pack_cmd', { id })
    : Promise.resolve(),

  listCollections: () => isTauri
    ? invoke<SkillCollection[]>('list_collections_cmd')
    : Promise.resolve([]),

  upsertCollection: (collection: SkillCollection) => isTauri
    ? invoke<SkillCollection>('upsert_collection_cmd', { collection })
    : Promise.resolve(collection),

  deleteCollection: (id: string) => isTauri
    ? invoke('delete_collection_cmd', { id })
    : Promise.resolve(),

  exportCollection: (id: string) => isTauri
    ? invoke<string>('export_collection_cmd', { id })
    : Promise.resolve(''),

  importCollection: (json: string) => isTauri
    ? invoke<SkillCollection>('import_collection_cmd', { json })
    : Promise.resolve(JSON.parse(json).collection ?? JSON.parse(json)),

  batchInstallCollection: (collection: SkillCollection, targetAgents: string[]) => isTauri
    ? invoke('batch_install_collection_cmd', { collection, targetAgents })
    : Promise.resolve(),

  applyPack: (pack: SkillPack) => isTauri
    ? invoke('apply_pack_cmd', { pack })
    : Promise.resolve(),

  configureSyncConfig: (config: SyncConfig) => isTauri
    ? invoke('configure_sync_cmd', { config })
    : Promise.resolve(),

  pushSync: () => isTauri
    ? invoke<SyncResult>('push_sync_cmd')
    : Promise.resolve({ success: true, message: '', conflicts: [] }),

  pullSync: () => isTauri
    ? invoke<SyncResult>('pull_sync_cmd')
    : Promise.resolve({ success: true, message: '', conflicts: [] }),

  resolveConflicts: (resolutions: ConflictResolution[]) => isTauri
    ? invoke('resolve_conflicts_cmd', { resolutions })
    : Promise.resolve(),

  syncAgentPreview: (from: string, to: string) => isTauri
    ? invoke<SyncPreview>('sync_agent_to_agent_cmd', { from, to })
    : Promise.resolve({ toCopy: 0, toSkip: 0, toUpdate: 0, details: [] }),

  executeAgentSync: (from: string, to: string) => isTauri
    ? invoke('execute_agent_sync_cmd', { from, to })
    : Promise.resolve(),

  exportBackup: (path: string) => isTauri
    ? invoke('export_backup_cmd', { path })
    : Promise.resolve(),

  importBackup: (path: string) => isTauri
    ? invoke('import_backup_cmd', { path })
    : Promise.resolve(),

  getMetadata: () => isTauri
    ? invoke<{
      sources: Record<string, { origin: string }>
      packs: SkillPack[]
      collections: SkillCollection[]
      scanRoots: ScanRoot[]
      sync: SyncConfig | null
      marketplaceSources: MarketplaceSource[]
    }>('get_registry_metadata')
    : Promise.resolve({ sources: {}, packs: [], collections: [], scanRoots: [], sync: null, marketplaceSources: [] }),

  listMarketplaceItems: () => isTauri
    ? invoke<MarketplaceItem[]>('list_marketplace_items_cmd')
    : Promise.resolve([]),

  listRegistries: () => isTauri
    ? invoke<SkillRegistry[]>('list_registries')
    : Promise.resolve([]),

  addRegistry: (name: string, sourceType: string, url: string) => isTauri
    ? invoke<SkillRegistry>('add_registry', { name, sourceType, url })
    : Promise.resolve({
      id: name.toLowerCase().replace(/[^a-z0-9]+/g, '-'),
      name,
      sourceType,
      url,
      isBuiltin: false,
      isEnabled: true,
      lastSynced: null,
      lastAttemptedSync: null,
      lastSyncStatus: 'never',
      lastSyncError: null,
      cacheUpdatedAt: null,
      cacheExpiresAt: null,
      etag: null,
      lastModified: null,
      createdAt: new Date().toISOString(),
    }),

  removeRegistry: (registryId: string) => isTauri
    ? invoke('remove_registry', { registryId })
    : Promise.resolve(),

  syncRegistry: (registryId: string, options?: RegistrySyncOptions) => isTauri
    ? (options
        ? invoke<MarketplaceSkill[]>('sync_registry_with_options', { registryId, options })
        : invoke<MarketplaceSkill[]>('sync_registry', { registryId }))
    : Promise.resolve([]),

  searchMarketplaceSkills: (registryId?: string | null, query?: string | null) => isTauri
    ? invoke<MarketplaceSkill[]>('search_marketplace_skills', { registryId: registryId ?? null, query: query ?? null })
    : Promise.resolve([]),

  installMarketplaceSkill: (skillId: string) => isTauri
    ? invoke('install_marketplace_skill', { skillId })
    : Promise.resolve(),

  listMarketplaceSources: () => isTauri
    ? invoke<MarketplaceSource[]>('list_marketplace_sources_cmd')
    : Promise.resolve([]),

  upsertMarketplaceSource: (source: MarketplaceSource) => isTauri
    ? invoke('upsert_marketplace_source_cmd', { source })
    : Promise.resolve(),

  removeMarketplaceSource: (id: string) => isTauri
    ? invoke('remove_marketplace_source_cmd', { id })
    : Promise.resolve(),
}
