import { useMemo } from 'react'
import type { AgentType } from '../../../types/agent'
import { MascotCanvas } from './MascotCanvas'
import type { MascotAnimState } from './MascotCanvas'
import type { SessionPhase } from '../../../types/agent'

interface MascotRouterProps {
  toolType: AgentType | string
  phase: SessionPhase
  size?: number
}

function phaseToAnimState(phase: SessionPhase): MascotAnimState {
  switch (phase) {
    case 'processing': return 'running'
    case 'waiting_approval':
    case 'waiting_input': return 'alert'
    case 'idle':
    case 'done': return 'idle'
    default: return 'processing'
  }
}

export function MascotRouter({ toolType, phase, size = 32 }: MascotRouterProps) {
  const animState = useMemo(() => phaseToAnimState(phase), [phase])
  return <MascotCanvas toolType={toolType} animState={animState} size={size} />
}
