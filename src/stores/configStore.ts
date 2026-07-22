/* AgentBro — Configuration State Management (Zustand) */
import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { AgentType } from '../types/agent'
import { CUSTOM_NOTCH_HEIGHT_DEFAULT } from '../utils/islandLayout'

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

export type SoundChoice =
  | 'default'
  | 'synth'
  | 'eight-bit'
  | 'system'
  | 'off'
  | `builtin:${string}`
  | `custom:${string}`

export interface SoundRule {
  enabled: boolean
  sound: SoundChoice
}

const LEGACY_CHIME_SOUND_CHOICE = `builtin:${'p'}${'i'}${'n'}${'g'}` as const

function normalizeSoundChoice(sound: SoundChoice): SoundChoice {
  return sound === LEGACY_CHIME_SOUND_CHOICE ? 'builtin:chime' : sound
}

function normalizeSoundRules(rules: Record<string, SoundRule>): Record<string, SoundRule> {
  return Object.fromEntries(
    Object.entries(rules).map(([id, rule]) => [
      id,
      { ...rule, sound: normalizeSoundChoice(rule.sound) },
    ]),
  )
}

export interface CustomSound {
  id: string
  name: string
  path: string
  dataUrl?: string
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

export interface WebhookEntry {
  id: string
  name: string
  platform: 'dingtalk' | 'feishu'
  url: string
  secret: string | null
  sources: string[]
  events: string[]
  enabled: boolean
  delayEnabled: boolean
  delayMinutes: number
}

export interface RemoteHostEntry {
  id: string
  name: string
  sshTarget: string
  port: number | null
  remoteSocketPath: string
  autoConnect: boolean
  connectionStatus: 'disconnected' | 'connecting' | 'connected' | 'failed'
}

export interface LabFeature {
  id: string
  label: string
  description: string
  enabled: boolean
}

export interface EngineInstance {
  id: string
  label: string
  configRoot: string
  enabled: boolean
}

export type SessionSilenceRuleKind = 'cwd' | 'prompt'

export interface SessionSilenceRule {
  id: string
  kind: SessionSilenceRuleKind
  pattern: string
  label?: string
  enabled: boolean
  createdAt: number
}

interface ConfigState {
  // General
  launchAtLogin: boolean
  displayMonitor: string
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
  hoverSpeed: 'instant' | 'normal' | 'slow'
  islandAnimationScale: number
  contentFontSize: string
  completionCardHeight: number
  maxPanelHeight: number
  detailPanelMaxHeight: number
  showAgentActivityDetails: boolean

  // Token & Cost
  tokenDisplayMode: 'tokens' | 'cost' | 'both' | 'hidden'
  costModel: string

  // Sound
  soundEnabled: boolean
  volume: number
  soundEvents: SoundEvent[]
  soundRules: Record<string, SoundRule>
  customSounds: CustomSound[]
  probeSessionFilter: boolean
  soundPack: 'eight-bit' | 'subtle' | 'synth' | 'system' | 'none' | 'custom'
  bootSoundDefaultMigrated: boolean

  // Display toggles
  islandEnabled: boolean
  islandExternalEnabled: boolean
  islandMonitorSubagents: boolean
  tipsEnabled: boolean
  pixelCursorEnabled: boolean
  confettiEnabled: boolean
  analyticsEnabled: boolean
  analyticsConsentPromptCompleted: boolean
  islandSurfaceMode: 'island' | 'pet'
  petVitalsEnabled: boolean
  petVitalsDebugOpen: boolean
  islandPetScale: number
  islandPetWindowOrigin: { x: number; y: number } | null
  islandPetWindowAnchor: { left: boolean; top: boolean } | null
  /** Active pet identifier (e.g. 'codex:dewey'). `null` = auto-follow active session's agent type. */
  islandActivePetId: string | null
  /** AUTO 模式下每个 agent 默认显示的宠物。Key = adapter name（'claude-code'、'codex'...）。 */
  islandAgentPetMap: Record<string, string>

  // General extras
  followFocus: boolean
  globalShortcut: string
  shortcutApprove: string
  shortcutApproveEnabled: boolean
  shortcutDeny: string
  shortcutDenyEnabled: boolean
  shortcutSkip: string
  shortcutSkipEnabled: boolean
  permissionShortcutDefaultsMigrated: boolean
  shortcutSurfaceDefaultsMigrated: boolean
  shortcutPlatformDefaultsMigrated: boolean
  customHooksPath: string

  // Shortcuts
  shortcuts: ShortcutBinding[]

  // SSH Remote
  sshHosts: SSHHost[]
  tcpPort: number

  // Labs
  labFeatures: LabFeature[]

  // General — UI
  showUsageQuota: boolean
  usageQueryEnabled: boolean
  codexAppServerSyncEnabled: boolean
  codexAppServerSyncIntervalSeconds: number

  // Language
  language: 'en' | 'zh' | 'ja' | 'ko' | 'tr'

  // Quiet Hours
  quietHours: {
    enabled: boolean
    start: string
    end: string
  }

  // Idle Timeout
  idleTimeoutMinutes: number // 0 = disabled, otherwise minutes of inactivity before auto-hiding
  idleInteractionRoutingEnabled: boolean
  idleInteractionRoutingMinutes: number

  // Updates
  autoCheckUpdate: boolean
  autoInstallUpdate: boolean

  // Notification Mode
  notificationMode: 'turnEnd' | 'every' // turnEnd = only at turn completion, every = every tool call

  // Engine Instances
  engineInstances: EngineInstance[]

  // Webhooks
  webhooks: WebhookEntry[]

  // Remote Hosts
  remoteHostEntries: RemoteHostEntry[]

  // Panel layout
  allowHorizontalDrag: boolean
  panelHorizontalOffset: number
  collapsedWidthScale: number // 50-150 percentage
  microPillWidth: number
  compactPillWidth: number
  panelMaxWidth: number
  notchHeightMode: 'matchNotch' | 'matchMenuBar' | 'custom'
  customNotchHeight: number

  // Behavior
  excludedHookCwdSubstrings: string
  sessionSilenceRules: SessionSilenceRule[]
  autoApproveTools: string[]
  hapticOnHover: boolean
  hapticIntensity: number // 1-3
  carouselIntervalMs: number
  processingTimeoutSecs: number
  maxVisibleSessions: number
  showToolStatus: boolean
  sessionTimeoutMinutes: number

  // Display
  aiMessageLines: number

  // AgentBro parity — interaction timing
  clickToDetail: boolean
  jumpBeforeSend: boolean
  showCacheTTL: boolean
  hoverExpandDelay: number // ms before expanding on hover (0 = instant)
  microHoverExpandDelay: number // ms before expanding micro pill on hover
  collapseDelay: number // ms before collapsing after cursor leaves
  idleCompactDwellSeconds: number // seconds to keep compact after active sessions finish
  noSessionsHideDelay: number // minutes before hiding when no active sessions
  escSilenceDuration: number // seconds to silence wakeups after ESC
  interactionMode: 'persistent' | 'minimal'
}

interface ConfigActions {
  updateConfig: <K extends keyof ConfigState>(key: K, value: ConfigState[K]) => void
  resetIslandDefaults: () => void
  toggleAgentHook: (agentType: AgentType) => void
  toggleSoundEvent: (id: string) => void
  updateShortcut: (action: string, keys: string) => void
  addSSHHost: (host: SSHHost) => void
  removeSSHHost: (id: string) => void
  toggleLabFeature: (id: string) => void
  // Webhook actions
  addWebhook: (webhook: WebhookEntry) => void
  removeWebhook: (id: string) => void
  updateWebhook: (webhook: WebhookEntry) => void
  toggleWebhook: (id: string) => void
  // Remote host actions
  addRemoteHostEntry: (host: RemoteHostEntry) => void
  removeRemoteHostEntry: (id: string) => void
  updateRemoteHostStatus: (id: string, status: RemoteHostEntry['connectionStatus']) => void
  addSessionSilenceRule: (rule: Omit<SessionSilenceRule, 'id' | 'enabled' | 'createdAt'> & Partial<Pick<SessionSilenceRule, 'enabled'>>) => void
  removeSessionSilenceRule: (id: string) => void
  toggleSessionSilenceRule: (id: string) => void
}

type ConfigStore = ConfigState & ConfigActions

const defaultAgentHooks: AgentHook[] = [
  { agentType: 'claude-code', label: 'Claude Code', enabled: true, connected: true },
  { agentType: 'cline', label: 'Cline', enabled: false, connected: false },
  { agentType: 'codex', label: 'Codex CLI', enabled: true, connected: false },
  { agentType: 'gemini-cli', label: 'Gemini CLI', enabled: false, connected: false },
  { agentType: 'cursor', label: 'Cursor', enabled: false, connected: false },
  { agentType: 'cursor-cli', label: 'Cursor CLI', enabled: false, connected: false },
  { agentType: 'copilot', label: 'GitHub Copilot', enabled: false, connected: false },
  { agentType: 'qoder', label: 'Qoder', enabled: false, connected: false },
  { agentType: 'qoder-cli', label: 'Qoder CLI', enabled: false, connected: false },
  { agentType: 'codebuddy', label: 'CodeBuddy', enabled: false, connected: false },
  { agentType: 'codebuddycn', label: 'CodyBuddyCN', enabled: false, connected: false },
  { agentType: 'qwen', label: 'Qwen', enabled: false, connected: false },
  { agentType: 'kimi', label: 'Kimi', enabled: false, connected: false },
  { agentType: 'deepseek', label: 'DeepSeek', enabled: false, connected: false },
  { agentType: 'opencode', label: 'OpenCode', enabled: false, connected: false },
  { agentType: 'droid', label: 'Factory', enabled: false, connected: false },
  { agentType: 'stepfun', label: 'StepFun', enabled: false, connected: false },
  { agentType: 'antigravity', label: 'AntiGravity', enabled: false, connected: false },
  { agentType: 'workbuddy', label: 'WorkBuddy', enabled: false, connected: false },
  { agentType: 'zcode', label: 'ZCode', enabled: false, connected: false },
  { agentType: 'hermes', label: 'Hermes', enabled: false, connected: false },
  { agentType: 'pi', label: 'Pi', enabled: false, connected: false },
  { agentType: 'kiro', label: 'Kiro', enabled: false, connected: false },
]

const defaultSoundEvents: SoundEvent[] = [
  { id: 'session-start', label: 'Session Started', group: 'session', enabled: true },
  { id: 'session-end', label: 'Session Ended', group: 'session', enabled: true },
  { id: 'session-error', label: 'Session Error', group: 'session', enabled: true },
  { id: 'permission-request', label: 'Permission Request', group: 'interaction', enabled: true },
  { id: 'plan-approval', label: 'Plan Approval', group: 'interaction', enabled: true },
  { id: 'question-asked', label: 'Question Asked', group: 'interaction', enabled: true },
  { id: 'task-complete', label: 'Task Complete', group: 'interaction', enabled: true },
  { id: 'context-compact', label: 'Context Compacting', group: 'system', enabled: false },
  { id: 'token-limit', label: 'Token Limit Warning', group: 'system', enabled: true },
  { id: 'boot', label: 'Boot Sound', group: 'system', enabled: true },
]

const defaultSoundRules: Record<string, SoundRule> = {
  'session-start': { enabled: false, sound: 'builtin:hero' },
  'session-end': { enabled: true, sound: 'default' },
  'session-error': { enabled: true, sound: 'builtin:basso' },
  'permission-request': { enabled: true, sound: 'builtin:chime' },
  'plan-approval': { enabled: true, sound: 'builtin:submarine' },
  'question-asked': { enabled: true, sound: 'builtin:pop' },
  'task-complete': { enabled: true, sound: 'builtin:glass' },
  'context-compact': { enabled: false, sound: 'builtin:sosumi' },
  'token-limit': { enabled: true, sound: 'builtin:sosumi' },
  'boot': { enabled: true, sound: 'builtin:hey-bro' },
}

const DEFAULT_GLOBAL_APPROVE_SHORTCUT = 'CommandOrControl+Shift+A'
const DEFAULT_GLOBAL_DENY_SHORTCUT = 'CommandOrControl+Shift+D'
const DEFAULT_TOGGLE_PANEL_SHORTCUT = 'CommandOrControl+Shift+I'
const DEFAULT_OPEN_SETTINGS_SHORTCUT = 'CommandOrControl+,'
const LEGACY_IN_WINDOW_APPROVE_SHORTCUT = '⌘+Enter'
const LEGACY_IN_WINDOW_REJECT_SHORTCUT = '⌘+Backspace'
const LEGACY_TOGGLE_PANEL_SHORTCUT = '⌘+Shift+I'
const LEGACY_OPEN_SETTINGS_SHORTCUT = '⌘+,'
const LEGACY_EXPAND_PANEL_SHORTCUT = '⌘+Shift+E'
const LEGACY_NEXT_SESSION_SHORTCUT = '⌘+]'
const LEGACY_PREV_SESSION_SHORTCUT = '⌘+['

const defaultShortcuts: ShortcutBinding[] = [
  { action: 'toggle-panel', label: 'Toggle Panel', keys: DEFAULT_TOGGLE_PANEL_SHORTCUT },
  { action: 'expand-panel', label: 'Expand Panel', keys: '' },
  { action: 'collapse-panel', label: 'Collapse Panel', keys: 'Escape' },
  { action: 'next-session', label: 'Next Session', keys: '' },
  { action: 'prev-session', label: 'Previous Session', keys: '' },
  { action: 'approve-action', label: 'Approve Action', keys: '' },
  { action: 'reject-action', label: 'Reject Action', keys: '' },
  { action: 'open-settings', label: 'Open Settings', keys: DEFAULT_OPEN_SETTINGS_SHORTCUT },
]

function migrateInWindowPermissionShortcuts(shortcuts: ShortcutBinding[]): ShortcutBinding[] {
  return shortcuts.map((shortcut) => {
    if (shortcut.action === 'approve-action' && shortcut.keys === LEGACY_IN_WINDOW_APPROVE_SHORTCUT) {
      return { ...shortcut, keys: '' }
    }
    if (shortcut.action === 'reject-action' && shortcut.keys === LEGACY_IN_WINDOW_REJECT_SHORTCUT) {
      return { ...shortcut, keys: '' }
    }
    return shortcut
  })
}

function migrateInWindowShortcutSurfaceDefaults(shortcuts: ShortcutBinding[]): ShortcutBinding[] {
  return shortcuts.map((shortcut) => {
    if (shortcut.action === 'expand-panel' && shortcut.keys === LEGACY_EXPAND_PANEL_SHORTCUT) {
      return { ...shortcut, keys: '' }
    }
    if (shortcut.action === 'next-session' && shortcut.keys === LEGACY_NEXT_SESSION_SHORTCUT) {
      return { ...shortcut, keys: '' }
    }
    if (shortcut.action === 'prev-session' && shortcut.keys === LEGACY_PREV_SESSION_SHORTCUT) {
      return { ...shortcut, keys: '' }
    }
    return shortcut
  })
}

function migrateInWindowShortcutPlatformDefaults(shortcuts: ShortcutBinding[]): ShortcutBinding[] {
  return shortcuts.map((shortcut) => {
    if (shortcut.action === 'toggle-panel' && shortcut.keys === LEGACY_TOGGLE_PANEL_SHORTCUT) {
      return { ...shortcut, keys: DEFAULT_TOGGLE_PANEL_SHORTCUT }
    }
    if (shortcut.action === 'open-settings' && shortcut.keys === LEGACY_OPEN_SETTINGS_SHORTCUT) {
      return { ...shortcut, keys: DEFAULT_OPEN_SETTINGS_SHORTCUT }
    }
    return shortcut
  })
}

const defaultLabFeatures: LabFeature[] = [
  { id: 'streaming-diff', label: 'Streaming Diff View', description: 'Show file changes as they stream in real-time', enabled: false },
  { id: 'ai-summary', label: 'AI Session Summary', description: 'Auto-generate natural language summaries of completed sessions', enabled: false },
  { id: 'multi-monitor', label: 'Multi-Monitor Support', description: 'Show island on multiple displays simultaneously', enabled: false },
]

function createIslandDefaults(): Partial<ConfigState> {
  return {
    displayMonitor: 'auto',
    autoHideNoSessions: false,
    smartSuppression: true,
    autoCollapse: true,
    dwellDuration: 300,
    taskCompleteDwellSeconds: 6,
    notchStyle: 'compact',
    hoverSpeed: 'normal',
    islandAnimationScale: 1,
    contentFontSize: '13px',
    completionCardHeight: 200,
    maxPanelHeight: 600,
    detailPanelMaxHeight: 500,
    showAgentActivityDetails: true,
    showUsageQuota: true,
    usageQueryEnabled: true,
    tokenDisplayMode: 'both',
    soundEnabled: true,
    volume: 70,
    soundEvents: defaultSoundEvents.map((event) => ({ ...event })),
    soundRules: Object.fromEntries(Object.entries(defaultSoundRules).map(([id, rule]) => [id, { ...rule }])),
    customSounds: [],
    probeSessionFilter: false,
    soundPack: 'synth',
    bootSoundDefaultMigrated: true,
    islandEnabled: true,
    islandExternalEnabled: true,
    islandMonitorSubagents: true,
    tipsEnabled: true,
    pixelCursorEnabled: true,
    confettiEnabled: true,
    islandSurfaceMode: 'island',
    petVitalsEnabled: true,
    petVitalsDebugOpen: false,
    islandPetScale: 72,
    islandPetWindowOrigin: null,
    islandPetWindowAnchor: null,
    islandActivePetId: null,
    islandAgentPetMap: {},
    followFocus: false,
    globalShortcut: 'CommandOrControl+Shift+I',
    shortcutApprove: DEFAULT_GLOBAL_APPROVE_SHORTCUT,
    shortcutApproveEnabled: false,
    shortcutDeny: DEFAULT_GLOBAL_DENY_SHORTCUT,
    shortcutDenyEnabled: false,
    shortcutSkip: 'CommandOrControl+Shift+S',
    shortcutSkipEnabled: false,
    permissionShortcutDefaultsMigrated: true,
    shortcutSurfaceDefaultsMigrated: true,
    shortcutPlatformDefaultsMigrated: true,
    shortcuts: defaultShortcuts.map((shortcut) => ({ ...shortcut })),
    quietHours: { enabled: false, start: '22:00', end: '08:00' },
    idleTimeoutMinutes: 5,
    idleInteractionRoutingEnabled: false,
    idleInteractionRoutingMinutes: 5,
    allowHorizontalDrag: true,
    panelHorizontalOffset: 0,
    collapsedWidthScale: 100,
    microPillWidth: 120,
    compactPillWidth: 260,
    panelMaxWidth: 630,
    notchHeightMode: 'matchNotch',
    customNotchHeight: CUSTOM_NOTCH_HEIGHT_DEFAULT,
    excludedHookCwdSubstrings: '',
    sessionSilenceRules: [],
    autoApproveTools: [
      'TaskCreate', 'TaskUpdate', 'TaskGet', 'TaskList',
      'TaskOutput', 'TaskStop', 'TodoRead', 'TodoWrite', 'EnterPlanMode', 'ExitPlanMode', 'Read',
    ],
    hapticOnHover: false,
    hapticIntensity: 1,
    carouselIntervalMs: 3000,
    processingTimeoutSecs: 60,
    maxVisibleSessions: 5,
    showToolStatus: true,
    sessionTimeoutMinutes: 30,
    aiMessageLines: 1,
    clickToDetail: true,
    jumpBeforeSend: true,
    showCacheTTL: true,
    hoverExpandDelay: 50,
    microHoverExpandDelay: 50,
    collapseDelay: 200,
    idleCompactDwellSeconds: 8,
    noSessionsHideDelay: 10,
    escSilenceDuration: 30,
    interactionMode: 'persistent',
  }
}

export const useConfigStore = create<ConfigStore>()(
  persist(
    (set) => ({
  // General
  launchAtLogin: false,
  displayMonitor: 'auto',
  autoHide: false,
  autoHideNoSessions: false,
  smartSuppression: true,
  autoCollapse: true,
  completionPopupDuration: '5s',
  dwellDuration: 300,
  taskCompleteDwellSeconds: 6,
  agentHooks: defaultAgentHooks,

  // Display
  notchStyle: 'compact',
  hoverSpeed: 'normal',
  islandAnimationScale: 1,
  contentFontSize: '13px',
  completionCardHeight: 200,
  maxPanelHeight: 600,
  detailPanelMaxHeight: 500,
  showAgentActivityDetails: true,

  // Token & Cost
  tokenDisplayMode: 'both',
  costModel: 'claude-sonnet-4',

  // Sound
  soundEnabled: true,
  volume: 70,
  soundEvents: defaultSoundEvents,
  soundRules: defaultSoundRules,
  customSounds: [],
  probeSessionFilter: false,
  soundPack: 'synth',
  bootSoundDefaultMigrated: true,

  // Display toggles
  islandEnabled: true,
  islandExternalEnabled: true,
  islandMonitorSubagents: true,
  tipsEnabled: true,
  pixelCursorEnabled: true,
  confettiEnabled: true,
  analyticsEnabled: true,
  analyticsConsentPromptCompleted: false,
  islandSurfaceMode: 'island',
  petVitalsEnabled: true,
  petVitalsDebugOpen: false,
  islandPetScale: 72,
  islandPetWindowOrigin: null,
  islandPetWindowAnchor: null,
  islandActivePetId: null,
  islandAgentPetMap: {},

  // General extras
  followFocus: false,
  globalShortcut: 'CommandOrControl+Shift+I',
  shortcutApprove: DEFAULT_GLOBAL_APPROVE_SHORTCUT,
  shortcutApproveEnabled: false,
  shortcutDeny: DEFAULT_GLOBAL_DENY_SHORTCUT,
  shortcutDenyEnabled: false,
  shortcutSkip: 'CommandOrControl+Shift+S',
  shortcutSkipEnabled: false,
  permissionShortcutDefaultsMigrated: true,
  shortcutSurfaceDefaultsMigrated: true,
  shortcutPlatformDefaultsMigrated: true,
  customHooksPath: '',

  // Shortcuts
  shortcuts: defaultShortcuts,

  // SSH Remote
  sshHosts: [],
  tcpPort: 7399,

  // Labs
  labFeatures: defaultLabFeatures,

  // General — UI
  showUsageQuota: true,
  usageQueryEnabled: true,
  codexAppServerSyncEnabled: false,
  codexAppServerSyncIntervalSeconds: 30,

  // Language
  language: (() => {
    const lang = navigator.language.toLowerCase()
    if (lang.startsWith('zh')) return 'zh'
    if (lang.startsWith('ja')) return 'ja'
    if (lang.startsWith('ko')) return 'ko'
    if (lang.startsWith('tr')) return 'tr'
    return 'en'
  })() as 'en' | 'zh' | 'ja' | 'ko' | 'tr',

  // Quiet Hours
  quietHours: { enabled: false, start: '22:00', end: '08:00' },

  // Idle Timeout
  idleTimeoutMinutes: 5,
  idleInteractionRoutingEnabled: false,
  idleInteractionRoutingMinutes: 5,

  // Updates
  autoCheckUpdate: true,
  autoInstallUpdate: false,

  // Notification Mode
  notificationMode: 'turnEnd',

  // Engine Instances
  engineInstances: [],

  // Webhooks
  webhooks: [],

  // Remote Hosts
  remoteHostEntries: [],

  // Panel layout
  allowHorizontalDrag: true,
  panelHorizontalOffset: 0,
  collapsedWidthScale: 100,
  microPillWidth: 120,
  compactPillWidth: 260,
  panelMaxWidth: 630,
  notchHeightMode: 'matchNotch',
  customNotchHeight: CUSTOM_NOTCH_HEIGHT_DEFAULT,

  // Behavior
  excludedHookCwdSubstrings: '',
  sessionSilenceRules: [],
  autoApproveTools: [
    'TaskCreate', 'TaskUpdate', 'TaskGet', 'TaskList',
    'TaskOutput', 'TaskStop', 'TodoRead', 'TodoWrite', 'EnterPlanMode', 'ExitPlanMode', 'Read',
  ],
  hapticOnHover: false,
  hapticIntensity: 1,
  carouselIntervalMs: 3000,
  processingTimeoutSecs: 60,
  maxVisibleSessions: 5,
  showToolStatus: true,
  sessionTimeoutMinutes: 30,

  // Display
  aiMessageLines: 1,

  // AgentBro parity — interaction timing
  clickToDetail: true,
  jumpBeforeSend: true,
  showCacheTTL: true,
  hoverExpandDelay: 50,
  microHoverExpandDelay: 50,
  collapseDelay: 200,
  idleCompactDwellSeconds: 8,
  noSessionsHideDelay: 10,
  escSilenceDuration: 30,
  interactionMode: 'persistent',

  // Actions
  updateConfig: (key, value) => {
    set({ [key]: value } as Partial<ConfigState>)
  },

  resetIslandDefaults: () => {
    set(createIslandDefaults())
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
      soundRules: {
        ...state.soundRules,
        [id]: {
          ...(state.soundRules[id] ?? { enabled: true, sound: 'default' as const }),
          enabled: !(state.soundRules[id]?.enabled ?? state.soundEvents.find((e) => e.id === id)?.enabled ?? true),
        },
      },
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

  addWebhook: (webhook) => {
    set((state) => ({ webhooks: [...state.webhooks, webhook] }))
  },

  removeWebhook: (id) => {
    set((state) => ({ webhooks: state.webhooks.filter((w) => w.id !== id) }))
  },

  updateWebhook: (webhook) => {
    set((state) => ({
      webhooks: state.webhooks.map((w) => (w.id === webhook.id ? webhook : w)),
    }))
  },

  toggleWebhook: (id) => {
    set((state) => ({
      webhooks: state.webhooks.map((w) => (w.id === id ? { ...w, enabled: !w.enabled } : w)),
    }))
  },

  addRemoteHostEntry: (host) => {
    set((state) => ({ remoteHostEntries: [...state.remoteHostEntries, host] }))
  },

  removeRemoteHostEntry: (id) => {
    set((state) => ({ remoteHostEntries: state.remoteHostEntries.filter((h) => h.id !== id) }))
  },

  updateRemoteHostStatus: (id, status) => {
    set((state) => ({
      remoteHostEntries: state.remoteHostEntries.map((h) =>
        h.id === id ? { ...h, connectionStatus: status } : h
      ),
    }))
  },

  addSessionSilenceRule: (rule) => {
    const pattern = rule.pattern.trim()
    if (!pattern) return
    set((state) => {
      const exists = state.sessionSilenceRules.some((item) => (
        item.kind === rule.kind
        && item.pattern.trim().toLowerCase() === pattern.toLowerCase()
      ))
      if (exists) return state
      return {
        sessionSilenceRules: [
          ...state.sessionSilenceRules,
          {
            id: `silence-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
            kind: rule.kind,
            pattern,
            label: rule.label?.trim() || undefined,
            enabled: rule.enabled ?? true,
            createdAt: Date.now(),
          },
        ],
      }
    })
  },

  removeSessionSilenceRule: (id) => {
    set((state) => ({
      sessionSilenceRules: state.sessionSilenceRules.filter((rule) => rule.id !== id),
    }))
  },

  toggleSessionSilenceRule: (id) => {
    set((state) => ({
      sessionSilenceRules: state.sessionSilenceRules.map((rule) => (
        rule.id === id ? { ...rule, enabled: !rule.enabled } : rule
      )),
    }))
  },
    }),
    {
      name: 'agentbro-config',
      merge: (persistedState, currentState) => {
        const persisted = persistedState as Partial<ConfigState> | undefined
        const migratedPermissionShortcuts = persisted?.permissionShortcutDefaultsMigrated === true
        const merged = {
          ...currentState,
          ...persisted,
        }

        if (!migratedPermissionShortcuts) {
          if (merged.shortcutApproveEnabled && merged.shortcutApprove === DEFAULT_GLOBAL_APPROVE_SHORTCUT) {
            merged.shortcutApproveEnabled = false
          }
          if (merged.shortcutDenyEnabled && merged.shortcutDeny === DEFAULT_GLOBAL_DENY_SHORTCUT) {
            merged.shortcutDenyEnabled = false
          }
          merged.shortcuts = migrateInWindowPermissionShortcuts(merged.shortcuts)
        }

        if (persisted?.shortcutSurfaceDefaultsMigrated !== true) {
          merged.shortcuts = migrateInWindowShortcutSurfaceDefaults(merged.shortcuts)
        }

        if (persisted?.shortcutPlatformDefaultsMigrated !== true) {
          merged.shortcuts = migrateInWindowShortcutPlatformDefaults(merged.shortcuts)
        }

        if (persisted?.bootSoundDefaultMigrated !== true) {
          const bootRule = merged.soundRules?.boot
          const canUseHeyBroDefault = !bootRule || bootRule.sound === 'default'
          if (canUseHeyBroDefault) {
            merged.soundRules = {
              ...merged.soundRules,
              boot: {
                enabled: bootRule?.enabled ?? true,
                sound: 'builtin:hey-bro',
              },
            }
          }
        }
        merged.soundRules = normalizeSoundRules(merged.soundRules)

        return {
          ...merged,
          detailPanelMaxHeight: persisted?.detailPanelMaxHeight ?? currentState.detailPanelMaxHeight,
          sessionSilenceRules: Array.isArray(persisted?.sessionSilenceRules) ? persisted.sessionSilenceRules : currentState.sessionSilenceRules,
          permissionShortcutDefaultsMigrated: true,
          shortcutSurfaceDefaultsMigrated: true,
          shortcutPlatformDefaultsMigrated: true,
          bootSoundDefaultMigrated: true,
          notificationMode: 'turnEnd',
        }
      },
    }
  )
)
