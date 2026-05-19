import { useCallback, type MouseEvent } from 'react'
import { useTranslation } from 'react-i18next'
import type { OverlayItem, SessionState } from '../../types/agent'
import { getAgentDisplayName, getSessionAppLabel, getSessionTerminalLabel, getSessionTitle } from '../../utils/sessionDisplay'
import { formatDurationShort } from '../../utils/time'
import { MascotRouter } from '../notch/mascots/MascotRouter'
import './OverlayCompactingCard.css'

interface OverlayCompactingCardProps {
  overlay: OverlayItem
  session: SessionState
  onJumpToTerminal: () => void
  onDismiss: () => void
  onShowSessions?: () => void
  sessionCount?: number
}

function shouldIgnorePanelJump(target: EventTarget | null): boolean {
  return target instanceof Element && Boolean(
    target.closest('button, input, textarea, select, a, [role="button"]'),
  )
}

export function OverlayCompactingCard({ session, onJumpToTerminal, onDismiss, onShowSessions, sessionCount }: OverlayCompactingCardProps) {
  const { t } = useTranslation()
  const appLabel = getSessionAppLabel(session)
  const terminalLabel = getSessionTerminalLabel(session)
  const agentName = getAgentDisplayName(session)
  const handleJump = useCallback(() => {
    onJumpToTerminal()
    onDismiss()
  }, [onDismiss, onJumpToTerminal])
  const handlePanelMouseDown = useCallback((event: MouseEvent<HTMLDivElement>) => {
    if (event.button !== 0 || shouldIgnorePanelJump(event.target)) return
    event.preventDefault()
    handleJump()
  }, [handleJump])

  return (
    <div className="overlay-compacting" data-no-drag onMouseDown={handlePanelMouseDown}>
      <div className="overlay-compacting__session">
        <div className="overlay-compacting__avatar">
          <MascotRouter toolType={session.agentType} phase="compacting" size={28} />
        </div>
        <div className="overlay-compacting__copy">
          <div className="overlay-compacting__row">
            <span className="overlay-compacting__title">{getSessionTitle(session)}</span>
            {appLabel && <span className="overlay-compacting__badge overlay-compacting__badge--source">{appLabel}</span>}
            <span className="overlay-compacting__badge">{agentName}</span>
            {terminalLabel && <span className="overlay-compacting__badge">{terminalLabel}</span>}
            <span className="overlay-compacting__duration">{formatDurationShort(session.duration)}</span>
            <button
              type="button"
              className="overlay-compacting__close"
              aria-label={t('notch.dismiss', { defaultValue: 'Dismiss' })}
              onMouseDown={(event) => {
                event.preventDefault()
                event.stopPropagation()
                onDismiss()
              }}
            >
              ×
            </button>
          </div>
          {session.lastUserMessage && (
            <div className="overlay-compacting__prompt">
              <span>{t('notch.you', '你')}：</span>
              <span>{session.lastUserMessage}</span>
            </div>
          )}
        </div>
      </div>

      <div className="overlay-compacting__body">
        <span className="overlay-compacting__icon" aria-hidden="true">▦</span>
        <span className="overlay-compacting__text">
          {t('notch.compacting', { defaultValue: 'Compacting context...' })}
        </span>
        <span className="overlay-compacting__pulse" aria-hidden="true" />
      </div>

      <div className="overlay-card__secondary" data-no-drag>
        {onShowSessions && sessionCount != null ? (
          <button
            type="button"
            className="overlay-card__show-sessions"
            onMouseDown={(event) => {
              event.preventDefault()
              event.stopPropagation()
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
