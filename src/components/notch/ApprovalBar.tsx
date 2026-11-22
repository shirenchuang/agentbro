/* ApprovalBar — AgentBro style: warning card + 4 colored buttons, plan approval bar */
import { useEffect, useRef, useCallback, useState, type FocusEvent, type MouseEvent } from 'react'
import { useTranslation } from 'react-i18next'
import type { SessionState } from '../../types/agent'
import { planKey, questionKey, useSessionStore } from '../../stores/sessionStore'
import { restoreNotchWebviewFocus, setNotchFocusable } from '../../services/tauriApi'
import { islandActionProps, runIslandAction } from '../../utils/islandActions'
import { isWindowsPlatform } from '../../utils/platform'
import { getToolActivityLabel } from '../../utils/toolLabels'
import { composerHints, getComposerCapability, supportsPersistentPermissionActions, type ComposerLockReason } from '../../utils/sessionCapabilities'
import './ApprovalBar.css'

interface ApprovalBarProps {
  session: SessionState
  onAllow: () => void
  onAllowAlways: () => void
  onDeny: () => void
  restorePermissionFocus?: () => void
  onAutoApprove?: () => void
  onSendMessage: (msg: string) => void
  onDraftStateChange?: (hasDraft: boolean) => void
  onJumpToHostApp?: () => void
}

export function ApprovalBar({ session, onAllow, onAllowAlways, onDeny, restorePermissionFocus, onAutoApprove, onSendMessage, onDraftStateChange, onJumpToHostApp }: ApprovalBarProps) {
  const { t } = useTranslation()
  const codexAppServerLive = useSessionStore((state) => state.codexAppServerLive)
  const inputRef = useRef<HTMLInputElement>(null)
  const permissionButtonsRef = useCallback((buttons: HTMLDivElement | null) => {
    if (!buttons) return
    const document = buttons.ownerDocument
    const view = document.defaultView
    const recoverOnBlur = isWindowsPlatform()
    let timer: ReturnType<typeof setTimeout> | undefined
    let attemptedButton: Element | null = null

    const onBlur = () => {
      if (timer !== undefined) return
      timer = setTimeout(() => {
        timer = undefined
        const active = document.activeElement
        if (!active || !buttons.contains(active) || active === attemptedButton || document.hasFocus()) return
        attemptedButton = active
        const retry = () => {
          if (attemptedButton === active) attemptedButton = null
        }
        restoreNotchWebviewFocus().then((restored) => {
          if (!restored) retry()
        }, retry)
      }, 0)
    }
    if (recoverOnBlur) view?.addEventListener('blur', onBlur)
    return () => {
      if (recoverOnBlur) view?.removeEventListener('blur', onBlur)
      clearTimeout(timer)
      if (buttons.contains(document.activeElement)) restorePermissionFocus?.()
    }
  }, [restorePermissionFocus])
  const blurTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const hasPlan = Boolean(session.planTitle || session.planContent)
  const pendingQuestionKey = session.pendingQuestion ? questionKey(session.pendingQuestion) : ''
  const questionScope = `${session.id}:${pendingQuestionKey}`
  const draftScope = hasPlan ? `${session.id}:plan:${planKey(session)}` : pendingQuestionKey ? questionScope : session.id
  const [draftState, setDraftState] = useState({ scope: draftScope, value: '' })
  const [questionState, setQuestionState] = useState<{
    key: string
    selected: number[]
    answers: Record<number, number | number[]>
  }>({ key: '', selected: [], answers: {} })
  const draft = draftState.scope === draftScope ? draftState.value : ''
  const hasDraft = draft.trim().length > 0
  const selected = questionState.key === questionScope ? questionState.selected : []
  const answers = questionState.key === questionScope ? questionState.answers : {}
  const supportsPersistentActions = supportsPersistentPermissionActions(session.agentType)

  useEffect(() => {
    onDraftStateChange?.(hasDraft)
  }, [hasDraft, onDraftStateChange])

  useEffect(() => {
    return () => onDraftStateChange?.(false)
  }, [onDraftStateChange])

  const handleComposerFocus = useCallback((event: FocusEvent<HTMLDivElement>) => {
    if (event.currentTarget.contains(event.relatedTarget)) return
    if (blurTimerRef.current) {
      clearTimeout(blurTimerRef.current)
      blurTimerRef.current = null
    }
    setNotchFocusable(true).catch(() => {})
  }, [])

  const handleComposerBlur = useCallback((event: FocusEvent<HTMLDivElement>) => {
    if (event.currentTarget.contains(event.relatedTarget)) return
    blurTimerRef.current = setTimeout(() => {
      blurTimerRef.current = null
      setNotchFocusable(false).catch(() => {})
    }, 200)
  }, [])

  const sendDraft = useCallback(() => {
    const val = draft.trim()
    if (!val) return
    onSendMessage(draft)
    if (!hasPlan && !pendingQuestionKey) setDraftState({ scope: draftScope, value: '' })
  }, [draft, draftScope, hasPlan, onSendMessage, pendingQuestionKey])

  const handleSendButton = useCallback((event: MouseEvent<HTMLButtonElement>) => {
    runIslandAction(event, () => {
      if (blurTimerRef.current) {
        clearTimeout(blurTimerRef.current)
        blurTimerRef.current = null
      }
      sendDraft()
    })
  }, [sendDraft])

  // Plan approval uses the same waiting_approval phase as permissions, so it
  // must be checked first. Otherwise the detail composer labels it as a normal
  // deny/allow/always permission request.
  if (hasPlan) {
    return (
      <div className="approval-bar approval-bar--plan" onFocus={handleComposerFocus} onBlur={handleComposerBlur}>
        <div className="approval-bar__input-row">
          <input
            ref={inputRef}
            className="approval-bar__input"
            data-has-draft={hasDraft ? 'true' : 'false'}
            placeholder={t('notch.planFeedback', { defaultValue: '\u544A\u8BC9 Claude \u9700\u8981\u4FEE\u6539\u4EC0\u4E48...' })}
            value={draft}
            onChange={(e) => setDraftState({ scope: draftScope, value: e.target.value })}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                sendDraft()
              }
            }}
          />
        </div>
        <div className="approval-bar__buttons">
          <button className="approval-bar__btn approval-bar__btn--deny" {...islandActionProps(onDeny)}>
            {t('notch.manualReview', { defaultValue: '\u624B\u52A8\u5BA1\u6279' })}
          </button>
          <button className="approval-bar__btn approval-bar__btn--accept-edits" {...islandActionProps(onAllow)}>
            {t('notch.acceptEdits', { defaultValue: '\u81EA\u52A8\u63A5\u53D7\u7F16\u8F91' })}
          </button>
          <button className="approval-bar__btn approval-bar__btn--auto" {...islandActionProps(() => onAutoApprove?.())}>
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
        <div className="approval-bar__buttons" ref={permissionButtonsRef}>
          <button className="approval-bar__btn approval-bar__btn--deny" {...islandActionProps(onDeny)}>
            {t('notch.deny')}
          </button>
          <button className="approval-bar__btn approval-bar__btn--allow" {...islandActionProps(onAllow)}>
            {t('notch.allowOnce')}
          </button>
          {supportsPersistentActions && (
            <button className="approval-bar__btn approval-bar__btn--always" {...islandActionProps(onAllowAlways)}>
              {t('notch.allowAlways')}
            </button>
          )}
          {supportsPersistentActions && onAutoApprove && (
            <button className="approval-bar__btn approval-bar__btn--auto" {...islandActionProps(onAutoApprove)}>
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
        const base = current.key === questionScope ? current : { key: questionScope, selected: [], answers: {} }
        const next = base.selected.includes(index)
          ? base.selected.filter((item) => item !== index)
          : [...base.selected, index]
        return { ...base, selected: next }
      })
    }
    const toggleNestedQuestionOption = (questionIndex: number, optionIndex: number, multiSelect?: boolean) => {
      setQuestionState((current) => {
        const base = current.key === questionScope ? current : { key: questionScope, selected: [], answers: {} }
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
      <div className="approval-bar" onFocus={handleComposerFocus} onBlur={handleComposerBlur}>
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
                        {...islandActionProps(() => toggleNestedQuestionOption(questionIndex, optionIndex, question.multiSelect))}
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
              {...islandActionProps(submitNestedQuestions)}
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
                {...islandActionProps(() => {
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
                {...islandActionProps(submitSingleMultiSelect)}
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
            onChange={(e) => setDraftState({ scope: draftScope, value: e.target.value })}
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
    <div className="approval-bar" onFocus={handleComposerFocus} onBlur={handleComposerBlur}>
      <div className="approval-bar__input-row">
        <input
          ref={inputRef}
          className="approval-bar__input"
          data-has-draft={hasDraft ? 'true' : 'false'}
          placeholder={t('notch.typeMessage')}
          value={draft}
          onChange={(e) => setDraftState({ scope: draftScope, value: e.target.value })}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              sendDraft()
            }
          }}
        />
        <button
          className="approval-bar__send"
          onMouseDown={handleSendButton}
          onClick={handleSendButton}
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
  const { messageKey, actionKey: ctaKey } = composerHints[reason]
  return (
    <div className="approval-bar__hint" role="note">
      <span className="approval-bar__hint-text">{t(messageKey)}</span>
      {ctaKey && onJumpToHostApp && (
        <button
          className="approval-bar__hint-cta"
          type="button"
          {...islandActionProps(onJumpToHostApp)}
        >
          {t(ctaKey)}
        </button>
      )}
    </div>
  )
}
