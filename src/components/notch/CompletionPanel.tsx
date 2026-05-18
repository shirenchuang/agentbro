/* CompletionPanel — 3-variant task completion UI with auto-dismiss */
import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { SubagentInfo } from '../../types/agent'
import { getToolActivityLabel } from '../../utils/toolLabels'
import './CompletionPanel.css'

export type CompletionVariant = 'claude-stop' | 'subagent-done' | 'pending-tool'

interface ClaudeStopProps {
  variant: 'claude-stop'
  summaryLines: string[]
  onQuickReply: (reply: string) => void
  onGoToTerminal: () => void
  onDismiss: () => void
}

interface SubagentDoneProps {
  variant: 'subagent-done'
  subagents: SubagentInfo[]
  onDismiss: () => void
}

interface PendingToolProps {
  variant: 'pending-tool'
  toolName: string
  startedAt: number
  onDismiss: () => void
}

export type CompletionPanelProps = ClaudeStopProps | SubagentDoneProps | PendingToolProps

const QUICK_REPLIES = ['Continue', 'Explain more', 'Run tests', 'Commit']
const AUTO_DISMISS_MS = 15_000

export function CompletionPanel(props: CompletionPanelProps) {
  const { onDismiss } = props
  const isPending = props.variant === 'pending-tool'
  const remainingRef = useRef(AUTO_DISMISS_MS)
  const startedAtRef = useRef(0)
  const dismissTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  useEffect(() => {
    if (isPending) return
    remainingRef.current = AUTO_DISMISS_MS
    startedAtRef.current = Date.now()
    dismissTimerRef.current = setTimeout(onDismiss, AUTO_DISMISS_MS)
    return () => {
      if (dismissTimerRef.current) clearTimeout(dismissTimerRef.current)
    }
  }, [onDismiss, isPending])

  useEffect(() => {
    const handler = (e: KeyboardEvent) => { if (e.key === 'Escape') onDismiss() }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [onDismiss])

  const handleMouseEnter = useCallback(() => {
    if (dismissTimerRef.current) clearTimeout(dismissTimerRef.current)
    remainingRef.current = Math.max(0, remainingRef.current - (Date.now() - startedAtRef.current))
  }, [])
  const handleMouseLeave = useCallback(() => {
    startedAtRef.current = Date.now()
    dismissTimerRef.current = setTimeout(onDismiss, remainingRef.current)
  }, [onDismiss])

  return (
    <div className="completion-panel" onMouseEnter={handleMouseEnter} onMouseLeave={handleMouseLeave}>
      {props.variant === 'claude-stop' && <ClaudeStopVariant {...props} />}
      {props.variant === 'subagent-done' && <SubagentDoneVariant {...props} />}
      {props.variant === 'pending-tool' && <PendingToolVariant {...props} />}
      {!isPending && (
        <div className="completion-panel__countdown">
          <div className="completion-panel__countdown-bar" />
        </div>
      )}
    </div>
  )
}

function ClaudeStopVariant({ summaryLines, onQuickReply, onGoToTerminal, onDismiss }: ClaudeStopProps) {
  const lines = summaryLines.slice(0, 3)
  return (
    <div className="completion-panel__body">
      <div className="completion-panel__header">
        <svg className="completion-panel__check" width="15" height="15" viewBox="0 0 24 24" fill="none">
          <circle cx="12" cy="12" r="11" stroke="#30D158" strokeWidth="2" opacity="0.35" />
          <path d="M7 12.5l3 3 7-7" stroke="#30D158" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
        <span className="completion-panel__title">Task Complete</span>
        <button className="completion-panel__close" onClick={onDismiss} aria-label="Dismiss">×</button>
      </div>

      <div className="completion-panel__summary">
        {lines.map((line, i) => (
          <p key={i} className="completion-panel__summary-line">{line}</p>
        ))}
      </div>

      <div className="completion-panel__quick-replies">
        {QUICK_REPLIES.map(reply => (
          <button key={reply} className="completion-panel__pill" onClick={() => onQuickReply(reply)}>
            {reply}
          </button>
        ))}
      </div>

      <div className="completion-panel__footer">
        <button className="completion-panel__terminal-btn" onClick={onGoToTerminal}>
          Go to terminal
        </button>
        <span className="completion-panel__hint">⎋ to dismiss · ↵ to continue</span>
      </div>
    </div>
  )
}

function SubagentDoneVariant({ subagents, onDismiss }: SubagentDoneProps) {
  const completed = subagents.filter(a => a.status === 'completed').length
  const errored = subagents.filter(a => a.status === 'error').length
  return (
    <div className="completion-panel__body">
      <div className="completion-panel__header">
        <svg className="completion-panel__check" width="15" height="15" viewBox="0 0 24 24" fill="none">
          <circle cx="12" cy="12" r="11" stroke="#30D158" strokeWidth="2" opacity="0.35" />
          <path d="M7 12.5l3 3 7-7" stroke="#30D158" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
        <span className="completion-panel__title">
          {completed} Subagent{completed !== 1 ? 's' : ''} Done
          {errored > 0 && <span className="completion-panel__err-badge">{errored} failed</span>}
        </span>
        <button className="completion-panel__close" onClick={onDismiss} aria-label="Dismiss">×</button>
      </div>
      <div className="completion-panel__subagent-list">
        {subagents.map(agent => (
          <div key={agent.agentId} className="completion-panel__subagent-row">
            <span className={`completion-panel__status-badge completion-panel__status-badge--${agent.status}`}>
              {agent.status === 'completed' ? '✓' : agent.status === 'error' ? '✕' : '●'}
            </span>
            <span className="completion-panel__subagent-desc">
              {agent.name ? `@${agent.name} ${agent.description || agent.lastAssistantMessage || ''}` : (agent.lastAssistantMessage || agent.description || `Agent ${agent.agentId.slice(0, 8)}`)}
            </span>
          </div>
        ))}
      </div>
    </div>
  )
}

function PendingToolVariant({ toolName, startedAt, onDismiss }: PendingToolProps) {
  const { t } = useTranslation()
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(id)
  }, [])
  const elapsed = Math.floor((now - startedAt) / 1000)
  const fmt = (s: number) => s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${s % 60}s`

  return (
    <div className="completion-panel__body">
      <div className="completion-panel__header">
        <span className="completion-panel__title">Running Tool</span>
        <button className="completion-panel__close" onClick={onDismiss} aria-label="Dismiss">×</button>
      </div>
      <div className="completion-panel__tool-row">
        <span className="completion-panel__spinner" />
        <span className="completion-panel__tool-name">{getToolActivityLabel(t, toolName)}</span>
        <span className="completion-panel__elapsed">{fmt(elapsed)}</span>
      </div>
    </div>
  )
}
