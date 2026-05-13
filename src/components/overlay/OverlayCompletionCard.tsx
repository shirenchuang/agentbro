import { useCallback, useEffect, useRef } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import type { OverlayItem, SessionState } from '../../types/agent'
import { useConfigStore } from '../../stores/configStore'
import { OverlayCard } from './OverlayCard'
import './OverlayCompletionCard.css'

interface OverlayCompletionCardProps {
  overlay: OverlayItem
  session: SessionState
  onDismiss: () => void
}

export function OverlayCompletionCard({ overlay, session, onDismiss }: OverlayCompletionCardProps) {
  const data = overlay.data as { summary: string }
  const dwellSeconds = useConfigStore((s) => s.taskCompleteDwellSeconds) || 3
  const completionCardHeight = useConfigStore((s) => s.completionCardHeight)
  const dwellMs = dwellSeconds * 1000

  const remainingRef = useRef(dwellMs)
  const startedAtRef = useRef(0)
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  useEffect(() => {
    remainingRef.current = dwellMs

    const startTimer = () => {
      startedAtRef.current = Date.now()
      timerRef.current = setTimeout(onDismiss, remainingRef.current)
    }

    startTimer()

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

  return (
    <div className="overlay-completion__timer" onMouseEnter={handleMouseEnter} onMouseLeave={handleMouseLeave}>
      <OverlayCard session={session} onDismiss={onDismiss} maxHeight={completionCardHeight}>
        <div className="overlay-completion__body">
          <svg className="overlay-completion__icon" width="16" height="16" viewBox="0 0 24 24" fill="none">
            <circle cx="12" cy="12" r="11" stroke="#30D158" strokeWidth="2" opacity="0.4" />
            <path d="M7 12.5l3 3 7-7" stroke="#30D158" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
          <div className="overlay-completion__summary">
            <ReactMarkdown remarkPlugins={[remarkGfm]}>
              {data.summary}
            </ReactMarkdown>
          </div>
        </div>
        <div className="overlay-completion__progress">
          <div className="overlay-completion__progress-bar" style={{ animationDuration: `${dwellMs}ms` }} />
        </div>
      </OverlayCard>
    </div>
  )
}
