/* Hover List — Flat session list sorted by priority */
import { useCallback, useEffect, useRef, useState, useMemo, type KeyboardEvent, type MouseEvent } from 'react'
import { useTranslation } from 'react-i18next'
import { AnimatePresence, motion } from 'framer-motion'
import type { PermissionRequest, SessionState, SubagentInfo, TaskInfo } from '../../types/agent'
import { computePriority } from '../../types/priority'
import { PixelIndicator } from './PixelIndicator'
import { MascotRouter } from './mascots'
import { useConfigStore } from '../../stores/configStore'
import { useSessionStore } from '../../stores/sessionStore'
import { respondAutoApprove, respondPermission, respondPlan, respondQuestion } from '../../services/tauriApi'
import { formatDurationShort } from '../../utils/time'
import { getToolActivityLabel } from '../../utils/toolLabels'
import { getAgentDisplayName, getSessionAppLabel, getSessionTerminalLabel, isPassiveSession, shouldShowAgentBadge } from '../../utils/sessionDisplay'
import { DiffView } from './DiffView'
import './HoverList.css'

interface HoverListProps {
  sessions: SessionState[]
  onSessionClick: (sessionId: string) => void
  onJumpToTerminal?: (sessionId: string) => void
  focusFilteredEmpty?: boolean
}

const AGENT_BADGE_COLORS: Record<string, { bg: string; text: string }> = {
  'claude-code': { bg: 'rgba(59, 130, 246, 0.15)', text: '#3b82f6' },
  'codex': { bg: 'rgba(16, 185, 129, 0.15)', text: '#10b981' },
  'gemini-cli': { bg: 'rgba(99, 102, 241, 0.15)', text: '#818cf8' },
  'cursor': { bg: 'rgba(139, 92, 246, 0.15)', text: '#a78bfa' },
  'cursor-cli': { bg: 'rgba(139, 92, 246, 0.15)', text: '#a78bfa' },
  'opencode': { bg: 'rgba(139, 92, 246, 0.15)', text: '#8b5cf6' },
  'droid': { bg: 'rgba(239, 68, 68, 0.15)', text: '#ef4444' },
  'qoder': { bg: 'rgba(245, 158, 11, 0.15)', text: '#f59e0b' },
  'qoder-cli': { bg: 'rgba(245, 158, 11, 0.15)', text: '#f59e0b' },
  'codebuddy': { bg: 'rgba(20, 184, 166, 0.15)', text: '#14b8a6' },
  'codebuddycn': { bg: 'rgba(20, 184, 166, 0.15)', text: '#14b8a6' },
  'copilot': { bg: 'rgba(14, 165, 233, 0.15)', text: '#0ea5e9' },
  'kiro': { bg: 'rgba(245, 158, 11, 0.15)', text: '#f59e0b' },
  'trae': { bg: 'rgba(34, 197, 94, 0.15)', text: '#22c55e' },
  'traecli': { bg: 'rgba(34, 197, 94, 0.15)', text: '#22c55e' },
  'traecn': { bg: 'rgba(34, 197, 94, 0.15)', text: '#22c55e' },
  'kimi': { bg: 'rgba(168, 85, 247, 0.15)', text: '#a855f7' },
  'qwen': { bg: 'rgba(99, 102, 241, 0.15)', text: '#818cf8' },
  'stepfun': { bg: 'rgba(236, 72, 153, 0.15)', text: '#ec4899' },
  'antigravity': { bg: 'rgba(168, 85, 247, 0.15)', text: '#a855f7' },
  'workbuddy': { bg: 'rgba(14, 165, 233, 0.15)', text: '#0ea5e9' },
  'hermes': { bg: 'rgba(245, 158, 11, 0.15)', text: '#f59e0b' },
  'pi': { bg: 'rgba(34, 197, 94, 0.15)', text: '#22c55e' },
}

const TERMINAL_BADGE_COLORS: Record<string, { bg: string; text: string }> = {
  'iTerm2': { bg: 'rgba(255, 255, 255, 0.08)', text: '#8d8d93' },
  'Terminal': { bg: 'rgba(107, 114, 128, 0.15)', text: '#9ca3af' },
  'Warp': { bg: 'rgba(99, 102, 241, 0.15)', text: '#818cf8' },
  'Alacritty': { bg: 'rgba(245, 158, 11, 0.15)', text: '#f59e0b' },
  'Kitty': { bg: 'rgba(236, 72, 153, 0.15)', text: '#ec4899' },
  'WezTerm': { bg: 'rgba(168, 85, 247, 0.15)', text: '#a855f7' },
  'VS Code': { bg: 'rgba(14, 165, 233, 0.15)', text: '#0ea5e9' },
  'Cursor': { bg: 'rgba(139, 92, 246, 0.15)', text: '#a78bfa' },
  'Ghostty': { bg: 'rgba(255, 255, 255, 0.08)', text: '#9ca3af' },
  'Windsurf': { bg: 'rgba(14, 165, 233, 0.15)', text: '#0ea5e9' },
  'Zed': { bg: 'rgba(255, 255, 255, 0.08)', text: '#9ca3af' },
  'cmux': { bg: 'rgba(255, 255, 255, 0.08)', text: '#9ca3af' },
}

const DEFAULT_BADGE = { bg: 'rgba(107, 114, 128, 0.15)', text: '#9ca3af' }

const HOVER_SPEED_MS: Record<string, number> = {
  instant: 0.05,
  normal: 0.2,
  slow: 0.4,
}

function getAgentBadge(session: SessionState): { bg: string; text: string } {
  const label = getAgentDisplayName(session).toLowerCase()
  if (label === 'antcc') {
    return { bg: 'rgba(255, 47, 129, 0.16)', text: '#ff2f81' }
  }
  return AGENT_BADGE_COLORS[session.agentType] || DEFAULT_BADGE
}

function stripMarkdown(text: string): string {
  return text
    .replace(/```[\s\S]*?```/g, '')
    .replace(/`([^`]+)`/g, '$1')
    .replace(/!\[([^\]]*)\]\([^)]+\)/g, '$1')
    .replace(/\[([^\]]+)\]\([^)]+\)/g, '$1')
    .replace(/^#{1,6}\s+/gm, '')
    .replace(/(\*{1,3}|_{1,3})(.+?)\1/g, '$2')
    .replace(/~~(.+?)~~/g, '$1')
    .replace(/^[-*_]{3,}\s*$/gm, '')
    .replace(/^>\s?/gm, '')
    .replace(/^[\s]*[-*+]\s+/gm, '')
    .replace(/^[\s]*\d+\.\s+/gm, '')
    .replace(/\n{2,}/g, ' ')
    .replace(/\n/g, ' ')
    .trim()
}

function truncateText(text: string, maxLength: number): string {
  if (text.length <= maxLength) return text
  return text.slice(0, maxLength - 3) + '...'
}

function formatTokens(value: number): string {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`
  return String(value)
}

function formatCacheTtl(session: SessionState): string | null {
  if (!session.lastMainAgentAt || !session.cacheTtlMs) return null
  const remainingMs = Math.max(0, session.lastMainAgentAt + session.cacheTtlMs - Date.now())
  const minutes = Math.ceil(remainingMs / 60000)
  if (minutes <= 0) return 'cache expired'
  if (minutes >= 60) return `cache ${Math.floor(minutes / 60)}h${minutes % 60}m`
  return `cache ${minutes}m`
}

const INTERNAL_CODEX_PROMPT_PREFIXES = [
  'you are a helpful assistant. you will be presented with a user prompt',
  'you are codex, a coding agent',
  'you are an ai assistant accessed via an api',
]

function isInternalCodexPrompt(text: string | undefined): boolean {
  const normalized = (text || '').trim().replace(/\s+/g, ' ').toLowerCase()
  return INTERNAL_CODEX_PROMPT_PREFIXES.some((prefix) => normalized.startsWith(prefix))
}

function latestChatText(session: SessionState, role: 'user' | 'assistant'): string | undefined {
  for (let index = session.chatHistory.length - 1; index >= 0; index -= 1) {
    const message = session.chatHistory[index]
    const text = role === 'assistant'
      ? (message.role === 'assistant' ? (message.trailingContent || message.content) : '')
      : (message.role === 'user' ? message.content : '')
    const cleaned = stripMarkdown(text || '')
    if (cleaned && !isInternalCodexPrompt(cleaned)) return cleaned
  }
  return undefined
}

function latestUserMessage(session: SessionState): string | undefined {
  const direct = stripMarkdown(session.lastUserMessage || '')
  if (direct && !isInternalCodexPrompt(direct)) return direct
  return latestChatText(session, 'user')
}

function latestAssistantPreview(session: SessionState): string | undefined {
  const direct = stripMarkdown(session.responseText || '')
  if (direct) return direct
  const chat = latestChatText(session, 'assistant')
  if (chat) return chat
  if (session.description && session.phase !== 'processing') {
    return stripMarkdown(session.description)
  }
  return undefined
}

function getHoverSessionTitle(session: SessionState, recentUserMessage?: string): string {
  const project = (session.project || '').trim()
  const explicitTitle = stripMarkdown(session.sessionTitle || '')
  const title = explicitTitle && !isInternalCodexPrompt(explicitTitle)
    ? explicitTitle
    : (recentUserMessage || '')

  if (project && title && title !== project && !title.startsWith(`${project} ·`)) {
    return `${project} · ${truncateText(title, 80)}`
  }
  return title || project || 'Session'
}

function isGenericProcessingDescription(text: string | undefined): boolean {
  const normalized = (text || '').trim().replace(/\s+/g, ' ').toLowerCase()
  return normalized === 'processing user input' || normalized.startsWith('processing user input:')
}

function splitToolTargetChanges(target: string): { name: string; additions?: string; deletions?: string } | null {
  const match = target.match(/^(.*?)\s+(\+\d+)(?:\s+(-\d+))?$/)
    || target.match(/^(.*?)\s+(-\d+)$/)
  if (!match) return null
  const name = match[1].trim()
  if (!name) return null
  const firstCount = match[2]
  return {
    name,
    additions: firstCount?.startsWith('+') ? firstCount : undefined,
    deletions: firstCount?.startsWith('-') ? firstCount : match[3],
  }
}

function ToolTarget({ target }: { target: string }) {
  const changes = splitToolTargetChanges(target)
  if (!changes) {
    return <span className="hover-list__tool-target">{target}</span>
  }

  return (
    <span className="hover-list__tool-target hover-list__tool-target--changes" title={target}>
      <span className="hover-list__tool-target-name">{changes.name}</span>
      {changes.additions && <span className="hover-list__tool-count hover-list__tool-count--add">{changes.additions}</span>}
      {changes.deletions && <span className="hover-list__tool-count hover-list__tool-count--del">{changes.deletions}</span>}
    </span>
  )
}

/* ── Subagent Row ── */
function SubagentRow({ subagents }: { subagents: SubagentInfo[] }) {
  if (subagents.length === 0) return null
  return (
    <div className="hover-list__subagents">
      <div className="hover-list__subagents-header">
        <span className="hover-list__subagents-icon">⑂</span>
        <span>Subagents ({subagents.length})</span>
      </div>
      <div className="hover-list__subagents-list">
        {subagents.map((sa) => (
          <div key={sa.agentId} className="hover-list__subagent-item">
            <span className={`hover-list__subagent-dot${sa.status === 'running' ? ' hover-list__subagent-dot--running' : ''}`} />
            <span className="hover-list__subagent-type">{sa.agentType || sa.description.split(':')[0] || 'agent'}</span>
            <span className="hover-list__subagent-desc">({sa.lastAssistantMessage || sa.description})</span>
            {sa.status === 'completed' && (
              <span className="hover-list__subagent-done">完成</span>
            )}
          </div>
        ))}
      </div>
    </div>
  )
}

/* ── Task Row ── */
const MAX_VISIBLE_TASKS = 5

function TaskRow({ tasks }: { tasks: TaskInfo[] }) {
  const [expanded, setExpanded] = useState(false)
  if (tasks.length === 0) return null

  const completed = tasks.filter(t => t.status === 'completed').length
  const inProgress = tasks.filter(t => t.status === 'in_progress').length
  const pending = tasks.filter(t => t.status === 'pending').length
  const hasMore = tasks.length > MAX_VISIBLE_TASKS
  const visibleTasks = expanded ? tasks : tasks.slice(0, MAX_VISIBLE_TASKS)

  return (
    <div className="hover-list__tasks">
      <div className="hover-list__tasks-header">
        任务 <span className="hover-list__tasks-stats">({completed} 已完成, {inProgress} 进行中, {pending} 待处理)</span>
      </div>
      <div className="hover-list__tasks-list">
        {visibleTasks.map((task) => (
          <div key={task.id} className="hover-list__task-item">
            <TaskStatusIcon status={task.status} />
            <span className={`hover-list__task-subject${task.status === 'completed' ? ' hover-list__task-subject--done' : ''}`}>
              {task.name}
            </span>
          </div>
        ))}
        {hasMore && (
          <button
            className="hover-list__tasks-more"
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => { e.stopPropagation(); setExpanded(!expanded) }}
          >
            {expanded ? '收起' : `查看更多 (${tasks.length - MAX_VISIBLE_TASKS} 项)`}
          </button>
        )}
      </div>
    </div>
  )
}

function TaskStatusIcon({ status }: { status: TaskInfo['status'] }) {
  if (status === 'completed') {
    return (
      <svg width="14" height="14" viewBox="0 0 14 14" fill="none" className="hover-list__task-icon hover-list__task-icon--done">
        <rect x="1" y="1" width="12" height="12" rx="3" fill="currentColor" />
        <path d="M4 7l2 2 4-4" stroke="white" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    )
  }
  if (status === 'in_progress') {
    return (
      <svg width="14" height="14" viewBox="0 0 14 14" fill="none" className="hover-list__task-icon hover-list__task-icon--active">
        <circle cx="7" cy="7" r="6" fill="currentColor" />
        <circle cx="5" cy="7" r="1" fill="white" />
        <circle cx="9" cy="7" r="1" fill="white" />
      </svg>
    )
  }
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none" className="hover-list__task-icon">
      <rect x="1.5" y="1.5" width="11" height="11" rx="2.5" stroke="currentColor" strokeWidth="1" />
    </svg>
  )
}

/* ── Inline Permission Preview ── */
function parsePermissionInput(toolInput?: string): Record<string, unknown> {
  if (!toolInput) return {}
  try {
    const parsed = JSON.parse(toolInput)
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return parsed as Record<string, unknown>
    }
  } catch {
    // Plain command/path input is still useful as a fallback preview.
  }
  return { raw: toolInput }
}

function getStringField(input: Record<string, unknown>, keys: string[]): string {
  for (const key of keys) {
    const value = input[key]
    if (typeof value === 'string' && value.trim()) return value
  }
  return ''
}

function shortenMiddle(value: string, maxLength: number): string {
  if (value.length <= maxLength) return value
  const head = Math.max(8, Math.floor(maxLength * 0.35))
  const tail = Math.max(12, maxLength - head - 3)
  return `${value.slice(0, head)}...${value.slice(-tail)}`
}

function getPermissionTarget(perm: PermissionRequest, input: Record<string, unknown>): string {
  if (perm.diff?.filePath) return perm.diff.filePath
  return getStringField(input, ['file_path', 'filePath', 'path', 'url'])
}

function getPermissionPreviewText(input: Record<string, unknown>): string {
  const primary = getStringField(input, ['command', 'query', 'file_path', 'filePath', 'path', 'url', 'pattern', 'raw'])
  if (primary) return primary

  const entries = Object.entries(input)
    .filter(([, value]) => value != null && ['string', 'number', 'boolean'].includes(typeof value))
    .slice(0, 3)
  if (entries.length === 0) return ''
  return entries.map(([key, value]) => `${key}: ${String(value)}`).join(' · ')
}

function InlinePermissionPreview({ session }: { session: SessionState }) {
  const { t } = useTranslation()
  const perm = session.pendingPermission
  if (!perm) return null

  const parsedInput = parsePermissionInput(perm.toolInput)
  const toolLabel = getToolActivityLabel(t, perm.toolName)
  const target = getPermissionTarget(perm, parsedInput)
  const previewText = getPermissionPreviewText(parsedInput)

  const clearAfter = (work: Promise<void>) => {
    work
      .then(() => useSessionStore.getState().clearPermission(session.id))
      .catch((error) => console.warn('[notch] inline permission response:', error))
  }

  return (
    <div
      className="hover-list__inline-perm"
      data-no-open
      onMouseDown={(e) => e.stopPropagation()}
      onClick={(e) => e.stopPropagation()}
    >
      <div className="hover-list__inline-perm-titlebar">
        <span className="hover-list__inline-perm-alert" aria-hidden="true">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M10.3 3.9 1.8 18.1A2 2 0 0 0 3.5 21h17a2 2 0 0 0 1.7-2.9L13.7 3.9a2 2 0 0 0-3.4 0Z" />
            <path d="M12 9v4" />
            <path d="M12 17h.01" />
          </svg>
        </span>
        <span className="hover-list__inline-perm-tool">{toolLabel}</span>
        <span className="hover-list__inline-perm-tool-name">{perm.toolName}</span>
      </div>

      {perm.diff ? (
        <div className="hover-list__inline-perm-diff">
          <DiffView diff={perm.diff} />
        </div>
      ) : (
        <div className="hover-list__inline-perm-preview">
          <div className="hover-list__inline-perm-preview-label">{getToolActivityLabel(t, perm.toolName)}</div>
          {(target || previewText) && (
            <code className="hover-list__inline-perm-preview-text">
              {shortenMiddle(target || previewText, 140)}
            </code>
          )}
        </div>
      )}

      <div className="hover-list__inline-perm-actions">
        <button
          type="button"
          className="hover-list__inline-btn hover-list__inline-btn--deny"
          onMouseDown={(e) => {
            e.preventDefault()
            e.stopPropagation()
            clearAfter(respondPermission(session.id, false))
          }}
        >
          {t('notch.deny', { defaultValue: '拒绝' })}
        </button>
        <button
          type="button"
          className="hover-list__inline-btn hover-list__inline-btn--allow"
          onMouseDown={(e) => {
            e.preventDefault()
            e.stopPropagation()
            clearAfter(respondPermission(session.id, true))
          }}
        >
          {t('notch.allowOnce', { defaultValue: '允许一次' })}
        </button>
        <button
          type="button"
          className="hover-list__inline-btn hover-list__inline-btn--always"
          onMouseDown={(e) => {
            e.preventDefault()
            e.stopPropagation()
            clearAfter(respondPermission(session.id, true, true))
          }}
        >
          <span>{t('notch.allowAlways', { defaultValue: '始终允许' })}</span>
          <kbd>^A</kbd>
        </button>
        <button
          type="button"
          className="hover-list__inline-btn hover-list__inline-btn--auto"
          onMouseDown={(e) => {
            e.preventDefault()
            e.stopPropagation()
            clearAfter(respondAutoApprove(session.id))
          }}
        >
          {t('notch.autoApprove', { defaultValue: '自动批准' })}
        </button>
      </div>
    </div>
  )
}

/* ── Inline Question Preview ── */
function InlineQuestionPreview({ session }: { session: SessionState }) {
  const q = session.pendingQuestion
  const [selectedIndices, setSelectedIndices] = useState<Set<number>>(new Set())
  const [multiAnswers, setMultiAnswers] = useState<Record<string, string>>({})
  if (!q) return null

  const submitAnswer = (answer: string) => {
    respondQuestion(session.id, answer)
      .then(() => useSessionStore.getState().clearQuestion(session.id))
      .catch((error) => console.warn('[notch] inline respondQuestion:', error))
  }

  const nestedQuestions = q.questions || []
  const allNestedAnswered = nestedQuestions.length > 0 && nestedQuestions.every((item) => multiAnswers[item.question])

  if (nestedQuestions.length > 1) {
    return (
      <div className="hover-list__inline-question" onClick={(e) => e.stopPropagation()}>
        <div className="hover-list__inline-question-header">
          <span>💬</span>
          <span className="hover-list__inline-question-title">Claude 的提问</span>
          <span className="hover-list__inline-question-count">({nestedQuestions.length})</span>
        </div>
        <div className="hover-list__inline-question-multi">
          {nestedQuestions.map((item, questionIndex) => (
            <div key={`${questionIndex}-${item.question}`} className="hover-list__inline-question-group">
              <p className="hover-list__inline-question-text">
                <span className="hover-list__inline-question-num">{questionIndex + 1}</span>
                {item.question.length > 100 ? `${item.question.slice(0, 100)}…` : item.question}
              </p>
              <div className="hover-list__inline-question-chips">
                {item.options.map((opt) => (
                  <button
                    key={opt.label}
                    type="button"
                    className={`hover-list__inline-chip${(item.multiSelect ? multiAnswers[item.question]?.split(', ').includes(opt.label) : multiAnswers[item.question] === opt.label) ? ' hover-list__inline-chip--selected' : ''}`}
                    onMouseDown={(e) => {
                      e.preventDefault()
                      e.stopPropagation()
                      setMultiAnswers((prev) => {
                        if (!item.multiSelect) return { ...prev, [item.question]: opt.label }
                        const current = prev[item.question]?.split(', ').filter(Boolean) ?? []
                        const next = current.includes(opt.label)
                          ? current.filter((label) => label !== opt.label)
                          : [...current, opt.label]
                        const updated = { ...prev }
                        if (next.length > 0) updated[item.question] = next.join(', ')
                        else delete updated[item.question]
                        return updated
                      })
                    }}
                  >
                    {opt.label}
                  </button>
                ))}
              </div>
            </div>
          ))}
        </div>
        <button
          type="button"
          className="hover-list__inline-submit"
          disabled={!allNestedAnswered}
          onMouseDown={(e) => {
            e.preventDefault()
            e.stopPropagation()
            if (!allNestedAnswered) return
            submitAnswer(JSON.stringify(multiAnswers))
          }}
        >
          提交所有回答
        </button>
      </div>
    )
  }

  const toggleMultiSelect = (index: number) => {
    setSelectedIndices((prev) => {
      const next = new Set(prev)
      if (next.has(index)) next.delete(index)
      else next.add(index)
      return next
    })
  }

  const submitMultiSelect = () => {
    const answer = Array.from(selectedIndices)
      .sort((a, b) => a - b)
      .map((index) => q.options[index])
      .filter(Boolean)
      .join(', ')
    if (answer) submitAnswer(answer)
  }

  return (
    <div className="hover-list__inline-question" onClick={(e) => e.stopPropagation()}>
      <div className="hover-list__inline-question-header">
        <span>💬</span>
        <span className="hover-list__inline-question-title">Claude 的提问</span>
      </div>
      <p className="hover-list__inline-question-text">
        {q.question.length > 120 ? `${q.question.slice(0, 120)}…` : q.question}
      </p>
      {q.options && q.options.length > 0 && (
        <div className="hover-list__inline-question-options">
          {q.options.slice(0, 4).map((opt, i) => (
            <button
              key={i}
              type="button"
              className={`hover-list__inline-question-opt${selectedIndices.has(i) ? ' hover-list__inline-question-opt--selected' : ''}`}
              onMouseDown={(e) => {
                e.preventDefault()
                e.stopPropagation()
                if (q.multiSelect) toggleMultiSelect(i)
                else submitAnswer(typeof opt === 'string' ? opt : opt)
              }}
            >
              <span className="hover-list__inline-question-num">
                {q.multiSelect && selectedIndices.has(i) ? '✓' : i + 1}
              </span>
              <span>{typeof opt === 'string' ? opt : opt}</span>
            </button>
          ))}
          {q.multiSelect && (
            <button
              type="button"
              className="hover-list__inline-submit"
              disabled={selectedIndices.size === 0}
              onMouseDown={(e) => {
                e.preventDefault()
                e.stopPropagation()
                submitMultiSelect()
              }}
            >
              确认 ({selectedIndices.size})
            </button>
          )}
        </div>
      )}
    </div>
  )
}

/* ── Inline Plan Preview ── */
function InlinePlanPreview({ session }: { session: SessionState }) {
  const [feedback, setFeedback] = useState('')
  if (!session.planTitle && !session.planContent) return null

  const submitPlan = (mode: string, message?: string) => {
    respondPlan(session.id, mode, message)
      .then(() => useSessionStore.getState().clearPlan(session.id))
      .catch((error) => console.warn('[notch] inline respondPlan:', error))
  }

  return (
    <div className="hover-list__inline-plan" onClick={(e) => e.stopPropagation()}>
      <div className="hover-list__inline-plan-header">
        <div className="hover-list__inline-plan-title">
          {session.planTitle || 'Plan'}
        </div>
        <span className="hover-list__inline-plan-badge">Plan</span>
      </div>
      {session.planContent && (
        <div className="hover-list__inline-plan-content">
          {truncateText(stripMarkdown(session.planContent), 200)}
        </div>
      )}
      <div className="hover-list__inline-plan-feedback">
        <input
          className="hover-list__inline-plan-input"
          placeholder="Tell Claude what to change..."
          value={feedback}
          onChange={(e) => setFeedback(e.target.value)}
          onMouseDown={(e) => e.stopPropagation()}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && feedback.trim()) {
              submitPlan('feedback', feedback.trim())
              setFeedback('')
            }
          }}
        />
      </div>
      <div className="hover-list__inline-plan-actions">
        <button
          type="button"
          className="hover-list__inline-btn hover-list__inline-btn--deny"
          onMouseDown={(e) => {
            e.preventDefault()
            e.stopPropagation()
            const trimmed = feedback.trim()
            if (trimmed) {
              submitPlan('feedback', trimmed)
              setFeedback('')
            } else {
              submitPlan('manual')
            }
          }}
        >
          {feedback.trim() ? 'Send Feedback' : 'Manual Review'}
        </button>
        <button
          type="button"
          className="hover-list__inline-btn hover-list__inline-btn--allow"
          onMouseDown={(e) => {
            e.preventDefault()
            e.stopPropagation()
            submitPlan('acceptEdits')
          }}
        >
          Accept Edits
        </button>
        <button
          type="button"
          className="hover-list__inline-btn hover-list__inline-btn--always"
          onMouseDown={(e) => {
            e.preventDefault()
            e.stopPropagation()
            submitPlan('bypassPermissions')
          }}
        >
          Auto
        </button>
      </div>
    </div>
  )
}

/* ── Session Card ── */
function SessionCard({
  session,
  onSessionClick,
  onJumpToTerminal,
  animDuration,
  animDelay,
  isAlertActive,
  selected,
}: {
  session: SessionState
  onSessionClick: (id: string) => void
  onJumpToTerminal?: (id: string) => void
  animDuration: number
  animDelay: number
  isAlertActive: boolean
  selected: boolean
}) {
  const { t } = useTranslation()
  const badge = getAgentBadge(session)
  const agentName = getAgentDisplayName(session)
  const appLabel = getSessionAppLabel(session)
  const terminalLabel = getSessionTerminalLabel(session)
  const showAgentBadge = shouldShowAgentBadge(session)
  const termBadge = terminalLabel ? (TERMINAL_BADGE_COLORS[terminalLabel] || DEFAULT_BADGE) : null
  const recentUserMessage = latestUserMessage(session)
  const title = getHoverSessionTitle(session, recentUserMessage)
  const assistantPreview = latestAssistantPreview(session)
  const priority = computePriority(session)
  const showCacheTTL = useConfigStore((s) => s.showCacheTTL)
  const cacheTtl = showCacheTTL ? formatCacheTtl(session) : null
  const mutedSessions = useSessionStore((s) => s.mutedSessions)
  const muteSession = useSessionStore((s) => s.muteSession)
  const unmuteSession = useSessionStore((s) => s.unmuteSession)
  const isMuted = Boolean(mutedSessions[session.id])

  const isStatic = session.phase === 'idle' || session.phase === 'done'
  const showPassiveDot = isPassiveSession(session)
  const showInlinePermission = !!session.pendingPermission
  const showInlineQuestion = !isAlertActive && !!session.pendingQuestion
  const showInlinePlan = !isAlertActive && !!(session.planTitle || session.planContent)
  const iconAgentType = appLabel === 'Codex App' ? 'codex' : session.agentType
  const shouldShowAgentIcon = !showPassiveDot && (isStatic || appLabel === 'Codex App' || session.agentType === 'codex')
  const handleOpen = () => onSessionClick(session.id)
  const shouldIgnoreOpen = (target: EventTarget | null): boolean => {
    return target instanceof Element && Boolean(
      target.closest('button, a, input, select, textarea, [data-no-open]'),
    )
  }
  const handleClickCapture = (event: MouseEvent<HTMLDivElement>) => {
    if (shouldIgnoreOpen(event.target)) return
    event.preventDefault()
    event.stopPropagation()
    handleOpen()
  }
  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== 'Enter' && event.key !== ' ') return
    event.preventDefault()
    handleOpen()
  }
  const handleJump = (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault()
    event.stopPropagation()
    onJumpToTerminal?.(session.id)
  }

  return (
    <motion.div
      key={session.id}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: animDuration, delay: animDelay }}
    >
      <div
        data-no-drag
        role="button"
        tabIndex={0}
        aria-selected={selected}
        className={`hover-list__card${selected ? ' hover-list__card--selected' : ''}${session.phase === 'done' ? ' hover-list__card--done' : ''}${showPassiveDot ? ' hover-list__card--passive' : ''}${isMuted ? ' hover-list__card--muted' : ''}`}
        onClickCapture={handleClickCapture}
        onKeyDown={handleKeyDown}
      >
        <div className="hover-list__row-layout">
          {/* Left: mascot / status indicator */}
          <div className="hover-list__status-col">
            {showPassiveDot ? (
              <span className="hover-list__expired-dot" aria-hidden="true" />
            ) : shouldShowAgentIcon ? (
              <span className="hover-list__status-icon">
                <MascotRouter toolType={iconAgentType} phase={session.phase} size={18} />
                {!isStatic && <span className="hover-list__status-activity" />}
              </span>
            ) : (
              <PixelIndicator priority={priority} size={16} />
            )}
          </div>

          {/* Right: multi-row content */}
          <div className="hover-list__content-col">
            {/* Row 1: title + badges + duration + mute + jump */}
            <div className="hover-list__row1">
              <span className="hover-list__session-title">{title}</span>

              <div className="hover-list__meta">
                {appLabel && (
                  <span className="hover-list__agent-badge hover-list__source-badge">
                    {appLabel}
                  </span>
                )}
                {showAgentBadge && (
                  <span className="hover-list__agent-badge" style={{ background: badge.bg, color: badge.text }}>
                    {agentName}
                  </span>
                )}
                {terminalLabel && termBadge && (
                  <span className="hover-list__agent-badge" style={{ background: termBadge.bg, color: termBadge.text }}>
                    {terminalLabel}
                  </span>
                )}
                {session.isYoloMode && (
                  <span className="hover-list__yolo-badge">YOLO</span>
                )}
                <span className="hover-list__duration">{formatDurationShort(session.duration)}</span>
                <button
                  type="button"
                  data-no-drag
                  className={`hover-list__mute-btn${isMuted ? ' hover-list__mute-btn--active' : ''}`}
                  title={isMuted ? t('notch.unmuteSession', 'Unmute session') : t('notch.muteSession', 'Mute notifications (30 min)')}
                  onMouseDown={(e) => { e.preventDefault(); e.stopPropagation() }}
                  onClick={(e) => {
                    e.preventDefault()
                    e.stopPropagation()
                    if (isMuted) unmuteSession(session.id)
                    else muteSession(session.id)
                  }}
                >
                  {isMuted ? (
                    <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                      <path d="M11 5 6 9H2v6h4l5 4V5Z"/>
                      <line x1="23" y1="9" x2="17" y2="15"/><line x1="17" y1="9" x2="23" y2="15"/>
                    </svg>
                  ) : (
                    <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                      <path d="M11 5 6 9H2v6h4l5 4V5Z"/>
                      <path d="M15.54 8.46a5 5 0 0 1 0 7.07"/>
                    </svg>
                  )}
                </button>
                <button
                  type="button"
                  data-no-drag
                  className="hover-list__jump"
                  onMouseDown={(e) => { e.preventDefault(); e.stopPropagation() }}
                  onClick={handleJump}
                  title={t('notch.jumpToTerminal')}
                  aria-label={t('notch.jumpToTerminal')}
                >
                  ↗
                </button>
              </div>
            </div>

            {/* Row 2: user's last message */}
            {recentUserMessage && (
              <div className="hover-list__row2">
                <span className="hover-list__you-label">{t('notch.you', '你')}：</span>
                <span className="hover-list__user-msg">{truncateText(recentUserMessage, 100)}</span>
              </div>
            )}

            {/* Row 3: tool action or status */}
            {session.lastToolName ? (
              <div className="hover-list__row3">
                <span className={`hover-list__tool-label${session.lastToolName.startsWith('Compacting') ? ' hover-list__tool-label--compact' : ''}`}>
                  {getToolActivityLabel(t, session.lastToolName)}
                </span>
                {session.lastToolTarget && (
                  <ToolTarget target={session.lastToolTarget} />
                )}
              </div>
            ) : session.phase === 'processing' ? (
              <div className="hover-list__row3">
                <span className="hover-list__tool-label">
                  {session.description && !isGenericProcessingDescription(session.description)
                    ? truncateText(stripMarkdown(session.description), 100)
                    : t('notch.thinking')}
                </span>
              </div>
            ) : assistantPreview ? (
              <div className="hover-list__row3-preview">{truncateText(assistantPreview, 120)}</div>
            ) : null}

            {(session.statusLineText || session.contextWindow || cacheTtl) && (
              <div className="hover-list__statusline">
                {session.statusLineText && (
                  <span className="hover-list__statusline-text">{truncateText(session.statusLineText, 80)}</span>
                )}
                {session.contextWindow?.usedPercentage != null && (
                  <span className="hover-list__statusline-pill">
                    ctx {session.contextWindow.usedPercentage.toFixed(0)}%
                  </span>
                )}
                {session.contextWindow && (
                  <span className="hover-list__statusline-pill">
                    {formatTokens(session.contextWindow.totalInputTokens + session.contextWindow.totalOutputTokens)}
                  </span>
                )}
                {cacheTtl && <span className="hover-list__statusline-pill">{cacheTtl}</span>}
              </div>
            )}
          </div>
        </div>

        {/* Row 4: error */}
        {session.phase === 'error' && session.description && (
          <div className="hover-list__row4-error">
            <span className="hover-list__error-icon">⚠</span>
            <span className="hover-list__error-text">{session.description}</span>
          </div>
        )}

        {/* Row 5: subagents */}
        {session.subagents && session.subagents.length > 0 && (
          <SubagentRow subagents={session.subagents} />
        )}

        {/* Row 6: tasks */}
        {session.tasks && session.tasks.length > 0 && (
          <TaskRow tasks={session.tasks} />
        )}

        {/* Row 7: inline permission */}
        {showInlinePermission && <InlinePermissionPreview session={session} />}

        {/* Row 8: inline question */}
        {showInlineQuestion && <InlineQuestionPreview session={session} />}

        {/* Row 9: inline plan */}
        {showInlinePlan && <InlinePlanPreview session={session} />}
      </div>
    </motion.div>
  )
}

export function HoverList({ sessions, onSessionClick, onJumpToTerminal, focusFilteredEmpty = false }: HoverListProps) {
  const { t } = useTranslation()

  const hoverSpeed = useConfigStore((s) => s.hoverSpeed)
  const islandAnimationScaleValue = useConfigStore((s) => s.islandAnimationScale)
  const maxVisibleSessions = useConfigStore((s) => s.maxVisibleSessions)
  const islandAnimationScale = Math.max(0.1, islandAnimationScaleValue || 1)
  const animDuration = (HOVER_SPEED_MS[hoverSpeed] ?? 0.2) * islandAnimationScale

  const [showAll, setShowAll] = useState(false)
  const [selectedIndex, setSelectedIndexState] = useState(-1)
  const selectedIndexRef = useRef(-1)
  const setSelectedIndex = useCallback((next: number | ((index: number) => number)) => {
    const resolved = typeof next === 'function' ? next(selectedIndexRef.current) : next
    selectedIndexRef.current = resolved
    setSelectedIndexState(resolved)
  }, [])

  const activeOverlay = useSessionStore((s) => s.activeOverlay)
  const hookNotification = useSessionStore((s) => s.hookNotification)
  const isAlertActive = activeOverlay?.type === 'permission' || activeOverlay?.type === 'question' || activeOverlay?.type === 'plan'

  const sorted = useMemo(() => {
    return [...sessions].sort((a, b) => computePriority(b) - computePriority(a))
  }, [sessions])

  const totalSessions = sorted.length
  const isLimited = maxVisibleSessions > 0 && !showAll && totalSessions > maxVisibleSessions
  const visibleSessions = isLimited ? sorted.slice(0, maxVisibleSessions) : sorted

  useEffect(() => {
    if (visibleSessions.length === 0) return

    const handler = (event: globalThis.KeyboardEvent) => {
      const target = event.target
      if (target instanceof HTMLElement && target.closest('input, textarea, select, [contenteditable="true"]')) return

      if (event.key === 'ArrowDown') {
        event.preventDefault()
        setSelectedIndex((index) => Math.min(index + 1, visibleSessions.length - 1))
        return
      }

      if (event.key === 'ArrowUp') {
        event.preventDefault()
        setSelectedIndex((index) => index < 0 ? 0 : Math.max(index - 1, 0))
        return
      }

      if (event.key === 'Enter') {
        event.preventDefault()
        const session = visibleSessions[selectedIndexRef.current]
        if (!session) return
        if (onJumpToTerminal) onJumpToTerminal(session.id)
        else onSessionClick(session.id)
      }
    }

    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [onJumpToTerminal, onSessionClick, setSelectedIndex, visibleSessions])

  if (sorted.length === 0) {
    return (
      <div className="hover-list__empty">
        <svg className="hover-list__empty-logo" viewBox="0 0 48 48" fill="none" aria-hidden>
          <rect x="4" y="10" width="40" height="28" rx="6" stroke="var(--island-text-muted)" strokeWidth="2" opacity="0.5" />
          <circle cx="24" cy="24" r="6" stroke="var(--island-text-muted)" strokeWidth="2" opacity="0.5" />
          <path d="M18 24h-4M34 24h-4M24 18v-4M24 30v4" stroke="var(--island-text-muted)" strokeWidth="2" strokeLinecap="round" opacity="0.35" />
        </svg>
        <span className="hover-list__empty-text">
          {focusFilteredEmpty ? t('notch.noSessionInFocus') : t('notch.noSessions')}<br />
          {focusFilteredEmpty ? t('notch.noSessionInFocusHint') : t('notch.noSessionsHint')}
        </span>
      </div>
    )
  }

  return (
    <div className="hover-list">
      {hookNotification && (
        <div className={`hover-list__hook-notice hover-list__hook-notice--${hookNotification}`}>
          <span className="hover-list__hook-dot" />
          <span>
            {hookNotification === 'restored'
              ? t('notch.hooksRestored', 'Hooks restored')
              : t('notch.hooksRateLimited', 'Hook recovery rate-limited')}
          </span>
        </div>
      )}
      <AnimatePresence initial={false}>
        {visibleSessions.map((session, index) => (
          <SessionCard
            key={session.id}
            session={session}
            onSessionClick={onSessionClick}
            onJumpToTerminal={onJumpToTerminal}
            animDuration={animDuration}
            animDelay={index * 0.03 * islandAnimationScale}
            isAlertActive={isAlertActive}
            selected={index === selectedIndex}
          />
        ))}
      </AnimatePresence>

      {isLimited && (
        <button
          className="hover-list__show-all"
          onClick={() => setShowAll(true)}
        >
          {t('notch.showAllSessions', { count: totalSessions })}
        </button>
      )}
    </div>
  )
}
