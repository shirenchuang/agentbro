import { invoke } from '@tauri-apps/api/core'

export interface AgentSkillState {
  agent: string
  installPath: string
  installMode: 'direct' | 'symlink'
  enabled: boolean
}

export interface ScannedSkill {
  id: string
  name: string
  description: string
  skillType: 'skill' | 'mcp'
  icon: string | null
  source: 'island' | 'local'
  originUrl: string | null
  hasUpdate: boolean
  filePath: string
  fileSize: number
  modifiedAt: number
  agents: AgentSkillState[]
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

const isTauri = '__TAURI__' in window

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

  uninstall: (skillPath: string) => isTauri
    ? invoke('uninstall_skill_cmd', { skillPath })
    : Promise.resolve(),

  toggle: (skillId: string, agent: string, enabled: boolean) => isTauri
    ? invoke('toggle_skill_cmd', { skillId, agent, enabled })
    : Promise.resolve(),

  readFileTree: (skillPath: string) => isTauri
    ? invoke<FileTreeNode>('read_skill_files', { skillPath })
    : Promise.resolve({ name: '', nodeType: 'dir' as const, path: '', children: [] }),

  readFileContent: (filePath: string) => isTauri
    ? invoke<string>('read_skill_file_content', { filePath })
    : Promise.resolve(''),

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

  configureSyncConfig: (config: SyncConfig) => isTauri
    ? invoke('configure_sync_cmd', { config })
    : Promise.resolve(),

  pushSync: () => isTauri
    ? invoke<SyncResult>('push_sync_cmd')
    : Promise.resolve({ success: true, message: '', conflicts: [] }),

  pullSync: () => isTauri
    ? invoke<SyncResult>('pull_sync_cmd')
    : Promise.resolve({ success: true, message: '', conflicts: [] }),

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
    ? invoke<{ sources: Record<string, { origin: string }>; packs: SkillPack[]; sync: SyncConfig | null }>('get_registry_metadata')
    : Promise.resolve({ sources: {}, packs: [], sync: null }),
}
