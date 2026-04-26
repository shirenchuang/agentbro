/* PixelIndicator — Priority-driven animated pixel art grid */
import { useMemo } from 'react'
import type { Priority } from '../../types/priority'
import { PRIORITY, computePriority, priorityName } from '../../types/priority'
import type { SessionPhase } from '../../types/agent'
import './PixelIndicator.css'

interface PixelIndicatorProps {
  priority?: Priority
  phase?: SessionPhase
  size?: number
}

const PRIORITY_COLORS: Record<number, string[]> = {
  [PRIORITY.dormant]:   ['#666', '#777', '#555', '#666'],
  [PRIORITY.idle]:      ['#30D158', '#28a745', '#22c55e', '#4ade80'],
  [PRIORITY.done]:      ['#30D158', '#4ade80', '#86efac', '#22c55e'],
  [PRIORITY.thinking]:  ['#007AFF', '#2196F3', '#60a5fa', '#3b82f6'],
  [PRIORITY.working]:   ['#FF9500', '#30D158', '#007AFF', '#E8654A'],
  [PRIORITY.compacting]:['#9C27B0', '#AB47BC', '#CE93D8', '#7B1FA2'],
  [PRIORITY.attention]: ['#FF3B30', '#FF453A', '#FF9500', '#FFD60A'],
}

const PRIORITY_SPEED: Record<number, number> = {
  [PRIORITY.dormant]: 0,
  [PRIORITY.idle]: 2000,
  [PRIORITY.done]: 1500,
  [PRIORITY.thinking]: 800,
  [PRIORITY.working]: 600,
  [PRIORITY.compacting]: 500,
  [PRIORITY.attention]: 300,
}

function phaseToFallbackPriority(phase: SessionPhase): Priority {
  return computePriority({ phase, startedAt: Date.now() })
}

export function PixelIndicator({ priority, phase, size = 14 }: PixelIndicatorProps) {
  const p = priority ?? (phase ? phaseToFallbackPriority(phase) : PRIORITY.idle)
  const isAnimated = p >= PRIORITY.thinking
  const isAttention = p === PRIORITY.attention

  const colors = PRIORITY_COLORS[p] ?? PRIORITY_COLORS[PRIORITY.idle]
  const speed = PRIORITY_SPEED[p] ?? 1500

  const pixels = useMemo(() => {
    return [0, 1, 2, 3].map(i => colors[i % colors.length])
  }, [colors])

  const pixelSize = Math.floor(size / 2)
  const gap = 1

  const className = [
    'pixel-indicator',
    isAnimated ? 'pixel-indicator--active' : '',
    isAttention ? 'pixel-indicator--error' : '',
    !isAnimated && !isAttention ? 'pixel-indicator--idle' : '',
  ].filter(Boolean).join(' ')

  return (
    <span
      className={className}
      style={{ width: size, height: size }}
      aria-hidden
      data-priority={priorityName(p)}
    >
      <span className="pixel-indicator__grid" style={{ gap }}>
        {pixels.map((color, i) => (
          <span
            key={i}
            className="pixel-indicator__pixel"
            style={{
              width: pixelSize,
              height: pixelSize,
              background: color,
              animationDelay: isAnimated ? `${i * (speed / 4000)}s` : undefined,
              animationDuration: isAnimated ? `${speed}ms` : undefined,
            }}
          />
        ))}
      </span>
    </span>
  )
}
