/* ChatHeader — Back button + session info + badges */
import { useTranslation } from 'react-i18next'
import type { SessionState } from '../../types/agent'
import { AgentIcon } from './AgentIcon'
import { StatusDot } from '../shared'
import { formatDuration } from '../../utils/time'
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
        <span className="chat-header__project">{session.project}</span>
      </div>

      <div className="chat-header__badges">
        <span className={`chat-header__badge chat-header__badge--agent${isAntCC ? ' chat-header__badge--antcc' : ''}`}>
          <AgentIcon agentType={session.agentType} size={12} />
          {getAgentName(session)}
        </span>
        <span className="chat-header__badge">{session.terminal}</span>
        <span className="chat-header__badge chat-header__badge--time">{formatDuration(session.duration)}</span>
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
