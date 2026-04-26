import type { SessionPhase } from '../../types/agent'

const phaseToStatus: Record<SessionPhase, string> = {
  idle: 'idle',
  processing: 'active',
  waiting_approval: 'waiting',
  waiting_input: 'waiting',
  compacting: 'active',
  done: 'active',
  error: 'error',
  interrupted: 'error',
}

interface StatusDotProps {
  phase: SessionPhase
  size?: number
}

export function StatusDot({ phase, size = 8 }: StatusDotProps) {
  const status = phaseToStatus[phase]
  return (
    <span
      className={`status-dot status-dot--${status}`}
      style={{ width: size, height: size }}
    />
  )
}
