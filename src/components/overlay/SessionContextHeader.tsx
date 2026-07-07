import type { SessionState } from '../../types/agent'
import { computePriority } from '../../types/priority'
import { PixelIndicator } from '../notch/PixelIndicator'

interface SessionContextHeaderProps {
  session: SessionState
}

const AGENT_NAMES: Record<string, string> = {
  'claude-code': 'Claude',
  'gemini-cli': 'Gemini',
  'workbuddy': 'WorkBuddy',
}

function getAgentName(type: string): string {
  return AGENT_NAMES[type] || type.charAt(0).toUpperCase() + type.slice(1)
}

export function SessionContextHeader({ session }: SessionContextHeaderProps) {
  const title = session.sessionTitle?.trim()

  return (
    <div className="overlay-ctx">
      <div className="overlay-ctx__row1">
        <PixelIndicator priority={computePriority(session)} size={10} />
        <span className="overlay-ctx__project">{session.project}</span>
        {title && (
          <>
            <span className="overlay-ctx__sep">&middot;</span>
            <span className="overlay-ctx__title">{title}</span>
          </>
        )}
        <span className="overlay-ctx__sep">&middot;</span>
        <span className="overlay-ctx__agent">{getAgentName(session.agentType)}</span>
      </div>
      {session.lastUserMessage && (
        <div className="overlay-ctx__row2">
          <span className="overlay-ctx__you">You:</span>
          <span className="overlay-ctx__msg">{session.lastUserMessage}</span>
        </div>
      )}
    </div>
  )
}
