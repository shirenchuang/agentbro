/* AgentBro — Session State Management (Zustand) */
import { create, type StoreApi, type UseBoundStore } from 'zustand'
import { useConfigStore } from './configStore'
import type { AgentEvent, BaseLayer, ChatMessage, OverlayItem, PanelState, RateLimitInfo, SessionState } from '../types/agent'
import { OVERLAY_PRIORITY } from '../types/agent'
import { isQuietHours } from '../utils/quietHours'
import { isSessionPastDisplayTimeout, timestampToMs } from '../utils/sessionDisplay'
import { sessionMatchesLegacyCwdExclusion, sessionMatchesSilenceRule } from '../utils/sessionSilence'
import { isCodexTitleMetadata } from '../utils/codexMetadata'
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
  usageSnapshots: Record<string, RateLimitInfo>
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
  setUsageSnapshots: (limits: RateLimitInfo[]) => void
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

function toList(sessions: Record<string, SessionState>, now = Date.now()): SessionState[] {
  return Object.values(sessions).filter((session) => isDisplayableSession(session, now))
}

function hasPendingInteraction(session: SessionState): boolean {
  return Boolean(session.pendingPermission || session.pendingQuestion || session.planTitle || session.planContent)
}

function clearPendingInteraction(session: SessionState): SessionState {
  return {
    ...session,
    pendingPermission: undefined,
    pendingQuestion: undefined,
    planTitle: undefined,
    planContent: undefined,
    planPermissions: undefined,
    unattendedSince: undefined,
  }
}

function clearBlockingOverlaysForSession(queue: OverlayItem[], sessionId: string): OverlayItem[] {
  return queue.filter((overlay) => !(
    overlay.sessionId === sessionId
    && (overlay.type === 'permission' || overlay.type === 'question' || overlay.type === 'plan')
  ))
}

function mergeLocalUserMessages(remoteMessages: ChatMessage[], localMessages: ChatMessage[]): ChatMessage[] {
  const merged = [...remoteMessages]
  const remoteUserKeys = new Set(
    remoteMessages
      .filter((message): message is Extract<ChatMessage, { role: 'user' }> => message.role === 'user')
      .map((message) => normalizedPromptText(message.content)),
  )
  const latestRemoteTimestamp = remoteMessages.reduce((latest, message) => Math.max(latest, message.timestamp || 0), 0)

  for (const message of localMessages) {
    if (message.role !== 'user') continue
    const key = normalizedPromptText(message.content)
    if (!key || remoteUserKeys.has(key)) continue
    if (latestRemoteTimestamp > 0 && message.timestamp < latestRemoteTimestamp) continue
    merged.push(message)
    remoteUserKeys.add(key)
  }

  return merged
}

function hasSessionContent(session: SessionState): boolean {
  const meaningfulText = (text: string | undefined | null) => Boolean((text || '').trim() && !isCodexTitleMetadata(text))

  return Boolean(
    meaningfulText(session.lastUserMessage)
    || meaningfulText(session.sessionTitle)
    || session.pendingPermission
    || session.pendingQuestion
    || session.planTitle
    || session.planContent
    || meaningfulText(session.responseText)
    || meaningfulText(session.description)
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

function isProbeSession(session: SessionState): boolean {
  const fields = [
    session.project,
    session.sessionTitle,
    session.lastUserMessage,
    session.cwd,
    session.description,
  ]
  const normalized = fields
    .filter(Boolean)
    .join(' ')
    .trim()
    .replace(/[\s_-]+/g, '')
    .toLowerCase()

  if (!normalized) return false
  return normalized.includes('claudeprobe')
    || normalized.includes('healthcheck')
    || /\bprobe\b/i.test(fields.filter(Boolean).join(' '))
}

function isDisplayableSession(session: SessionState, now = Date.now()): boolean {
  if (isProbeSession(session)) {
    return false
  }
  if (session.phase === 'waiting_approval' || session.phase === 'waiting_input' || session.phase === 'error' || hasPendingInteraction(session)) {
    return true
  }
  if (session.phase === 'done' && (sessionEndedText(session.responseText) || sessionEndedText(session.description))) {
    return false
  }
  if (isInternalCodexPromptSession(session)) {
    return false
  }
  if (isCodexTitleMetadataOnlySession(session)) {
    return false
  }
  const config = useConfigStore.getState()
  if (
    config.sessionSilenceRules.some((rule) => sessionMatchesSilenceRule(session, rule))
    || sessionMatchesLegacyCwdExclusion(session, config.excludedHookCwdSubstrings)
  ) {
    return false
  }
  if (isCodexAppPlaceholder(session) && !hasSessionContent(session)) {
    return false
  }
  if (isSessionPastDisplayTimeout(session, config.sessionTimeoutMinutes, now)) {
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

function isInternalCodexPromptSession(session: SessionState): boolean {
  if (session.agentType !== 'codex') return false
  if (session.pendingPermission || session.pendingQuestion || session.planTitle || session.planContent) return false
  if (!isInternalCodexPromptText(session.sessionTitle) && !isInternalCodexPromptText(session.lastUserMessage)) return false
  if (usefulCompletionText(session.responseText) || usefulCompletionText(session.description)) return false
  return true
}

function isCodexTitleMetadataOnlySession(session: SessionState): boolean {
  if (session.agentType !== 'codex') return false
  if (session.pendingPermission || session.pendingQuestion || session.planTitle || session.planContent) return false
  if (session.subagents.length > 0 || session.activeTools.length > 0 || (session.tasks && session.tasks.length > 0)) return false

  const texts = [
    session.sessionTitle,
    session.lastUserMessage,
    session.responseText,
    session.description,
  ].map((text) => (text || '').trim()).filter(Boolean)

  if (!texts.some(isCodexTitleMetadata)) return false
  return texts.every((text) => (
    isCodexTitleMetadata(text)
    || isInternalCodexPromptText(text)
    || isGenericCompletionText(text)
  ))
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
    || normalized.startsWith('processing user input:')
    || normalized === 'compacting context'
    || normalized.startsWith('compacting context')
    || normalized === 'compacting conversation'
    || normalized.startsWith('compacting conversation')
    || normalized === 'waiting for input'
}

function isCompactingContextText(text: string | undefined | null): boolean {
  const normalized = (text || '')
    .trim()
    .replace(/[.!。！]+$/g, '')
    .replace(/\s+/g, ' ')
    .toLowerCase()
  return normalized === 'compacting'
    || normalized === 'compacting context'
    || normalized.startsWith('compacting context')
    || normalized === 'compacting conversation'
    || normalized.startsWith('compacting conversation')
}

function usefulCompletionText(text: string | undefined | null): string | null {
  const trimmed = (text || '').trim()
  return trimmed && !isGenericCompletionText(trimmed) && !isCodexTitleMetadata(trimmed) ? trimmed : null
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

function shouldSuppressCompletionOverlay(session: SessionState, incoming?: string): boolean {
  return Boolean(
    sessionEndedText(incoming)
    || sessionEndedText(session.responseText)
    || sessionEndedText(session.description)
    || isCodexTitleMetadata(incoming)
    || isCodexTitleMetadata(session.responseText)
    || isCodexTitleMetadata(session.description)
  )
}

function isActivityPhase(phase: SessionState['phase']): boolean {
  return phase === 'processing'
    || phase === 'compacting'
    || phase === 'waiting_input'
    || phase === 'waiting_approval'
}

function didBackendActivityChange(incoming: SessionState, existing?: SessionState): boolean {
  if (!existing) return true
  const passivePhaseRefresh = (incoming.phase === 'ready' && existing.phase === 'idle')
    || (incoming.phase === 'idle' && existing.phase === 'ready')
  return incoming.phase !== existing.phase
    && !passivePhaseRefresh
    || incoming.description !== existing.description
    || incoming.responseText !== existing.responseText
    || incoming.lastUserMessage !== existing.lastUserMessage
    || incoming.lastToolName !== existing.lastToolName
    || incoming.lastToolTarget !== existing.lastToolTarget
    || incoming.lastToolStatus !== existing.lastToolStatus
    || incoming.activeTools?.length !== existing.activeTools?.length
}

export const selectSessionList = (s: SessionStore) => s.sessionList
export const selectActiveSession = (s: SessionStore): SessionState | undefined =>
  s.activeSessionId ? s.sessions[s.activeSessionId] : undefined
export const selectPanelState = (s: SessionStore) => s.panelState
export const selectActiveSessionId = (s: SessionStore) => s.activeSessionId
export const selectRateLimits = (s: SessionStore) => s.rateLimits
export const selectUsageSnapshots = (s: SessionStore) => s.usageSnapshots
export const selectBaseLayer = (s: SessionStore) => s.baseLayer
export const selectActiveOverlay = (s: SessionStore) => s.activeOverlay
export const selectOverlayQueue = (s: SessionStore) => s.overlayQueue

export const useSessionStore: UseBoundStore<StoreApi<SessionStore>> = create<SessionStore>((set) => ({
  sessions: {},
  sessionList: [],
  activeSessionId: null,
  panelState: 'collapsed',
  rateLimits: undefined,
  usageSnapshots: {},
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
      let overlayQueue = state.overlayQueue
      let activeSessionId = state.activeSessionId
      const clearBlockingState = (session: SessionState): SessionState => {
        if (!hasPendingInteraction(session)) return session
        overlayQueue = clearBlockingOverlaysForSession(overlayQueue, session.id)
        return clearPendingInteraction(session)
      }

      switch (event.type) {
        case 'session_start': {
          sessions[event.sessionId] = {
            id: event.sessionId,
            agentType: event.agentType,
            project: event.project,
            cwd: 'cwd' in event ? (event as { cwd?: string }).cwd : undefined,
            terminal: event.terminal,
            phase: 'ready',
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
            sessions[event.sessionId] = {
              ...clearBlockingState(session),
              phase: 'processing',
              description: event.description,
              idleSince: undefined,
              lastActivityAt: Date.now(),
            }
          }
          break
        }

        case 'user_message': {
          const session = sessions[event.sessionId]
          if (session) {
            const msg: ChatMessage = { role: 'user', content: event.content, timestamp: Date.now() }
            sessions[event.sessionId] = {
              ...clearBlockingState(session),
              phase: 'processing',
              lastUserMessage: event.content,
              lastUserMessageAt: Date.now(),
              chatHistory: [...session.chatHistory, msg],
              idleSince: undefined,
              lastActivityAt: Date.now(),
            }
          }
          break
        }

        case 'tool_use': {
          const session = sessions[event.sessionId]
          if (session) {
            const msg: ChatMessage = { role: 'tool_use', toolName: event.toolName, toolInput: event.toolInput, status: event.status, timestamp: Date.now() }
            const baseSession = clearBlockingState(session)
            const updatedSession = {
              ...baseSession,
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
              options: event.options.map((label, i) => ({
                label,
                description: event.descriptions?.[i],
              })),
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
          const summary = session ? deriveCompletionSummary(session, event.summary) : event.summary
          if (session) {
            const msg: ChatMessage = { role: 'assistant', content: summary, timestamp: Date.now() }
            sessions[event.sessionId] = {
              ...session,
              phase: 'done',
              description: summary,
              responseText: summary,
              pendingPermission: undefined,
              pendingQuestion: undefined,
              planTitle: undefined,
              planContent: undefined,
              planPermissions: undefined,
              chatHistory: [...session.chatHistory, msg],
              taskCompletedAt: Date.now(),
              idleSince: Date.now(),
              unattendedSince: undefined,
              subagents: session.subagents.map((s) => s.status === 'running' ? { ...s, status: 'completed' as const } : s),
            }
          }
          if (session && !shouldSuppressCompletionOverlay(session, event.summary)) {
            // Push completion overlay with auto-dismiss
            const completionId = `completion-${event.sessionId}-${Date.now()}`
            const completionOverlay: OverlayItem = {
              id: completionId,
              sessionId: event.sessionId,
              type: 'completion',
              data: { summary },
              createdAt: Date.now(),
            }
            setTimeout(() => useSessionStore.getState().pushOverlay(completionOverlay), 0)
          }
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
              description: event.phase === 'pre'
                ? 'Compacting context'
                : isCompactingContextText(session.description)
                  ? undefined
                  : session.description,
              lastToolName: event.phase === 'pre' ? 'Compacting' : undefined,
              lastToolTarget: event.phase === 'pre' ? 'context' : undefined,
              lastToolStatus: event.phase === 'pre' ? 'running' : undefined,
              lastActivityAt: Date.now(),
            }
            if (event.phase === 'pre') {
              const compactingOverlay: OverlayItem = {
                id: `compacting-${event.sessionId}-${Date.now()}`,
                sessionId: event.sessionId,
                type: 'compacting',
                data: {},
                createdAt: Date.now(),
              }
              setTimeout(() => useSessionStore.getState().pushOverlay(compactingOverlay), 0)
            } else {
              overlayQueue = overlayQueue.filter((overlay) => !(overlay.sessionId === event.sessionId && overlay.type === 'compacting'))
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

      return { sessions, sessionList: toList(sessions), panelState, activeSessionId, overlayQueue, activeOverlay: overlayQueue[0] ?? null }
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
    const now = Date.now()

    set((state) => {
      const sessions: Record<string, SessionState> = {}
      for (const incoming of newSessions) {
        const prev = prevState.sessions[incoming.id]
        const existing = state.sessions[incoming.id]
        const enteredIdle = incoming.phase === 'idle' && existing?.phase !== 'idle'
        const enteredDone = incoming.phase === 'done' && existing?.phase !== 'done'
        const lastUserMessageChanged = Boolean(incoming.lastUserMessage)
          && incoming.lastUserMessage !== existing?.lastUserMessage
        const activityChanged = isActivityPhase(incoming.phase) || (incoming.phase === 'ready' && Boolean(existing))
          ? didBackendActivityChange(incoming, existing)
          : false
        const basePhase = incoming.phase === 'ready' && existing?.phase === 'idle' && !activityChanged
          ? 'idle'
          : incoming.phase
        const hasCompactingHint = isCompactingContextText(incoming.description) || isCompactingContextText(incoming.lastToolName)
        const phase = basePhase === 'processing' && hasCompactingHint && existing?.phase !== 'compacting'
          ? 'compacting'
          : basePhase
        const enteredEffectiveIdle = phase === 'idle' && existing?.phase !== 'idle'
        const s: SessionState = {
          ...incoming,
          phase,
          description: phase === 'compacting'
            ? incoming.description ?? 'Compacting context'
            : isCompactingContextText(incoming.description)
              ? undefined
              : incoming.description,
          lastToolName: phase === 'compacting'
            ? incoming.lastToolName
            : isCompactingContextText(incoming.lastToolName)
              ? undefined
              : incoming.lastToolName,
          lastToolTarget: phase === 'compacting'
            ? incoming.lastToolTarget
            : isCompactingContextText(incoming.lastToolName)
              ? undefined
              : incoming.lastToolTarget,
          lastToolStatus: phase === 'compacting'
            ? incoming.lastToolStatus
            : isCompactingContextText(incoming.lastToolName)
              ? undefined
              : incoming.lastToolStatus,
          chatHistory: incoming.chatHistory?.length ? incoming.chatHistory : existing?.chatHistory ?? [],
          subagents: incoming.subagents ?? [],
          activeTools: incoming.activeTools ?? [],
          lastUserMessageAt: incoming.lastUserMessageAt
            ?? (lastUserMessageChanged ? now : existing?.lastUserMessageAt),
          idleSince: phase === 'idle'
            ? incoming.idleSince ?? existing?.idleSince ?? (enteredIdle || enteredEffectiveIdle ? now : undefined)
            : incoming.idleSince,
          taskCompletedAt: incoming.phase === 'done'
            ? incoming.taskCompletedAt ?? existing?.taskCompletedAt ?? (enteredDone ? now : undefined)
            : incoming.taskCompletedAt,
          lastActivityAt: incoming.lastActivityAt ?? (activityChanged ? now : existing?.lastActivityAt),
        }
        sessions[s.id] = s

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

        const hideNonBlockingOverlays = isInternalCodexPromptSession(s) || isCodexTitleMetadataOnlySession(s) || isProbeSession(s)

        // Detect context compaction starting from backend session snapshots.
        if (!hideNonBlockingOverlays && s.phase === 'compacting' && prev?.phase !== 'compacting') {
          const existingOverlay = state.overlayQueue.find((o) => o.sessionId === s.id && o.type === 'compacting')
          if (!existingOverlay) {
            newOverlays.push({
              id: `compacting-${s.id}-${Date.now()}`,
              sessionId: s.id,
              type: 'compacting',
              data: {},
              createdAt: Date.now(),
              suppressed,
            })
          }
        }

        // Detect a newly available assistant response and show the response overlay.
        const responseText = usefulCompletionText(s.responseText)
        const previousResponseText = usefulCompletionText(prev?.responseText)
        if (!hideNonBlockingOverlays && s.phase !== 'done' && responseText && responseText !== previousResponseText && !sessionEndedText(s.responseText)) {
          newOverlays.push({
            id: `response-${s.id}-${Date.now()}`,
            sessionId: s.id,
            type: 'response',
            data: {
              responseText,
              userMessage: s.lastUserMessage,
            },
            createdAt: Date.now(),
          })
        }

        // Backend session updates are the source of truth in Tauri mode, so
        // synthesize completion overlays when a session transitions to done.
        if (!hideNonBlockingOverlays && s.phase === 'done' && prev?.phase !== 'done' && !shouldSuppressCompletionOverlay(s)) {
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
      const sessionList = toList(sessions, now)
      if (activeSessionId && !sessions[activeSessionId]) {
        activeSessionId = sessionList[0]?.id ?? null
      }
      const overlayQueue = state.overlayQueue.filter((overlay) => {
        const session = sessions[overlay.sessionId]
        if (!session) return false
        if (overlay.type === 'permission') return Boolean(session.pendingPermission)
        if (overlay.type === 'question') return Boolean(session.pendingQuestion)
        if (overlay.type === 'plan') return Boolean(session.planTitle || session.planContent)
        if (overlay.type === 'compacting') return session.phase === 'compacting'
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
      const chatHistory = mergeLocalUserMessages(messages, session.chatHistory)
      const sessions = {
        ...state.sessions,
        [sessionId]: { ...session, chatHistory },
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

  setUsageSnapshots: (limits) => {
    set((state) => {
      const usageSnapshots = { ...state.usageSnapshots }
      for (const limit of limits) {
        if (!limit.provider) continue
        usageSnapshots[limit.provider] = limit
      }
      return {
        usageSnapshots,
        rateLimits: limits[0] ?? state.rateLimits,
      }
    })
  },

  setHookNotification: (notification) => {
    set({ hookNotification: notification })
  },

  applyIdleTimeout: (now = Date.now()) => {
    const idleTimeoutMinutes = useConfigStore.getState().idleTimeoutMinutes
    const timeoutMs = idleTimeoutMinutes * 60 * 1000
    let changed = false
    let shouldSave = false

    set((state) => {
      const sessions = { ...state.sessions }
      if (idleTimeoutMinutes > 0) {
        for (const session of Object.values(state.sessions)) {
          if (session.phase !== 'ready' && session.phase !== 'processing' && session.phase !== 'compacting') continue
          if (session.activeTools.some((tool) => tool.status === 'running')) continue
          const lastActivityAt = timestampToMs(session.lastActivityAt ?? session.startedAt) ?? now
          if (now - lastActivityAt <= timeoutMs) continue
          sessions[session.id] = {
            ...session,
            phase: 'idle',
            idleSince: session.idleSince ?? now,
            description: session.description,
          }
          changed = true
        }
      }

      const sessionList = toList(changed ? sessions : state.sessions, now)
      const listChanged = sessionList.length !== state.sessionList.length
        || sessionList.some((session, index) => session.id !== state.sessionList[index]?.id)

      if (!changed && !listChanged) return state
      shouldSave = changed
      return { sessions: changed ? sessions : state.sessions, sessionList }
    })

    if (shouldSave) saveSessionsDebounced()
  },

  setBaseLayer: (baseLayer) => {
    const panelMap: Record<BaseLayer, PanelState> = { compact: 'collapsed', expanded: 'hover', detail: 'expanded' }
    set({ baseLayer, panelState: panelMap[baseLayer] })
  },

  pushOverlay: (item) => {
    // Skip overlays for muted sessions
    const muted = useSessionStore.getState().mutedSessions[item.sessionId]
    if (muted && Date.now() < muted) return

    // During quiet hours, suppress non-blocking overlays.
    const isNonBlocking = item.type === 'response' || item.type === 'completion' || item.type === 'compacting'
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
