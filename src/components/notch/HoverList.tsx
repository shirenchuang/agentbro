/* Hover List — Flat session list sorted by priority */
import { useState, useMemo, type KeyboardEvent, type MouseEvent } from 'react'
import { useTranslation } from 'react-i18next'
import { AnimatePresence, motion } from 'framer-motion'
import type { SessionState, SubagentInfo, TaskInfo } from '../../types/agent'
import { computePriority } from '../../types/priority'
import { PixelIndicator } from './PixelIndicator'
import { MascotRouter } from './mascots'
import { useConfigStore } from '../../stores/configStore'
import { useSessionStore } from '../../stores/sessionStore'
import { respondPermission } from '../../services/tauriApi'
import { formatDuration } from '../../utils/time'
import { getToolActivityLabel } from '../../utils/toolLabels'
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
  'iTerm2': { bg: 'rgba(34, 197, 94, 0.15)', text: '#22c55e' },
  'Terminal': { bg: 'rgba(107, 114, 128, 0.15)', text: '#9ca3af' },
  'Warp': { bg: 'rgba(99, 102, 241, 0.15)', text: '#818cf8' },
  'Alacritty': { bg: 'rgba(245, 158, 11, 0.15)', text: '#f59e0b' },
  'Kitty': { bg: 'rgba(236, 72, 153, 0.15)', text: '#ec4899' },
  'WezTerm': { bg: 'rgba(168, 85, 247, 0.15)', text: '#a855f7' },
  'VS Code': { bg: 'rgba(14, 165, 233, 0.15)', text: '#0ea5e9' },
}

const DEFAULT_BADGE = { bg: 'rgba(107, 114, 128, 0.15)', text: '#9ca3af' }

const HOVER_SPEED_MS: Record<string, number> = {
  instant: 0.05,
  normal: 0.2,
  slow: 0.4,
}

function getAgentName(session: SessionState): string {
  if (session.agentType === 'claude-code' && session.engineLabel && session.engineLabel !== 'Claude Code') {
    return session.engineLabel
  }
  switch (session.agentType) {
    case 'claude-code': return 'Claude'
    case 'gemini-cli': return 'Gemini'
    default: return session.agentType.charAt(0).toUpperCase() + session.agentType.slice(1)
  }
}

function getAgentBadge(session: SessionState): { bg: string; text: string } {
  const label = getAgentName(session).toLowerCase()
  if (label === 'antcc') {
    return { bg: 'rgba(255, 47, 129, 0.16)', text: '#ff2f81' }
  }
  return AGENT_BADGE_COLORS[session.agentType] || DEFAULT_BADGE
}

function getSessionTitle(session: SessionState): string {
  return session.sessionTitle || session.project || 'Session'
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

function isCodexDesktopSession(session: SessionState): boolean {
  return session.agentType === 'codex' && (!session.tty || session.terminal.toLowerCase().includes('codex'))
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
function InlinePermissionPreview({ session }: { session: SessionState }) {
  const perm = session.pendingPermission
  if (!perm) return null

  const filePath = perm.toolInput ? (typeof perm.toolInput === 'string' ? perm.toolInput : '') : ''
  const param = filePath
  const { t } = useTranslation()

  return (
    <div className="hover-list__inline-perm" onClick={(e) => e.stopPropagation()}>
      <div className="hover-list__inline-perm-header">
        <span className="hover-list__inline-perm-icon">⚠️</span>
        <span className="hover-list__inline-perm-tool">{getToolActivityLabel(t, perm.toolName)}</span>
        {param && <span className="hover-list__inline-perm-param">{param.length > 50 ? `…${param.slice(-50)}` : param}</span>}
      </div>
      <div className="hover-list__inline-perm-actions">
        <button
          className="hover-list__inline-btn hover-list__inline-btn--deny"
          onClick={() => respondPermission(session.id, false)}
        >拒绝</button>
        <button
          className="hover-list__inline-btn hover-list__inline-btn--allow"
          onClick={() => respondPermission(session.id, true)}
        >允许</button>
        <button
          className="hover-list__inline-btn hover-list__inline-btn--always"
          onClick={() => respondPermission(session.id, true, true)}
        >始终允许</button>
      </div>
    </div>
  )
}

/* ── Inline Question Preview ── */
function InlineQuestionPreview({ session }: { session: SessionState }) {
  const q = session.pendingQuestion
  if (!q) return null

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
            <div key={i} className="hover-list__inline-question-opt">
              <span className="hover-list__inline-question-num">{i + 1}</span>
              <span>{typeof opt === 'string' ? opt : opt}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

/* ── Inline Plan Preview ── */
function InlinePlanPreview({ session }: { session: SessionState }) {
  if (!session.planTitle && !session.planContent) return null

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
    </div>
  )
}

/* ── Session Card ── */
function SessionCard({
  session,
  onSessionClick,
  onJumpToTerminal,
  animDuration,
  index,
  isAlertActive,
}: {
  session: SessionState
  onSessionClick: (id: string) => void
  onJumpToTerminal?: (id: string) => void
  animDuration: number
  index: number
  isAlertActive: boolean
}) {
  const { t } = useTranslation()
  const badge = getAgentBadge(session)
  const agentName = getAgentName(session)
  const termBadge = session.terminal ? (TERMINAL_BADGE_COLORS[session.terminal] || null) : null
  const title = getSessionTitle(session)
  const priority = computePriority(session)
  const showCacheTTL = useConfigStore((s) => s.showCacheTTL)
  const cacheTtl = showCacheTTL ? formatCacheTtl(session) : null

  const isStatic = session.phase === 'idle' || session.phase === 'done'
  const showInlinePermission = !isAlertActive && !!session.pendingPermission
  const showInlineQuestion = !isAlertActive && !!session.pendingQuestion
  const showInlinePlan = !isAlertActive && !!(session.planTitle || session.planContent)
  const canJumpToTerminal = Boolean(session.pid || session.tty || isCodexDesktopSession(session))
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
    if (!canJumpToTerminal) return
    onJumpToTerminal?.(session.id)
  }

  return (
    <motion.div
      key={session.id}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: animDuration, delay: index * 0.03 }}
    >
      <div
        data-no-drag
        role="button"
        tabIndex={0}
        className={`hover-list__card${session.phase === 'done' ? ' hover-list__card--done' : ''}`}
        onClickCapture={handleClickCapture}
        onKeyDown={handleKeyDown}
      >
        <div className="hover-list__row-layout">
          {/* Left: mascot / status indicator */}
          <div className="hover-list__status-col">
            {isStatic ? (
              <MascotRouter toolType={session.agentType} phase={session.phase} size={18} />
            ) : (
              <PixelIndicator priority={priority} size={16} />
            )}
          </div>

          {/* Right: multi-row content */}
          <div className="hover-list__content-col">
            {/* Row 1: title + badges + duration + jump */}
            <div className="hover-list__row1">
              <span className="hover-list__session-title">{title}</span>

              <div className="hover-list__meta">
                <span className="hover-list__agent-badge" style={{ background: badge.bg, color: badge.text }}>
                  {agentName}
                </span>
                {termBadge && (
                  <span className="hover-list__agent-badge" style={{ background: termBadge.bg, color: termBadge.text }}>
                    {session.terminal}
                  </span>
                )}
                {session.isYoloMode && (
                  <span className="hover-list__yolo-badge">YOLO</span>
                )}
                <span className="hover-list__duration">{formatDuration(session.duration)}</span>
                <button
                  type="button"
                  data-no-drag
                  className={`hover-list__jump${canJumpToTerminal ? '' : ' hover-list__jump--disabled'}`}
                  aria-disabled={!canJumpToTerminal}
                  onMouseDown={(e) => { e.preventDefault(); e.stopPropagation() }}
                  onClick={handleJump}
                  title={canJumpToTerminal ? t('notch.jumpToTerminal') : t('notch.terminalUnavailable', '等待终端信息')}
                  aria-label={canJumpToTerminal ? t('notch.jumpToTerminal') : t('notch.terminalUnavailable', '等待终端信息')}
                >
                  ↗
                </button>
              </div>
            </div>

            {/* Row 2: user's last message */}
            {session.lastUserMessage && (
              <div className="hover-list__row2">
                <span className="hover-list__you-label">{t('notch.you', '你')}：</span>
                <span className="hover-list__user-msg">{truncateText(stripMarkdown(session.lastUserMessage), 80)}</span>
              </div>
            )}

            {/* Row 3: tool action or status */}
            {session.phase === 'processing' && session.lastToolName ? (
              <div className="hover-list__row3">
                <span className={`hover-list__tool-label${session.lastToolName.startsWith('Compacting') ? ' hover-list__tool-label--compact' : ''}`}>
                  {getToolActivityLabel(t, session.lastToolName)}
                </span>
                {session.lastToolTarget && (
                  <span className="hover-list__tool-target">{session.lastToolTarget}</span>
                )}
              </div>
            ) : session.phase === 'processing' ? (
              <div className="hover-list__row3">
                <span className="hover-list__tool-label">{t('notch.working')}...</span>
              </div>
            ) : session.description ? (
              <div className="hover-list__row3-preview">{truncateText(stripMarkdown(session.description), 100)}</div>
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
  const maxVisibleSessions = useConfigStore((s) => s.maxVisibleSessions)
  const animDuration = HOVER_SPEED_MS[hoverSpeed] ?? 0.2

  const [showAll, setShowAll] = useState(false)

  const activeOverlay = useSessionStore((s) => s.activeOverlay)
  const hookNotification = useSessionStore((s) => s.hookNotification)
  const isAlertActive = activeOverlay?.type === 'permission' || activeOverlay?.type === 'question' || activeOverlay?.type === 'plan'

  const sorted = useMemo(() => {
    return [...sessions].sort((a, b) => computePriority(b) - computePriority(a))
  }, [sessions])

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

  const totalSessions = sorted.length
  const isLimited = !showAll && totalSessions > maxVisibleSessions
  const visibleSessions = isLimited ? sorted.slice(0, maxVisibleSessions) : sorted

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
            index={index}
            isAlertActive={isAlertActive}
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
