import type { CSSProperties, MouseEvent, ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import type { SessionState } from '../../types/agent'
import { SessionContextHeader } from './SessionContextHeader'
import './OverlayCard.css'

interface OverlayCardProps {
  session: SessionState
  children: ReactNode
  onDismiss?: () => void
  onShowSessions?: () => void
  sessionCount?: number
  maxHeight?: number
  className?: string
  bodyClassName?: string
  onCardClick?: () => void
}

function isInteractiveTarget(target: EventTarget | null): boolean {
  return target instanceof Element && Boolean(
    target.closest('button, a, input, select, textarea, [role="button"], [data-no-jump]'),
  )
}

export function OverlayCard({ session, children, onDismiss, onShowSessions, sessionCount, maxHeight, className, bodyClassName, onCardClick }: OverlayCardProps) {
  const { t } = useTranslation()
  const cardClassName = ['overlay-card', onCardClick ? 'overlay-card--clickable' : undefined, className].filter(Boolean).join(' ')
  const bodyClass = ['overlay-card__body', bodyClassName].filter(Boolean).join(' ')
  const handleCardClick = (event: MouseEvent<HTMLDivElement>) => {
    if (!onCardClick || isInteractiveTarget(event.target)) return
    onCardClick()
  }

  return (
    <div className={cardClassName} style={maxHeight ? ({ maxHeight } as CSSProperties) : undefined} onClick={handleCardClick}>
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
      <div className="overlay-card__secondary">
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
    </div>
  )
}
