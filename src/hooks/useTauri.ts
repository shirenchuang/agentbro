/* Agent Island — Tauri Event Hooks
 * Listens for backend events and syncs stores. No-ops in browser dev mode.
 */
import { useEffect } from 'react'
import { isTauri, getSessions, getConfig } from '../services/tauriApi'
import type { BackendSession, BackendConfig, ParsedMessage, ParsedMessageBlock } from '../services/tauriApi'
import { useSessionStore } from '../stores/sessionStore'
import { useConfigStore } from '../stores/configStore'
import type { SessionState, DiffContent, AgentType, ToolStatus, ChatMessage } from '../types/agent'

// ── Transform Backend → Frontend ─────────────────────────────────

function parseDiff(raw: string | null): DiffContent | undefined {
  if (!raw) return undefined
  try {
    return JSON.parse(raw) as DiffContent
  } catch {
    // Treat as raw unified diff text
    return {
      filePath: 'diff',
      lines: raw.split('\n').map((line, i) => ({
        type: line.startsWith('+') ? 'add' as const
          : line.startsWith('-') ? 'remove' as const
          : 'context' as const,
        lineNumber: i + 1,
        content: line.replace(/^[+-] ?/, ''),
      })),
    }
  }
}

function transformSession(bs: BackendSession): SessionState {
  // Preserve existing chatHistory/subagents/activeTools from the store
  // so that replaceAllSessions doesn't wipe them on each backend update
  const existing = useSessionStore.getState().sessions[bs.id]

  return {
    id: bs.id,
    agentType: bs.agentType as AgentType,
    project: bs.project,
    terminal: bs.terminal,
    phase: bs.phase as SessionState['phase'],
    startedAt: bs.startedAt,
    duration: bs.duration,
    tokens: bs.tokens,
    pendingPermission: bs.pendingPermission ? {
      toolName: bs.pendingPermission.toolName,
      toolInput: bs.pendingPermission.toolInput,
      diff: parseDiff(bs.pendingPermission.diff),
      options: bs.pendingPermission.options ?? undefined,
    } : undefined,
    pendingQuestion: bs.pendingQuestion ?? undefined,
    lastToolName: bs.lastToolName ?? undefined,
    lastToolTarget: bs.lastToolTarget ?? undefined,
    lastToolStatus: (bs.lastToolStatus as ToolStatus) ?? undefined,
    description: bs.description ?? undefined,
    sessionTitle: bs.sessionTitle ?? undefined,
    chatHistory: existing?.chatHistory ?? [],
    subagents: existing?.subagents ?? [],
    activeTools: existing?.activeTools ?? [],
  }
}

function applyBackendConfig(config: BackendConfig) {
  const store = useConfigStore.getState()
  store.updateConfig('soundEnabled', config.soundEnabled)
  store.updateConfig('volume', Math.round(config.soundVolume * 100))
  store.updateConfig('autoHide', config.autoHide)
  store.updateConfig('smartSuppression', config.smartSuppression)
  store.updateConfig('autoHideNoSessions', config.autoHideNoSessions)
  store.updateConfig('hideInFullscreen', config.hideInFullscreen)
  store.updateConfig('displayMonitor', config.displayId)
}

// ── Hooks ────────────────────────────────────────────────────────

/** Listen for session-update events from the backend and sync sessionStore. */
export function useSessionEvents() {
  useEffect(() => {
    if (!isTauri()) return

    let unlisten: (() => void) | undefined

    // Load initial sessions
    getSessions().then(sessions => {
      useSessionStore.getState().replaceAllSessions(sessions.map(transformSession))
    }).catch(e => console.error('[tauri] getSessions:', e))

    // Listen for live updates (dynamic import to avoid crash in browser dev mode)
    import('@tauri-apps/api/event').then(({ listen }) => {
      listen<{ sessions: BackendSession[]; suppressed?: boolean }>('session-update', (event) => {
        const store = useSessionStore.getState()
        store.replaceAllSessions(
          event.payload.sessions.map(transformSession),
        )
        // When suppressed, the backend signals that the user is looking at
        // the terminal — do NOT auto-expand the panel.
        if (event.payload.suppressed) {
          // Revert any auto-expand that replaceAllSessions may have triggered
          // by forcing the panel back to collapsed if it was just expanded
          const current = useSessionStore.getState().panelState
          if (current === 'expanded') {
            store.setPanelState('collapsed')
          }
        }
      }).then(fn => { unlisten = fn })
        .catch(e => console.error('[tauri] listen session-update:', e))
    }).catch(e => console.error('[tauri] import event:', e))

    return () => { unlisten?.() }
  }, [])
}

/** Listen for cursor-in-zone events for hot-zone panel expansion. */
export function useCursorHotzone() {
  const setPanelState = useSessionStore((s) => s.setPanelState)

  useEffect(() => {
    if (!isTauri()) return

    let unlisten: (() => void) | undefined

    import('@tauri-apps/api/event').then(({ listen }) => {
      listen<boolean>('cursor-in-zone', (event) => {
        const inZone = event.payload
        const store = useSessionStore.getState()
        if (inZone) {
          // Cursor entered hot zone — show panel if collapsed
          if (store.panelState === 'collapsed') {
            setPanelState('hover')
          }
        } else {
          // Cursor left hot zone — collapse after brief delay
          // (the mouse-leave handler in NotchPanel handles the actual delay)
          // Only auto-collapse if in hover state (not expanded by user)
          if (store.panelState === 'hover') {
            setTimeout(() => {
              const current = useSessionStore.getState().panelState
              if (current === 'hover') {
                setPanelState('collapsed')
              }
            }, 300)
          }
        }
      }).then(fn => { unlisten = fn })
        .catch(e => console.error('[tauri] listen cursor-in-zone:', e))
    }).catch(e => console.error('[tauri] import event:', e))

    return () => { unlisten?.() }
  }, [setPanelState])
}

/** Listen for fullscreen-changed events to hide/show panel. */
export function useFullscreenDetection() {
  useEffect(() => {
    if (!isTauri()) return

    let unlisten: (() => void) | undefined

    import('@tauri-apps/api/event').then(({ listen }) => {
      listen<boolean>('fullscreen-changed', (event) => {
        const isFullscreen = event.payload
        if (isFullscreen) {
          // Fullscreen entered — collapse panel (backend already handles opacity)
          useSessionStore.getState().setPanelState('collapsed')
        }
      }).then(fn => { unlisten = fn })
        .catch(e => console.error('[tauri] listen fullscreen-changed:', e))
    }).catch(e => console.error('[tauri] import event:', e))

    return () => { unlisten?.() }
  }, [])
}

/** Listen for config-changed events from the backend and sync configStore. */
export function useConfigSync() {
  useEffect(() => {
    if (!isTauri()) return

    let unlisten: (() => void) | undefined

    getConfig().then(applyBackendConfig)
      .catch(e => console.error('[tauri] getConfig:', e))

    // Dynamic import to avoid crash in browser dev mode
    import('@tauri-apps/api/event').then(({ listen }) => {
      listen<BackendConfig>('config-changed', (event) => {
        applyBackendConfig(event.payload)
      }).then(fn => { unlisten = fn })
        .catch(e => console.error('[tauri] listen config-changed:', e))
    }).catch(e => console.error('[tauri] import event:', e))

    return () => { unlisten?.() }
  }, [])
}

// ── Conversation Update Mapping ─────────────────────────────────

/** Map a list of ParsedMessages (from Rust) into ChatMessages (frontend type). */
export function mapParsedMessages(parsed: ParsedMessage[]): ChatMessage[] {
  const messages: ChatMessage[] = []

  for (const msg of parsed) {
    const ts = msg.timestamp ? new Date(msg.timestamp).getTime() : Date.now()

    for (const block of msg.blocks) {
      switch (block.type) {
        case 'text':
          messages.push({
            role: msg.role === 'user' ? 'user' : 'assistant',
            content: (block as Extract<ParsedMessageBlock, { type: 'text' }>).text,
            timestamp: ts,
          })
          break

        case 'tool_use': {
          const tu = block as Extract<ParsedMessageBlock, { type: 'tool_use' }>
          messages.push({
            role: 'tool_use',
            toolName: tu.name,
            toolInput: Object.entries(tu.input).map(([k, v]) => `${k}: ${v}`).join('\n'),
            status: 'success' as ToolStatus,
            timestamp: ts,
          })
          break
        }

        case 'tool_result': {
          const tr = block as Extract<ParsedMessageBlock, { type: 'tool_result' }>
          if (tr.isError && tr.content) {
            messages.push({
              role: 'error',
              message: tr.content,
              timestamp: ts,
            })
          }
          // Non-error tool results are already represented by tool_use entries
          break
        }

        case 'thinking': {
          const th = block as Extract<ParsedMessageBlock, { type: 'thinking' }>
          messages.push({
            role: 'thinking',
            content: th.thinking,
            timestamp: ts,
          })
          break
        }

        case 'interrupted':
          // Add an assistant message indicating interruption
          messages.push({
            role: 'assistant',
            content: '[Request interrupted by user]',
            timestamp: ts,
          })
          break
      }
    }
  }

  return messages
}

/** Listen for conversation-update events from the file watcher and sync chat history. */
export function useConversationUpdates() {
  useEffect(() => {
    if (!isTauri()) return

    let unlisten: (() => void) | undefined

    import('@tauri-apps/api/event').then(({ listen }) => {
      listen<{ sessionId: string; result: { allMessages: ParsedMessage[]; newMessages: ParsedMessage[]; clearDetected: boolean } }>(
        'conversation-update',
        (event) => {
          const { sessionId, result } = event.payload
          const store = useSessionStore.getState()

          // Only update if this session exists in our store
          if (!store.sessions[sessionId]) return

          const chatMessages = mapParsedMessages(result.allMessages)
          store.setChatHistory(sessionId, chatMessages)
        },
      ).then(fn => { unlisten = fn })
        .catch(e => console.error('[tauri] listen conversation-update:', e))
    }).catch(e => console.error('[tauri] import event:', e))

    return () => { unlisten?.() }
  }, [])
}

/** Combined init hook — call once in App. */
export function useTauriInit() {
  useSessionEvents()
  useConfigSync()
  useCursorHotzone()
  useFullscreenDetection()
  useConversationUpdates()
}
