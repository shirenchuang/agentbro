/* AgentBro — Shared TypeScript Types */

export type AgentType =
  | 'claude-code' | 'codex' | 'gemini-cli'
  | 'cursor' | 'cursor-cli'
  | 'copilot'
  | 'trae' | 'traecli' | 'traecn'
  | 'qoder' | 'qoder-cli'
  | 'codebuddy' | 'codebuddycn'
  | 'qwen' | 'kimi' | 'deepseek' | 'opencode'
  | 'droid' | 'stepfun' | 'antigravity'
  | 'workbuddy' | 'hermes' | 'pi' | 'kiro'

export type ToolStatus = 'running' | 'success' | 'error' | 'interrupted'

export type SessionPhase =
  | 'ready'
  | 'idle'
  | 'processing'
  | 'waiting_approval'
  | 'waiting_input'
  | 'compacting'
  | 'done'
  | 'error'
  | 'interrupted'

export type PanelState = 'collapsed' | 'hover' | 'expanded'

export type BaseLayer = 'compact' | 'expanded' | 'detail'
export type DisplayLevel = 'dormant' | 'compact' | 'visible'

export type OverlayType = 'permission' | 'question' | 'plan' | 'completion' | 'response' | 'compacting'

export const OVERLAY_PRIORITY: Record<OverlayType, number> = {
  permission: 100,
  plan: 90,
  question: 80,
  completion: 20,
  response: 10,
  compacting: 15,
}

export interface OverlayItem {
  id: string
  sessionId: string
  type: OverlayType
  data: unknown
  createdAt: number
  suppressed?: boolean
}

export interface TokenUsage {
  input: number
  output: number
  cacheRead: number
  cacheCreate: number
}

export interface DiffLine {
  type: 'add' | 'remove' | 'context'
  lineNumber: number
  content: string
}

export interface DiffContent {
  filePath: string
  lines: DiffLine[]
}

export interface PermissionRequest {
  toolUseId?: string
  toolName: string
  toolInput: string
  diff?: DiffContent
  options?: string[]
}

export interface AskQuestion {
  question: string
  options: string[]
  descriptions?: string[]
  header?: string
  multiSelect?: boolean
  toolUseId?: string
  source?: string
  responseMode?: string
  questions?: Array<{
    id?: string | null
    question: string
    header?: string | null
    options: Array<{ label: string; description?: string | null }>
    multiSelect?: boolean
  }>
}

export interface UsageRateWindow {
  id: string
  title: string
  usedPercent: number
  remainingPercent?: number | null
  remainingLabel?: string
  resetsAt?: string | null
  windowMinutes?: number | null
}

export interface RateLimitInfo {
  fiveHourUsage: number     // percentage 0-100
  fiveHourRemaining: string // "1d6h" format
  sevenDayUsage: number
  sevenDayRemaining: string
  provider?: AgentType | string
  providerLabel?: string
  source?: string
  updatedAt?: number
  windows?: UsageRateWindow[]
}

export interface ContextWindowInfo {
  totalInputTokens: number
  totalOutputTokens: number
  contextWindowSize: number
  usedPercentage: number | null
}

export interface TerminalInfo {
  app: string
  pid: number
  tty: string
  tabId?: string
  paneId?: string
}

export type SessionNoticeKind =
  | 'terminal_approval'
  | 'terminal_question'
  | 'restart'
  | 'trust'
  | 'extension'
  | 'status_warning'
  | 'compact_complete'

export interface SessionNotice {
  kind: SessionNoticeKind
  title: string
  detail?: string
  actionLabel?: string
}

export interface SessionState {
  id: string
  agentType: AgentType
  engineLabel?: string
  engineConfigRoot?: string
  project: string
  cwd?: string
  terminal: string
  phase: SessionPhase
  startedAt: number
  idleSince?: number
  unattendedSince?: number // timestamp when session entered a waiting state
  duration: number
  tokens: TokenUsage
  rateLimits?: RateLimitInfo
  statusLineText?: string
  contextWindow?: ContextWindowInfo
  lastMainAgentAt?: number
  cacheTtlMs?: number
  pendingPermission?: PermissionRequest
  pendingQuestion?: AskQuestion
  lastToolName?: string
  lastToolTarget?: string
  lastToolStatus?: ToolStatus
  description?: string
  chatHistory: ChatMessage[]
  subagents: SubagentInfo[]
  activeTools: ToolResult[]
  tasks?: TaskInfo[]
  lastUserMessage?: string
  lastUserMessageAt?: number
  sessionTitle?: string
  pid?: number
  tty?: string
  termProgram?: string
  termBundleId?: string
  weztermPane?: string
  zellijPaneId?: string
  zellijSessionName?: string
  cmuxSurfaceId?: string
  cmuxWorkspaceId?: string
  planTitle?: string
  planContent?: string
  planPermissions?: string[]
  responseText?: string
  taskCompletedAt?: number // timestamp when task completed
  isYoloMode?: boolean
  notice?: SessionNotice
  lastActivityAt?: number // timestamp for processing timeout
}

export interface SubagentInfo {
  agentId: string
  name?: string
  agentType?: string
  description: string
  transcriptPath?: string
  agentTranscriptPath?: string
  lastAssistantMessage?: string
  startedAt: number
  completedAt?: number
  status: 'running' | 'completed' | 'error'
  tools: string[]
}

export interface ToolResult {
  toolUseId: string
  toolName: string
  status: 'running' | 'success' | 'error'
  startedAt: number
  completedAt?: number
  error?: string
  toolInput?: string
  diff?: DiffContent
}

export interface TaskInfo {
  id: string
  name: string
  status: 'pending' | 'in_progress' | 'completed'
}

export interface McpToolInfo {
  server: string
  tool: string
  displayName: string
}

export interface ChatToolCall {
  toolUseId?: string
  toolName: string
  toolInput?: string
  status: ToolStatus
  result?: string
  diff?: DiffContent
}

// Chat message types for conversation view
export type ChatMessage =
  | { role: 'user'; content: string; timestamp: number; images?: string[] }
  | {
      role: 'assistant'
      content: string
      timestamp: number
      images?: string[]
      toolCalls?: ChatToolCall[]
      thinking?: string
      thinkingCount?: number
      messageCount?: number
      trailingContent?: string
    }
  | ({ role: 'tool_use'; timestamp: number } & ChatToolCall)
  | { role: 'permission'; toolName: string; toolInput?: string; diff?: DiffContent; options?: string[]; timestamp: number }
  | { role: 'thinking'; content: string; timestamp: number }
  | { role: 'error'; message: string; timestamp: number }

export type AgentEvent =
  | { type: 'session_start'; sessionId: string; project: string; terminal: string; agentType: AgentType; cwd?: string }
  | { type: 'session_end'; sessionId: string }
  | { type: 'processing'; sessionId: string; description: string }
  | { type: 'tool_use'; sessionId: string; toolName: string; toolInput: string; toolTarget?: string; status: ToolStatus }
  | { type: 'permission_request'; sessionId: string; toolName: string; toolInput?: string; diff?: DiffContent; options?: string[] }
  | { type: 'ask_question'; sessionId: string; question: string; options: string[]; descriptions?: string[]; header?: string; multiSelect?: boolean; questions?: AskQuestion['questions'] }
  | { type: 'task_complete'; sessionId: string; summary: string }
  | { type: 'error'; sessionId: string; message: string }
  | { type: 'interrupt'; sessionId: string }
  | { type: 'context_compact'; sessionId: string; phase: 'pre' | 'post' }
  | { type: 'token_usage'; sessionId: string; input: number; output: number; cacheRead: number; cacheCreate: number }
  | { type: 'plan_request'; sessionId: string; planTitle: string; planContent: string; requestedPermissions?: string[] }
  | { type: 'task_update'; sessionId: string; taskId: string; subject: string; status: 'pending' | 'in_progress' | 'completed' }
  | { type: 'user_message'; sessionId: string; content: string }
