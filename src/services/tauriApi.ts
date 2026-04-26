/* Agent Island — Tauri IPC API Service
 * Typed wrappers for Tauri commands with graceful browser-dev-mode fallbacks.
 */

import type { SessionState } from '../types/agent'

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
  project: string
  cwd: string
  terminal: string
  phase: string
  startedAt: number
  duration: number
  tokens: { input: number; output: number; cacheRead: number; cacheCreate: number }
  pendingPermission: {
    toolName: string
    toolInput: string
    diff: string | null
    options: string[] | null
  } | null
  pendingQuestion: { question: string; options: string[] } | null
  lastToolName: string | null
  lastToolTarget: string | null
  lastToolStatus: string | null
  description: string | null
  sessionTitle: string | null
  pid: number | null
  tty: string | null
}

export interface BackendConfig {
  soundEnabled: boolean
  soundVolume: number
  autoHide: boolean
  smartSuppression: boolean
  completionTimeout: number
  showTokenUsage: boolean
  theme: string
  displayId: string
  autoHideNoSessions: boolean
  hideInFullscreen: boolean
}

export interface BackendDisplayInfo {
  name: string
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
      autoHide: true,
      smartSuppression: true,
      completionTimeout: 5,
      showTokenUsage: true,
      theme: 'system',
      displayId: 'primary',
      autoHideNoSessions: false,
      hideInFullscreen: true,
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

// ── Chat History Commands ────────────────────────────────────────

/** Parsed message block from Rust ConversationParser */
export type ParsedMessageBlock =
  | { type: 'text'; text: string }
  | { type: 'tool_use'; id: string; name: string; input: Record<string, string> }
  | { type: 'tool_result'; toolUseId: string; content: string | null; isError: boolean }
  | { type: 'thinking'; thinking: string }
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

// ── Window Management ───────────────────────────────────────────

export async function resizeNotch(width: number, height: number): Promise<void> {
  if (!isTauri()) return
  return invoke('resize_notch', { width, height })
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

// ── Suppression Commands ────────────────────────────────────────

/** Check if a session's terminal is currently focused (smart suppression). */
export async function shouldSuppress(sessionId: string): Promise<boolean> {
  if (!isTauri()) return false
  return invoke<boolean>('should_suppress', { sessionId })
}

/** Get current cursor screen coordinates (macOS only). */
export async function getCursorPosition(): Promise<[number, number]> {
  if (!isTauri()) return [0, 0]
  return invoke<[number, number]>('get_cursor_position')
}

// ── Display Commands ────────────────────────────────────────────

export async function listDisplays(): Promise<BackendDisplayInfo[]> {
  if (!isTauri()) return []
  return invoke<BackendDisplayInfo[]>('list_displays')
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
    // Fallback: close the window if the quit command isn't registered
    window.close()
  }
}
