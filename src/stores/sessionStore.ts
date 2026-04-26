/* Agent Island — Session State Management (Zustand) */
import { create } from 'zustand'
import type { AgentEvent, ChatMessage, PanelState, RateLimitInfo, SessionState } from '../types/agent'
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
  // actions
  updateSession: (event: AgentEvent) => void
  setActiveSession: (id: string | null) => void
  setPanelState: (state: PanelState) => void
  removeSession: (id: string) => void
  replaceAllSessions: (sessions: SessionState[]) => void
  setChatHistory: (sessionId: string, messages: ChatMessage[]) => void
  clearPermission: (sessionId: string) => void
  clearQuestion: (sessionId: string) => void
  setRateLimits: (limits: RateLimitInfo) => void
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

export const useSessionStore = create<SessionStore>((set) => ({
  sessions: {},
  sessionList: [],
  activeSessionId: null,
  panelState: 'collapsed',
  rateLimits: undefined,

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
          break
        }

        case 'processing': {
          const session = sessions[event.sessionId]
          if (session) {
            sessions[event.sessionId] = { ...session, phase: 'processing', description: event.description }
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
            sessions[event.sessionId] = {
              ...session,
              phase: 'processing',
              lastToolName: event.toolName,
              lastToolTarget: event.toolTarget,
              lastToolStatus: event.status,
              chatHistory: [...session.chatHistory, msg],
            }
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
          panelState = 'expanded'
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
          panelState = 'expanded'
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
            }
          }
          // Store session ID and verify it's still active before collapsing
          const completedSessionId = event.sessionId
          const dwellSeconds = useConfigStore.getState().taskCompleteDwellSeconds || 3
          setTimeout(() => {
            const store = useSessionStore.getState()
            if (store.activeSessionId === completedSessionId) {
              store.setPanelState('collapsed')
            }
          }, dwellSeconds * 1000)
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
  setPanelState: (panelState) => set({ panelState }),
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
      return { sessions, sessionList: toList(sessions) }
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
      return { sessions, sessionList: toList(sessions) }
    })
  },

  setRateLimits: (limits) => {
    set({ rateLimits: limits })
  },
}))
