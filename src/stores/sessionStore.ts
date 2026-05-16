/* AgentBro — Session State Management (Zustand) */
import { create, type StoreApi, type UseBoundStore } from 'zustand'
import { useConfigStore } from './configStore'
import type { AgentEvent, BaseLayer, ChatMessage, OverlayItem, PanelState, RateLimitInfo, SessionState } from '../types/agent'
import { OVERLAY_PRIORITY } from '../types/agent'
import { isQuietHours } from '../utils/quietHours'
import { respondPermission, saveSessions as saveSessionsToBackend } from '../services/tauriApi'

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

interface SessionStore {
  sessions: Record<string, SessionState>
  sessionList: SessionState[]
  activeSessionId: string | null
  panelState: PanelState
  rateLimits?: RateLimitInfo
  hookNotification: 'restored' | 'rate_limited' | null
  // Layered state machine
  baseLayer: BaseLayer
  overlayQueue: OverlayItem[]
  activeOverlay: OverlayItem | null
  // Session mute
  mutedSessions: Record<string, number>  // sessionId → mute expiry timestamp
  wakeSilencedUntil: number
  // actions
  updateSession: (event: AgentEvent) => void
  setActiveSession: (id: string | null) => void
  setPanelState: (state: PanelState) => void
  setBaseLayer: (layer: BaseLayer) => void
  pushOverlay: (item: OverlayItem) => void
  dismissOverlay: (id: string) => void
  clearSessionOverlays: (sessionId: string) => void
  removeSession: (id: string) => void
  replaceAllSessions: (sessions: SessionState[], options?: { suppressed?: boolean }) => void
  setChatHistory: (sessionId: string, messages: ChatMessage[]) => void
  clearPermission: (sessionId: string) => void
  clearQuestion: (sessionId: string) => void
  clearPlan: (sessionId: string) => void
  setRateLimits: (limits: RateLimitInfo) => void
  setHookNotification: (notification: 'restored' | 'rate_limited' | null) => void
  applyIdleTimeout: (now?: number) => void
  muteSession: (id: string, durationMs?: number) => void
  unmuteSession: (id: string) => void
  isSessionMuted: (id: string) => boolean
  setWakeSilencedUntil: (timestamp: number) => void
  isWakeSilenced: () => boolean
  // Follow-focus
  focusedTerminal: string | null
  setFocusedTerminal: (name: string | null) => void
}

function toList(sessions: Record<string, SessionState>): SessionState[] {
  return Object.values(sessions).filter(isDisplayableSession)
}

function hasSessionContent(session: SessionState): boolean {
  return Boolean(
    session.lastUserMessage
    || session.sessionTitle
    || session.pendingPermission
    || session.pendingQuestion
    || session.planTitle
    || session.planContent
    || session.responseText
    || session.description
    || session.lastToolName
    || session.statusLineText
    || session.contextWindow
    || session.rateLimits
    || session.subagents.length > 0
    || session.activeTools.length > 0
    || (session.tasks && session.tasks.length > 0)
    || session.tokens.input > 0
    || session.tokens.output > 0
    || session.tokens.cacheRead > 0
    || session.tokens.cacheCreate > 0
  )
}

function isCodexAppPlaceholder(session: SessionState): boolean {
  const cwd = session.cwd || ''
  const terminal = session.terminal || ''
  const bundle = session.termBundleId || ''
  return session.agentType === 'codex' && (
    terminal.toLowerCase().includes('codex')
    || bundle.toLowerCase().includes('codex')
    || cwd.includes('.evolab-desktop')
    || session.project === 'free-chat'
  )
}

function isDisplayableSession(session: SessionState): boolean {
  if (session.phase === 'waiting_approval' || session.phase === 'waiting_input' || session.phase === 'error') {
    return true
  }
  if (session.phase === 'done' && (sessionEndedText(session.responseText) || sessionEndedText(session.description))) {
    return false
  }
  if (isExpiredClaudeListItem(session)) {
    return false
  }
  if (isInternalCodexPromptSession(session)) {
    return false
  }
  if (isCodexAppPlaceholder(session) && !hasSessionContent(session)) {
    return false
  }
  return true
}

const INTERNAL_CODEX_PROMPT_PREFIXES = [
  'you are a helpful assistant. you will be presented with a user prompt',
  'you are codex, a coding agent',
  'you are an ai assistant accessed via an api',
]

function normalizedPromptText(text: string | undefined | null): string {
  return (text || '').trim().replace(/\s+/g, ' ').toLowerCase()
}

function isInternalCodexPromptText(text: string | undefined | null): boolean {
  const normalized = normalizedPromptText(text)
  if (!normalized) return false
  return INTERNAL_CODEX_PROMPT_PREFIXES.some((prefix) => normalized.startsWith(prefix))
}

function hasCodexDetailAnchor(session: SessionState): boolean {
  return Boolean(
    session.chatHistory.length > 0
    || session.pid
    || session.tty
    || session.termBundleId
    || session.weztermPane
    || session.zellijPaneId
    || session.zellijSessionName
    || session.cmuxSurfaceId
    || session.cmuxWorkspaceId
    || session.terminal?.trim()
  )
}

function isInternalCodexPromptSession(session: SessionState): boolean {
  if (session.agentType !== 'codex') return false
  if (session.phase !== 'idle' && session.phase !== 'done') return false
  if (session.pendingPermission || session.pendingQuestion || session.planTitle || session.planContent) return false
  if (!isInternalCodexPromptText(session.sessionTitle) && !isInternalCodexPromptText(session.lastUserMessage)) return false
  if (usefulCompletionText(session.responseText) || usefulCompletionText(session.description)) return false
  if (hasCodexDetailAnchor(session)) return false
  return true
}

function isExpiredClaudeListItem(session: SessionState): boolean {
  if (session.agentType !== 'claude-code') return false
  if (session.phase !== 'idle' && session.phase !== 'done') return false
  if (session.pendingPermission || session.pendingQuestion || session.planTitle || session.planContent) return false
  if (usefulCompletionText(session.responseText) || usefulCompletionText(session.description)) return false
  if (session.lastToolName || session.statusLineText || session.contextWindow || session.rateLimits) return false
  if (session.activeTools.some((tool) => tool.status === 'running')) return false
  if (session.subagents.some((agent) => agent.status === 'running')) return false
  if (session.tasks?.some((task) => task.status !== 'completed')) return false
  return true
}

function isGenericCompletionText(text: string | undefined | null): boolean {
  const normalized = (text || '')
    .trim()
    .replace(/[.!。！]+$/g, '')
    .toLowerCase()
  return normalized === ''
    || normalized === 'done'
    || normalized === 'task complete'
    || normalized === 'task completed'
    || normalized === 'session ended'
    || normalized === 'processing user input'
    || normalized === 'compacting context'
    || normalized === 'waiting for input'
}

function usefulCompletionText(text: string | undefined | null): string | null {
  const trimmed = (text || '').trim()
  return trimmed && !isGenericCompletionText(trimmed) ? trimmed : null
}

function sessionEndedText(text: string | undefined | null): string | null {
  const normalized = (text || '')
    .trim()
    .replace(/[.!。！]+$/g, '')
    .toLowerCase()
  return normalized === 'session ended' ? 'Session ended' : null
}

function getLastAssistantText(session: SessionState): string | null {
  for (let index = session.chatHistory.length - 1; index >= 0; index -= 1) {
    const message = session.chatHistory[index]
    if (message.role !== 'assistant') continue
    const text = usefulCompletionText(message.trailingContent) || usefulCompletionText(message.content)
    if (text) return text
  }
  return null
}

function deriveCompletionSummary(session: SessionState, incoming?: string): string {
  return sessionEndedText(incoming)
    || (session.phase === 'done' && (sessionEndedText(session.responseText) || sessionEndedText(session.description)))
    || usefulCompletionText(incoming)
    || usefulCompletionText(session.responseText)
    || getLastAssistantText(session)
    || usefulCompletionText(session.description)
    || 'Task completed'
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

export const useSessionStore: UseBoundStore<StoreApi<SessionStore>> = create<SessionStore>((set) => ({
  sessions: {},
  sessionList: [],
  activeSessionId: null,
  panelState: 'collapsed',
  rateLimits: undefined,
  hookNotification: null,
  baseLayer: 'compact',
  overlayQueue: [],
  activeOverlay: null,
  mutedSessions: {},
  wakeSilencedUntil: 0,
  focusedTerminal: null,

  updateSession: (event: AgentEvent) => {
    set((state) => {
      const sessions = { ...state.sessions }
      const panelState = state.panelState
      let activeSessionId = state.activeSessionId

      switch (event.type) {
        case 'session_start': {
          // Check CWD exclusion list
          const excludedStr = useConfigStore.getState().excludedHookCwdSubstrings
          if (excludedStr && 'cwd' in event && (event as { cwd?: string }).cwd) {
            const cwd = (event as { cwd?: string }).cwd!
            const excluded = excludedStr.split(',').map(s => s.trim()).filter(Boolean)
            if (excluded.some(sub => cwd.includes(sub))) {
              return state
            }
          }
          sessions[event.sessionId] = {
            id: event.sessionId,
            agentType: event.agentType,
            project: event.project,
            cwd: 'cwd' in event ? (event as { cwd?: string }).cwd : undefined,
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
            sessions[event.sessionId] = { ...session, phase: 'processing', description: event.description, idleSince: undefined, unattendedSince: undefined, lastActivityAt: Date.now() }
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
              lastActivityAt: Date.now(),
            }

            // Task tracking: detect TaskCreate/TaskUpdate tool calls
            if (event.toolName === 'TaskCreate' && event.toolInput && event.status === 'success') {
              try {
                const parsed = JSON.parse(event.toolInput)
                if (parsed.subject) {
                  const newTask = { id: `task-${Date.now()}`, name: parsed.subject, status: 'pending' as const }
                  updatedSession.tasks = [...(session.tasks || []), newTask]
                }
              } catch (error) {
                console.warn('[session] ignore invalid TaskCreate payload:', error)
              }
            } else if (event.toolName === 'TaskUpdate' && event.toolInput && event.status === 'success') {
              try {
                const parsed = JSON.parse(event.toolInput)
                if (parsed.taskId && parsed.status && session.tasks) {
                  updatedSession.tasks = session.tasks.map((t) =>
                    t.id === parsed.taskId ? { ...t, status: parsed.status } : t
                  )
                }
              } catch (error) {
                console.warn('[session] ignore invalid TaskUpdate payload:', error)
              }
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
              } catch (error) {
                console.warn('[session] ignore invalid Agent payload:', error)
              }
            }

            sessions[event.sessionId] = updatedSession
          }
          break
        }

        case 'permission_request': {
          // Auto-approve if tool is in the auto-approve list
          const autoApproveTools = useConfigStore.getState().autoApproveTools
          if (autoApproveTools.includes(event.toolName)) {
            const session = sessions[event.sessionId]
            if (session) {
              sessions[event.sessionId] = { ...session, phase: 'processing', lastActivityAt: Date.now() }
            }
            // Fire auto-approve response asynchronously
            respondPermission(event.sessionId, true).catch(() => {})
            break
          }

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
              unattendedSince: session.unattendedSince ?? Date.now(),
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
              pendingQuestion: {
                question: event.question,
                options: event.options,
                descriptions: event.descriptions,
                header: event.header,
                multiSelect: event.multiSelect,
                questions: event.questions,
              },
              unattendedSince: session.unattendedSince ?? Date.now(),
            }
          }
          const qOverlay: OverlayItem = {
            id: `question-${event.sessionId}-${Date.now()}`,
            sessionId: event.sessionId,
            type: 'question',
            data: {
              question: event.question,
              options: event.options,
              descriptions: event.descriptions,
              header: event.header,
              multiSelect: event.multiSelect,
              questions: event.questions,
            },
            createdAt: Date.now(),
          }
          setTimeout(() => useSessionStore.getState().pushOverlay(qOverlay), 0)
          activeSessionId = event.sessionId
          break
        }

        case 'task_complete': {
          const session = sessions[event.sessionId]
          if (session) {
            const summary = deriveCompletionSummary(session, event.summary)
            const msg: ChatMessage = { role: 'assistant', content: summary, timestamp: Date.now() }
            sessions[event.sessionId] = {
              ...session,
              phase: 'done',
              description: summary,
              responseText: summary,
              pendingPermission: undefined,
              pendingQuestion: undefined,
              chatHistory: [...session.chatHistory, msg],
              taskCompletedAt: Date.now(),
              idleSince: Date.now(),
              unattendedSince: undefined,
              subagents: session.subagents.map((s) => s.status === 'running' ? { ...s, status: 'completed' as const } : s),
            }
          }
          // Push completion overlay with auto-dismiss
          const completionId = `completion-${event.sessionId}-${Date.now()}`
          const completionOverlay: OverlayItem = {
            id: completionId,
            sessionId: event.sessionId,
            type: 'completion',
            data: { summary: session ? deriveCompletionSummary(session, event.summary) : event.summary },
            createdAt: Date.now(),
          }
          setTimeout(() => useSessionStore.getState().pushOverlay(completionOverlay), 0)
          break
        }

        case 'plan_request': {
          const session = sessions[event.sessionId]
          if (session) {
            sessions[event.sessionId] = {
              ...session,
              phase: 'waiting_approval',
              planTitle: event.planTitle,
              planContent: event.planContent,
              planPermissions: event.requestedPermissions,
              unattendedSince: session.unattendedSince ?? Date.now(),
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

  replaceAllSessions: (newSessions, options) => {
    const prevState = useSessionStore.getState()
    const newOverlays: OverlayItem[] = []
    const suppressed = options?.suppressed === true

    set((state) => {
      const sessions: Record<string, SessionState> = {}
      for (const s of newSessions) {
        sessions[s.id] = {
          ...s,
          chatHistory: state.sessions[s.id]?.chatHistory ?? s.chatHistory ?? [],
          subagents: s.subagents ?? [],
          activeTools: s.activeTools ?? [],
        }

        const prev = prevState.sessions[s.id]

        // Detect new pendingQuestion — create question overlay
        if (s.pendingQuestion && !prev?.pendingQuestion) {
          const existingOverlay = state.overlayQueue.find((o) => o.sessionId === s.id && o.type === 'question')
          if (!existingOverlay) {
            newOverlays.push({
              id: `question-${s.id}-${Date.now()}`,
              sessionId: s.id,
              type: 'question',
              data: {
                question: s.pendingQuestion.question,
                options: (s.pendingQuestion.options || []).map((label: string, i: number) => ({
                  label,
                  description: s.pendingQuestion?.descriptions?.[i],
                })),
                header: s.pendingQuestion.header,
                multiSelect: s.pendingQuestion.multiSelect,
                questions: s.pendingQuestion.questions,
              },
              createdAt: Date.now(),
              suppressed,
            })
          }
        }

        // Detect new pendingPermission — create permission overlay
        if (s.pendingPermission && !prev?.pendingPermission) {
          const existingOverlay = state.overlayQueue.find((o) => o.sessionId === s.id && o.type === 'permission')
          if (!existingOverlay) {
            newOverlays.push({
              id: `permission-${s.id}-${Date.now()}`,
              sessionId: s.id,
              type: 'permission',
              data: {
                toolName: s.pendingPermission.toolName,
                toolInput: s.pendingPermission.toolInput,
                diff: s.pendingPermission.diff,
              },
              createdAt: Date.now(),
              suppressed,
            })
          }
        }

        // Detect new pending plan approval from backend hook flow.
        if ((s.planTitle || s.planContent) && (s.planContent !== prev?.planContent || s.planTitle !== prev?.planTitle)) {
          const existingOverlay = state.overlayQueue.find((o) => o.sessionId === s.id && o.type === 'plan')
          if (!existingOverlay) {
            newOverlays.push({
              id: `plan-${s.id}-${Date.now()}`,
              sessionId: s.id,
              type: 'plan',
              data: {
                planTitle: s.planTitle,
                planContent: s.planContent || '',
                requestedPermissions: s.planPermissions || [],
              },
              createdAt: Date.now(),
              suppressed,
            })
          }
        }

        const hideNonBlockingOverlays = isInternalCodexPromptSession(s)

        // Detect a newly available assistant response and show the response overlay.
        if (!hideNonBlockingOverlays && s.phase !== 'done' && s.responseText && s.responseText !== prev?.responseText) {
          newOverlays.push({
            id: `response-${s.id}-${Date.now()}`,
            sessionId: s.id,
            type: 'response',
            data: {
              responseText: s.responseText,
              userMessage: s.lastUserMessage,
            },
            createdAt: Date.now(),
          })
        }

        // Backend session updates are the source of truth in Tauri mode, so
        // synthesize completion overlays when a session transitions to done.
        if (!hideNonBlockingOverlays && s.phase === 'done' && prev?.phase !== 'done') {
          const summary = deriveCompletionSummary(s)
          newOverlays.push({
            id: `completion-${s.id}-${Date.now()}`,
            sessionId: s.id,
            type: 'completion',
            data: { summary },
            createdAt: Date.now(),
          })
        }
      }
      let activeSessionId = state.activeSessionId
      const sessionList = toList(sessions)
      if (activeSessionId && (!sessions[activeSessionId] || !isDisplayableSession(sessions[activeSessionId]))) {
        activeSessionId = sessionList[0]?.id ?? null
      }
      const overlayQueue = state.overlayQueue.filter((overlay) => {
        const session = sessions[overlay.sessionId]
        if (!session) return false
        if (overlay.type === 'permission') return Boolean(session.pendingPermission)
        if (overlay.type === 'question') return Boolean(session.pendingQuestion)
        if (overlay.type === 'plan') return Boolean(session.planTitle || session.planContent)
        return true
      })
      return { sessions, sessionList, activeSessionId, overlayQueue, activeOverlay: overlayQueue[0] ?? null }
    })
    // Push new overlays after state update
    if (newOverlays.length > 0) {
      for (const overlay of newOverlays) {
        useSessionStore.getState().pushOverlay(overlay)
      }
    }
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
        [sessionId]: { ...session, phase: 'processing' as const, pendingPermission: undefined, unattendedSince: undefined },
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
        [sessionId]: { ...session, phase: 'processing' as const, pendingQuestion: undefined, unattendedSince: undefined },
      }
      const overlayQueue = state.overlayQueue.filter((o) => !(o.sessionId === sessionId && o.type === 'question'))
      return { sessions, sessionList: toList(sessions), overlayQueue, activeOverlay: overlayQueue[0] ?? null }
    })
  },

  clearPlan: (sessionId) => {
    set((state) => {
      const session = state.sessions[sessionId]
      if (!session) return state
      const sessions = {
        ...state.sessions,
        [sessionId]: {
          ...session,
          phase: 'processing' as const,
          planTitle: undefined,
          planContent: undefined,
          planPermissions: undefined,
          unattendedSince: undefined,
        },
      }
      const overlayQueue = state.overlayQueue.filter((o) => !(o.sessionId === sessionId && o.type === 'plan'))
      return { sessions, sessionList: toList(sessions), overlayQueue, activeOverlay: overlayQueue[0] ?? null }
    })
  },

  setRateLimits: (limits) => {
    set({ rateLimits: limits })
  },

  setHookNotification: (notification) => {
    set({ hookNotification: notification })
  },

  applyIdleTimeout: (now = Date.now()) => {
    const idleTimeoutMinutes = useConfigStore.getState().idleTimeoutMinutes
    if (idleTimeoutMinutes <= 0) return
    const timeoutMs = idleTimeoutMinutes * 60 * 1000
    let changed = false

    set((state) => {
      const sessions = { ...state.sessions }
      for (const session of Object.values(state.sessions)) {
        if (session.phase !== 'processing' && session.phase !== 'compacting') continue
        if (session.activeTools.some((tool) => tool.status === 'running')) continue
        const lastActivityAt = session.lastActivityAt ?? session.startedAt
        if (now - lastActivityAt <= timeoutMs) continue
        sessions[session.id] = {
          ...session,
          phase: 'idle',
          idleSince: session.idleSince ?? now,
          description: session.description,
        }
        changed = true
      }
      return changed ? { sessions, sessionList: toList(sessions) } : state
    })

    if (changed) saveSessionsDebounced()
  },

  setBaseLayer: (baseLayer) => {
    const panelMap: Record<BaseLayer, PanelState> = { compact: 'collapsed', expanded: 'hover', detail: 'expanded' }
    set({ baseLayer, panelState: panelMap[baseLayer] })
  },

  pushOverlay: (item) => {
    // Skip overlays for muted sessions
    const muted = useSessionStore.getState().mutedSessions[item.sessionId]
    if (muted && Date.now() < muted) return

    // During quiet hours, suppress non-blocking overlays (response/completion)
    const isNonBlocking = item.type === 'response' || item.type === 'completion'
    if (isNonBlocking && isQuietHours()) return
    if (isNonBlocking && useSessionStore.getState().isWakeSilenced()) return

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

  setWakeSilencedUntil: (timestamp) => {
    set({ wakeSilencedUntil: timestamp })
  },

  isWakeSilenced: (): boolean => {
    return Date.now() < useSessionStore.getState().wakeSilencedUntil
  },

  setFocusedTerminal: (name) => {
    useSessionStore.setState({ focusedTerminal: name })
  },
}))
