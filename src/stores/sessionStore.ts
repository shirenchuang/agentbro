/* Agent Island — Session State Management (Zustand) */
import { create } from 'zustand'
import type { AgentEvent, BaseLayer, ChatMessage, OverlayItem, OverlayType, OVERLAY_PRIORITY as _OP, PanelState, RateLimitInfo, SessionState } from '../types/agent'
import { OVERLAY_PRIORITY } from '../types/agent'
import { useConfigStore } from './configStore'
import { saveSessions as saveSessionsToBackend, loadSessions as loadSessionsFromBackend } from '../services/tauriApi'

// Debounce helper
function debounce<T extends (...args: Parameters<T>) => void>(fn: T, ms: number): T {
  let timeoutId: ReturnType<typeof setTimeout> | null = null
  return ((...args: Parameters<T>) => {
    if (timeoutId) clearTimeout(timeoutId)
    timeoutId = setTimeout(() => fn(...args), ms)
  }) as T
}

// Debounced save (1000ms to avoid excessive writes)
const saveSessionsDebounced = debounce(() => {
  const sessions = useSessionStore.getState().sessionList
  saveSessionsToBackend(sessions).catch((err) => console.warn('Failed to persist sessions:', err))
}, 1000)

// Load persisted sessions on init
async function loadPersistedSessions() {
  try {
    const persisted = await loadSessionsFromBackend()
    if (persisted && persisted.length > 0) {
      useSessionStore.getState().replaceAllSessions(persisted)
      console.log('Loaded', persisted.length, 'persisted sessions')
    }
  } catch (err) {
    console.warn('Failed to load persisted sessions:', err)
  }
}

// Trigger initial load
loadPersistedSessions()

interface SessionStore {
  sessions: Record<string, SessionState>
  sessionList: SessionState[]
  activeSessionId: string | null
  panelState: PanelState
  rateLimits?: RateLimitInfo
  // Layered state machine
  baseLayer: BaseLayer
  overlayQueue: OverlayItem[]
  activeOverlay: OverlayItem | null
  // Session mute
  mutedSessions: Record<string, number>  // sessionId → mute expiry timestamp
  // actions
  updateSession: (event: AgentEvent) => void
  setActiveSession: (id: string | null) => void
  setPanelState: (state: PanelState) => void
  setBaseLayer: (layer: BaseLayer) => void
  pushOverlay: (item: OverlayItem) => void
  dismissOverlay: (id: string) => void
  clearSessionOverlays: (sessionId: string) => void
  removeSession: (id: string) => void
  replaceAllSessions: (sessions: SessionState[]) => void
  setChatHistory: (sessionId: string, messages: ChatMessage[]) => void
  clearPermission: (sessionId: string) => void
  clearQuestion: (sessionId: string) => void
  setRateLimits: (limits: RateLimitInfo) => void
  muteSession: (id: string, durationMs?: number) => void
  unmuteSession: (id: string) => void
  isSessionMuted: (id: string) => boolean
}

function toList(sessions: Record<string, SessionState>): SessionState[] {
  return Object.values(sessions)
}

export const selectSessionList = (s: SessionStore) => s.sessionList
export const selectActiveSession = (s: SessionStore): SessionState | undefined =>
  s.activeSessionId ? s.sessions[s.activeSessionId] : undefined
export const selectPanelState = (s: SessionStore) => s.panelState
export const selectActiveSessionId = (s: SessionStore) => s.activeSessionId
export const selectRateLimits = (s: SessionStore) => s.rateLimits
export const selectBaseLayer = (s: SessionStore) => s.baseLayer
export const selectActiveOverlay = (s: SessionStore) => s.activeOverlay
export const selectOverlayQueue = (s: SessionStore) => s.overlayQueue

export const useSessionStore = create<SessionStore>((set) => ({
  sessions: {},
  sessionList: [],
  activeSessionId: null,
  panelState: 'collapsed',
  rateLimits: undefined,
  baseLayer: 'compact',
  overlayQueue: [],
  activeOverlay: null,
  mutedSessions: {},

  updateSession: (event: AgentEvent) => {
    set((state) => {
      const sessions = { ...state.sessions }
      let panelState = state.panelState
      let activeSessionId = state.activeSessionId

      switch (event.type) {
        case 'session_start': {
          sessions[event.sessionId] = {
            id: event.sessionId,
            agentType: event.agentType,
            project: event.project,
            terminal: event.terminal,
            phase: 'idle',
            startedAt: Date.now(),
            duration: 0,
            tokens: { input: 0, output: 0, cacheRead: 0, cacheCreate: 0 },
            chatHistory: [],
            subagents: [],
            activeTools: [],
          }
          activeSessionId = activeSessionId ?? event.sessionId
          break
        }

        case 'session_end': {
          delete sessions[event.sessionId]
          if (activeSessionId === event.sessionId) {
            const ids = Object.keys(sessions)
            activeSessionId = ids[0] ?? null
          }
          setTimeout(() => useSessionStore.getState().clearSessionOverlays(event.sessionId), 0)
          break
        }

        case 'processing': {
          const session = sessions[event.sessionId]
          if (session) {
            sessions[event.sessionId] = { ...session, phase: 'processing', description: event.description, idleSince: undefined }
          }
          break
        }

        case 'user_message': {
          const session = sessions[event.sessionId]
          if (session) {
            const msg: ChatMessage = { role: 'user', content: event.content, timestamp: Date.now() }
            sessions[event.sessionId] = { ...session, chatHistory: [...session.chatHistory, msg] }
          }
          break
        }

        case 'tool_use': {
          const session = sessions[event.sessionId]
          if (session) {
            const msg: ChatMessage = { role: 'tool_use', toolName: event.toolName, toolInput: event.toolInput, status: event.status, timestamp: Date.now() }
            const updatedSession = {
              ...session,
              phase: 'processing' as const,
              lastToolName: event.toolName,
              lastToolTarget: event.toolTarget,
              lastToolStatus: event.status,
              chatHistory: [...session.chatHistory, msg],
              idleSince: undefined,
            }

            // Task tracking: detect TaskCreate/TaskUpdate tool calls
            if (event.toolName === 'TaskCreate' && event.toolInput && event.status === 'success') {
              try {
                const parsed = JSON.parse(event.toolInput)
                if (parsed.subject) {
                  const newTask = { id: `task-${Date.now()}`, name: parsed.subject, status: 'pending' as const }
                  updatedSession.tasks = [...(session.tasks || []), newTask]
                }
              } catch {}
            } else if (event.toolName === 'TaskUpdate' && event.toolInput && event.status === 'success') {
              try {
                const parsed = JSON.parse(event.toolInput)
                if (parsed.taskId && parsed.status && session.tasks) {
                  updatedSession.tasks = session.tasks.map((t) =>
                    t.id === parsed.taskId ? { ...t, status: parsed.status } : t
                  )
                }
              } catch {}
            }

            // Subagent tracking: detect Agent tool calls
            if (event.toolName === 'Agent' && event.toolInput && event.status === 'running') {
              try {
                const parsed = JSON.parse(event.toolInput)
                const subagent = {
                  agentId: `sub-${Date.now()}`,
                  description: parsed.description || parsed.prompt?.slice(0, 80) || 'Subagent',
                  startedAt: Date.now(),
                  status: 'running' as const,
                  tools: [],
                }
                updatedSession.subagents = [...session.subagents, subagent]
              } catch {}
            }

            sessions[event.sessionId] = updatedSession
          }
          break
        }

        case 'permission_request': {
          const session = sessions[event.sessionId]
          if (session) {
            const msg: ChatMessage = { role: 'permission', toolName: event.toolName, toolInput: event.toolInput, diff: event.diff, options: event.options, timestamp: Date.now() }
            sessions[event.sessionId] = {
              ...session,
              phase: 'waiting_approval',
              pendingPermission: {
                toolName: event.toolName,
                toolInput: event.toolInput || '',
                diff: event.diff,
                options: event.options,
              },
              chatHistory: [...session.chatHistory, msg],
            }
          }
          // Push overlay instead of forcing panel state
          const permOverlayId = `perm-${event.sessionId}-${Date.now()}`
          const permOverlay: OverlayItem = {
            id: permOverlayId,
            sessionId: event.sessionId,
            type: 'permission',
            data: {
              toolName: event.toolName,
              toolInput: event.toolInput || '',
              diff: event.diff,
              options: event.options,
            },
            createdAt: Date.now(),
          }
          // Queue overlay after set() — use setTimeout(0) to run after state update
          setTimeout(() => useSessionStore.getState().pushOverlay(permOverlay), 0)
          activeSessionId = event.sessionId
          break
        }

        case 'ask_question': {
          const session = sessions[event.sessionId]
          if (session) {
            sessions[event.sessionId] = {
              ...session,
              phase: 'waiting_input',
              pendingQuestion: { question: event.question, options: event.options },
            }
          }
          const qOverlay: OverlayItem = {
            id: `question-${event.sessionId}-${Date.now()}`,
            sessionId: event.sessionId,
            type: 'question',
            data: { question: event.question, options: event.options },
            createdAt: Date.now(),
          }
          setTimeout(() => useSessionStore.getState().pushOverlay(qOverlay), 0)
          activeSessionId = event.sessionId
          break
        }

        case 'task_complete': {
          const session = sessions[event.sessionId]
          if (session) {
            const msg: ChatMessage = { role: 'assistant', content: event.summary, timestamp: Date.now() }
            sessions[event.sessionId] = {
              ...session,
              phase: 'done',
              description: event.summary,
              pendingPermission: undefined,
              pendingQuestion: undefined,
              chatHistory: [...session.chatHistory, msg],
              taskCompletedAt: Date.now(),
              idleSince: Date.now(),
              subagents: session.subagents.map((s) => s.status === 'running' ? { ...s, status: 'completed' as const } : s),
            }
          }
          // Push completion overlay with auto-dismiss
          const completionId = `completion-${event.sessionId}-${Date.now()}`
          const completionOverlay: OverlayItem = {
            id: completionId,
            sessionId: event.sessionId,
            type: 'completion',
            data: { summary: event.summary },
            createdAt: Date.now(),
          }
          const dwellSeconds = useConfigStore.getState().taskCompleteDwellSeconds || 3
          setTimeout(() => useSessionStore.getState().pushOverlay(completionOverlay), 0)
          setTimeout(() => useSessionStore.getState().dismissOverlay(completionId), dwellSeconds * 1000)
          break
        }

        case 'plan_request': {
          const session = sessions[event.sessionId]
          if (session) {
            sessions[event.sessionId] = {
              ...session,
              planTitle: event.planTitle,
              planContent: event.planContent,
            }
          }
          const planOverlay: OverlayItem = {
            id: `plan-${event.sessionId}-${Date.now()}`,
            sessionId: event.sessionId,
            type: 'plan',
            data: { planTitle: event.planTitle, planContent: event.planContent, requestedPermissions: event.requestedPermissions },
            createdAt: Date.now(),
          }
          setTimeout(() => useSessionStore.getState().pushOverlay(planOverlay), 0)
          activeSessionId = event.sessionId
          break
        }

        case 'task_update': {
          const session = sessions[event.sessionId]
          if (session) {
            const tasks = [...(session.tasks || [])]
            const idx = tasks.findIndex((t) => t.id === event.taskId)
            if (idx >= 0) {
              tasks[idx] = { ...tasks[idx], name: event.subject, status: event.status }
            } else {
              tasks.push({ id: event.taskId, name: event.subject, status: event.status })
            }
            sessions[event.sessionId] = { ...session, tasks }
          }
          break
        }

        case 'error': {
          const session = sessions[event.sessionId]
          if (session) {
            sessions[event.sessionId] = { ...session, phase: 'error', description: event.message }
          }
          break
        }

        case 'interrupt': {
          const session = sessions[event.sessionId]
          if (session) {
            sessions[event.sessionId] = { ...session, phase: 'interrupted' }
          }
          break
        }

        case 'context_compact': {
          const session = sessions[event.sessionId]
          if (session) {
            sessions[event.sessionId] = {
              ...session,
              phase: event.phase === 'pre' ? 'compacting' : 'processing',
            }
          }
          break
        }

        case 'token_usage': {
          const session = sessions[event.sessionId]
          if (session) {
            sessions[event.sessionId] = {
              ...session,
              tokens: {
                input: session.tokens.input + event.input,
                output: session.tokens.output + event.output,
                cacheRead: session.tokens.cacheRead + event.cacheRead,
                cacheCreate: session.tokens.cacheCreate + event.cacheCreate,
              },
            }
          }
          break
        }
      }

      return { sessions, sessionList: toList(sessions), panelState, activeSessionId }
    })
    // Trigger debounced save after update
    saveSessionsDebounced()
  },

  setActiveSession: (id) => set({ activeSessionId: id }),
  setPanelState: (panelState) => {
    const layerMap: Record<PanelState, BaseLayer> = { collapsed: 'compact', hover: 'expanded', expanded: 'detail' }
    set({ panelState, baseLayer: layerMap[panelState] })
  },
  removeSession: (id) => {
    set((state) => {
      const sessions = { ...state.sessions }
      delete sessions[id]
      let activeSessionId = state.activeSessionId
      if (activeSessionId === id) {
        const ids = Object.keys(sessions)
        activeSessionId = ids[0] ?? null
      }
      return { sessions, sessionList: toList(sessions), activeSessionId }
    })
    saveSessionsDebounced()
  },

  replaceAllSessions: (newSessions) => {
    set((state) => {
      const sessions: Record<string, SessionState> = {}
      for (const s of newSessions) {
        // Ensure new fields have defaults for backward compatibility
        sessions[s.id] = {
          ...s,
          chatHistory: state.sessions[s.id]?.chatHistory ?? s.chatHistory ?? [],
          subagents: s.subagents ?? [],
          activeTools: s.activeTools ?? [],
        }
      }
      let activeSessionId = state.activeSessionId
      if (activeSessionId && !sessions[activeSessionId]) {
        activeSessionId = Object.keys(sessions)[0] ?? null
      }
      return { sessions, sessionList: toList(sessions), activeSessionId }
    })
    saveSessionsDebounced()
  },

  setChatHistory: (sessionId, messages) => {
    set((state) => {
      const session = state.sessions[sessionId]
      if (!session) return state
      const sessions = {
        ...state.sessions,
        [sessionId]: { ...session, chatHistory: messages },
      }
      return { sessions, sessionList: toList(sessions) }
    })
  },

  clearPermission: (sessionId) => {
    set((state) => {
      const session = state.sessions[sessionId]
      if (!session) return state
      const sessions = {
        ...state.sessions,
        [sessionId]: { ...session, phase: 'processing' as const, pendingPermission: undefined },
      }
      const overlayQueue = state.overlayQueue.filter((o) => !(o.sessionId === sessionId && o.type === 'permission'))
      return { sessions, sessionList: toList(sessions), overlayQueue, activeOverlay: overlayQueue[0] ?? null }
    })
  },

  clearQuestion: (sessionId) => {
    set((state) => {
      const session = state.sessions[sessionId]
      if (!session) return state
      const sessions = {
        ...state.sessions,
        [sessionId]: { ...session, phase: 'processing' as const, pendingQuestion: undefined },
      }
      const overlayQueue = state.overlayQueue.filter((o) => !(o.sessionId === sessionId && o.type === 'question'))
      return { sessions, sessionList: toList(sessions), overlayQueue, activeOverlay: overlayQueue[0] ?? null }
    })
  },

  setRateLimits: (limits) => {
    set({ rateLimits: limits })
  },

  setBaseLayer: (baseLayer) => {
    const panelMap: Record<BaseLayer, PanelState> = { compact: 'collapsed', expanded: 'hover', detail: 'expanded' }
    set({ baseLayer, panelState: panelMap[baseLayer] })
  },

  pushOverlay: (item) => {
    set((state) => {
      const queue = [...state.overlayQueue, item].sort(
        (a, b) => (OVERLAY_PRIORITY[b.type] ?? 0) - (OVERLAY_PRIORITY[a.type] ?? 0)
      )
      return { overlayQueue: queue, activeOverlay: queue[0] ?? null }
    })
  },

  dismissOverlay: (id) => {
    set((state) => {
      const queue = state.overlayQueue.filter((o) => o.id !== id)
      return { overlayQueue: queue, activeOverlay: queue[0] ?? null }
    })
  },

  clearSessionOverlays: (sessionId) => {
    set((state) => {
      const queue = state.overlayQueue.filter((o) => o.sessionId !== sessionId)
      return { overlayQueue: queue, activeOverlay: queue[0] ?? null }
    })
  },

  muteSession: (id, durationMs = 30 * 60 * 1000) => {
    const expiry = Date.now() + durationMs
    set((state) => ({
      mutedSessions: { ...state.mutedSessions, [id]: expiry },
    }))
    setTimeout(() => {
      useSessionStore.getState().unmuteSession(id)
    }, durationMs)
  },

  unmuteSession: (id) => {
    set((state) => {
      const mutedSessions = { ...state.mutedSessions }
      delete mutedSessions[id]
      return { mutedSessions }
    })
  },

  isSessionMuted: (id) => {
    const expiry = useSessionStore.getState().mutedSessions[id]
    if (!expiry) return false
    if (Date.now() > expiry) {
      useSessionStore.getState().unmuteSession(id)
      return false
    }
    return true
  },
}))
