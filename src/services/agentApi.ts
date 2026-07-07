import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { isTauri as isTauriRuntime } from './tauriApi'

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
  iconName?: string | null
  configDir?: string | null
  settingsFile?: string | null
  mcpConfig?: string | null
  pluginDir?: string | null
}

export interface UpdateCustomAgentConfig {
  displayName: string
  category?: string | null
  globalSkillsDir: string
  iconName?: string | null
  configDir?: string | null
  settingsFile?: string | null
  mcpConfig?: string | null
  pluginDir?: string | null
}

export interface AgentOutputEvent {
  agentId: string
  operation: string
  stream: 'info' | 'stdout' | 'stderr' | string
  line: string
  done: boolean
  success: boolean | null
}

function runtimePlatform() {
  if (typeof navigator === 'undefined') return 'linux'
  const nav = navigator as Navigator & { userAgentData?: { platform?: string } }
  const platform = `${nav.userAgentData?.platform || navigator.platform || ''}`.toLowerCase()
  if (platform.includes('mac')) return 'macos'
  if (platform.includes('win')) return 'windows'
  return 'linux'
}

function macAppPath(path: string) {
  return runtimePlatform() === 'macos' ? path : null
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
    updateCommand: 'npm install -g @anthropic-ai/claude-code@latest',
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
    updateCommand: 'npm install -g @openai/codex@latest',
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
    updateCommand: 'npm install -g @google/gemini-cli@latest',
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
    appPath: macAppPath('/Applications/Cursor.app'),
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
  { id: 'opencode', displayName: 'OpenCode', icon: 'opencode', kind: 'cli', status: 'notInstalled', packageManager: 'npm', packageName: 'opencode-ai', installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.config/opencode', appPath: null, downloadUrl: 'https://opencode.ai', installCommand: 'npm install -g opencode-ai', updateCommand: 'npm install -g opencode-ai@latest', uninstallCommand: 'npm uninstall -g opencode-ai', hooksInstalled: false, skillsDir: '~/.opencode/skills', isCustom: false },
  { id: 'copilot', displayName: 'GitHub Copilot', icon: 'copilot', kind: 'cli', status: 'notInstalled', packageManager: 'gh', packageName: 'github/gh-copilot', installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.config/gh', appPath: null, downloadUrl: 'https://github.com/github/gh-copilot', installCommand: 'gh extension install github/gh-copilot', updateCommand: 'gh extension upgrade gh-copilot', uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.copilot/skills', isCustom: false },
  { id: 'cline', displayName: 'Cline', icon: 'cline', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/Documents/Cline', appPath: macAppPath('/Applications/Visual Studio Code.app'), downloadUrl: 'https://cline.bot', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: null, isCustom: false },
  { id: 'qoder-cli', displayName: 'Qoder CLI', icon: 'qoder', kind: 'cli', status: 'notInstalled', packageManager: 'vendor', packageName: 'qoder-cli', installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.qoder', appPath: null, downloadUrl: 'https://qoder.com', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.qoder/skills', isCustom: false },
  { id: 'qoder', displayName: 'Qoder', icon: 'qoder', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.qoder', appPath: macAppPath('/Applications/Qoder.app'), downloadUrl: 'https://qoder.com', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.qoder/skills', isCustom: false },
  { id: 'hermes', displayName: 'Hermes', icon: 'hermes', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.hermes', appPath: macAppPath('/Applications/Hermes.app'), downloadUrl: 'https://hermes.ai', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.hermes/skills', isCustom: false },
  { id: 'antigravity', displayName: 'Antigravity', icon: 'antigravity', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.antigravity', appPath: macAppPath('/Applications/Antigravity.app'), downloadUrl: 'https://antigravity.google', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.agents/skills', isCustom: false },
  { id: 'qwen', displayName: 'Qwen Code', icon: 'qwen', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.qwen', appPath: macAppPath('/Applications/Qwen Code.app'), downloadUrl: 'https://qwenlm.github.io', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.qwen/skills', isCustom: false },
  { id: 'deepseek', displayName: 'DeepSeek', icon: 'deepseek', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.deepseek', appPath: macAppPath('/Applications/DeepSeek.app'), downloadUrl: 'https://www.deepseek.com', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: null, isCustom: false },
  { id: 'kimi', displayName: 'Kimi', icon: 'kimi', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.kimi', appPath: macAppPath('/Applications/Kimi.app'), downloadUrl: 'https://www.kimi.com', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.kimi/skills', isCustom: false },
  { id: 'droid', displayName: 'Factory Droid', icon: 'droid', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.factory', appPath: macAppPath('/Applications/Factory.app'), downloadUrl: 'https://www.factory.ai', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.factory/skills', isCustom: false },
  { id: 'stepfun', displayName: 'StepFun', icon: 'stepfun', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.stepfun', appPath: macAppPath('/Applications/StepFun.app'), downloadUrl: 'https://platform.stepfun.com', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.stepfun/skills', isCustom: false },
  { id: 'codebuddy', displayName: 'CodeBuddy', icon: 'codebuddy', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.codebuddy', appPath: macAppPath('/Applications/CodeBuddy.app'), downloadUrl: 'https://codebuddy.ai', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.codebuddy/skills', isCustom: false },
  { id: 'codebuddycn', displayName: 'CodyBuddyCN', icon: 'codebuddy', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.codybuddycn', appPath: macAppPath('/Applications/CodyBuddyCN.app'), downloadUrl: 'https://www.codebuddy.ai', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.codybuddycn/skills', isCustom: false },
  { id: 'workbuddy', displayName: 'WorkBuddy', icon: 'workbuddy', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.workbuddy', appPath: macAppPath('/Applications/WorkBuddy.app'), downloadUrl: 'https://workbuddy.ai', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.workbuddy/skills', isCustom: false },
  { id: 'kiro', displayName: 'Kiro', icon: 'kiro', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.kiro', appPath: macAppPath('/Applications/Kiro.app'), downloadUrl: 'https://kiro.dev', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.kiro/skills', isCustom: false },
  { id: 'pi', displayName: 'Pi', icon: 'pi', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.pi', appPath: macAppPath('/Applications/Pi.app'), downloadUrl: 'https://pi.ai', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.pi/agent/skills', isCustom: false },
  { id: 'factory-droid', displayName: 'Factory Droid', icon: 'factory-droid', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.factory', appPath: macAppPath('/Applications/Factory.app'), downloadUrl: 'https://www.factory.ai', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.factory/skills', isCustom: false },
  { id: 'junie', displayName: 'Junie', icon: 'junie', kind: 'cli', status: 'notInstalled', packageManager: 'vendor', packageName: 'junie', installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.junie', appPath: null, downloadUrl: 'https://www.jetbrains.com/junie/', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.junie/skills', isCustom: false },
  { id: 'windsurf', displayName: 'Windsurf', icon: 'windsurf', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.codeium/windsurf', appPath: macAppPath('/Applications/Windsurf.app'), downloadUrl: 'https://windsurf.com', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.windsurf/skills', isCustom: false },
  { id: 'augment', displayName: 'Augment', icon: 'augment', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.augment', appPath: macAppPath('/Applications/Augment.app'), downloadUrl: 'https://www.augmentcode.com', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.augment/skills', isCustom: false },
  { id: 'kilocode', displayName: 'KiloCode', icon: 'kilocode', kind: 'cli', status: 'notInstalled', packageManager: 'vendor', packageName: 'kilocode', installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.kilocode', appPath: null, downloadUrl: 'https://kilocode.ai', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.kilocode/skills', isCustom: false },
  { id: 'ob1', displayName: 'OB1', icon: 'ob1', kind: 'cli', status: 'notInstalled', packageManager: 'vendor', packageName: 'ob1', installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.ob1', appPath: null, downloadUrl: 'https://ob1.ai', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.ob1/skills', isCustom: false },
  { id: 'amp', displayName: 'Amp', icon: 'amp', kind: 'cli', status: 'notInstalled', packageManager: 'npm', packageName: '@sourcegraph/amp', installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.amp', appPath: null, downloadUrl: 'https://ampcode.com', installCommand: 'npm install -g @sourcegraph/amp', updateCommand: 'npm install -g @sourcegraph/amp@latest', uninstallCommand: 'npm uninstall -g @sourcegraph/amp', hooksInstalled: false, skillsDir: '~/.amp/skills', isCustom: false },
  { id: 'aider', displayName: 'Aider', icon: 'aider', kind: 'cli', status: 'notInstalled', packageManager: 'pipx', packageName: 'aider-chat', installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.aider', appPath: null, downloadUrl: 'https://aider.chat', installCommand: 'pipx install aider-chat', updateCommand: 'pipx upgrade aider-chat', uninstallCommand: 'pipx uninstall aider-chat', hooksInstalled: false, skillsDir: '~/.aider/skills', isCustom: false },
  { id: 'openclaw', displayName: 'OpenClaw', icon: 'openclaw', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.openclaw', appPath: macAppPath('/Applications/OpenClaw.app'), downloadUrl: 'https://github.com/openclaw-ai/openclaw', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.openclaw/skills', isCustom: false },
  { id: 'qclaw', displayName: 'QClaw', icon: 'qclaw', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.qclaw', appPath: macAppPath('/Applications/QClaw.app'), downloadUrl: 'https://github.com/openclaw-ai/qclaw', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.qclaw/skills', isCustom: false },
  { id: 'easyclaw', displayName: 'EasyClaw', icon: 'easyclaw', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.easyclaw', appPath: macAppPath('/Applications/EasyClaw.app'), downloadUrl: 'https://github.com/openclaw-ai/easyclaw', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.easyclaw/skills', isCustom: false },
  { id: 'easyclaw-v2', displayName: 'EasyClaw V2', icon: 'easyclaw', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.easyclaw-20260322-01', appPath: macAppPath('/Applications/EasyClaw.app'), downloadUrl: 'https://github.com/openclaw-ai/easyclaw', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.easyclaw-20260322-01/skills', isCustom: false },
  { id: 'autoclaw', displayName: 'AutoClaw', icon: 'autoclaw', kind: 'app', status: 'notInstalled', packageManager: 'app', packageName: null, installedVersion: null, latestVersion: null, binaryPath: null, configDir: '~/.openclaw-autoclaw', appPath: macAppPath('/Applications/AutoClaw.app'), downloadUrl: 'https://github.com/openclaw-ai/autoclaw', installCommand: null, updateCommand: null, uninstallCommand: null, hooksInstalled: false, skillsDir: '~/.openclaw-autoclaw/skills', isCustom: false },
]

export function seedAgentPrograms() {
  return baseAgentPrograms.map(agent => ({ ...agent }))
}

export const agentApi = {
  list: () => isTauriRuntime()
    ? invoke<AgentProgramInfo[]>('agent_list')
    : Promise.resolve(seedAgentPrograms()),

  refresh: () => isTauriRuntime()
    ? invoke<AgentProgramInfo[]>('agent_refresh')
    : Promise.resolve(seedAgentPrograms()),

  install: (agentId: string) => isTauriRuntime()
    ? invoke('agent_install', { agentId })
    : Promise.resolve(),

  update: (agentId: string) => isTauriRuntime()
    ? invoke('agent_update', { agentId })
    : Promise.resolve(),

  uninstall: (agentId: string) => isTauriRuntime()
    ? invoke('agent_uninstall', { agentId })
    : Promise.resolve(),

  openDownload: (agentId: string) => isTauriRuntime()
    ? invoke('agent_open_download', { agentId })
    : Promise.resolve(),

  openApp: (agentId: string) => isTauriRuntime()
    ? invoke('agent_open_app', { agentId })
    : Promise.resolve(),

  installHook: (agentId: string) => isTauriRuntime()
    ? invoke('install_agent_hook', { toolName: agentId })
    : Promise.resolve(),

  uninstallHook: (agentId: string) => isTauriRuntime()
    ? invoke('uninstall_agent_hook', { toolName: agentId })
    : Promise.resolve(),

  openPath: (path: string) => isTauriRuntime()
    ? invoke('open_system_path', { path })
    : Promise.resolve(),

  addCustom: (config: CustomAgentConfig) => isTauriRuntime()
    ? invoke<AgentProgramInfo>('add_custom_agent', { config })
    : Promise.resolve({
      id: config.id || `custom-${config.displayName.toLowerCase().replace(/\s+/g, '-')}`,
      displayName: config.displayName,
      icon: config.iconName || 'custom',
      kind: 'cli' as const,
      status: 'installed' as const,
      packageManager: 'custom',
      packageName: null,
      installedVersion: null,
      latestVersion: null,
      binaryPath: null,
      configDir: config.configDir || config.globalSkillsDir,
      appPath: null,
      downloadUrl: null,
      installCommand: null,
      updateCommand: null,
      uninstallCommand: null,
      hooksInstalled: false,
      skillsDir: config.globalSkillsDir,
      isCustom: true,
    }),

  updateCustom: (agentId: string, config: UpdateCustomAgentConfig) => isTauriRuntime()
    ? invoke<AgentProgramInfo>('update_custom_agent', { agentId, config })
    : Promise.resolve({
      id: agentId,
      displayName: config.displayName,
      icon: config.iconName || 'custom',
      kind: 'cli' as const,
      status: 'installed' as const,
      packageManager: 'custom',
      packageName: null,
      installedVersion: null,
      latestVersion: null,
      binaryPath: null,
      configDir: config.configDir || config.globalSkillsDir,
      appPath: null,
      downloadUrl: null,
      installCommand: null,
      updateCommand: null,
      uninstallCommand: null,
      hooksInstalled: false,
      skillsDir: config.globalSkillsDir,
      isCustom: true,
    }),

  removeCustom: (agentId: string) => isTauriRuntime()
    ? invoke('remove_custom_agent', { agentId })
    : Promise.resolve(),

  onOutput: async (handler: (event: AgentOutputEvent) => void): Promise<UnlistenFn> => {
    if (!isTauriRuntime()) return () => {}
    return listen<AgentOutputEvent>('agent-output', (event) => handler(event.payload))
  },
}
