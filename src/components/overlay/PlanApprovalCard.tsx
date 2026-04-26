import { useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import type { OverlayItem, SessionState } from '../../types/agent'
import { OverlayCard } from './OverlayCard'
import './PlanApprovalCard.css'

interface PlanApprovalCardProps {
  overlay: OverlayItem
  session: SessionState
  onSendFeedback: (feedback: string) => void
  onAcceptEdits: () => void
  onAutoApprove: () => void
  onDismiss: () => void
}

export function PlanApprovalCard({ overlay, session, onSendFeedback, onAcceptEdits, onAutoApprove, onDismiss }: PlanApprovalCardProps) {
  const { t } = useTranslation()
  const data = overlay.data as { planTitle?: string; planContent: string; requestedPermissions?: string[] }
  const [feedback, setFeedback] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)

  const handleSendFeedback = () => {
    const val = feedback.trim()
    if (val) {
      onSendFeedback(val)
      setFeedback('')
    }
  }

  return (
    <OverlayCard session={session} onDismiss={onDismiss}>
      {/* Plan header */}
      <div className="plan-approval__header">
        {data.planTitle && <span className="plan-approval__title">{data.planTitle}</span>}
        <span className="plan-approval__tag">{t('notch.plan', { defaultValue: '\u8BA1\u5212' })}</span>
      </div>

      {/* Markdown content */}
      <div className="plan-approval__content">
        <ReactMarkdown remarkPlugins={[remarkGfm]}>
          {data.planContent}
        </ReactMarkdown>
      </div>

      {/* Requested permissions */}
      {data.requestedPermissions && data.requestedPermissions.length > 0 && (
        <div className="plan-approval__perms">
          <span className="plan-approval__perms-label">Requested permissions:</span>
          {data.requestedPermissions.map((p, i) => (
            <span key={i} className="plan-approval__perm-item">{p}</span>
          ))}
        </div>
      )}

      {/* Feedback input */}
      <div className="plan-approval__input-row">
        <input
          ref={inputRef}
          className="plan-approval__input"
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
          onClick={feedback.trim() ? handleSendFeedback : onDismiss}
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
