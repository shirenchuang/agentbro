/* AgentBro — Tauri IPC API Service
 * Typed wrappers for Tauri commands with graceful browser-dev-mode fallbacks.
 */

import type { SessionState } from '../types/agent'
import type { ThemeConfig } from '../types/theme'

/** Returns true when running inside a Tauri webview. */
export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

/** Lazy invoke — dynamically imports to avoid crash in browser dev mode. */
async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke: tauriInvoke } = await import('@tauri-apps/api/core')
  return tauriInvoke<T>(cmd, args)
}

// ── Backend Types (match Rust serde camelCase output) ────────────

export interface BackendSession {
  id: string
  agentType: string
  engineLabel: string | null
  engineConfigRoot: string | null
  project: string
  cwd: string
  terminal: string
  phase: string
  startedAt: number
  duration: number
  tokens: { input: number; output: number; cacheRead: number; cacheCreate: number }
  rateLimits: {
    fiveHourUsage: number
    fiveHourRemaining: string
    sevenDayUsage: number
    sevenDayRemaining: string
  } | null
  statusLineText: string | null
  contextWindow: {
    totalInputTokens: number
    totalOutputTokens: number
    contextWindowSize: number
    usedPercentage: number | null
  } | null
  lastMainAgentAt: number | null
  cacheTtlMs: number | null
  pendingPermission: {
    toolName: string
    toolInput: string
    diff: string | null
    options: string[] | null
  } | null
  pendingQuestion: {
    question: string
    options: string[]
    descriptions: string[]
    header: string | null
    multiSelect: boolean
    questions: Array<{
      question: string
      header: string | null
      options: Array<{ label: string; description: string | null }>
      multiSelect: boolean
    }>
  } | null
  pendingPlan: {
    title: string
    content: string
    permissions: string[]
  } | null
  lastToolName: string | null
  lastToolTarget: string | null
  lastToolStatus: string | null
  description: string | null
  sessionTitle: string | null
  pid: number | null
  tty: string | null
  termBundleId: string | null
  weztermPane: string | null
  zellijPaneId: string | null
  zellijSessionName: string | null
  cmuxSurfaceId: string | null
  cmuxWorkspaceId: string | null
  subagents: Array<{
    agentId: string
    agentType: string | null
    description: string
    transcriptPath: string | null
    agentTranscriptPath: string | null
    lastAssistantMessage: string | null
    startedAt: number
    completedAt: number | null
    status: string
    tools: string[]
  }>
  activeTools: Array<{
    toolUseId: string
    toolName: string
    status: string
    startedAt: number
    completedAt: number | null
    error: string | null
  }>
  tasks: Array<{
    id: string
    name: string
    status: string
  }>
  isYoloMode: boolean
  lastUserMessage: string | null
  lastResponse: string | null
  lastThought: string | null
}

export interface BackendConfig {
  soundEnabled: boolean
  soundVolume: number
  launchAtLogin: boolean
  autoHide: boolean
  smartSuppression: boolean
  hideInFullscreen: boolean
  completionTimeout: number
  showTokenUsage: boolean
  theme: string
  displayId: string
  autoHideNoSessions: boolean
  soundEvents: Record<string, boolean>
  soundRules: Record<string, { enabled: boolean; sound: string }>
  customSounds: Array<{ id: string; name: string; path: string; dataUrl?: string }>
  soundPack: string
  probeSessionFilter: boolean
  hookDoctorEnabled: boolean
  sessionLauncherEnabled: boolean
  customHookTemplatesEnabled: boolean
  tipsEnabled: boolean
  pixelCursorEnabled: boolean
  confettiEnabled: boolean
  islandSurfaceMode: 'island' | 'pet'
  islandPetScale: number
  islandPetWindowOrigin: { x: number; y: number } | null
  followFocus: boolean
  quietHoursEnabled: boolean
  quietHoursStart: string
  quietHoursEnd: string
  idleTimeoutMinutes: number
  globalShortcut: string
  shortcutApprove: string
  shortcutApproveEnabled: boolean
  shortcutDeny: string
  shortcutDenyEnabled: boolean
  shortcutSkip: string
  shortcutSkipEnabled: boolean
}

export interface BackendDisplayInfo {
  id: string
  name: string
  label: string
  width: number
  height: number
  scaleFactor: number
  isPrimary: boolean
}

export interface BackendAdapterInfo {
  name: string
  displayName: string
  icon: string
  status: string
}

// ── Session Commands ─────────────────────────────────────────────

export async function getSessions(): Promise<BackendSession[]> {
  if (!isTauri()) return []
  return invoke<BackendSession[]>('get_sessions')
}

export async function respondPermission(sessionId: string, allowed: boolean, always?: boolean): Promise<void> {
  if (!isTauri()) {
    console.log(`[mock] respondPermission(${sessionId}, ${allowed}, always=${always})`)
    return
  }
  return invoke('respond_permission', { sessionId, allowed, always: always ?? false })
}

export async function respondQuestion(sessionId: string, answer: string): Promise<void> {
  if (!isTauri()) {
    console.log(`[mock] respondQuestion(${sessionId}, "${answer}")`)
    return
  }
  return invoke('respond_question', { sessionId, answer })
}

export async function respondPlan(sessionId: string, mode: string, message?: string): Promise<void> {
  if (!isTauri()) {
    console.log(`[mock] respondPlan(${sessionId}, "${mode}", "${message ?? ''}")`)
    return
  }
  return invoke('respond_plan', { sessionId, mode, message: message ?? null })
}

export async function respondAutoApprove(sessionId: string): Promise<void> {
  if (!isTauri()) {
    console.log(`[mock] respondAutoApprove(${sessionId})`)
    return
  }
  return invoke('respond_auto_approve', { sessionId })
}

export async function sendMessage(sessionId: string, message: string): Promise<void> {
  if (!isTauri()) {
    console.log(`[mock] sendMessage(${sessionId}, "${message}")`)
    return
  }
  return invoke('send_message', { sessionId, message })
}

export async function jumpToTerminal(sessionId: string): Promise<void> {
  if (!isTauri()) {
    console.log(`[mock] jumpToTerminal(${sessionId})`)
    return
  }
  return invoke('jump_to_terminal', { sessionId })
}

// ── Config Commands ──────────────────────────────────────────────

export async function getConfig(): Promise<BackendConfig> {
  if (!isTauri()) {
    return {
      soundEnabled: true,
      soundVolume: 0.7,
      launchAtLogin: false,
      autoHide: true,
      smartSuppression: true,
      hideInFullscreen: false,
      completionTimeout: 5,
      showTokenUsage: true,
      theme: 'system',
      displayId: 'primary',
      autoHideNoSessions: false,
      soundEvents: {},
      soundRules: {},
      customSounds: [],
      soundPack: 'synth',
      probeSessionFilter: false,
      hookDoctorEnabled: false,
      sessionLauncherEnabled: false,
      customHookTemplatesEnabled: false,
      tipsEnabled: true,
      pixelCursorEnabled: true,
      confettiEnabled: true,
      islandSurfaceMode: 'island',
      islandPetScale: 72,
      islandPetWindowOrigin: null,
      followFocus: false,
      quietHoursEnabled: false,
      quietHoursStart: '22:00',
      quietHoursEnd: '08:00',
      idleTimeoutMinutes: 5,
      globalShortcut: 'CommandOrControl+Shift+I',
      shortcutApprove: 'CommandOrControl+Shift+A',
      shortcutApproveEnabled: true,
      shortcutDeny: 'CommandOrControl+Shift+D',
      shortcutDenyEnabled: true,
      shortcutSkip: 'CommandOrControl+Shift+S',
      shortcutSkipEnabled: false,
    }
  }
  return invoke<BackendConfig>('get_config')
}

export async function updateConfig(config: BackendConfig): Promise<void> {
  if (!isTauri()) {
    console.log('[mock] updateConfig:', config)
    return
  }
  return invoke('update_config', { config })
}

export async function setLaunchAtLogin(enabled: boolean): Promise<void> {
  if (!isTauri()) {
    console.log(`[mock] setLaunchAtLogin(${enabled})`)
    return
  }
  return invoke('set_launch_at_login', { enabled })
}

export interface CustomHookTemplate {
  id: string
  label: string
  agent: string
  configPath: string
  format: 'json' | 'yaml' | 'yml' | 'toml'
  events: string[]
  command: string
  enabled: boolean
}

export async function isFrontmostAppFullscreen(): Promise<boolean> {
  if (!isTauri()) return false
  return invoke<boolean>('is_frontmost_app_fullscreen')
}

export async function listCustomHookTemplates(): Promise<CustomHookTemplate[]> {
  if (!isTauri()) return []
  return invoke<CustomHookTemplate[]>('list_custom_hook_templates')
}

export async function upsertCustomHookTemplate(template: CustomHookTemplate): Promise<CustomHookTemplate[]> {
  if (!isTauri()) return [template]
  return invoke<CustomHookTemplate[]>('upsert_custom_hook_template', { template })
}

export async function removeCustomHookTemplate(id: string): Promise<CustomHookTemplate[]> {
  if (!isTauri()) return []
  return invoke<CustomHookTemplate[]>('remove_custom_hook_template', { id })
}

export async function installCustomHookTemplate(template: CustomHookTemplate): Promise<void> {
  if (!isTauri()) return
  return invoke('install_custom_hook_template', { template })
}

export async function removeCustomHookTemplateHooks(template: CustomHookTemplate): Promise<void> {
  if (!isTauri()) return
  return invoke('remove_custom_hook_template_hooks', { template })
}

export interface HookDoctorCheck {
  id: string
  label: string
  status: 'ok' | 'warn' | 'error'
  detail: string
}

export interface HookDoctorReport {
  generatedAt: number
  checks: HookDoctorCheck[]
}

export interface LaunchAgentSessionRequest {
  agentId: string
  cwd: string
  terminal: string
  extraArgs: string
}

export async function runHookDoctor(): Promise<HookDoctorReport> {
  if (!isTauri()) {
    return {
      generatedAt: Math.floor(Date.now() / 1000),
      checks: [
        { id: 'browser', label: 'Browser mode', status: 'warn', detail: 'Hook Doctor runs inside the Tauri app.' },
      ],
    }
  }
  return invoke<HookDoctorReport>('run_hook_doctor')
}

export async function launchAgentSession(request: LaunchAgentSessionRequest): Promise<void> {
  if (!isTauri()) {
    console.log('[mock] launchAgentSession:', request)
    return
  }
  return invoke('launch_agent_session', { request })
}

export interface BuddyDeviceConfig {
  enabled: boolean
  transport: string
  port: number
  sharedSecret: string
}

export interface BuddyDeviceSnapshot {
  protocolVersion: number
  sessions: Array<{
    id: string
    agentType: string
    sourceSlot: number
    project: string
    phase: string
    statusCode: number
    needsAttention: boolean
    lastToolName?: string | null
  }>
  focusAction: string
}

export async function buddyDeviceSnapshot(): Promise<BuddyDeviceSnapshot> {
  if (!isTauri()) return { protocolVersion: 1, sessions: [], focusAction: 'buddy_reverse_focus' }
  return invoke<BuddyDeviceSnapshot>('buddy_device_snapshot')
}

export async function getBuddyDeviceConfig(): Promise<BuddyDeviceConfig> {
  if (!isTauri()) return { enabled: false, transport: 'http', port: 17893, sharedSecret: '' }
  return invoke<BuddyDeviceConfig>('get_buddy_device_config')
}

export async function setBuddyDeviceConfig(config: BuddyDeviceConfig): Promise<BuddyDeviceConfig> {
  if (!isTauri()) return config
  return invoke<BuddyDeviceConfig>('set_buddy_device_config', { config })
}

export async function buddyReverseFocus(sessionId: string): Promise<void> {
  if (!isTauri()) return
  return invoke('buddy_reverse_focus', { sessionId })
}

// ── Theme Commands ──────────────────────────────────────────────

export async function listThemes(): Promise<ThemeConfig[]> {
  if (!isTauri()) return []
  return invoke<ThemeConfig[]>('list_themes')
}

export async function getActiveThemeBundle(name: string): Promise<ThemeConfig> {
  if (!isTauri()) {
    throw new Error(`Theme '${name}' not available in browser dev mode`)
  }
  return invoke<ThemeConfig>('get_active_theme_bundle', { name })
}

export async function setActiveBackendTheme(name: string): Promise<void> {
  if (!isTauri()) return
  window.dispatchEvent(new CustomEvent('agentbro-theme-sync', { detail: { status: 'pending', name } }))
  try {
    return await invoke('set_active_theme', { name })
  } catch (error) {
    window.dispatchEvent(new CustomEvent('agentbro-theme-sync', { detail: { status: 'failed', name } }))
    throw error
  }
}

// ── Hook Management ──────────────────────────────────────────────

export async function installHooks(agent: string): Promise<void> {
  if (!isTauri()) {
    console.log(`[mock] installHooks(${agent})`)
    return
  }
  return invoke('install_hooks', { agent })
}

export async function removeHooks(agent: string): Promise<void> {
  if (!isTauri()) {
    console.log(`[mock] removeHooks(${agent})`)
    return
  }
  return invoke('remove_hooks', { agent })
}

export async function getAdapterStatus(): Promise<BackendAdapterInfo[]> {
  if (!isTauri()) return []
  return invoke<BackendAdapterInfo[]>('get_adapter_status')
}

export type HookVerificationResult = 'ok' | 'needs_reinstall' | 'settings_corrupted'

export async function verifyHooks(agent: string): Promise<HookVerificationResult> {
  if (!isTauri()) return 'ok'
  return invoke<HookVerificationResult>('verify_hooks', { agent })
}

// ── Sound Commands ──────────────────────────────────────────

export async function setSoundVolume(volume: number): Promise<void> {
  if (!isTauri()) return
  return invoke('set_sound_volume', { volume })
}

export async function setSoundEnabled(enabled: boolean): Promise<void> {
  if (!isTauri()) return
  return invoke('set_sound_enabled', { enabled })
}

export async function setSoundPack(pack: string): Promise<void> {
  if (!isTauri()) return
  return invoke('set_sound_pack', { pack })
}

export async function setProbeSessionFilter(enabled: boolean): Promise<void> {
  if (!isTauri()) return
  return invoke('set_probe_session_filter', { enabled })
}

export async function setIslandFeatureFlags(options: {
  tipsEnabled: boolean
  pixelCursorEnabled: boolean
  confettiEnabled: boolean
  followFocus: boolean
}): Promise<void> {
  if (!isTauri()) return
  return invoke('set_island_feature_flags', options)
}

export async function setIslandSurfaceOptions(options: {
  islandSurfaceMode: 'island' | 'pet'
  islandPetScale: number
}): Promise<void> {
  if (!isTauri()) return
  return invoke('set_island_surface_options', {
    islandSurfaceMode: options.islandSurfaceMode,
    islandPetScale: options.islandPetScale,
  })
}

export async function setAdvancedToolFlags(options: {
  hookDoctorEnabled: boolean
  sessionLauncherEnabled: boolean
  customHookTemplatesEnabled: boolean
}): Promise<void> {
  if (!isTauri()) return
  return invoke('set_advanced_tool_flags', options)
}

export async function setSoundQuietHours(enabled: boolean, start: string, end: string): Promise<void> {
  if (!isTauri()) return
  return invoke('set_sound_quiet_hours', { enabled, start, end })
}

export async function setSoundEventEnabled(eventId: string, enabled: boolean): Promise<void> {
  if (!isTauri()) return
  return invoke('set_sound_event_enabled', { eventId, enabled })
}

export async function setSoundEventRule(eventId: string, enabled: boolean, sound: string): Promise<void> {
  if (!isTauri()) return
  return invoke('set_sound_event_rule', { eventId, enabled, sound })
}

export async function importCustomSound(filePath: string): Promise<{ id: string; name: string; path: string; dataUrl?: string }> {
  if (!isTauri()) {
    const normalized = filePath.replace(/\\/g, '/')
    return {
      id: `${Date.now()}`,
      name: normalized.split('/').pop() || 'Custom sound',
      path: filePath,
    }
  }
  return invoke('import_custom_sound', { filePath })
}

export async function setCustomSounds(sounds: Array<{ id: string; name: string; path: string; dataUrl?: string }>): Promise<void> {
  if (!isTauri()) return
  return invoke('set_custom_sounds', { sounds })
}

export async function setGlobalActionShortcuts(options: {
  approve: string
  approveEnabled: boolean
  deny: string
  denyEnabled: boolean
  skip: string
  skipEnabled: boolean
}): Promise<void> {
  if (!isTauri()) return
  return invoke('set_global_action_shortcuts', { shortcuts: options })
}

// ── Chat History Commands ────────────────────────────────────────

/** Parsed message block from Rust ConversationParser */
export type ParsedMessageBlock =
  | { type: 'text'; text: string }
  | { type: 'tool_use'; id: string; name: string; input: Record<string, string> }
  | { type: 'tool_result'; toolUseId: string; content: string | null; isError: boolean }
  | { type: 'thinking'; thinking: string }
  | { type: 'image'; source: string }
  | { type: 'interrupted' }

/** Parsed message from Rust ConversationParser */
export interface ParsedMessage {
  id: string
  role: 'user' | 'assistant'
  timestamp: string | null
  blocks: ParsedMessageBlock[]
}

/** Request a full parse of a session's JSONL conversation file. */
export async function getChatHistory(sessionId: string): Promise<ParsedMessage[]> {
  if (!isTauri()) return []
  return invoke<ParsedMessage[]>('get_chat_history', { sessionId })
}

/** Request a full parse of a completed subagent transcript JSONL file. */
export async function getSubagentChatHistory(sessionId: string, transcriptPath: string): Promise<ParsedMessage[]> {
  if (!isTauri()) return []
  return invoke<ParsedMessage[]>('get_subagent_chat_history', { sessionId, transcriptPath })
}

export async function openImage(src: string): Promise<void> {
  if (!isTauri()) {
    window.open(src, '_blank', 'noopener,noreferrer')
    return
  }
  return invoke('open_image', { src })
}

// ── Window Management ───────────────────────────────────────────

export type ResizeNotchResult = {
  anchorOffsetX: number
}

export async function resizeNotch(width: number, height: number, horizontalOffset = 0, displayId?: string): Promise<ResizeNotchResult> {
  if (!isTauri()) return { anchorOffsetX: 0 }
  return invoke<ResizeNotchResult>('resize_notch', { width, height, horizontalOffset, displayId })
}

/**
 * Set notch window opacity (0.0 = invisible, 1.0 = fully visible).
 * Uses opacity instead of hide/show because macOS breaks transparent
 * window compositing after a hide()/show() cycle.
 */
export async function setNotchOpacity(opacity: number): Promise<void> {
  if (!isTauri()) return
  return invoke('set_notch_opacity', { opacity })
}

// ── Window Focus Commands ───────────────────────────────────────

export async function setNotchFocusable(focusable: boolean): Promise<void> {
  if (!isTauri()) return
  return invoke('set_notch_focusable', { focusable })
}

export async function setNotchIgnoreCursorEvents(ignore: boolean): Promise<void> {
  if (!isTauri()) return
  return invoke('set_notch_ignore_cursor_events', { ignore })
}

export async function openSettingsWindow(): Promise<void> {
  if (!isTauri()) return
  return invoke('open_settings_window')
}

// ── Suppression Commands ────────────────────────────────────────

/** Check if a session's terminal is currently focused (smart suppression). */
export async function shouldSuppress(sessionId: string): Promise<boolean> {
  if (!isTauri()) return false
  return invoke<boolean>('should_suppress', { sessionId })
}

/** Check if a session's owning terminal tab/window is currently focused. */
export async function isTerminalFocused(sessionId: string): Promise<boolean> {
  if (!isTauri()) return false
  return invoke<boolean>('is_terminal_focused', { sessionId })
}

/** Get current cursor screen coordinates (macOS only). */
export async function getCursorPosition(): Promise<[number, number]> {
  if (!isTauri()) return [0, 0]
  return invoke<[number, number]>('get_cursor_position')
}

/** Native fallback for transparent-window hover hit testing. */
export async function isCursorOverNotch(width?: number, height?: number): Promise<boolean> {
  if (!isTauri()) return false
  return invoke<boolean>('is_cursor_over_notch', { width, height })
}

// ── Display Commands ────────────────────────────────────────────

export async function listDisplays(): Promise<BackendDisplayInfo[]> {
  if (!isTauri()) return []
  return invoke<BackendDisplayInfo[]>('list_displays')
}

export async function setDisplayId(displayId: string): Promise<void> {
  if (!isTauri()) return
  return invoke('set_display_id', { displayId })
}

export async function repositionNotch(displayId?: string, horizontalOffset?: number): Promise<void> {
  if (!isTauri()) return
  return invoke('reposition_notch', { displayId, horizontalOffset })
}

export type IslandLayoutPreviewMode = 'micro' | 'compact' | 'expanded' | 'completion'

export interface IslandLayoutPreviewOptions {
  collapsedWidthScale?: number
  microPillWidth?: number
  compactPillWidth?: number
  panelMaxWidth?: number
  notchHeightMode?: 'matchNotch' | 'matchMenuBar' | 'custom'
  customNotchHeight?: number
  contentFontSize?: string
  completionCardHeight?: number
  maxPanelHeight?: number
}

export async function previewIslandLayout(mode: IslandLayoutPreviewMode, options?: IslandLayoutPreviewOptions): Promise<void> {
  if (!isTauri()) return
  return invoke('preview_island_layout', { mode, options: options ?? null })
}

export async function clearIslandLayoutPreview(): Promise<void> {
  if (!isTauri()) return
  return invoke('clear_island_layout_preview')
}

export async function startNotchDrag(
  horizontalOffset: number,
  width: number,
  height: number,
  displayId?: string,
): Promise<boolean> {
  if (!isTauri()) return false
  return invoke<boolean>('start_notch_drag', { horizontalOffset, width, height, displayId })
}

export async function endNotchDrag(): Promise<number | null> {
  if (!isTauri()) return null
  return invoke<number | null>('end_notch_drag')
}

export async function startPetDrag(): Promise<boolean> {
  if (!isTauri()) return false
  return invoke<boolean>('start_pet_drag')
}

export async function endPetDrag(): Promise<{ x: number; y: number } | null> {
  if (!isTauri()) return null
  return invoke<{ x: number; y: number } | null>('end_pet_drag')
}

// ── License Commands ────────────────────────────────────────────

export type BackendLicenseStatus =
  | { type: 'active'; licenseKey: string; deviceId: string }
  | { type: 'trial'; daysRemaining: number; deviceId: string }
  | { type: 'trialExpired'; deviceId: string }
  | { type: 'invalid'; reason: string }
  | { type: 'offlineGrace'; daysRemaining: number; licenseKey: string }

export async function getLicenseStatus(): Promise<BackendLicenseStatus> {
  if (!isTauri()) {
    return { type: 'trial', daysRemaining: 12, deviceId: 'browser-dev' }
  }
  return invoke<BackendLicenseStatus>('get_license_status')
}

export async function activateLicense(key: string): Promise<BackendLicenseStatus> {
  if (!isTauri()) {
    return { type: 'active', licenseKey: key, deviceId: 'browser-dev' }
  }
  return invoke<BackendLicenseStatus>('activate_license', { licenseKey: key })
}

export async function deactivateLicense(): Promise<BackendLicenseStatus> {
  if (!isTauri()) {
    return { type: 'trial', daysRemaining: 12, deviceId: 'browser-dev' }
  }
  return invoke<BackendLicenseStatus>('deactivate_license')
}

// ── Diagnostics Commands ────────────────────────────────────────

export async function exportDiagnostics(): Promise<string> {
  if (!isTauri()) {
    return JSON.stringify({
      appVersion: '0.1.0-alpha',
      os: navigator.platform,
      userAgent: navigator.userAgent,
      timestamp: new Date().toISOString(),
    }, null, 2)
  }
  return invoke<string>('export_diagnostics')
}

// ── App Lifecycle ──────────────────────────────────────────

// ── Persistence Commands ─────────────────────────────────────────

export async function saveSessions(sessions: SessionState[]): Promise<void> {
  if (!isTauri()) {
    console.log('[mock] saveSessions:', sessions.length, 'sessions')
    return
  }
  return invoke('save_sessions', { sessionsJson: JSON.stringify(sessions) })
}

export async function loadSessions(): Promise<SessionState[]> {
  if (!isTauri()) return []
  try {
    const data = await invoke<string>('load_sessions')
    return data ? JSON.parse(data) : []
  } catch (err) {
    console.warn('Failed to parse loaded sessions:', err)
    return []
  }
}

// ── App Lifecycle ────────────────────────────────────────────────

export async function quitApp(): Promise<void> {
  if (!isTauri()) {
    window.close()
    return
  }
  try {
    await invoke('quit_app')
  } catch {
    window.close()
  }
}

// ── Engine Instance Commands ────────────────────────────────────

export interface BackendEngineInstance {
  id: string
  label: string
  configRoot: string
  enabled: boolean
}

export async function addEngineInstance(label: string, configRoot: string): Promise<BackendEngineInstance> {
  if (!isTauri()) throw new Error('Not in Tauri')
  return await invoke<BackendEngineInstance>('add_engine_instance', { label, configRoot })
}

export async function removeEngineInstance(id: string): Promise<void> {
  if (!isTauri()) return
  await invoke('remove_engine_instance', { id })
}

export async function setEngineInstanceEnabled(id: string, enabled: boolean): Promise<void> {
  if (!isTauri()) return
  await invoke('set_engine_instance_enabled', { id, enabled })
}

export async function verifyEnginePath(path: string): Promise<boolean> {
  if (!isTauri()) return false
  return await invoke<boolean>('verify_engine_path', { path })
}

// ── Agent Detection & Hook Management ────────────────────────────

export interface DetectedTool {
  name: string
  displayName: string
  status: 'Active' | 'Installed' | 'Available' | 'Unavailable'
  binaryPath: string | null
  configDir: string | null
}

export type HookEventCategory = 'approvals' | 'notifications' | 'lifecycle' | 'activity'

export interface HookEventStatus {
  name: string
  category: HookEventCategory
  categoryTitle: string
  categorySubtitle: string
  timeout?: number | null
  enabled: boolean
}

export interface HookStatus {
  toolId?: string
  adapterId?: string
  profileId?: string
  name: string
  displayName: string
  installed: boolean
  installStatus?: 'installed' | 'not_installed' | 'error' | string
  configPath?: string
  configDir?: string
  status: string
  supportsEventSelection?: boolean
  events?: HookEventStatus[]
  enabledEventNames?: string[]
}

export async function detectTools(): Promise<DetectedTool[]> {
  if (!isTauri()) return []
  return invoke<DetectedTool[]>('detect_tools')
}

export async function installAgentHook(toolName: string): Promise<void> {
  if (!isTauri()) return
  return invoke('install_agent_hook', { toolName })
}

export async function uninstallAgentHook(toolName: string): Promise<void> {
  if (!isTauri()) return
  return invoke('uninstall_agent_hook', { toolName })
}

export async function configureAgentHookEvents(toolName: string, enabledEvents: string[]): Promise<void> {
  if (!isTauri()) return
  return invoke('configure_agent_hook_events', { toolName, enabledEvents })
}

export async function simulateHookEvent(eventName: string, toolName?: string): Promise<void> {
  if (!isTauri()) return
  return invoke('simulate_hook_event', { eventName, toolName })
}

export async function simulateHookLifecycle(): Promise<void> {
  if (!isTauri()) return
  for (const eventName of ['SessionStart', 'UserPromptSubmit', 'PreToolUse', 'PostToolUse', 'Stop']) {
    await simulateHookEvent(eventName)
  }
}

export async function getAllHookStatus(): Promise<HookStatus[]> {
  if (!isTauri()) return []
  return invoke<HookStatus[]>('get_all_hook_status')
}

export async function reinstallAllHooks(): Promise<string[]> {
  if (!isTauri()) return []
  return invoke<string[]>('reinstall_all_hooks')
}

// ── Remote SSH Management ────────────────────────────────────────

export interface RemoteHost {
  id: string
  name: string
  sshTarget: string
  port: number | null
  identityFile: string | null
  authSocket: string | null
  remoteSocketPath: string
  autoConnect: boolean
}

export type ConnectionStatus =
  | { state: 'disconnected' }
  | { state: 'connecting' }
  | { state: 'connected' }
  | { state: 'failed'; message: string }

export interface SshConfigHost {
  name: string
  hostname: string | null
  user: string | null
  port: number | null
  identityFile: string | null
}

export async function listRemoteHosts(): Promise<RemoteHost[]> {
  if (!isTauri()) return []
  return invoke<RemoteHost[]>('list_remote_hosts')
}

export async function addRemoteHost(host: RemoteHost): Promise<void> {
  if (!isTauri()) return
  return invoke('add_remote_host', { host })
}

export async function removeRemoteHost(id: string): Promise<void> {
  if (!isTauri()) return
  return invoke('remove_remote_host', { id })
}

export async function connectRemote(id: string): Promise<void> {
  if (!isTauri()) return
  return invoke('connect_remote', { id })
}

export async function disconnectRemote(id: string): Promise<void> {
  if (!isTauri()) return
  return invoke('disconnect_remote', { id })
}

export async function installRemoteHooks(id: string): Promise<string> {
  if (!isTauri()) return 'ok'
  return invoke<string>('install_remote_hooks', { id })
}

export async function getRemoteStatus(id: string): Promise<ConnectionStatus> {
  if (!isTauri()) return { state: 'disconnected' }
  return invoke<ConnectionStatus>('get_remote_status', { id })
}

export async function listSshConfigHosts(): Promise<SshConfigHost[]> {
  if (!isTauri()) return []
  return invoke<SshConfigHost[]>('list_ssh_config_hosts')
}

// ── Webhook Management ───────────────────────────────────────────

export interface WebhookConfig {
  id: string
  name: string
  platform: 'dingtalk' | 'feishu'
  url: string
  secret: string | null
  sources: string[]
  enabled: boolean
}

export async function listWebhooks(): Promise<WebhookConfig[]> {
  if (!isTauri()) return []
  return invoke<WebhookConfig[]>('list_webhooks')
}

export async function addWebhook(config: WebhookConfig): Promise<void> {
  if (!isTauri()) return
  return invoke('add_webhook', { config })
}

export async function removeWebhook(id: string): Promise<void> {
  if (!isTauri()) return
  return invoke('remove_webhook', { id })
}

export async function updateWebhook(config: WebhookConfig): Promise<void> {
  if (!isTauri()) return
  return invoke('update_webhook', { config })
}

export async function testWebhook(id: string): Promise<string> {
  if (!isTauri()) return 'mock: test skipped in browser'
  return invoke<string>('test_webhook', { id })
}

export async function getWebhookLogs(): Promise<DiagnosticEvent[]> {
  if (!isTauri()) return []
  return invoke<DiagnosticEvent[]>('get_webhook_logs')
}

// ── Haptic Feedback ─────────────────────────────────────────────

export async function performHaptic(intensity: number): Promise<void> {
  if (!isTauri()) return
  return invoke('perform_haptic', { intensity })
}

// ── Diagnostic Events ────────────────────────────────────────────

export interface DiagnosticEvent {
  seq: number
  timestampMs: number
  severity: 'debug' | 'info' | 'warning' | 'error'
  component: string
  message: string
  payload: unknown | null
}

export async function getDiagnosticEvents(sinceSeq?: number, component?: string): Promise<DiagnosticEvent[]> {
  if (!isTauri()) return []
  return invoke<DiagnosticEvent[]>('get_diagnostic_events', {
    sinceSeq: sinceSeq ?? null,
    component: component ?? null,
  })
}
