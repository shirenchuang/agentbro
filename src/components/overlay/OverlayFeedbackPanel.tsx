import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from 'react'
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
  maxHeight?: number
  dwellMs: number
  statusLabel: string
  onJumpToTerminal: () => void
  onDismiss: () => void
}

export function OverlayFeedbackPanel({
  session,
  userMessage,
  text,
  maxHeight,
  dwellMs,
  statusLabel,
  onJumpToTerminal,
  onDismiss,
}: OverlayFeedbackPanelProps) {
  const { t } = useTranslation()
  const [inputValue, setInputValue] = useState('')
  const [sending, setSending] = useState(false)
  const [isTimerPaused, setIsTimerPaused] = useState(false)
  const [progressRatio, setProgressRatio] = useState(1)
  const overlayRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const inputFocusedRef = useRef(false)
  const remainingRef = useRef(dwellMs)
  const startedAtRef = useRef(0)
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const progressIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const blurTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const shownUserMessage = userMessage || session.lastUserMessage
  const appLabel = getSessionAppLabel(session)
  const terminalLabel = getSessionTerminalLabel(session)
  const agentName = getAgentDisplayName(session)

  const updateProgress = useCallback(() => {
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
      onDismiss()
    }, Math.max(0, delayMs))
  }, [onDismiss])

  useEffect(() => {
    remainingRef.current = dwellMs
    setProgressRatio(1)
    scheduleDismiss(dwellMs)
    progressIntervalRef.current = setInterval(updateProgress, 100)
    setIsTimerPaused(false)
    inputFocusedRef.current = false

    return () => {
      if (timerRef.current) clearTimeout(timerRef.current)
      if (progressIntervalRef.current) clearInterval(progressIntervalRef.current)
      if (blurTimerRef.current) clearTimeout(blurTimerRef.current)
    }
  }, [dwellMs, scheduleDismiss, updateProgress])

  const pauseTimer = useCallback(() => {
    if (!timerRef.current) return
    clearTimeout(timerRef.current)
    timerRef.current = null
    remainingRef.current = Math.max(0, remainingRef.current - (Date.now() - startedAtRef.current))
    setProgressRatio(Math.max(0, remainingRef.current / dwellMs))
    setIsTimerPaused(true)
  }, [dwellMs])

  const resumeTimer = useCallback(() => {
    if (timerRef.current) return
    if (remainingRef.current <= 0) {
      onDismiss()
      return
    }
    setIsTimerPaused(false)
    scheduleDismiss(remainingRef.current)
  }, [onDismiss, scheduleDismiss])

  useEffect(() => {
    if (!isTimerPaused) return

    const interval = window.setInterval(() => {
      if (inputFocusedRef.current) return
      const isActuallyHovered = overlayRef.current?.matches(':hover') ?? false
      if (!isActuallyHovered) resumeTimer()
    }, 250)

    return () => window.clearInterval(interval)
  }, [isTimerPaused, resumeTimer])

  const focusInput = useCallback(() => {
    inputFocusedRef.current = true
    pauseTimer()
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
  }, [pauseTimer])

  const releaseInputFocus = useCallback(() => {
    if (blurTimerRef.current) clearTimeout(blurTimerRef.current)
    blurTimerRef.current = setTimeout(() => {
      blurTimerRef.current = null
      inputFocusedRef.current = false
      setNotchFocusable(false).catch(() => {})
      if (!(overlayRef.current?.matches(':hover') ?? false)) resumeTimer()
    }, 200)
  }, [resumeTimer])

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
      setTimeout(onDismiss, 500)
    } finally {
      setSending(false)
    }
  }, [inputValue, onDismiss, sending, session.id])

  const handleKeyDown = useCallback((event: KeyboardEvent<HTMLInputElement>) => {
    event.stopPropagation()
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault()
      void handleSend()
    } else if (event.key === 'Escape') {
      event.preventDefault()
      if (inputValue) setInputValue('')
      else onDismiss()
    }
  }, [handleSend, inputValue, onDismiss])

  const handleJump = useCallback(() => {
    onJumpToTerminal()
    onDismiss()
  }, [onDismiss, onJumpToTerminal])

  return (
    <div
      ref={overlayRef}
      className={`overlay-feedback${isTimerPaused ? ' overlay-feedback--paused' : ''}`}
      onMouseEnter={pauseTimer}
      onMouseLeave={() => {
        if (!inputFocusedRef.current) resumeTimer()
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
            <span className="overlay-feedback__badge">{agentName}</span>
            {terminalLabel && <span className="overlay-feedback__badge">{terminalLabel}</span>}
            <span className="overlay-feedback__duration">{formatDurationShort(session.duration)}</span>
            <button
              type="button"
              className="overlay-feedback__jump-icon"
              aria-label={t('notch.jumpToTerminal')}
              onMouseDown={(event) => {
                event.preventDefault()
                event.stopPropagation()
                handleJump()
              }}
            >
              ↗
            </button>
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
        <div className="overlay-feedback__scroll" style={maxHeight ? { maxHeight } : undefined}>
          <div className="overlay-feedback__transcript">
            <div className="overlay-feedback__transcript-head">
              <span className="overlay-feedback__status">
                <span className="overlay-feedback__status-dot" />
                {statusLabel}
              </span>
            </div>
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

      <div className="overlay-feedback__progress" aria-hidden>
        <div className="overlay-feedback__progress-bar" style={{ transform: `scaleX(${progressRatio})` }} />
      </div>
    </div>
  )
}
