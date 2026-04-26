import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { OverlayItem, SessionState } from '../../types/agent'
import { useConfigStore } from '../../stores/configStore'
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
  const data = overlay.data as { responseText: string }
  const dwellSeconds = useConfigStore((s) => s.taskCompleteDwellSeconds) || 5
  const dwellMs = dwellSeconds * 1000

  const [remaining, setRemaining] = useState(dwellMs)
  const paused = useRef(false)

  useEffect(() => {
    const interval = setInterval(() => {
      if (!paused.current) {
        setRemaining((r) => Math.max(0, r - 100))
      }
    }, 100)
    return () => clearInterval(interval)
  }, [dwellMs])

  useEffect(() => {
    if (remaining <= 0) onDismiss()
  }, [remaining, onDismiss])

  const handleMouseEnter = useCallback(() => { paused.current = true }, [])
  const handleMouseLeave = useCallback(() => { paused.current = false }, [])

  const handleClick = () => {
    onJumpToTerminal()
    onDismiss()
  }

  const progress = remaining / dwellMs

  return (
    <div onMouseEnter={handleMouseEnter} onMouseLeave={handleMouseLeave}>
      <OverlayCard session={session} onDismiss={onDismiss}>
        <div className="overlay-response__content" onClick={handleClick}>
          <span className="overlay-response__text">{data.responseText}</span>
          <span className="overlay-response__jump">
            {t('notch.jumpToTerminal', { defaultValue: 'Jump to terminal' })} &rarr;
          </span>
        </div>
        <div className="overlay-response__progress">
          <div className="overlay-response__progress-bar" style={{ width: `${progress * 100}%` }} />
        </div>
      </OverlayCard>
    </div>
  )
}
