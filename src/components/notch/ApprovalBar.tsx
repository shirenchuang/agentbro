/* ApprovalBar — AgentBro style: warning card + 4 colored buttons, plan approval bar */
import { useEffect, useRef, useCallback, useState, type MouseEvent } from 'react'
import { useTranslation } from 'react-i18next'
import type { SessionState } from '../../types/agent'
import { useSessionStore } from '../../stores/sessionStore'
import { setNotchFocusable } from '../../services/tauriApi'
import { getToolActivityLabel } from '../../utils/toolLabels'
import { getComposerCapability, type ComposerLockReason } from '../../utils/sessionCapabilities'
import './ApprovalBar.css'

interface ApprovalBarProps {
  session: SessionState
  onAllow: () => void
  onAllowAlways: () => void
  onDeny: () => void
  onAutoApprove?: () => void
  onSendMessage: (msg: string) => void
  onDraftStateChange?: (hasDraft: boolean) => void
  onJumpToHostApp?: () => void
}

export function ApprovalBar({ session, onAllow, onAllowAlways, onDeny, onAutoApprove, onSendMessage, onDraftStateChange, onJumpToHostApp }: ApprovalBarProps) {
  const { t } = useTranslation()
  const codexAppServerLive = useSessionStore((state) => state.codexAppServerLive)
  const inputRef = useRef<HTMLInputElement>(null)
  const blurTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const [draftState, setDraftState] = useState({ sessionId: session.id, value: '' })
  const [questionState, setQuestionState] = useState<{
    key: string
    selected: number[]
    answers: Record<number, number | number[]>
  }>({ key: '', selected: [], answers: {} })
  const draft = draftState.sessionId === session.id ? draftState.value : ''
  const hasDraft = draft.trim().length > 0
  const questionKey = session.pendingQuestion
    ? `${session.id}:${session.pendingQuestion.question}:${session.pendingQuestion.options.join('|')}:${session.pendingQuestion.questions?.length ?? 0}`
    : ''
  const selected = questionState.key === questionKey ? questionState.selected : []
  const answers = questionState.key === questionKey ? questionState.answers : {}
  const supportsPersistentPermissionActions = session.agentType !== 'codex'

  useEffect(() => {
    onDraftStateChange?.(hasDraft)
  }, [hasDraft, onDraftStateChange])

  useEffect(() => {
    return () => onDraftStateChange?.(false)
  }, [onDraftStateChange])

  const handleInputFocus = useCallback(() => {
    if (blurTimerRef.current) {
      clearTimeout(blurTimerRef.current)
      blurTimerRef.current = null
    }
    setNotchFocusable(true).catch(() => {})
  }, [])

  const handleInputBlur = useCallback(() => {
    blurTimerRef.current = setTimeout(() => {
      blurTimerRef.current = null
      setNotchFocusable(false).catch(() => {})
    }, 200)
  }, [])

  const sendDraft = useCallback(() => {
    const val = draft.trim()
    if (!val) return
    onSendMessage(draft)
    setDraftState({ sessionId: session.id, value: '' })
  }, [draft, onSendMessage, session.id])

  const runActionOnMouseDown = useCallback((event: MouseEvent<HTMLButtonElement>, action: () => void) => {
    event.preventDefault()
    event.stopPropagation()
    action()
  }, [])

  // Plan approval uses the same waiting_approval phase as permissions, so it
  // must be checked first. Otherwise the detail composer labels it as a normal
  // deny/allow/always permission request.
  if (session.planContent || session.planTitle) {
    return (
      <div className="approval-bar approval-bar--plan">
        <div className="approval-bar__input-row">
          <input
            ref={inputRef}
            className="approval-bar__input"
            data-has-draft={hasDraft ? 'true' : 'false'}
            placeholder={t('notch.planFeedback', { defaultValue: '\u544A\u8BC9 Claude \u9700\u8981\u4FEE\u6539\u4EC0\u4E48...' })}
            value={draft}
            onChange={(e) => setDraftState({ sessionId: session.id, value: e.target.value })}
            onFocus={handleInputFocus}
            onBlur={handleInputBlur}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                sendDraft()
              }
            }}
          />
        </div>
        <div className="approval-bar__buttons">
          <button className="approval-bar__btn approval-bar__btn--deny" onMouseDown={(event) => runActionOnMouseDown(event, onDeny)}>
            {t('notch.manualReview', { defaultValue: '\u624B\u52A8\u5BA1\u6279' })}
          </button>
          <button className="approval-bar__btn approval-bar__btn--accept-edits" onMouseDown={(event) => runActionOnMouseDown(event, onAllow)}>
            {t('notch.acceptEdits', { defaultValue: '\u81EA\u52A8\u63A5\u53D7\u7F16\u8F91' })}
          </button>
          <button className="approval-bar__btn approval-bar__btn--auto" onMouseDown={(event) => runActionOnMouseDown(event, () => onAutoApprove?.())}>
            {t('notch.autoApprovePerms', { defaultValue: '\u81EA\u52A8\u6279\u51C6\u6743\u9650' })}
          </button>
        </div>
      </div>
    )
  }

  if (session.phase === 'waiting_approval') {
    const toolName = session.pendingPermission?.toolName || ''
    const toolLabel = toolName ? getToolActivityLabel(t, toolName) : ''
    const toolInput = session.pendingPermission?.toolInput || ''

    return (
      <div className="approval-bar">
        {/* Warning header */}
        <div className="approval-bar__warning">
          <span className="approval-bar__warning-icon">{'\u26A0'}</span>
          <span className="approval-bar__warning-text">{toolLabel}</span>
        </div>

        {/* Tool detail card */}
        <div className="approval-bar__tool-card">
          <div className="approval-bar__tool-header">
            <span className="approval-bar__tool-icon">{'\uD83D\uDD0D'}</span>
            <span className="approval-bar__tool-name">{toolLabel}</span>
          </div>
          {toolInput && (
            <div className="approval-bar__tool-input">{toolInput}</div>
          )}
        </div>

        {/* 4 action buttons */}
        <div className="approval-bar__buttons">
          <button className="approval-bar__btn approval-bar__btn--deny" onMouseDown={(event) => runActionOnMouseDown(event, onDeny)}>
            {t('notch.deny')}
          </button>
          <button className="approval-bar__btn approval-bar__btn--allow" onMouseDown={(event) => runActionOnMouseDown(event, onAllow)}>
            {t('notch.allowOnce')}
          </button>
          {supportsPersistentPermissionActions && (
            <button className="approval-bar__btn approval-bar__btn--always" onMouseDown={(event) => runActionOnMouseDown(event, onAllowAlways)}>
              {t('notch.allowAlways')}
            </button>
          )}
          {supportsPersistentPermissionActions && onAutoApprove && (
            <button className="approval-bar__btn approval-bar__btn--auto" onMouseDown={(event) => runActionOnMouseDown(event, onAutoApprove)}>
              {t('notch.autoApprove')}
            </button>
          )}
        </div>
      </div>
    )
  }

  if (session.phase === 'waiting_input' && session.pendingQuestion) {
    const pendingQuestion = session.pendingQuestion
    const nestedQuestions = pendingQuestion.questions || []
    const hasMultipleQuestions = nestedQuestions.length > 1
    const toggleSingleQuestionOption = (index: number) => {
      setQuestionState((current) => {
        const base = current.key === questionKey ? current : { key: questionKey, selected: [], answers: {} }
        const next = base.selected.includes(index)
          ? base.selected.filter((item) => item !== index)
          : [...base.selected, index]
        return { ...base, selected: next }
      })
    }
    const toggleNestedQuestionOption = (questionIndex: number, optionIndex: number, multiSelect?: boolean) => {
      setQuestionState((current) => {
        const base = current.key === questionKey ? current : { key: questionKey, selected: [], answers: {} }
        if (!multiSelect) return { ...base, answers: { ...base.answers, [questionIndex]: optionIndex } }

        const existing = Array.isArray(base.answers[questionIndex])
          ? [...(base.answers[questionIndex] as number[])]
          : []
        const next = existing.includes(optionIndex)
          ? existing.filter((item) => item !== optionIndex)
          : [...existing, optionIndex]
        const answers = { ...base.answers }
        if (next.length > 0) answers[questionIndex] = next
        else delete answers[questionIndex]
        return { ...base, answers }
      })
    }
    const submitSingleMultiSelect = () => {
      if (selected.length === 0) return
      const labels = selected.map((index) => pendingQuestion.options[index]).filter(Boolean)
      if (labels.length > 0) onSendMessage(labels.join(', '))
    }
    const allNestedAnswered = hasMultipleQuestions
      && nestedQuestions.every((_, index) => answers[index] !== undefined)
    const submitNestedQuestions = () => {
      if (!allNestedAnswered) return
      const payload: Record<string, string> = {}
      nestedQuestions.forEach((question, index) => {
        const value = answers[index]
        payload[question.question] = Array.isArray(value)
          ? value.map((optionIndex) => question.options[optionIndex]?.label).filter(Boolean).join(', ')
          : question.options[value as number]?.label ?? ''
      })
      onSendMessage(JSON.stringify(payload))
    }

    return (
      <div className="approval-bar">
        {hasMultipleQuestions ? (
          <div className="approval-bar__question-set">
            <div className="approval-bar__question-title">
              {nestedQuestions.length} {t('notch.questionsCount', { defaultValue: 'questions' })}
            </div>
            {nestedQuestions.map((question, questionIndex) => {
              const current = answers[questionIndex]
              const selectedSet = new Set(Array.isArray(current) ? current : current !== undefined ? [current] : [])
              return (
                <div className="approval-bar__question-group" key={`${questionIndex}-${question.question}`}>
                  <div className="approval-bar__question-text">
                    {questionIndex + 1}. {question.header && <span>[{question.header}] </span>}{question.question}
                    {question.multiSelect && <span className="approval-bar__question-mode">{t('notch.multiSelect', { defaultValue: 'Multi-select' })}</span>}
                  </div>
                  <div className="approval-bar__options">
                    {question.options.map((opt, optionIndex) => (
                      <button
                        key={`${questionIndex}-${optionIndex}-${opt.label}`}
                        className={`approval-bar__btn approval-bar__btn--option ${selectedSet.has(optionIndex) ? 'approval-bar__btn--selected' : ''}`}
                        onMouseDown={(event) => runActionOnMouseDown(event, () => toggleNestedQuestionOption(questionIndex, optionIndex, question.multiSelect))}
                      >
                        {question.multiSelect && selectedSet.has(optionIndex) ? '✓ ' : ''}
                        {opt.label}
                      </button>
                    ))}
                  </div>
                </div>
              )
            })}
            <button
              className="approval-bar__btn approval-bar__btn--confirm"
              disabled={!allNestedAnswered}
              onMouseDown={(event) => runActionOnMouseDown(event, submitNestedQuestions)}
            >
              {t('notch.submitAll', { defaultValue: 'Submit All' })}
            </button>
          </div>
        ) : pendingQuestion.options.length > 0 && (
          <div className="approval-bar__options">
            {pendingQuestion.options.map((opt, i) => (
              <button
                key={i}
                className={`approval-bar__btn approval-bar__btn--option ${selected.includes(i) ? 'approval-bar__btn--selected' : ''}`}
                onMouseDown={(event) => runActionOnMouseDown(event, () => {
                  if (pendingQuestion.multiSelect) toggleSingleQuestionOption(i)
                  else onSendMessage(opt)
                })}
              >
                {i < 3 && <kbd className="approval-bar__shortcut">{'\u2318'}{i + 1}</kbd>}
                {pendingQuestion.multiSelect && selected.includes(i) ? '✓ ' : ''}
                {opt}
              </button>
            ))}
            {pendingQuestion.multiSelect && (
              <button
                className="approval-bar__btn approval-bar__btn--confirm"
                disabled={selected.length === 0}
                onMouseDown={(event) => runActionOnMouseDown(event, submitSingleMultiSelect)}
              >
                {t('notch.confirmSelection', { defaultValue: 'Confirm' })} ({selected.length})
              </button>
            )}
          </div>
        )}
        <div className="approval-bar__input-row">
          <input
            className="approval-bar__input"
            data-has-draft={hasDraft ? 'true' : 'false'}
            placeholder={t('notch.typeReply')}
            value={draft}
            onChange={(e) => setDraftState({ sessionId: session.id, value: e.target.value })}
            onFocus={handleInputFocus}
            onBlur={handleInputBlur}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                sendDraft()
              }
            }}
          />
        </div>
      </div>
    )
  }

  // Default: text input
  const capability = getComposerCapability(session, {
    codexAppServerLive,
    codexDesktopRepliesSupported: true,
  })
  if (capability.kind === 'locked') {
    return (
      <div className="approval-bar">
        <ComposerHint reason={capability.reason} onJumpToHostApp={onJumpToHostApp} />
      </div>
    )
  }

  return (
    <div className="approval-bar">
      <div className="approval-bar__input-row">
        <input
          ref={inputRef}
          className="approval-bar__input"
          data-has-draft={hasDraft ? 'true' : 'false'}
          placeholder={t('notch.typeMessage')}
          value={draft}
          onChange={(e) => setDraftState({ sessionId: session.id, value: e.target.value })}
          onFocus={handleInputFocus}
          onBlur={handleInputBlur}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              sendDraft()
            }
          }}
        />
        <button
          className="approval-bar__send"
          onMouseDown={(e) => {
            e.preventDefault()
            if (blurTimerRef.current) {
              clearTimeout(blurTimerRef.current)
              blurTimerRef.current = null
            }
            sendDraft()
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

interface ComposerHintProps {
  reason: ComposerLockReason
  onJumpToHostApp?: () => void
}

function ComposerHint({ reason, onJumpToHostApp }: ComposerHintProps) {
  const { t } = useTranslation()
  const messageKey = composerHintMessageKey(reason)
  const ctaKey = composerHintCtaKey(reason)
  return (
    <div className="approval-bar__hint" role="note">
      <span className="approval-bar__hint-text">{t(messageKey)}</span>
      {ctaKey && onJumpToHostApp && (
        <button
          className="approval-bar__hint-cta"
          type="button"
          onMouseDown={(event) => {
            event.preventDefault()
            event.stopPropagation()
            onJumpToHostApp()
          }}
        >
          {t(ctaKey)}
        </button>
      )}
    </div>
  )
}

function composerHintMessageKey(reason: ComposerLockReason): string {
  switch (reason) {
    case 'codex-app': return 'notch.composerHintCodexApp'
    case 'qoder-app': return 'notch.composerHintQoderApp'
    case 'remote': return 'notch.composerHintRemote'
    case 'no-terminal': return 'notch.composerHintNoTerminal'
  }
}

function composerHintCtaKey(reason: ComposerLockReason): string | null {
  switch (reason) {
    case 'codex-app':
    case 'qoder-app':
      return 'notch.openHostApp'
    case 'remote':
    case 'no-terminal':
      return null
  }
}
