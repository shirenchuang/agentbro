import type { CSSProperties, ReactNode } from 'react'
import type { SessionState } from '../../types/agent'
import { SessionContextHeader } from './SessionContextHeader'
import './OverlayCard.css'

interface OverlayCardProps {
  session: SessionState
  children: ReactNode
  onDismiss?: () => void
  maxHeight?: number
  className?: string
  bodyClassName?: string
}

export function OverlayCard({ session, children, onDismiss, maxHeight, className, bodyClassName }: OverlayCardProps) {
  const cardClassName = ['overlay-card', className].filter(Boolean).join(' ')
  const bodyClass = ['overlay-card__body', bodyClassName].filter(Boolean).join(' ')

  return (
    <div className={cardClassName} style={maxHeight ? ({ maxHeight } as CSSProperties) : undefined}>
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
      <div className={bodyClass}>
        {children}
      </div>
    </div>
  )
}
