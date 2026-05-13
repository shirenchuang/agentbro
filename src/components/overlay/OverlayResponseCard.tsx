import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from 'react'
import { useTranslation } from 'react-i18next'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import type { OverlayItem, SessionState } from '../../types/agent'
import { useConfigStore } from '../../stores/configStore'
import { sendMessage, setNotchFocusable } from '../../services/tauriApi'
import { OverlayCard } from './OverlayCard'
import './OverlayResponseCard.css'

interface OverlayResponseCardProps {
  overlay: OverlayItem
  session: SessionState
  onJumpToTerminal: () => void
  onDismiss: () => void
}

export function OverlayResponseCard({ overlay, session, onJumpToTerminal, onDismiss }: OverlayResponseCardProps) {
  const { t } = useTranslation()
  const data = overlay.data as { responseText: string; userMessage?: string }
  const dwellSeconds = useConfigStore((s) => s.taskCompleteDwellSeconds) || 5
  const completionCardHeight = useConfigStore((s) => s.completionCardHeight)
  const dwellMs = dwellSeconds * 1000
  const [inputValue, setInputValue] = useState('')
  const [sending, setSending] = useState(false)
  const inputRef = useRef<HTMLInputElement>(null)

  const remainingRef = useRef(dwellMs)
  const startedAtRef = useRef(0)
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  useEffect(() => {
    remainingRef.current = dwellMs
    startedAtRef.current = Date.now()
    timerRef.current = setTimeout(onDismiss, dwellMs)

    return () => {
      if (timerRef.current) clearTimeout(timerRef.current)
    }
  }, [dwellMs, onDismiss])

  const handleMouseEnter = useCallback(() => {
    if (timerRef.current) clearTimeout(timerRef.current)
    remainingRef.current = Math.max(0, remainingRef.current - (Date.now() - startedAtRef.current))
  }, [])
  const handleMouseLeave = useCallback(() => {
    startedAtRef.current = Date.now()
    timerRef.current = setTimeout(onDismiss, remainingRef.current)
  }, [onDismiss])

  const handleClick = () => {
    onJumpToTerminal()
    onDismiss()
  }

  const handleSend = useCallback(async () => {
    const text = inputValue.trim()
    if (!text || sending) return
    setSending(true)
    try {
      await sendMessage(session.id, text)
      setInputValue('')
      setTimeout(onDismiss, 500)
    } finally {
      setSending(false)
    }
  }, [inputValue, onDismiss, sending, session.id])

  const handleKeyDown = useCallback((event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault()
      void handleSend()
    } else if (event.key === 'Escape') {
      event.preventDefault()
      if (inputValue) {
        setInputValue('')
      } else {
        onDismiss()
      }
    }
  }, [handleSend, inputValue, onDismiss])

  return (
    <div className="overlay-response__timer" onMouseEnter={handleMouseEnter} onMouseLeave={handleMouseLeave}>
      <OverlayCard session={session} onDismiss={onDismiss} maxHeight={completionCardHeight}>
        <div className="overlay-response__content">
          {data.userMessage && (
            <div className="overlay-response__bubble overlay-response__bubble--user">
              <span className="overlay-response__bubble-label">{t('notch.you', { defaultValue: 'You' })}</span>
              <span className="overlay-response__bubble-text">{data.userMessage}</span>
            </div>
          )}
          <div className="overlay-response__bubble overlay-response__bubble--assistant" onClick={handleClick}>
            <div className="overlay-response__text overlay-response__markdown">
              <ReactMarkdown remarkPlugins={[remarkGfm]}>
                {data.responseText}
              </ReactMarkdown>
            </div>
            <span className="overlay-response__jump">
              {t('notch.jumpToTerminal', { defaultValue: 'Jump to terminal' })} &rarr;
            </span>
          </div>
        </div>
        <div className="overlay-response__reply" data-no-drag>
          <input
            ref={inputRef}
            className="overlay-response__input"
            value={inputValue}
            placeholder={t('notch.typeMessage', { defaultValue: 'Send a message... (Enter)' })}
            disabled={sending}
            onChange={(event) => setInputValue(event.target.value)}
            onKeyDown={handleKeyDown}
            onMouseDown={(event) => {
              event.stopPropagation()
              setNotchFocusable(true).then(() => inputRef.current?.focus()).catch(() => {})
            }}
            onClick={(event) => event.stopPropagation()}
            onFocus={() => setNotchFocusable(true).catch(() => {})}
            onBlur={() => setNotchFocusable(false).catch(() => {})}
          />
          <button
            className="overlay-response__send"
            disabled={!inputValue.trim() || sending}
            onMouseDown={(event) => {
              event.preventDefault()
              event.stopPropagation()
              void handleSend()
            }}
          >
            {sending ? '...' : t('notch.send', { defaultValue: 'Send' })}
          </button>
        </div>
        <div className="overlay-response__progress">
          <div className="overlay-response__progress-bar" style={{ animationDuration: `${dwellMs}ms` }} />
        </div>
      </OverlayCard>
    </div>
  )
}
