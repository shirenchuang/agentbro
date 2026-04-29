/* Hover List — Session cards grouped by project */
import { useState, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { AnimatePresence, motion } from 'framer-motion'
import type { SessionState } from '../../types/agent'
import { computePriority } from '../../types/priority'
import { PixelIndicator } from './PixelIndicator'
import { ToolCallDisplay } from './ToolCallDisplay'
import { TaskSummary } from './TaskSummary'
import { useTick } from '../../hooks/useTick'
import { useConfigStore } from '../../stores/configStore'
import { formatDuration } from '../../utils/time'
import './HoverList.css'

interface HoverListProps {
  sessions: SessionState[]
  onSessionClick: (sessionId: string) => void
  onJumpToTerminal?: (sessionId: string) => void
}

// Badge colors for agent types
const AGENT_BADGE_COLORS: Record<string, string> = {
  'claude-code': '#E8654A',
  'codex': '#10B981',
  'gemini-cli': '#6366F1',
  'cursor': '#3B82F6',
  'opencode': '#8B5CF6',
  'droid': '#EF4444',
  'qoder': '#EC4899',
  'codebuddy': '#14B8A6',
  'copilot': '#6B7280',
  'kiro': '#F59E0B',
}

const HOVER_SPEED_MS: Record<string, number> = {
  instant: 0.05,
  normal: 0.2,
  slow: 0.4,
}

function getAgentName(session: SessionState): string {
  switch (session.agentType) {
    case 'claude-code': return 'Claude'
    case 'gemini-cli': return 'Gemini'
    default: return session.agentType.charAt(0).toUpperCase() + session.agentType.slice(1)
  }
}

interface ProjectGroup {
  project: string
  sessions: SessionState[]
  topPriority: number
}

function SessionCard({
  session,
  onSessionClick,
  onJumpToTerminal,
  animDuration,
  index,
}: {
  session: SessionState
  onSessionClick: (id: string) => void
  onJumpToTerminal?: (id: string) => void
  animDuration: number
  index: number
}) {
  const { t } = useTranslation()
  const badgeColor = AGENT_BADGE_COLORS[session.agentType] || '#6B7280'

  return (
    <motion.div
      key={session.id}
      initial={{ opacity: 0, x: -12 }}
      animate={{ opacity: 1, x: 0 }}
      exit={{ opacity: 0, x: 12 }}
      transition={{ duration: animDuration, delay: index * 0.03 }}
    >
      <div
        className={`hover-list__card${session.phase === 'done' ? ' hover-list__card--done' : ''}`}
        onClick={() => onSessionClick(session.id)}
      >
        {/* Row 1: Pixel + project + session title + badges + duration + jump */}
        <div className="hover-list__row1">
          <PixelIndicator priority={computePriority(session)} size={14} />
          <span className="hover-list__project">{session.project}</span>
          {session.sessionTitle && (
            <>
              <span className="hover-list__sep">&middot;</span>
              <span className="hover-list__title">{session.sessionTitle}</span>
            </>
          )}
          <div className="hover-list__badges">
            <span
              className="hover-list__agent-badge"
              style={{ background: badgeColor }}
            >
              {getAgentName(session)}
            </span>
            <span className="hover-list__terminal-badge">{session.terminal}</span>
            <span className="hover-list__duration">{formatDuration(session.startedAt)}</span>
          </div>
          <button
            className="hover-list__jump"
            onClick={(e) => { e.stopPropagation(); onJumpToTerminal?.(session.id) }}
            title={t('notch.jumpToTerminal')}
            aria-label={t('notch.jumpToTerminal')}
          >
            <span className="hover-list__jump-text">^G</span>
            <svg width="10" height="10" viewBox="0 0 16 16" fill="none">
              <path d="M4 12L12 4M12 4H6M12 4v6" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
            </svg>
          </button>
        </div>

        {session.lastUserMessage && (
          <div className="hover-list__user-msg">
            <span className="hover-list__user-msg-prefix">{t('notch.you')}:</span>
            <span className="hover-list__user-msg-text">{session.lastUserMessage}</span>
          </div>
        )}

        {session.lastToolName && session.lastToolStatus && (
          <div className="hover-list__tool-row">
            <ToolCallDisplay
              toolName={session.lastToolName}
              toolInput={undefined}
              status={session.lastToolStatus}
            />
          </div>
        )}

        {session.tasks && session.tasks.length > 0 && (
          <div className="hover-list__task-row">
            <TaskSummary tasks={session.tasks} />
          </div>
        )}
      </div>
    </motion.div>
  )
}

export function HoverList({ sessions, onSessionClick, onJumpToTerminal }: HoverListProps) {
  const { t } = useTranslation()
  useTick(1000, sessions.length > 0)

  const hoverSpeed = useConfigStore((s) => s.hoverSpeed)
  const animDuration = HOVER_SPEED_MS[hoverSpeed] ?? 0.2

  const [collapsedGroups, setCollapsedGroups] = useState<Record<string, boolean>>({})

  const groups = useMemo((): ProjectGroup[] => {
    const sorted = [...sessions].sort((a, b) => computePriority(b) - computePriority(a))
    const groupMap = new Map<string, SessionState[]>()
    for (const session of sorted) {
      if (!groupMap.has(session.project)) groupMap.set(session.project, [])
      groupMap.get(session.project)!.push(session)
    }
    return Array.from(groupMap.entries())
      .map(([project, groupSessions]) => ({
        project,
        sessions: groupSessions,
        topPriority: Math.max(...groupSessions.map(computePriority)),
      }))
      .sort((a, b) => b.topPriority - a.topPriority)
  }, [sessions])

  const multiGroup = groups.length > 1

  function toggleGroup(project: string) {
    setCollapsedGroups(prev => ({ ...prev, [project]: !prev[project] }))
  }

  if (groups.length === 0) {
    return (
      <div className="hover-list__empty">
        <svg className="hover-list__empty-logo" viewBox="0 0 48 48" fill="none" aria-hidden>
          <rect x="4" y="10" width="40" height="28" rx="6" stroke="var(--vi-text-tertiary)" strokeWidth="2" opacity="0.5" />
          <circle cx="24" cy="24" r="6" stroke="var(--vi-text-tertiary)" strokeWidth="2" opacity="0.5" />
          <path d="M18 24h-4M34 24h-4M24 18v-4M24 30v4" stroke="var(--vi-text-tertiary)" strokeWidth="2" strokeLinecap="round" opacity="0.35" />
        </svg>
        <span className="hover-list__empty-text">
          {t('notch.noSessions')}<br />
          {t('notch.noSessionsHint')}
        </span>
      </div>
    )
  }

  return (
    <div className="hover-list">
      <AnimatePresence initial={false}>
        {groups.map((group) => {
          const isCollapsed = !!collapsedGroups[group.project]

          return (
            <div key={group.project} className="hover-list__group">
              {/* Project group header — only show when multiple projects */}
              {multiGroup && (
                <button
                  className="hover-list__group-header"
                  onClick={() => toggleGroup(group.project)}
                  aria-expanded={!isCollapsed}
                >
                  <svg className="hover-list__group-folder" width="12" height="12" viewBox="0 0 16 16" fill="none">
                    <path d="M1 4a1 1 0 011-1h4l1.5 1.5H14a1 1 0 011 1V12a1 1 0 01-1 1H2a1 1 0 01-1-1V4z" fill="currentColor"/>
                  </svg>
                  <span className="hover-list__group-name">{group.project}</span>
                  <span className="hover-list__group-count">{group.sessions.length}</span>
                  <svg
                    className={`hover-list__group-chevron${isCollapsed ? '' : ' hover-list__group-chevron--open'}`}
                    width="10" height="10" viewBox="0 0 16 16" fill="none"
                  >
                    <path d="M4 6l4 4 4-4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
                  </svg>
                </button>
              )}

              {/* Session cards */}
              <AnimatePresence initial={false}>
                {!isCollapsed && group.sessions.map((session, index) => (
                  <SessionCard
                    key={session.id}
                    session={session}
                    onSessionClick={onSessionClick}
                    onJumpToTerminal={onJumpToTerminal}
                    animDuration={animDuration}
                    index={index}
                  />
                ))}
              </AnimatePresence>
            </div>
          )
        })}
      </AnimatePresence>
    </div>
  )
}
