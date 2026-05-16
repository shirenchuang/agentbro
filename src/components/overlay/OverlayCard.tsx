import type { CSSProperties, ReactNode } from 'react'
import type { SessionState } from '../../types/agent'
import { SessionContextHeader } from './SessionContextHeader'
import './OverlayCard.css'

interface OverlayCardProps {
  session: SessionState
  children: ReactNode
  onDismiss?: () => void
  maxHeight?: number
}

export function OverlayCard({ session, children, onDismiss, maxHeight }: OverlayCardProps) {
  return (
    <div className="overlay-card" style={maxHeight ? ({ maxHeight } as CSSProperties) : undefined}>
      <div className="overlay-card__header">
        <SessionContextHeader session={session} />
        {onDismiss && (
          <button
            className="overlay-card__close"
            onClick={onDismiss}
            aria-label="Dismiss"
          >
            <svg width="10" height="10" viewBox="0 0 16 16" fill="none">
              <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
            </svg>
          </button>
        )}
      </div>
      <div className="overlay-card__body">
        {children}
      </div>
    </div>
  )
}
