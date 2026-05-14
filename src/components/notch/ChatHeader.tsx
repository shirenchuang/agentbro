/* ChatHeader — Back button + session info + badges */
import { useTranslation } from 'react-i18next'
import type { SessionState } from '../../types/agent'
import { AgentIcon } from './AgentIcon'
import { StatusDot } from '../shared'
import { formatDurationShort } from '../../utils/time'
import './ChatHeader.css'

interface ChatHeaderProps {
  session: SessionState
  onBack: () => void
  onJump?: () => void
}

function getAgentName(session: SessionState): string {
  if (session.agentType === 'claude-code' && session.engineLabel && session.engineLabel !== 'Claude Code') {
    return session.engineLabel
  }
  return session.agentType === 'claude-code' ? 'Claude' : session.agentType
}

function getSessionTitle(session: SessionState): string {
  const title = (session.sessionTitle || '').trim()
  const project = (session.project || '').trim()
  if (title && project && title !== project && !title.startsWith(`${project} ·`)) {
    return `${project} · ${title}`
  }
  return title || project || 'Session'
}

function isEvolabSession(session: SessionState): boolean {
  const cwd = session.cwd || ''
  return (
    cwd.includes('.evolab-desktop')
    || cwd.endsWith('/evolab')
    || cwd.includes('/evolab/')
    || session.project === 'free-chat'
  )
}

export function ChatHeader({ session, onBack, onJump }: ChatHeaderProps) {
  const { t } = useTranslation()
  const isAntCC = getAgentName(session).toLowerCase() === 'antcc'

  return (
    <div className="chat-header">
      <button className="chat-header__back" onClick={onBack} aria-label={t('notch.back')}>
        <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
          <path d="M10 2L4 8l6 6" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
        </svg>
      </button>

      <div className="chat-header__info">
        <StatusDot phase={session.phase} size={6} />
        <span className="chat-header__project">{getSessionTitle(session)}</span>
      </div>

      <div className="chat-header__badges">
        {isEvolabSession(session) && (
          <span className="chat-header__badge chat-header__badge--source">Evolab</span>
        )}
        <span className={`chat-header__badge chat-header__badge--agent${isAntCC ? ' chat-header__badge--antcc' : ''}`}>
          <AgentIcon agentType={session.agentType} size={12} />
          {getAgentName(session)}
        </span>
        {session.terminal && <span className="chat-header__badge">{session.terminal}</span>}
        <span className="chat-header__badge chat-header__badge--time">{formatDurationShort(session.duration)}</span>
        {onJump && (
          <button className="chat-header__jump" onClick={onJump} aria-label={t('notch.jumpToTerminal')}>
            <svg width="10" height="10" viewBox="0 0 16 16" fill="none">
              <path d="M4 12L12 4M12 4H6M12 4v6" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
            </svg>
          </button>
        )}
      </div>
    </div>
  )
}
