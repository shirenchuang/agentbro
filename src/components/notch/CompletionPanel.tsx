/* CompletionPanel — 3-variant task completion UI with auto-dismiss */
import { useCallback, useEffect, useRef, useState } from 'react'
import type { SubagentInfo } from '../../types/agent'
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
  const [remaining, setRemaining] = useState(AUTO_DISMISS_MS)
  const paused = useRef(false)
  const { onDismiss } = props
  const isPending = props.variant === 'pending-tool'

  useEffect(() => {
    if (isPending) return
    const id = setInterval(() => {
      if (!paused.current) setRemaining(r => Math.max(0, r - 100))
    }, 100)
    return () => clearInterval(id)
  }, [isPending])

  useEffect(() => {
    if (!isPending && remaining <= 0) onDismiss()
  }, [remaining, onDismiss, isPending])

  useEffect(() => {
    const handler = (e: KeyboardEvent) => { if (e.key === 'Escape') onDismiss() }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [onDismiss])

  const handleMouseEnter = useCallback(() => { paused.current = true }, [])
  const handleMouseLeave = useCallback(() => { paused.current = false }, [])
  const progress = remaining / AUTO_DISMISS_MS

  return (
    <div className="completion-panel" onMouseEnter={handleMouseEnter} onMouseLeave={handleMouseLeave}>
      {props.variant === 'claude-stop' && <ClaudeStopVariant {...props} />}
      {props.variant === 'subagent-done' && <SubagentDoneVariant {...props} />}
      {props.variant === 'pending-tool' && <PendingToolVariant {...props} />}
      {!isPending && (
        <div className="completion-panel__countdown">
          <div className="completion-panel__countdown-bar" style={{ width: `${progress * 100}%` }} />
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
              {agent.description || `Agent ${agent.agentId.slice(0, 8)}`}
            </span>
          </div>
        ))}
      </div>
    </div>
  )
}

function PendingToolVariant({ toolName, startedAt, onDismiss }: PendingToolProps) {
  const [elapsed, setElapsed] = useState(0)

  useEffect(() => {
    const id = setInterval(() => setElapsed(Math.floor((Date.now() - startedAt) / 1000)), 1000)
    return () => clearInterval(id)
  }, [startedAt])

  const fmt = (s: number) => s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${s % 60}s`

  return (
    <div className="completion-panel__body">
      <div className="completion-panel__header">
        <span className="completion-panel__title">Running Tool</span>
        <button className="completion-panel__close" onClick={onDismiss} aria-label="Dismiss">×</button>
      </div>
      <div className="completion-panel__tool-row">
        <span className="completion-panel__spinner" />
        <span className="completion-panel__tool-name">{toolName}</span>
        <span className="completion-panel__elapsed">{fmt(elapsed)}</span>
      </div>
    </div>
  )
}
