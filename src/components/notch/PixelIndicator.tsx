/* PixelIndicator — Animated multi-color pixel art grid indicator */
import { useMemo } from 'react'
import type { SessionPhase } from '../../types/agent'
import './PixelIndicator.css'

interface PixelIndicatorProps {
  phase: SessionPhase
  size?: number
}

// Color palettes per state
const ACTIVE_COLORS = ['#FF9500', '#30D158', '#007AFF', '#FF3B30', '#E8654A', '#FFD60A']
const IDLE_COLORS = ['#30D158', '#28a745', '#22c55e', '#4ade80']
const ERROR_COLORS = ['#FF3B30', '#FF453A', '#EF4444', '#DC2626']
const DONE_COLORS = ['#30D158', '#4ade80', '#86efac', '#22c55e']

function getColors(phase: SessionPhase): string[] {
  switch (phase) {
    case 'processing':
    case 'waiting_approval':
    case 'waiting_input':
    case 'compacting':
      return ACTIVE_COLORS
    case 'error':
      return ERROR_COLORS
    case 'done':
      return DONE_COLORS
    default:
      return IDLE_COLORS
  }
}

export function PixelIndicator({ phase, size = 14 }: PixelIndicatorProps) {
  const isActive = phase === 'waiting_approval' || phase === 'waiting_input' || phase === 'processing' || phase === 'compacting'
  const isError = phase === 'error'

  const colors = getColors(phase)

  // Generate a 2x2 grid of pixel colors
  const pixels = useMemo(() => {
    return [0, 1, 2, 3].map(i => colors[i % colors.length])
  }, [colors])

  const pixelSize = Math.floor(size / 2)
  const gap = 1

  const className = [
    'pixel-indicator',
    isActive ? 'pixel-indicator--active' : '',
    isError ? 'pixel-indicator--error' : '',
    !isActive && !isError ? 'pixel-indicator--idle' : '',
  ].filter(Boolean).join(' ')

  return (
    <span
      className={className}
      style={{ width: size, height: size }}
      aria-hidden
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
              animationDelay: isActive ? `${i * 0.15}s` : undefined,
            }}
          />
        ))}
      </span>
    </span>
  )
}
