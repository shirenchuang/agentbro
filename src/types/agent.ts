/* Agent Island — Shared TypeScript Types */

export type AgentType = 'claude-code' | 'codex' | 'gemini-cli' | 'cursor' | 'opencode' | 'droid' | 'qoder' | 'codebuddy' | 'copilot' | 'kiro'

export type ToolStatus = 'running' | 'success' | 'error' | 'interrupted'

export type SessionPhase =
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

export type OverlayType = 'permission' | 'question' | 'plan' | 'completion' | 'response'

export const OVERLAY_PRIORITY: Record<OverlayType, number> = {
  permission: 100,
  plan: 90,
  question: 80,
  completion: 20,
  response: 10,
}

export interface OverlayItem {
  id: string
  sessionId: string
  type: OverlayType
  data: unknown
  createdAt: number
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
  toolName: string
  toolInput: string
  diff?: DiffContent
  options?: string[]
}

export interface AskQuestion {
  question: string
  options: string[]
}

export interface RateLimitInfo {
  fiveHourUsage: number     // percentage 0-100
  fiveHourRemaining: string // "1d6h" format
  sevenDayUsage: number
  sevenDayRemaining: string
}

export interface TerminalInfo {
  app: string
  pid: number
  tty: string
  tabId?: string
  paneId?: string
}

export interface SessionState {
  id: string
  agentType: AgentType
  project: string
  terminal: string
  phase: SessionPhase
  startedAt: number
  idleSince?: number
  duration: number
  tokens: TokenUsage
  rateLimits?: RateLimitInfo
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
  sessionTitle?: string
  planTitle?: string
  planContent?: string
  responseText?: string
  taskCompletedAt?: number // timestamp when task completed
}

export interface SubagentInfo {
  agentId: string
  description: string
  startedAt: number
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

// Chat message types for conversation view
export type ChatMessage =
  | { role: 'user'; content: string; timestamp: number }
  | { role: 'assistant'; content: string; timestamp: number }
  | { role: 'tool_use'; toolName: string; toolInput?: string; status: ToolStatus; result?: string; diff?: DiffContent; timestamp: number }
  | { role: 'permission'; toolName: string; toolInput?: string; diff?: DiffContent; options?: string[]; timestamp: number }
  | { role: 'thinking'; content: string; timestamp: number }
  | { role: 'error'; message: string; timestamp: number }

export type AgentEvent =
  | { type: 'session_start'; sessionId: string; project: string; terminal: string; agentType: AgentType }
  | { type: 'session_end'; sessionId: string }
  | { type: 'processing'; sessionId: string; description: string }
  | { type: 'tool_use'; sessionId: string; toolName: string; toolInput: string; toolTarget?: string; status: ToolStatus }
  | { type: 'permission_request'; sessionId: string; toolName: string; toolInput?: string; diff?: DiffContent; options?: string[] }
  | { type: 'ask_question'; sessionId: string; question: string; options: string[] }
  | { type: 'task_complete'; sessionId: string; summary: string }
  | { type: 'error'; sessionId: string; message: string }
  | { type: 'interrupt'; sessionId: string }
  | { type: 'context_compact'; sessionId: string; phase: 'pre' | 'post' }
  | { type: 'token_usage'; sessionId: string; input: number; output: number; cacheRead: number; cacheCreate: number }
  | { type: 'user_message'; sessionId: string; content: string }
