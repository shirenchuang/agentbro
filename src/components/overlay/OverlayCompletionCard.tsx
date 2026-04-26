import { useCallback, useEffect, useRef, useState } from 'react'
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
  const dwellMs = dwellSeconds * 1000

  const [remaining, setRemaining] = useState(dwellMs)
  const paused = useRef(false)

  useEffect(() => {
    const interval = setInterval(() => {
      if (!paused.current) {
        setRemaining((r) => {
          if (r <= 100) {
            onDismiss()
            return 0
          }
          return r - 100
        })
      }
    }, 100)
    return () => clearInterval(interval)
  }, [dwellMs, onDismiss])

  const handleMouseEnter = useCallback(() => { paused.current = true }, [])
  const handleMouseLeave = useCallback(() => { paused.current = false }, [])

  const progress = remaining / dwellMs

  return (
    <div onMouseEnter={handleMouseEnter} onMouseLeave={handleMouseLeave}>
      <OverlayCard session={session} onDismiss={onDismiss}>
        <div className="overlay-completion__body">
          <svg className="overlay-completion__icon" width="16" height="16" viewBox="0 0 24 24" fill="none">
            <circle cx="12" cy="12" r="11" stroke="#30D158" strokeWidth="2" opacity="0.4" />
            <path d="M7 12.5l3 3 7-7" stroke="#30D158" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
          <span className="overlay-completion__summary">{data.summary}</span>
        </div>
        <div className="overlay-completion__progress">
          <div className="overlay-completion__progress-bar" style={{ width: `${progress * 100}%` }} />
        </div>
      </OverlayCard>
    </div>
  )
}
