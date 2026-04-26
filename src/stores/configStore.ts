/* Agent Island — Configuration State Management (Zustand) */
import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { AgentType } from '../types/agent'

export interface AgentHook {
  agentType: AgentType
  label: string
  enabled: boolean
  connected: boolean
}

export interface SoundEvent {
  id: string
  label: string
  group: 'session' | 'interaction' | 'system'
  enabled: boolean
}

export interface ShortcutBinding {
  action: string
  label: string
  keys: string
}

export interface SSHHost {
  id: string
  name: string
  host: string
  enabled: boolean
}

export interface LabFeature {
  id: string
  label: string
  description: string
  enabled: boolean
}

interface ConfigState {
  // General
  launchAtLogin: boolean
  displayMonitor: string
  hideInFullscreen: boolean
  autoHide: boolean
  autoHideNoSessions: boolean
  smartSuppression: boolean
  autoCollapse: boolean
  completionPopupDuration: string
  dwellDuration: number // ms delay before expanding on hover (100-1000)
  taskCompleteDwellSeconds: number // dwell before auto-collapse after task completes
  agentHooks: AgentHook[]

  // Display
  notchStyle: 'compact' | 'detailed'
  contentFontSize: string
  completionCardHeight: number
  maxPanelHeight: number
  showAgentActivityDetails: boolean

  // Token & Cost
  tokenDisplayMode: 'tokens' | 'cost' | 'both' | 'hidden'
  costModel: string

  // Sound
  soundEnabled: boolean
  volume: number
  soundEvents: SoundEvent[]
  probeSessionFilter: boolean
  soundPack: 'eight-bit' | 'subtle' | 'custom'

  // Shortcuts
  shortcuts: ShortcutBinding[]

  // SSH Remote
  sshHosts: SSHHost[]
  tcpPort: number

  // Labs
  betaUpdates: boolean
  labFeatures: LabFeature[]

  // License
  licenseKey: string
  licenseStatus: 'active' | 'expired' | 'trial' | 'unlicensed'

  // General — UI
  showUsageQuota: boolean

  // About
  telemetryEnabled: boolean

  // Language
  language: 'en' | 'zh'
}

interface ConfigActions {
  updateConfig: <K extends keyof ConfigState>(key: K, value: ConfigState[K]) => void
  toggleAgentHook: (agentType: AgentType) => void
  toggleSoundEvent: (id: string) => void
  updateShortcut: (action: string, keys: string) => void
  addSSHHost: (host: SSHHost) => void
  removeSSHHost: (id: string) => void
  toggleLabFeature: (id: string) => void
}

type ConfigStore = ConfigState & ConfigActions

const defaultAgentHooks: AgentHook[] = [
  { agentType: 'claude-code', label: 'Claude Code', enabled: true, connected: true },
  { agentType: 'codex', label: 'Codex CLI', enabled: true, connected: false },
  { agentType: 'gemini-cli', label: 'Gemini CLI', enabled: false, connected: false },
  { agentType: 'cursor', label: 'Cursor', enabled: false, connected: false },
  { agentType: 'copilot', label: 'GitHub Copilot', enabled: false, connected: false },
  { agentType: 'kiro', label: 'Kiro', enabled: false, connected: false },
]

const defaultSoundEvents: SoundEvent[] = [
  { id: 'session-start', label: 'Session Started', group: 'session', enabled: true },
  { id: 'session-end', label: 'Session Ended', group: 'session', enabled: true },
  { id: 'session-error', label: 'Session Error', group: 'session', enabled: true },
  { id: 'permission-request', label: 'Permission Request', group: 'interaction', enabled: true },
  { id: 'question-asked', label: 'Question Asked', group: 'interaction', enabled: true },
  { id: 'task-complete', label: 'Task Complete', group: 'interaction', enabled: true },
  { id: 'context-compact', label: 'Context Compacting', group: 'system', enabled: false },
  { id: 'token-limit', label: 'Token Limit Warning', group: 'system', enabled: true },
]

const defaultShortcuts: ShortcutBinding[] = [
  { action: 'toggle-panel', label: 'Toggle Panel', keys: '⌘+Shift+I' },
  { action: 'expand-panel', label: 'Expand Panel', keys: '⌘+Shift+E' },
  { action: 'collapse-panel', label: 'Collapse Panel', keys: 'Escape' },
  { action: 'next-session', label: 'Next Session', keys: '⌘+]' },
  { action: 'prev-session', label: 'Previous Session', keys: '⌘+[' },
  { action: 'approve-action', label: 'Approve Action', keys: '⌘+Enter' },
  { action: 'reject-action', label: 'Reject Action', keys: '⌘+Backspace' },
  { action: 'open-settings', label: 'Open Settings', keys: '⌘+,' },
]

const defaultLabFeatures: LabFeature[] = [
  { id: 'streaming-diff', label: 'Streaming Diff View', description: 'Show file changes as they stream in real-time', enabled: false },
  { id: 'ai-summary', label: 'AI Session Summary', description: 'Auto-generate natural language summaries of completed sessions', enabled: false },
  { id: 'multi-monitor', label: 'Multi-Monitor Support', description: 'Show island on multiple displays simultaneously', enabled: false },
]

export const useConfigStore = create<ConfigStore>()(
  persist(
    (set) => ({
  // General
  launchAtLogin: false,
  displayMonitor: 'auto',
  hideInFullscreen: true,
  autoHide: false,
  autoHideNoSessions: false,
  smartSuppression: true,
  autoCollapse: true,
  completionPopupDuration: '5s',
  dwellDuration: 300,
  taskCompleteDwellSeconds: 3,
  agentHooks: defaultAgentHooks,

  // Display
  notchStyle: 'compact',
  contentFontSize: '13px',
  completionCardHeight: 120,
  maxPanelHeight: 560,
  showAgentActivityDetails: true,

  // Token & Cost
  tokenDisplayMode: 'both',
  costModel: 'claude-sonnet-4',

  // Sound
  soundEnabled: true,
  volume: 70,
  soundEvents: defaultSoundEvents,
  probeSessionFilter: false,
  soundPack: 'eight-bit',

  // Shortcuts
  shortcuts: defaultShortcuts,

  // SSH Remote
  sshHosts: [],
  tcpPort: 7399,

  // Labs
  betaUpdates: false,
  labFeatures: defaultLabFeatures,

  // License
  licenseKey: '',
  licenseStatus: 'trial',

  // General — UI
  showUsageQuota: true,

  // About
  telemetryEnabled: true,

  // Language
  language: (navigator.language.startsWith('zh') ? 'zh' : 'en') as 'en' | 'zh',

  // Actions
  updateConfig: (key, value) => {
    set({ [key]: value } as Partial<ConfigState>)
  },

  toggleAgentHook: (agentType) => {
    set((state) => ({
      agentHooks: state.agentHooks.map((h) =>
        h.agentType === agentType ? { ...h, enabled: !h.enabled } : h
      ),
    }))
  },

  toggleSoundEvent: (id) => {
    set((state) => ({
      soundEvents: state.soundEvents.map((e) =>
        e.id === id ? { ...e, enabled: !e.enabled } : e
      ),
    }))
  },

  updateShortcut: (action, keys) => {
    set((state) => ({
      shortcuts: state.shortcuts.map((s) =>
        s.action === action ? { ...s, keys } : s
      ),
    }))
  },

  addSSHHost: (host) => {
    set((state) => ({ sshHosts: [...state.sshHosts, host] }))
  },

  removeSSHHost: (id) => {
    set((state) => ({ sshHosts: state.sshHosts.filter((h) => h.id !== id) }))
  },

  toggleLabFeature: (id) => {
    set((state) => ({
      labFeatures: state.labFeatures.map((f) =>
        f.id === id ? { ...f, enabled: !f.enabled } : f
      ),
    }))
  },
    }),
    { name: 'agent-island-config' }
  )
)
