import { useCallback, useEffect, useRef, useState, type CSSProperties, type KeyboardEvent, type MouseEvent } from 'react'
import { useTranslation } from 'react-i18next'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import type { SessionState } from '../../types/agent'
import { useSessionStore } from '../../stores/sessionStore'
import { sendMessage, setNotchFocusable } from '../../services/tauriApi'
import { getAgentDisplayName, getSessionAppLabel, getSessionTerminalLabel, getSessionTitle } from '../../utils/sessionDisplay'
import { formatDurationShort } from '../../utils/time'
import { MascotRouter } from '../notch/mascots/MascotRouter'
import './OverlayFeedbackPanel.css'

interface OverlayFeedbackPanelProps {
  session: SessionState
  userMessage?: string
  text: string
  kind?: 'completion' | 'response'
  maxHeight?: number
  dwellMs: number
  startedAt?: number
  statusLabel: string
  onJumpToTerminal: () => void
  onShowSessions?: () => void
  onDismiss: () => void
  onDraftStateChange?: (hasDraft: boolean) => void
  sessionCount?: number
}

function shouldIgnorePanelJump(target: EventTarget | null): boolean {
  return target instanceof Element && Boolean(
    target.closest('button, input, textarea, select, a, [role="button"]'),
  )
}

export function OverlayFeedbackPanel({
  session,
  userMessage,
  text,
  kind = 'response',
  maxHeight,
  dwellMs,
  startedAt,
  statusLabel,
  onJumpToTerminal,
  onShowSessions,
  onDismiss,
  onDraftStateChange,
  sessionCount,
}: OverlayFeedbackPanelProps) {
  const { t } = useTranslation()
  const [inputValue, setInputValue] = useState('')
  const [sending, setSending] = useState(false)
  const [isTimerPaused, setIsTimerPaused] = useState(false)
  const [progressRatio, setProgressRatio] = useState(1)
  const overlayRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const fallbackStartedAtRef = useRef(startedAt ?? Date.now())
  const onDismissRef = useRef(onDismiss)
  const pointerInsideRef = useRef(false)
  const inputFocusedRef = useRef(false)
  const dismissPendingRef = useRef(false)
  const remainingRef = useRef(dwellMs)
  const startedAtRef = useRef(0)
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const progressIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const blurTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const countdownStartedAt = startedAt ?? fallbackStartedAtRef.current
  const countdownDeadline = countdownStartedAt + dwellMs
  const hasInputDraft = inputValue.trim().length > 0

  const shownUserMessage = userMessage || session.lastUserMessage
  const appLabel = getSessionAppLabel(session)
  const terminalLabel = getSessionTerminalLabel(session)
  const agentName = getAgentDisplayName(session)

  useEffect(() => {
    onDismissRef.current = onDismiss
  }, [onDismiss])

  useEffect(() => {
    onDraftStateChange?.(hasInputDraft)
  }, [hasInputDraft, onDraftStateChange])

  useEffect(() => () => onDraftStateChange?.(false), [onDraftStateChange])

  const updateProgress = useCallback(() => {
    if (dwellMs <= 0) {
      setProgressRatio(0)
      return
    }
    if (timerRef.current) {
      const elapsed = Date.now() - startedAtRef.current
      setProgressRatio(Math.max(0, (remainingRef.current - elapsed) / dwellMs))
    } else {
      setProgressRatio(Math.max(0, remainingRef.current / dwellMs))
    }
  }, [dwellMs])

  const scheduleDismiss = useCallback((delayMs: number) => {
    startedAtRef.current = Date.now()
    timerRef.current = setTimeout(() => {
      timerRef.current = null
      remainingRef.current = 0
      setProgressRatio(0)
      if (pointerInsideRef.current || inputFocusedRef.current) {
        setIsTimerPaused(true)
      }
      dismissPendingRef.current = true
    }, Math.max(0, delayMs))
  }, [])

  useEffect(() => {
    const remaining = Math.max(0, countdownDeadline - Date.now())
    remainingRef.current = remaining
    setProgressRatio(dwellMs <= 0 ? 0 : Math.min(1, remaining / dwellMs))
    dismissPendingRef.current = false
    pointerInsideRef.current = false
    scheduleDismiss(remaining)
    progressIntervalRef.current = setInterval(updateProgress, 100)
    setIsTimerPaused(false)
    inputFocusedRef.current = false

    return () => {
      if (timerRef.current) clearTimeout(timerRef.current)
      if (progressIntervalRef.current) clearInterval(progressIntervalRef.current)
      if (blurTimerRef.current) clearTimeout(blurTimerRef.current)
    }
  }, [countdownDeadline, dwellMs, scheduleDismiss, updateProgress])

  const releaseDismissHold = useCallback(() => {
    if (inputValue.trim()) return
    if (pointerInsideRef.current || inputFocusedRef.current) return
    if (dismissPendingRef.current || remainingRef.current <= 0) {
      onDismissRef.current()
      return
    }
    setIsTimerPaused(false)
  }, [inputValue])

  useEffect(() => {
    if (!isTimerPaused) return undefined

    const interval = window.setInterval(() => {
      if (inputFocusedRef.current) return
      const isActuallyHovered = overlayRef.current?.matches(':hover') ?? false
      pointerInsideRef.current = isActuallyHovered
      if (!isActuallyHovered) releaseDismissHold()
    }, 250)

    return () => window.clearInterval(interval)
  }, [isTimerPaused, releaseDismissHold])

  const focusInput = useCallback(() => {
    inputFocusedRef.current = true
    setIsTimerPaused(true)
    if (blurTimerRef.current) {
      clearTimeout(blurTimerRef.current)
      blurTimerRef.current = null
    }
    setNotchFocusable(true)
      .then(() => {
        window.requestAnimationFrame(() => inputRef.current?.focus())
      })
      .catch(() => {
        inputRef.current?.focus()
      })
  }, [])

  const releaseInputFocus = useCallback(() => {
    if (blurTimerRef.current) clearTimeout(blurTimerRef.current)
    blurTimerRef.current = setTimeout(() => {
      blurTimerRef.current = null
      inputFocusedRef.current = false
      setNotchFocusable(false).catch(() => {})
      pointerInsideRef.current = overlayRef.current?.matches(':hover') ?? false
      releaseDismissHold()
    }, 200)
  }, [releaseDismissHold])

  const handleSend = useCallback(async () => {
    const value = inputValue.trim()
    if (!value || sending) return
    setSending(true)
    try {
      await sendMessage(session.id, value)
      useSessionStore.getState().updateSession({
        type: 'user_message',
        sessionId: session.id,
        content: value,
      })
      setInputValue('')
      inputFocusedRef.current = false
    } finally {
      setSending(false)
    }
  }, [inputValue, sending, session.id])

  const handleKeyDown = useCallback((event: KeyboardEvent<HTMLInputElement>) => {
    event.stopPropagation()
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault()
      void handleSend()
    } else if (event.key === 'Escape') {
      event.preventDefault()
      if (inputValue) setInputValue('')
      else onDismissRef.current()
    }
  }, [handleSend, inputValue])

  const handleJump = useCallback(() => {
    onJumpToTerminal()
  }, [onJumpToTerminal])

  const handlePanelMouseDown = useCallback((event: MouseEvent<HTMLDivElement>) => {
    if (event.button !== 0 || shouldIgnorePanelJump(event.target)) return
    event.preventDefault()
    handleJump()
  }, [handleJump])

  return (
    <div
      ref={overlayRef}
      className={`overlay-feedback overlay-feedback--${kind}${isTimerPaused ? ' overlay-feedback--paused' : ''}`}
      style={maxHeight ? ({ '--overlay-feedback-reader-height': `${maxHeight}px` } as CSSProperties) : undefined}
      onMouseDown={handlePanelMouseDown}
      onMouseEnter={() => {
        pointerInsideRef.current = true
        setIsTimerPaused(true)
      }}
      onMouseLeave={() => {
        pointerInsideRef.current = false
        releaseDismissHold()
      }}
    >
      <div className="overlay-feedback__session" data-no-drag>
        <div className="overlay-feedback__avatar">
          <MascotRouter toolType={session.agentType} phase={session.phase} size={28} />
        </div>
        <div className="overlay-feedback__session-copy">
          <div className="overlay-feedback__session-row">
            <span className="overlay-feedback__title">{getSessionTitle(session)}</span>
            {appLabel && <span className="overlay-feedback__badge overlay-feedback__badge--source">{appLabel}</span>}
            <span className="overlay-feedback__badge overlay-feedback__badge--status">
              <span className="overlay-feedback__status-dot" />
              {statusLabel}
            </span>
            <span className="overlay-feedback__badge">{agentName}</span>
            {terminalLabel && <span className="overlay-feedback__badge">{terminalLabel}</span>}
            <span className="overlay-feedback__duration">{formatDurationShort(session.duration)}</span>
            <div className="overlay-feedback__actions">
              <button
                type="button"
                className="overlay-feedback__icon-btn overlay-feedback__jump-icon"
                aria-label={t('notch.jumpToTerminal')}
                onMouseDown={(event) => {
                  event.preventDefault()
                  event.stopPropagation()
                  handleJump()
                }}
              >
                ↗
              </button>
              <button
                type="button"
                className="overlay-feedback__icon-btn overlay-feedback__close"
                aria-label={t('notch.dismiss', { defaultValue: 'Dismiss' })}
                onMouseDown={(event) => {
                  event.preventDefault()
                  event.stopPropagation()
                  onDismissRef.current()
                }}
              >
                ×
              </button>
            </div>
          </div>
          {shownUserMessage && (
            <div className="overlay-feedback__user-line">
              <span>{t('notch.you', '你')}：</span>
              <span>{shownUserMessage}</span>
            </div>
          )}
          <div className="overlay-feedback__preview">{text}</div>
        </div>
      </div>

      <button type="button" className="overlay-feedback__detail" onMouseDown={handleJump}>
        <div className="overlay-feedback__scroll">
          <div className="overlay-feedback__transcript">
            <div className="overlay-feedback__conversation">
              {shownUserMessage && (
                <div className="overlay-feedback__message overlay-feedback__message--user">
                  <span className="overlay-feedback__message-prefix">{t('notch.you', '你')}：</span>
                  <span className="overlay-feedback__message-text">{shownUserMessage}</span>
                </div>
              )}
              <div className="overlay-feedback__message overlay-feedback__message--assistant">
                <div className="overlay-feedback__markdown">
                  <ReactMarkdown remarkPlugins={[remarkGfm]}>
                    {text}
                  </ReactMarkdown>
                </div>
              </div>
            </div>
          </div>
        </div>
      </button>

      <div className="overlay-feedback__reply" data-no-drag>
        <input
          ref={inputRef}
          className="overlay-feedback__input"
          data-has-draft={hasInputDraft ? 'true' : 'false'}
          value={inputValue}
          placeholder={t('notch.typeMessage', { defaultValue: 'Send a message...' })}
          disabled={sending}
          onChange={(event) => setInputValue(event.target.value)}
          onKeyDown={handleKeyDown}
          onMouseDown={(event) => {
            event.preventDefault()
            event.stopPropagation()
            focusInput()
          }}
          onClick={(event) => event.stopPropagation()}
          onFocus={focusInput}
          onBlur={releaseInputFocus}
        />
        <button
          type="button"
          className="overlay-feedback__send"
          disabled={!inputValue.trim() || sending}
          onMouseDown={(event) => {
            event.preventDefault()
            event.stopPropagation()
            if (blurTimerRef.current) {
              clearTimeout(blurTimerRef.current)
              blurTimerRef.current = null
            }
            void handleSend()
          }}
        >
          {sending ? '...' : t('notch.send', { defaultValue: 'Send' })}
        </button>
      </div>

      <div className="overlay-card__secondary" data-no-drag>
        {onShowSessions && sessionCount != null ? (
          <button
            type="button"
            className="overlay-card__show-sessions"
            onMouseDown={(event) => {
              event.preventDefault()
              onShowSessions()
            }}
          >
            <span className="overlay-card__brand-logo-stack" aria-hidden="true">
              <img className="overlay-card__brand-logo overlay-card__brand-logo--light" src="/agentbro-logo.png" alt="" />
              <img className="overlay-card__brand-logo overlay-card__brand-logo--dark" src="/agentbro-logo-dark.png" alt="" />
            </span>
            <span>{t('notch.slogan', { defaultValue: '让 Agent 更好用' })}</span>
          </button>
        ) : (
          <div className="overlay-card__show-sessions overlay-card__show-sessions--static">
            <span className="overlay-card__brand-logo-stack" aria-hidden="true">
              <img className="overlay-card__brand-logo overlay-card__brand-logo--light" src="/agentbro-logo.png" alt="" />
              <img className="overlay-card__brand-logo overlay-card__brand-logo--dark" src="/agentbro-logo-dark.png" alt="" />
            </span>
            <span>{t('notch.slogan', { defaultValue: '让 Agent 更好用' })}</span>
          </div>
        )}
      </div>

      <div className="overlay-feedback__progress" aria-hidden>
        <div className="overlay-feedback__progress-bar" style={{ transform: `scaleX(${progressRatio})` }} />
      </div>
    </div>
  )
}
