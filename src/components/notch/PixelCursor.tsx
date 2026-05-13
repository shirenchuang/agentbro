import { motion } from 'framer-motion'
import './PixelCursor.css'

const TOTAL = 24
const TOP = 8
const RIGHT = 4
const BOTTOM = 8
const LEFT = 4

function getDotPosition(index: number): { x: string; y: string } {
  if (index < TOP) {
    return { x: `${((index + 1) / (TOP + 1)) * 100}%`, y: '0%' }
  } else if (index < TOP + RIGHT) {
    const i = index - TOP
    return { x: '100%', y: `${((i + 1) / (RIGHT + 1)) * 100}%` }
  } else if (index < TOP + RIGHT + BOTTOM) {
    const i = index - TOP - RIGHT
    return { x: `${((i + 1) / (BOTTOM + 1)) * 100}%`, y: '100%' }
  } else {
    const i = index - TOP - RIGHT - BOTTOM
    return { x: '0%', y: `${((i + 1) / (LEFT + 1)) * 100}%` }
  }
}

function getDotAnimation(priority: number, index: number) {
  if (priority < 2) {
    return { animate: { opacity: 0.06 }, transition: {} }
  }
  if (priority === 2) {
    return {
      animate: { opacity: [0.05, 0.4, 0.05] },
      transition: { duration: 2, repeat: Infinity, ease: 'easeInOut' as const },
    }
  }
  if (priority <= 4) {
    return {
      animate: { opacity: [0.05, 0.8, 0.05] },
      transition: { duration: 1.4, repeat: Infinity, ease: 'easeInOut' as const, delay: index * 0.06 },
    }
  }
  return {
    animate: { opacity: [0.1, 0.9, 0.1] },
    transition: { duration: 0.6, repeat: Infinity, ease: 'easeInOut' as const },
  }
}

interface PixelCursorProps {
  priority: number
  visible: boolean
}

export function PixelCursor({ priority, visible }: PixelCursorProps) {
  if (!visible) return null

  return (
    <div className="pixel-cursor" aria-hidden="true">
      {Array.from({ length: TOTAL }, (_, i) => {
        const pos = getDotPosition(i)
        const { animate, transition } = getDotAnimation(priority, i)
        return (
          <motion.div
            key={i}
            className="pixel-cursor__dot"
            style={{ left: pos.x, top: pos.y }}
            animate={animate}
            transition={transition}
          />
        )
      })}
    </div>
  )
}
