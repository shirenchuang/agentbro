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

export interface SkillPack {
  id: string
  name: string
  description: string
  skills: string[]
  targetAgents: string[]
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

  install: (source: string, targets: TargetConfig[], mode: 'direct' | 'symlink') => isTauri
    ? invoke('install_skill_cmd', { source, targets, mode })
    : Promise.resolve(),

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
    ? invoke<{ sources: Record<string, { origin: string }>; packs: SkillPack[]; sync: SyncConfig | null; marketplaceSources: MarketplaceSource[] }>('get_registry_metadata')
    : Promise.resolve({ sources: {}, packs: [], sync: null, marketplaceSources: [] }),

  listMarketplaceItems: () => isTauri
    ? invoke<MarketplaceItem[]>('list_marketplace_items_cmd')
    : Promise.resolve([]),

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
