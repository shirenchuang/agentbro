import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export type AgentProgramKind = 'cli' | 'app'
export type AgentProgramStatus = 'installed' | 'notInstalled' | 'updateAvailable' | 'unavailable'

export interface AgentProgramInfo {
  id: string
  displayName: string
  icon: string
  kind: AgentProgramKind
  status: AgentProgramStatus
  packageManager: string | null
  packageName: string | null
  installedVersion: string | null
  latestVersion: string | null
  binaryPath: string | null
  configDir: string | null
  appPath: string | null
  downloadUrl: string | null
  installCommand: string | null
  updateCommand: string | null
  uninstallCommand: string | null
  hooksInstalled: boolean
}

export interface AgentOutputEvent {
  agentId: string
  operation: string
  stream: 'info' | 'stdout' | 'stderr' | string
  line: string
  done: boolean
  success: boolean | null
}

const mockAgents: AgentProgramInfo[] = [
  {
    id: 'claude-code',
    displayName: 'Claude Code',
    icon: 'claude',
    kind: 'cli',
    status: 'installed',
    packageManager: 'npm',
    packageName: '@anthropic-ai/claude-code',
    installedVersion: null,
    latestVersion: null,
    binaryPath: '/usr/local/bin/claude',
    configDir: '~/.claude',
    appPath: null,
    downloadUrl: 'https://docs.anthropic.com/en/docs/claude-code',
    installCommand: 'npm install -g @anthropic-ai/claude-code',
    updateCommand: 'npm update -g @anthropic-ai/claude-code',
    uninstallCommand: 'npm uninstall -g @anthropic-ai/claude-code',
    hooksInstalled: true,
  },
  {
    id: 'codex',
    displayName: 'OpenAI Codex',
    icon: 'codex',
    kind: 'cli',
    status: 'notInstalled',
    packageManager: 'npm',
    packageName: '@openai/codex',
    installedVersion: null,
    latestVersion: null,
    binaryPath: null,
    configDir: '~/.codex',
    appPath: null,
    downloadUrl: 'https://developers.openai.com/codex',
    installCommand: 'npm install -g @openai/codex',
    updateCommand: 'npm update -g @openai/codex',
    uninstallCommand: 'npm uninstall -g @openai/codex',
    hooksInstalled: false,
  },
  {
    id: 'cursor',
    displayName: 'Cursor',
    icon: 'cursor',
    kind: 'app',
    status: 'notInstalled',
    packageManager: 'app',
    packageName: null,
    installedVersion: null,
    latestVersion: null,
    binaryPath: null,
    configDir: '~/.cursor',
    appPath: '/Applications/Cursor.app',
    downloadUrl: 'https://cursor.com',
    installCommand: null,
    updateCommand: null,
    uninstallCommand: null,
    hooksInstalled: false,
  },
]

export const agentApi = {
  list: () => isTauri
    ? invoke<AgentProgramInfo[]>('agent_list')
    : Promise.resolve(mockAgents),

  refresh: () => isTauri
    ? invoke<AgentProgramInfo[]>('agent_refresh')
    : Promise.resolve(mockAgents),

  install: (agentId: string) => isTauri
    ? invoke('agent_install', { agentId })
    : Promise.resolve(),

  update: (agentId: string) => isTauri
    ? invoke('agent_update', { agentId })
    : Promise.resolve(),

  uninstall: (agentId: string) => isTauri
    ? invoke('agent_uninstall', { agentId })
    : Promise.resolve(),

  openDownload: (agentId: string) => isTauri
    ? invoke('agent_open_download', { agentId })
    : Promise.resolve(),

  openApp: (agentId: string) => isTauri
    ? invoke('agent_open_app', { agentId })
    : Promise.resolve(),

  onOutput: async (handler: (event: AgentOutputEvent) => void): Promise<UnlistenFn> => {
    if (!isTauri) return () => {}
    return listen<AgentOutputEvent>('agent-output', (event) => handler(event.payload))
  },
}
