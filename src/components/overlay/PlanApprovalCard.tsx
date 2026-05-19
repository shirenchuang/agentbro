import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import type { OverlayItem, SessionState, SubagentInfo } from '../../types/agent'
import { OverlayCard } from './OverlayCard'
import './PlanApprovalCard.css'

interface PlanApprovalCardProps {
  overlay: OverlayItem
  session: SessionState
  onSendFeedback: (feedback: string) => void
  onManualReview: () => void
  onAcceptEdits: () => void
  onAutoApprove: () => void
  onShowSessions?: () => void
  onDismiss: () => void
  onDraftStateChange?: (hasDraft: boolean) => void
  sessionCount?: number
}

function subagentStatusLabel(status: SubagentInfo['status']) {
  if (status === 'running') return '运行中'
  if (status === 'completed') return '完成'
  return '失败'
}

function CompactSubagentSummary({ subagents }: { subagents: SubagentInfo[] }) {
  if (subagents.length === 0) return null

  return (
    <div className="plan-approval__subagents">
      <div className="plan-approval__subagents-header">
        <span className="plan-approval__subagents-icon">⑂</span>
        <span>Subagents ({subagents.length})</span>
      </div>
      <div className="plan-approval__subagents-list">
        {subagents.map((subagent) => {
          const title = subagent.name ? `@${subagent.name}` : (subagent.agentType || `@${subagent.agentId.slice(0, 8)}`)
          const detail = subagent.description || subagent.lastAssistantMessage

          return (
            <div key={subagent.agentId} className="plan-approval__subagent">
              <span className={`plan-approval__subagent-dot plan-approval__subagent-dot--${subagent.status}`} />
              <span className="plan-approval__subagent-title">{title}</span>
              {detail && <span className="plan-approval__subagent-detail">({detail})</span>}
              <span className={`plan-approval__subagent-status plan-approval__subagent-status--${subagent.status}`}>
                {subagentStatusLabel(subagent.status)}
              </span>
            </div>
          )
        })}
      </div>
    </div>
  )
}

function formatPlanMarkdown(content: string): string {
  return content
    .split('\n')
    .map((line) => {
      const trimmed = line.trim()
      if (!trimmed || trimmed.startsWith('#')) return line
      if (/^(context|plan|test plan|root cause|assumptions|requested permissions)$/i.test(trimmed)) {
        return `### ${trimmed}`
      }
      return line
    })
    .join('\n')
}

function parsePlanPermission(permission: string): { tool: string; prompt?: string } {
  const match = permission.match(/^([^:：]+)[:：]\s*(.*)$/)
  if (!match) return { tool: permission }
  return { tool: match[1].trim(), prompt: match[2].trim() }
}

export function PlanApprovalCard({ overlay, session, onSendFeedback, onManualReview, onAcceptEdits, onAutoApprove, onShowSessions, onDismiss, onDraftStateChange, sessionCount }: PlanApprovalCardProps) {
  const { t } = useTranslation()
  const data = overlay.data as { planTitle?: string; planContent: string; requestedPermissions?: Array<string | { tool: string; prompt: string }> }
  const [feedback, setFeedback] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)
  const hasDraft = feedback.trim().length > 0

  useEffect(() => {
    onDraftStateChange?.(hasDraft)
  }, [hasDraft, onDraftStateChange])

  useEffect(() => () => onDraftStateChange?.(false), [onDraftStateChange])

  const handleSendFeedback = () => {
    const val = feedback.trim()
    if (val) {
      onSendFeedback(val)
      setFeedback('')
    }
  }

  return (
    <OverlayCard
      session={session}
      onDismiss={onDismiss}
      onShowSessions={onShowSessions}
      sessionCount={sessionCount}
      className="overlay-card--plan-approval"
      bodyClassName="plan-approval"
    >
      <CompactSubagentSummary subagents={session.subagents || []} />

      {/* Plan header */}
      <div className="plan-approval__header">
        {data.planTitle && <span className="plan-approval__title">{data.planTitle}</span>}
        <span className="plan-approval__tag">{t('notch.plan', { defaultValue: '\u8BA1\u5212' })}</span>
      </div>

      {/* Markdown content */}
      <div className="plan-approval__content">
        <ReactMarkdown remarkPlugins={[remarkGfm]}>
          {formatPlanMarkdown(data.planContent)}
        </ReactMarkdown>
      </div>

      {/* Requested permissions */}
      {data.requestedPermissions && data.requestedPermissions.length > 0 && (
        <div className="plan-approval__perms">
          <span className="plan-approval__perms-label">{t('notch.requestedPermissions', { defaultValue: '请求的权限:' })}</span>
          {data.requestedPermissions.map((p, i) => {
            const permission = typeof p === 'string' ? parsePlanPermission(p) : p
            return (
              <div key={i} className="plan-approval__perm-item">
                <span><span className="plan-approval__perm-tool">{permission.tool}</span>{permission.prompt ? `: ${permission.prompt}` : ''}</span>
              </div>
            )
          })}
        </div>
      )}

      {/* Feedback input */}
      <div className="plan-approval__input-row">
        <input
          ref={inputRef}
          className="plan-approval__input"
          data-has-draft={hasDraft ? 'true' : 'false'}
          placeholder={t('notch.planFeedback', { defaultValue: 'Tell Claude what to change...' })}
          value={feedback}
          onChange={(e) => setFeedback(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter') handleSendFeedback() }}
        />
      </div>

      {/* Actions */}
      <div className="plan-approval__actions">
        <button
          className="plan-approval__btn plan-approval__btn--feedback"
          onClick={feedback.trim() ? handleSendFeedback : onManualReview}
        >
          {feedback.trim() ? t('notch.sendFeedback', { defaultValue: 'Send Feedback' }) : t('notch.manualReview', { defaultValue: 'Manual Review' })}
        </button>
        <button className="plan-approval__btn plan-approval__btn--accept" onClick={onAcceptEdits}>
          {t('notch.acceptEdits', { defaultValue: 'Accept Edits' })}
        </button>
        <button className="plan-approval__btn plan-approval__btn--auto" onClick={onAutoApprove}>
          {t('notch.autoApprovePerms', { defaultValue: 'Auto' })}
        </button>
      </div>
    </OverlayCard>
  )
}
