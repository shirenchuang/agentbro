/* ApprovalBar — Vibe Island style: warning card + 4 colored buttons, plan approval bar */
import { useRef } from 'react'
import { useTranslation } from 'react-i18next'
import type { SessionState } from '../../types/agent'
import './ApprovalBar.css'

interface ApprovalBarProps {
  session: SessionState
  onAllow: () => void
  onAllowAlways: () => void
  onDeny: () => void
  onAutoApprove?: () => void
  onSendMessage: (msg: string) => void
}

export function ApprovalBar({ session, onAllow, onAllowAlways, onDeny, onAutoApprove, onSendMessage }: ApprovalBarProps) {
  const { t } = useTranslation()
  const inputRef = useRef<HTMLInputElement>(null)

  if (session.phase === 'waiting_approval') {
    const toolName = session.pendingPermission?.toolName || ''
    const toolInput = session.pendingPermission?.toolInput || ''

    return (
      <div className="approval-bar">
        {/* Warning header */}
        <div className="approval-bar__warning">
          <span className="approval-bar__warning-icon">{'\u26A0'}</span>
          <span className="approval-bar__warning-text">{toolName}</span>
        </div>

        {/* Tool detail card */}
        <div className="approval-bar__tool-card">
          <div className="approval-bar__tool-header">
            <span className="approval-bar__tool-icon">{'\uD83D\uDD0D'}</span>
            <span className="approval-bar__tool-name">{toolName}</span>
          </div>
          {toolInput && (
            <div className="approval-bar__tool-input">{toolInput}</div>
          )}
        </div>

        {/* 4 action buttons */}
        <div className="approval-bar__buttons">
          <button className="approval-bar__btn approval-bar__btn--deny" onClick={onDeny}>
            {t('notch.deny')}
          </button>
          <button className="approval-bar__btn approval-bar__btn--allow" onClick={onAllow}>
            {t('notch.allowOnce')}
          </button>
          <button className="approval-bar__btn approval-bar__btn--always" onClick={onAllowAlways}>
            {t('notch.allowAlways')}
          </button>
          {onAutoApprove && (
            <button className="approval-bar__btn approval-bar__btn--auto" onClick={onAutoApprove}>
              {t('notch.autoApprove')}
            </button>
          )}
        </div>
      </div>
    )
  }

  if (session.phase === 'waiting_input' && session.pendingQuestion) {
    return (
      <div className="approval-bar">
        {session.pendingQuestion.options.length > 0 && (
          <div className="approval-bar__options">
            {session.pendingQuestion.options.map((opt, i) => (
              <button
                key={i}
                className="approval-bar__btn approval-bar__btn--option"
                onClick={() => onSendMessage(opt)}
              >
                {i < 3 && <kbd className="approval-bar__shortcut">{'\u2318'}{i + 1}</kbd>}
                {opt}
              </button>
            ))}
          </div>
        )}
        <div className="approval-bar__input-row">
          <input
            className="approval-bar__input"
            placeholder={t('notch.typeReply')}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                const val = (e.target as HTMLInputElement).value.trim()
                if (val) {
                  onSendMessage(val);
                  (e.target as HTMLInputElement).value = ''
                }
              }
            }}
          />
        </div>
      </div>
    )
  }

  // Plan approval mode: input + 3 colored buttons
  if (session.planContent) {
    return (
      <div className="approval-bar approval-bar--plan">
        <div className="approval-bar__input-row">
          <input
            ref={inputRef}
            className="approval-bar__input"
            placeholder={t('notch.planFeedback', { defaultValue: '\u544A\u8BC9 Claude \u9700\u8981\u4FEE\u6539\u4EC0\u4E48...' })}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                const val = (e.target as HTMLInputElement).value.trim()
                if (val) {
                  onSendMessage(val);
                  (e.target as HTMLInputElement).value = ''
                }
              }
            }}
          />
        </div>
        <div className="approval-bar__buttons">
          <button className="approval-bar__btn approval-bar__btn--deny" onClick={onDeny}>
            {t('notch.manualReview', { defaultValue: '\u624B\u52A8\u5BA1\u6279' })}
          </button>
          <button className="approval-bar__btn approval-bar__btn--accept-edits" onClick={onAllow}>
            {t('notch.acceptEdits', { defaultValue: '\u81EA\u52A8\u63A5\u53D7\u7F16\u8F91' })}
          </button>
          <button className="approval-bar__btn approval-bar__btn--auto" onClick={() => onAutoApprove?.()}>
            {t('notch.autoApprovePerms', { defaultValue: '\u81EA\u52A8\u6279\u51C6\u6743\u9650' })}
          </button>
        </div>
      </div>
    )
  }

  // Default: text input
  return (
    <div className="approval-bar">
      <div className="approval-bar__input-row">
        <input
          ref={inputRef}
          className="approval-bar__input"
          placeholder={t('notch.typeMessage')}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              const val = (e.target as HTMLInputElement).value.trim()
              if (val) {
                onSendMessage(val);
                (e.target as HTMLInputElement).value = ''
              }
            }
          }}
        />
        <button
          className="approval-bar__send"
          onClick={() => {
            if (inputRef.current && inputRef.current.value.trim()) {
              onSendMessage(inputRef.current.value)
              inputRef.current.value = ''
            }
          }}
          aria-label={t('notch.send')}
        >
          <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
            <path d="M1 8h14M9 2l6 6-6 6" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
          </svg>
        </button>
      </div>
    </div>
  )
}
