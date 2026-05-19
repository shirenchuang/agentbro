/* AgentBro — Tauri Event Hooks
 * Listens for backend events and syncs stores. No-ops in browser dev mode.
 */
import { useEffect } from 'react'
import { isTauri, getSessions, getUsageRateLimits, getUsageSnapshots, getConfig, listThemes, setCustomSounds, setSoundEventRule, setSoundQuietHours } from '../services/tauriApi'
import type { BackendSession, BackendConfig, ParsedMessage, ParsedMessageBlock } from '../services/tauriApi'
import { useSessionStore } from '../stores/sessionStore'
import { useConfigStore } from '../stores/configStore'
import { useThemeStore } from '../stores/themeStore'
import type { SoundChoice } from '../stores/configStore'
import type { SessionState, DiffContent, AgentType, ToolStatus, ChatMessage, RateLimitInfo } from '../types/agent'

let lastBackendThemeName: string | null = null
let pendingBackendThemeName: string | null = null
let pendingBackendThemeTimer: ReturnType<typeof setTimeout> | null = null

function normalizeBackendThemeName(theme?: string | null): string | null {
  return theme && theme !== 'system' ? theme : null
}

function markPendingBackendTheme(name: string) {
  pendingBackendThemeName = name
  if (pendingBackendThemeTimer) clearTimeout(pendingBackendThemeTimer)
  pendingBackendThemeTimer = setTimeout(() => {
    pendingBackendThemeName = null
    pendingBackendThemeTimer = null
  }, 5000)
}

function clearPendingBackendTheme(name?: string) {
  if (name && pendingBackendThemeName !== name) return
  pendingBackendThemeName = null
  if (pendingBackendThemeTimer) {
    clearTimeout(pendingBackendThemeTimer)
    pendingBackendThemeTimer = null
  }
}

function applyBackendThemeChange(theme?: string | null) {
  const backendThemeName = normalizeBackendThemeName(theme)
  if (!backendThemeName) return

  lastBackendThemeName = backendThemeName

  if (pendingBackendThemeName && pendingBackendThemeName !== backendThemeName) {
    return
  }

  clearPendingBackendTheme(backendThemeName)

  const store = useThemeStore.getState()
  if (store.activeThemeName !== backendThemeName) {
    store.setActiveTheme(backendThemeName)
  }
}

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

function parseToolInput(raw?: string | null): Record<string, unknown> | undefined {
  if (!raw) return undefined
  try {
    const parsed = JSON.parse(raw)
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return parsed as Record<string, unknown>
    }
  } catch {
    return undefined
  }
  return undefined
}

function getInputString(input: Record<string, unknown>, key: string): string {
  const value = input[key]
  return typeof value === 'string' ? value : ''
}

function buildDiffFromToolInput(name: string, input: Record<string, unknown>): DiffContent | undefined {
  const filePath = getInputString(input, 'file_path')
    || getInputString(input, 'filePath')
    || getInputString(input, 'path')
    || getInputString(input, 'notebook_path')
  if (!filePath) return undefined

  if (name === 'Edit' || name === 'NotebookEdit') {
    const oldContent = getInputString(input, 'old_string')
    const newContent = getInputString(input, 'new_string')
    if (!oldContent && !newContent) return undefined
    return contentPairToDiff(filePath, oldContent, newContent)
  }

  if (name === 'Write') {
    const newContent = getInputString(input, 'content')
    if (!newContent) return undefined
    return contentPairToDiff(filePath, '', newContent)
  }

  if (name === 'MultiEdit') {
    const rawEdits = input.edits
    let edits: Array<{ old_string?: string; new_string?: string }> = []
    if (Array.isArray(rawEdits)) {
      edits = rawEdits.filter((item): item is { old_string?: string; new_string?: string } => (
        Boolean(item) && typeof item === 'object'
      ))
    } else if (typeof rawEdits === 'string') {
      try {
        const parsed = JSON.parse(rawEdits)
        if (Array.isArray(parsed)) edits = parsed
      } catch {
        return undefined
      }
    }

    const oldContent = edits.map((edit) => edit.old_string ?? '').filter(Boolean).join('\n...\n')
    const newContent = edits.map((edit) => edit.new_string ?? '').filter(Boolean).join('\n...\n')
    if (!oldContent && !newContent) return undefined
    return contentPairToDiff(filePath, oldContent, newContent)
  }

  return undefined
}

function transformSession(bs: BackendSession): SessionState {
  // Preserve existing chatHistory/subagents/activeTools from the store
  // so that replaceAllSessions doesn't wipe them on each backend update
  const existing = useSessionStore.getState().sessions[bs.id]
  const backendSubagents = bs.subagents ?? []
  const subagents = backendSubagents.length > 0
    ? backendSubagents
    : (existing?.subagents ?? [])
  const lastUserMessage = bs.lastUserMessage ?? existing?.lastUserMessage

  return {
    id: bs.id,
    agentType: bs.agentType as AgentType,
    engineLabel: bs.engineLabel ?? undefined,
    engineConfigRoot: bs.engineConfigRoot ?? undefined,
    project: bs.project,
    cwd: bs.cwd,
    terminal: bs.terminal,
    phase: bs.phase as SessionState['phase'],
    startedAt: bs.startedAt,
    duration: bs.duration,
    tokens: bs.tokens,
    rateLimits: bs.rateLimits ?? undefined,
    statusLineText: bs.statusLineText ?? undefined,
    contextWindow: bs.contextWindow ?? undefined,
    lastMainAgentAt: bs.lastMainAgentAt ?? undefined,
    cacheTtlMs: bs.cacheTtlMs ?? undefined,
    pendingPermission: bs.pendingPermission ? {
      toolName: bs.pendingPermission.toolName,
      toolInput: bs.pendingPermission.toolInput,
      diff: parseDiff(bs.pendingPermission.diff),
      options: bs.pendingPermission.options ?? undefined,
    } : undefined,
    pendingQuestion: bs.pendingQuestion ? {
      ...bs.pendingQuestion,
      descriptions: bs.pendingQuestion.descriptions ?? undefined,
      header: bs.pendingQuestion.header ?? undefined,
      multiSelect: bs.pendingQuestion.multiSelect || undefined,
    } : undefined,
    planTitle: bs.pendingPlan?.title ?? undefined,
    planContent: bs.pendingPlan?.content ?? undefined,
    planPermissions: bs.pendingPlan?.permissions ?? undefined,
    lastToolName: bs.lastToolName ?? undefined,
    lastToolTarget: bs.lastToolTarget ?? undefined,
    lastToolStatus: (bs.lastToolStatus as ToolStatus) ?? undefined,
    sessionTitle: bs.sessionTitle ?? undefined,
    pid: bs.pid ?? undefined,
    tty: bs.tty ?? undefined,
    termProgram: bs.termProgram ?? undefined,
    termBundleId: bs.termBundleId ?? undefined,
    weztermPane: bs.weztermPane ?? undefined,
    zellijPaneId: bs.zellijPaneId ?? undefined,
    zellijSessionName: bs.zellijSessionName ?? undefined,
    cmuxSurfaceId: bs.cmuxSurfaceId ?? undefined,
    cmuxWorkspaceId: bs.cmuxWorkspaceId ?? undefined,
    chatHistory: existing?.chatHistory ?? [],
    subagents: subagents.map((subagent) => ({
      agentId: subagent.agentId,
      name: subagent.name ?? undefined,
      agentType: subagent.agentType ?? undefined,
      description: subagent.description,
      transcriptPath: subagent.transcriptPath ?? undefined,
      agentTranscriptPath: subagent.agentTranscriptPath ?? undefined,
      lastAssistantMessage: subagent.lastAssistantMessage ?? undefined,
      startedAt: subagent.startedAt,
      completedAt: subagent.completedAt ?? undefined,
      status: subagent.status === 'error'
        ? 'error'
        : subagent.status === 'completed'
          ? 'completed'
          : 'running',
      tools: subagent.tools ?? [],
    })),
    activeTools: (bs.activeTools ?? existing?.activeTools ?? []).map((tool) => ({
      toolUseId: tool.toolUseId,
      toolName: tool.toolName,
      status: tool.status === 'error'
        ? 'error'
        : tool.status === 'success'
          ? 'success'
          : 'running',
      startedAt: tool.startedAt,
      completedAt: tool.completedAt ?? undefined,
      error: tool.error ?? undefined,
      toolInput: tool.toolInput ?? undefined,
      diff: buildDiffFromToolInput(tool.toolName, parseToolInput(tool.toolInput) ?? {}),
    })),
    tasks: (bs.tasks ?? existing?.tasks ?? []).map((task) => ({
      id: task.id,
      name: task.name,
      status: task.status === 'completed'
        ? 'completed'
        : task.status === 'in_progress'
          ? 'in_progress'
          : 'pending',
    })),
    isYoloMode: bs.isYoloMode || undefined,
    notice: bs.notice ?? existing?.notice,
    lastUserMessage,
    lastUserMessageAt: lastUserMessage === existing?.lastUserMessage ? existing?.lastUserMessageAt : undefined,
    responseText: bs.lastResponse ?? undefined,
    description: bs.lastThought ?? bs.description ?? undefined,
  }
}

function usageProviderForAgent(agentType: AgentType | string): string {
  if (agentType === 'claude-code') return 'claude-code'
  return agentType
}

function providerLabelForAgent(agentType: AgentType | string, engineLabel?: string): string {
  if (engineLabel) return engineLabel
  if (agentType === 'claude-code') return 'Claude'
  if (agentType === 'codex') return 'Codex'
  return String(agentType)
}

function usageSnapshotFromSession(session: SessionState): RateLimitInfo | undefined {
  if (!session.rateLimits) return undefined
  const provider = usageProviderForAgent(session.agentType)
  return {
    ...session.rateLimits,
    provider,
    providerLabel: providerLabelForAgent(session.agentType, session.engineLabel),
    source: session.rateLimits.source ?? 'agent-statusline',
    updatedAt: session.rateLimits.updatedAt ?? Date.now(),
    windows: session.rateLimits.windows && session.rateLimits.windows.length > 0
      ? session.rateLimits.windows
      : [
        {
          id: 'five_hour',
          title: '5h',
          usedPercent: session.rateLimits.fiveHourUsage,
          remainingPercent: Math.max(0, 100 - session.rateLimits.fiveHourUsage),
          remainingLabel: session.rateLimits.fiveHourRemaining,
          windowMinutes: 300,
        },
        {
          id: 'seven_day',
          title: '7d',
          usedPercent: session.rateLimits.sevenDayUsage,
          remainingPercent: Math.max(0, 100 - session.rateLimits.sevenDayUsage),
          remainingLabel: session.rateLimits.sevenDayRemaining,
          windowMinutes: 10080,
        },
      ],
  }
}

function sessionUsageSnapshots(sessions: SessionState[]): RateLimitInfo[] {
  return sessions.map(usageSnapshotFromSession).filter(Boolean) as RateLimitInfo[]
}

function applyBackendConfig(config: BackendConfig) {
  const store = useConfigStore.getState()
  store.updateConfig('soundEnabled', config.soundEnabled)
  store.updateConfig('volume', Math.round(config.soundVolume * 100))
  store.updateConfig('launchAtLogin', config.launchAtLogin ?? false)
  store.updateConfig('autoHide', config.autoHide)
  store.updateConfig('smartSuppression', config.smartSuppression)
  store.updateConfig('hideInFullscreen', config.hideInFullscreen ?? false)
  store.updateConfig('showUsageQuota', config.showTokenUsage ?? true)
  store.updateConfig('usageQueryEnabled', config.usageQueryEnabled ?? true)
  store.updateConfig('autoHideNoSessions', config.autoHideNoSessions)
  store.updateConfig('displayMonitor', config.displayId)
  store.updateConfig('globalShortcut', config.globalShortcut)
  store.updateConfig('shortcutApprove', config.shortcutApprove)
  store.updateConfig('shortcutApproveEnabled', config.shortcutApproveEnabled)
  store.updateConfig('shortcutDeny', config.shortcutDeny)
  store.updateConfig('shortcutDenyEnabled', config.shortcutDenyEnabled)
  store.updateConfig('shortcutSkip', config.shortcutSkip)
  store.updateConfig('shortcutSkipEnabled', config.shortcutSkipEnabled)
  if (config.soundEvents && Object.keys(config.soundEvents).length > 0) {
    const updatedEvents = store.soundEvents.map((event) => ({
      ...event,
      enabled: config.soundRules?.[event.id]?.enabled ?? config.soundEvents[event.id] ?? event.enabled,
    }))
    store.updateConfig('soundEvents', updatedEvents)
  }
  if (config.soundRules && Object.keys(config.soundRules).length > 0) {
    store.updateConfig('soundRules', Object.fromEntries(
      Object.entries(config.soundRules).map(([id, rule]) => [
        id,
        { enabled: rule.enabled, sound: rule.sound as SoundChoice },
      ]),
    ))
  }
  if (Array.isArray(config.customSounds)) {
    store.updateConfig('customSounds', config.customSounds)
  }
  store.updateConfig('soundPack', config.soundPack as 'eight-bit' | 'subtle' | 'synth' | 'system' | 'none' | 'custom')
  store.updateConfig('probeSessionFilter', config.probeSessionFilter)
  store.updateConfig('hookDoctorEnabled', config.hookDoctorEnabled ?? false)
  store.updateConfig('sessionLauncherEnabled', config.sessionLauncherEnabled ?? false)
  store.updateConfig('customHookTemplatesEnabled', config.customHookTemplatesEnabled ?? false)
  store.updateConfig('tipsEnabled', config.tipsEnabled)
  store.updateConfig('pixelCursorEnabled', config.pixelCursorEnabled)
  store.updateConfig('confettiEnabled', config.confettiEnabled)
  store.updateConfig('islandSurfaceMode', config.islandSurfaceMode ?? 'island')
  store.updateConfig('islandPetScale', config.islandPetScale ?? 72)
  store.updateConfig('islandPetWindowOrigin', config.islandPetWindowOrigin ?? null)
  store.updateConfig('followFocus', config.followFocus)
  store.updateConfig('quietHours', {
    enabled: config.quietHoursEnabled,
    start: config.quietHoursStart,
    end: config.quietHoursEnd,
  })
  store.updateConfig('idleTimeoutMinutes', config.idleTimeoutMinutes ?? 5)
}

function syncSoundEventSettingsToBackend() {
  const { soundEvents, soundRules } = useConfigStore.getState()
  const events = soundEvents
  events.forEach((event) => {
    const rule = soundRules[event.id] ?? { enabled: event.enabled, sound: 'default' }
    setSoundEventRule(event.id, rule.enabled, rule.sound)
      .catch((e) => console.error('[tauri] setSoundEventRule:', e))
  })
}

function syncQuietHoursToBackend() {
  const quietHours = useConfigStore.getState().quietHours
  setSoundQuietHours(quietHours.enabled, quietHours.start, quietHours.end)
    .catch((e) => console.error('[tauri] setSoundQuietHours:', e))
}

function syncCustomSoundsToBackend() {
  const customSounds = useConfigStore.getState().customSounds
  setCustomSounds(customSounds)
    .catch((e) => console.error('[tauri] setCustomSounds:', e))
}

function syncThemesFromBackend(configTheme?: string) {
  const backendThemeName = normalizeBackendThemeName(configTheme)
  const activeThemeNameAtSyncStart = useThemeStore.getState().activeThemeName
  if (backendThemeName) {
    lastBackendThemeName = backendThemeName
  }

  listThemes().then((themes) => {
    const store = useThemeStore.getState()
    store.loadThemes(themes)
    if (
      backendThemeName &&
      backendThemeName === lastBackendThemeName &&
      store.activeThemeName === activeThemeNameAtSyncStart
    ) {
      store.setActiveTheme(backendThemeName)
    }
  }).catch(e => console.error('[tauri] listThemes:', e))
}

let usageRateLimitRefreshInFlight = false
let usageRateLimitLastRefreshAt = 0
let usageRateLimitRefreshQueued = false
const USAGE_RATE_LIMIT_REFRESH_INTERVAL_MS = 60_000

function refreshUsageRateLimits(force = false) {
  if (!useConfigStore.getState().usageQueryEnabled) return
  const now = Date.now()
  if (!force && now - usageRateLimitLastRefreshAt < USAGE_RATE_LIMIT_REFRESH_INTERVAL_MS) return
  if (usageRateLimitRefreshInFlight) {
    usageRateLimitRefreshQueued = true
    return
  }

  usageRateLimitRefreshInFlight = true
  usageRateLimitLastRefreshAt = now
  getUsageSnapshots()
    .then(async (snapshots) => {
      if (snapshots.length > 0) {
        useSessionStore.getState().setUsageSnapshots(snapshots)
        return
      }
      const rateLimits = await getUsageRateLimits()
      if (rateLimits) {
        useSessionStore.getState().setUsageSnapshots([rateLimits])
      }
    })
    .catch(e => console.error('[tauri] getUsageSnapshots:', e))
    .finally(() => {
      usageRateLimitRefreshInFlight = false
      if (usageRateLimitRefreshQueued) {
        usageRateLimitRefreshQueued = false
        window.setTimeout(() => refreshUsageRateLimits(), 500)
      }
    })
}

// ── Hooks ────────────────────────────────────────────────────────

/** Listen for session-update events from the backend and sync sessionStore. */
export function useSessionEvents() {
  useEffect(() => {
    if (!isTauri()) return

    let unlisten: (() => void) | undefined
    const usageRateLimitTimer = window.setInterval(refreshUsageRateLimits, 60_000)

    // Load initial sessions
    getSessions().then(sessions => {
      const transformed = sessions.map(transformSession)
      const store = useSessionStore.getState()
      store.replaceAllSessions(transformed)
      const usageSnapshots = sessionUsageSnapshots(transformed)
      if (usageSnapshots.length > 0) store.setUsageSnapshots(usageSnapshots)
      refreshUsageRateLimits(true)
    }).catch(e => console.error('[tauri] getSessions:', e))

    // Listen for live updates (dynamic import to avoid crash in browser dev mode)
    import('@tauri-apps/api/event').then(({ listen }) => {
      listen<{ sessions: BackendSession[]; suppressed?: boolean }>('session-update', (event) => {
        const store = useSessionStore.getState()
        const sessions = event.payload.sessions.map(transformSession)
        store.replaceAllSessions(sessions, { suppressed: event.payload.suppressed === true })
        const usageSnapshots = sessionUsageSnapshots(sessions)
        if (usageSnapshots.length > 0) store.setUsageSnapshots(usageSnapshots)
        refreshUsageRateLimits()
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

    return () => {
      window.clearInterval(usageRateLimitTimer)
      unlisten?.()
    }
  }, [])
}

/** Listen for config-changed events from the backend and sync configStore. */
export function useConfigSync() {
  useEffect(() => {
    if (!isTauri()) return

    let unlisten: (() => void) | undefined

    getConfig().then((config) => {
      applyBackendConfig(config)
      syncThemesFromBackend(config.theme)
      if (!config.soundRules || Object.keys(config.soundRules).length === 0) {
        syncSoundEventSettingsToBackend()
      }
      syncQuietHoursToBackend()
      syncCustomSoundsToBackend()
    })
      .catch(e => console.error('[tauri] getConfig:', e))

    // Dynamic import to avoid crash in browser dev mode
    import('@tauri-apps/api/event').then(({ listen }) => {
      listen<BackendConfig>('config-changed', (event) => {
        applyBackendConfig(event.payload)
        applyBackendThemeChange(event.payload.theme)
      }).then(fn => { unlisten = fn })
        .catch(e => console.error('[tauri] listen config-changed:', e))
    }).catch(e => console.error('[tauri] import event:', e))

    const handleThemeSync = (event: Event) => {
      const detail = (event as CustomEvent<{ status?: string; name?: string }>).detail
      if (!detail?.name) return
      if (detail.status === 'pending') {
        markPendingBackendTheme(detail.name)
      } else if (detail.status === 'failed') {
        clearPendingBackendTheme(detail.name)
      }
    }
    window.addEventListener('agentbro-theme-sync', handleThemeSync)

    return () => {
      unlisten?.()
      window.removeEventListener('agentbro-theme-sync', handleThemeSync)
    }
  }, [])
}

// ── Conversation Update Mapping ─────────────────────────────────

/** Map a list of ParsedMessages (from Rust) into ChatMessages (frontend type). */
export function mapParsedMessages(parsed: ParsedMessage[]): ChatMessage[] {
  const messages: ChatMessage[] = []

  const lastAssistant = () => {
    const last = messages[messages.length - 1]
    return last?.role === 'assistant' ? last : undefined
  }

  const formatToolInput = (input: Record<string, string>) =>
    Object.entries(input).map(([k, v]) => `${k}: ${v}`).join('\n')

  const applyToolResult = (block: Extract<ParsedMessageBlock, { type: 'tool_result' }>) => {
    const assistant = lastAssistant()
    const tool = assistant?.toolCalls?.find((candidate) => candidate.toolUseId === block.toolUseId)
    if (!tool) return
    tool.status = block.isError ? 'error' : 'success'
    if (block.content) {
      tool.result = block.content.length > 1200 ? `${block.content.slice(0, 1200)}...` : block.content
    }
  }

  for (const msg of parsed) {
    const ts = msg.timestamp ? new Date(msg.timestamp).getTime() : Date.now()
    const imageBlocks = msg.blocks.filter((block): block is Extract<ParsedMessageBlock, { type: 'image' }> => block.type === 'image')
    const images = imageBlocks.map((block) => block.source).filter(Boolean)

    if (msg.role === 'user') {
      const text = msg.blocks
        .filter((block): block is Extract<ParsedMessageBlock, { type: 'text' }> => block.type === 'text')
        .map((block) => block.text)
        .join('')
      const toolResults = msg.blocks.filter((block): block is Extract<ParsedMessageBlock, { type: 'tool_result' }> => block.type === 'tool_result')

      for (const result of toolResults) {
        applyToolResult(result)
      }

      if (!text && images.length === 0) continue

      messages.push({
        role: 'user',
        content: text,
        timestamp: ts,
        ...(images.length > 0 ? { images } : {}),
      })
      continue
    }

    const textParts: string[] = []
    const thinkingParts: string[] = []
    const toolCalls: NonNullable<Extract<ChatMessage, { role: 'assistant' }>['toolCalls']> = []
    const assistantImages = [...images]

    for (const block of msg.blocks) {
      if (block.type === 'text') {
        textParts.push(block.text)
      } else if (block.type === 'thinking') {
        thinkingParts.push(block.thinking)
      } else if (block.type === 'tool_use') {
        const tu = block as Extract<ParsedMessageBlock, { type: 'tool_use' }>
        toolCalls.push({
          toolUseId: tu.id,
          toolName: tu.name,
          toolInput: formatToolInput(tu.input),
          diff: buildDiffFromToolInput(tu.name, tu.input),
          status: 'success' as ToolStatus,
        })
      } else if (block.type === 'tool_result') {
        applyToolResult(block)
      } else if (block.type === 'interrupted') {
        textParts.push('[Request interrupted by user]')
      }
    }

    const content = textParts.join('')
    const thinking = thinkingParts.join('\n\n')

    if (!content && !thinking && toolCalls.length === 0 && assistantImages.length === 0) continue

    const previous = lastAssistant()
    if (previous) {
      if (content) {
        if (previous.trailingContent) {
          previous.content = previous.content
            ? `${previous.content}\n\n${previous.trailingContent}`
            : previous.trailingContent
          previous.messageCount = (previous.messageCount ?? 0) + 1
        }
        previous.trailingContent = content
      }
      if (thinking) {
        previous.thinking = previous.thinking ? `${previous.thinking}\n\n${thinking}` : thinking
        previous.thinkingCount = (previous.thinkingCount ?? 0) + 1
      }
      if (toolCalls.length > 0) {
        previous.toolCalls = previous.toolCalls ? [...previous.toolCalls, ...toolCalls] : toolCalls
      }
      if (assistantImages.length > 0) {
        previous.images = previous.images ? [...previous.images, ...assistantImages] : assistantImages
      }
      continue
    }

    messages.push({
      role: 'assistant',
      content: '',
      timestamp: ts,
      trailingContent: content || undefined,
      thinking: thinking || undefined,
      thinkingCount: thinking ? 1 : undefined,
      toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
      ...(assistantImages.length > 0 ? { images: assistantImages } : {}),
    })
  }

  for (const msg of messages) {
    if (msg.role !== 'assistant') continue
    const hasProcess = !!msg.thinking || !!msg.toolCalls?.length || !!msg.content
    if (!hasProcess && msg.trailingContent) {
      msg.content = msg.trailingContent
      msg.trailingContent = undefined
    }
  }

  return messages
}

function contentPairToDiff(filePath: string, oldContent: string, newContent: string): DiffContent {
  const lines = [
    ...oldContent.split('\n').filter((line) => line.length > 0).map((content, index) => ({
      type: 'remove' as const,
      lineNumber: index + 1,
      content,
    })),
    ...newContent.split('\n').filter((line) => line.length > 0).map((content, index) => ({
      type: 'add' as const,
      lineNumber: index + 1,
      content,
    })),
  ]

  return { filePath, lines }
}

/** Map a single live block into ChatMessages without assistant turn aggregation. */
export function mapParsedBlocksFlat(msg: ParsedMessage): ChatMessage[] {
  const ts = msg.timestamp ? new Date(msg.timestamp).getTime() : Date.now()
  const messages: ChatMessage[] = []

  for (const block of msg.blocks) {
    switch (block.type) {
      case 'text':
        messages.push({
          role: msg.role === 'user' ? 'user' : 'assistant',
          content: block.text,
          timestamp: ts,
        })
        break

      case 'tool_use': {
        const tu = block as Extract<ParsedMessageBlock, { type: 'tool_use' }>
          messages.push({
            role: 'tool_use',
            toolName: tu.name,
            toolUseId: tu.id,
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
        break
      }

      case 'thinking':
        messages.push({
          role: 'thinking',
          content: block.thinking,
          timestamp: ts,
        })
        break

      case 'image':
        messages.push({
          role: msg.role === 'user' ? 'user' : 'assistant',
          content: '',
          timestamp: ts,
          images: [block.source],
        })
        break

      case 'interrupted':
        messages.push({
          role: 'assistant',
          content: '[Request interrupted by user]',
          timestamp: ts,
        })
        break
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

/** Listen for hook self-recovery notices and expose them in the island. */
export function useHookRecoveryEvents() {
  useEffect(() => {
    if (!isTauri()) return

    let unlistenRestored: (() => void) | undefined
    let unlistenFailed: (() => void) | undefined
    let clearTimer: ReturnType<typeof setTimeout> | undefined

    const show = (notification: 'restored' | 'rate_limited') => {
      const store = useSessionStore.getState()
      store.setHookNotification(notification)
      if (clearTimer) clearTimeout(clearTimer)
      clearTimer = setTimeout(() => {
        useSessionStore.getState().setHookNotification(null)
      }, 5000)
    }

    import('@tauri-apps/api/event').then(({ listen }) => {
      listen<string>('hook-recovery', () => show('restored'))
        .then(fn => { unlistenRestored = fn })
        .catch(e => console.error('[tauri] listen hook-recovery:', e))
      listen<void>('hook-recovery-failed', () => show('rate_limited'))
        .then(fn => { unlistenFailed = fn })
        .catch(e => console.error('[tauri] listen hook-recovery-failed:', e))
    }).catch(e => console.error('[tauri] import event:', e))

    return () => {
      unlistenRestored?.()
      unlistenFailed?.()
      if (clearTimer) clearTimeout(clearTimer)
    }
  }, [])
}

/** Combined init hook — call once in App. */
export function useTauriInit() {
  useSessionEvents()
  useConfigSync()
  useConversationUpdates()
  useHookRecoveryEvents()
}
