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
  skillsDir?: string | null
  isCustom?: boolean
}

export interface CustomAgentConfig {
  id?: string | null
  displayName: string
  category?: string | null
  globalSkillsDir: string
}

export interface UpdateCustomAgentConfig {
  displayName: string
  category?: string | null
  globalSkillsDir: string
}

export interface AgentOutputEvent {
  agentId: string
  operation: string
  stream: 'info' | 'stdout' | 'stderr' | string
  line: string
  done: boolean
  success: boolean | null
}

const baseAgentPrograms: AgentProgramInfo[] = [
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
    skillsDir: '~/.claude/skills',
    isCustom: false,
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
    skillsDir: '~/.agents/skills',
    isCustom: false,
  },
  {
    id: 'gemini',
    displayName: 'Gemini CLI',
    icon: 'gemini',
    kind: 'cli',
    status: 'notInstalled',
    packageManager: 'npm',
    packageName: '@google/gemini-cli',
    installedVersion: null,
    latestVersion: null,
    binaryPath: null,
    configDir: '~/.gemini',
    appPath: null,
    downloadUrl: 'https://github.com/google-gemini/gemini-cli',
    installCommand: 'npm install -g @google/gemini-cli',
    updateCommand: 'npm update -g @google/gemini-cli',
    uninstallCommand: 'npm uninstall -g @google/gemini-cli',
    hooksInstalled: false,
    skillsDir: '~/.gemini/skills',
    isCustom: false,
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
    skillsDir: '~/.cursor/skills',
    isCustom: false,
  },
  {
    id: 'cursor-cli',
    displayName: 'Cursor CLI',
    icon: 'cursor',
    kind: 'cli',
    status: 'notInstalled',
    packageManager: 'vendor',
    packageName: 'cursor-cli',
    installedVersion: null,
    latestVersion: null,
    binaryPath: null,
    configDir: '~/.cursor',
    appPath: null,
    downloadUrl: 'https://cursor.com/cli',
    installCommand: null,
    updateCommand: null,
    uninstallCommand: null,
    hooksInstalled: false,
    skillsDir: '~/.cursor/skills',
    isCustom: false,
  },
  { id: 'opencode', displayName: 'OpenCode', icon: 'opencode', kind: 'cli', status: 'notInstalled', packageManager: 'npm', packageName: 'opencode-ai', installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.config/opencode', appPath: null, downloadUrl: 'https://opencode.ai', installCommand: 'npm install -g opencode-ai', updateCommand: 'npm update -g opencode-ai', uninstallCommand: 'npm uninstall -g opencode-ai', hooksInstalled: false, skillsDir: '~/.opencode/skills', isCustom: false },
  { id: 'copilot', displayName: 'GitHub Copilot', icon: 'copilot', kind: 'cli', status: 'notInstalled', packageManager: 'gh', packageName: 'github/gh-copilot', installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.config/gh', appPath: null, downloadUrl: 'https://github.com/github/gh-copilot', installCommand: 'gh extension install github/gh-copilot', updateCommand: 'gh extension upgrade gh-copilot', uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.copilot/skills', isCustom: false },
  { id: 'qoder-cli', displayName: 'Qoder CLI', icon: 'qoder', kind: 'cli', status: 'notInstalled', packageManager: 'vendor', packageName: 'qoder-cli', installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.qoder', appPath: null, downloadUrl: 'https://qoder.com', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.qoder/skills', isCustom: false },
  { id: 'qoder', displayName: 'Qoder', icon: 'qoder', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.qoder', appPath: '/Applications/Qoder.app', downloadUrl: 'https://qoder.com', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.qoder/skills', isCustom: false },
  { id: 'hermes', displayName: 'Hermes', icon: 'hermes', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.hermes', appPath: '/Applications/Hermes.app', downloadUrl: 'https://hermes.ai', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.hermes/skills', isCustom: false },
  { id: 'antigravity', displayName: 'Antigravity', icon: 'antigravity', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.antigravity', appPath: '/Applications/Antigravity.app', downloadUrl: 'https://antigravity.google', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.agents/skills', isCustom: false },
  { id: 'trae', displayName: 'Trae', icon: 'trae', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.trae', appPath: '/Applications/Trae.app', downloadUrl: 'https://www.trae.ai', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.trae/skills', isCustom: false },
  { id: 'traecn', displayName: 'Trae CN', icon: 'trae', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.trae', appPath: '/Applications/Trae CN.app', downloadUrl: 'https://www.trae.com.cn', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.trae-cn/skills', isCustom: false },
  { id: 'qwen', displayName: 'Qwen Code', icon: 'qwen', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.qwen', appPath: '/Applications/Qwen Code.app', downloadUrl: 'https://qwenlm.github.io', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.qwen/skills', isCustom: false },
  { id: 'kimi', displayName: 'Kimi', icon: 'kimi', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.kimi', appPath: '/Applications/Kimi.app', downloadUrl: 'https://www.kimi.com', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.agents/skills', isCustom: false },
  { id: 'droid', displayName: 'Factory Droid', icon: 'droid', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.factory', appPath: '/Applications/Factory.app', downloadUrl: 'https://www.factory.ai', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.factory/skills', isCustom: false },
  { id: 'stepfun', displayName: 'StepFun', icon: 'stepfun', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.stepfun', appPath: '/Applications/StepFun.app', downloadUrl: 'https://platform.stepfun.com', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.stepfun/skills', isCustom: false },
  { id: 'codebuddy', displayName: 'CodeBuddy', icon: 'codebuddy', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.codebuddy', appPath: '/Applications/CodeBuddy.app', downloadUrl: 'https://codebuddy.ai', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.codebuddy/skills', isCustom: false },
  { id: 'codebuddycn', displayName: 'CodeBuddy CN', icon: 'codebuddy', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.codebuddy', appPath: '/Applications/CodeBuddy CN.app', downloadUrl: 'https://www.codebuddy.ai', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.codebuddy/skills', isCustom: false },
  { id: 'workbuddy', displayName: 'WorkBuddy', icon: 'workbuddy', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.workbuddy', appPath: '/Applications/WorkBuddy.app', downloadUrl: 'https://workbuddy.ai', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.workbuddy/skills-marketplace/skills', isCustom: false },
  { id: 'kiro', displayName: 'Kiro', icon: 'kiro', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.kiro', appPath: '/Applications/Kiro.app', downloadUrl: 'https://kiro.dev', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.kiro/skills', isCustom: false },
  { id: 'pi', displayName: 'Pi', icon: 'pi', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.pi', appPath: '/Applications/Pi.app', downloadUrl: 'https://pi.ai', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.pi/agent/skills', isCustom: false },
]

export function seedAgentPrograms() {
  return baseAgentPrograms.map(agent => ({ ...agent }))
}

export const agentApi = {
  list: () => isTauri
    ? invoke<AgentProgramInfo[]>('agent_list')
    : Promise.resolve(seedAgentPrograms()),

  refresh: () => isTauri
    ? invoke<AgentProgramInfo[]>('agent_refresh')
    : Promise.resolve(seedAgentPrograms()),

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

  installHook: (agentId: string) => isTauri
    ? invoke('install_agent_hook', { toolName: agentId })
    : Promise.resolve(),

  uninstallHook: (agentId: string) => isTauri
    ? invoke('uninstall_agent_hook', { toolName: agentId })
    : Promise.resolve(),

  openPath: (path: string) => isTauri
    ? invoke('open_system_path', { path })
    : Promise.resolve(),

  addCustom: (config: CustomAgentConfig) => isTauri
    ? invoke<AgentProgramInfo>('add_custom_agent', { config })
    : Promise.resolve({
      id: config.id || `custom-${config.displayName.toLowerCase().replace(/\s+/g, '-')}`,
      displayName: config.displayName,
      icon: 'custom',
      kind: 'cli' as const,
      status: 'installed' as const,
      packageManager: 'custom',
      packageName: null,
      installedVersion: null,
      latestVersion: null,
      binaryPath: null,
      configDir: config.globalSkillsDir,
      appPath: null,
      downloadUrl: null,
      installCommand: null,
      updateCommand: null,
      uninstallCommand: null,
      hooksInstalled: false,
      skillsDir: config.globalSkillsDir,
      isCustom: true,
    }),

  updateCustom: (agentId: string, config: UpdateCustomAgentConfig) => isTauri
    ? invoke<AgentProgramInfo>('update_custom_agent', { agentId, config })
    : Promise.resolve({
      id: agentId,
      displayName: config.displayName,
      icon: 'custom',
      kind: 'cli' as const,
      status: 'installed' as const,
      packageManager: 'custom',
      packageName: null,
      installedVersion: null,
      latestVersion: null,
      binaryPath: null,
      configDir: config.globalSkillsDir,
      appPath: null,
      downloadUrl: null,
      installCommand: null,
      updateCommand: null,
      uninstallCommand: null,
      hooksInstalled: false,
      skillsDir: config.globalSkillsDir,
      isCustom: true,
    }),

  removeCustom: (agentId: string) => isTauri
    ? invoke('remove_custom_agent', { agentId })
    : Promise.resolve(),

  onOutput: async (handler: (event: AgentOutputEvent) => void): Promise<UnlistenFn> => {
    if (!isTauri) return () => {}
    return listen<AgentOutputEvent>('agent-output', (event) => handler(event.payload))
  },
}
