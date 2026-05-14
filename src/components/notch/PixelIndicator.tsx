/* PixelIndicator — Priority-driven animated pixel art grid (theme-aware) */
import { useMemo } from 'react'
import type { Priority } from '../../types/priority'
import { PRIORITY, computePriority, priorityName } from '../../types/priority'
import type { SessionPhase } from '../../types/agent'
import { useThemeStore } from '../../stores/themeStore'
import './PixelIndicator.css'

interface PixelIndicatorProps {
  priority?: Priority
  phase?: SessionPhase
  size?: number
}

const FALLBACK_COLORS: Record<number, string[]> = {
  [PRIORITY.dormant]:   ['#666', '#777', '#555', '#666'],
  [PRIORITY.idle]:      ['#30D158', '#28a745', '#22c55e', '#4ade80'],
  [PRIORITY.done]:      ['#30D158', '#4ade80', '#86efac', '#22c55e'],
  [PRIORITY.thinking]:  ['#007AFF', '#2196F3', '#60a5fa', '#3b82f6'],
  [PRIORITY.working]:   ['#FF9500', '#30D158', '#007AFF', '#E8654A'],
  [PRIORITY.compacting]:['#9C27B0', '#AB47BC', '#CE93D8', '#7B1FA2'],
  [PRIORITY.attention]: ['#FF3B30', '#FF453A', '#FF9500', '#FFD60A'],
}

function phaseToFallbackPriority(phase: SessionPhase): Priority {
  return computePriority({ phase, startedAt: Date.now() })
}

export function PixelIndicator({ priority, phase, size = 14 }: PixelIndicatorProps) {
  const theme = useThemeStore((s) => s.activeTheme)
  const p = priority ?? (phase ? phaseToFallbackPriority(phase) : PRIORITY.idle)
  const pName = priorityName(p)

  const themeColor = theme.priorityColors[pName]
  const themeSpeed = theme.prioritySpeeds[pName]

  const isAnimated = p >= PRIORITY.thinking
  const isAttention = p === PRIORITY.attention

  const speed = themeSpeed ?? 1500

  const pixels = useMemo(() => {
    const colors = themeColor
      ? [themeColor, themeColor, themeColor, themeColor]
      : FALLBACK_COLORS[p] ?? FALLBACK_COLORS[PRIORITY.idle]
    return [0, 1, 2, 3].map(i => colors[i % colors.length])
  }, [p, themeColor])

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
      data-priority={pName}
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
