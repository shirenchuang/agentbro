import { motion, type HTMLMotionProps } from 'framer-motion'
import type { ReactNode } from 'react'

interface GlassCardProps extends HTMLMotionProps<'div'> {
  hoverable?: boolean
  children: ReactNode
  className?: string
}

export function GlassCard({ hoverable = true, children, className = '', ...props }: GlassCardProps) {
  return (
    <motion.div
      className={`glass-card ${className}`}
      whileHover={hoverable ? { scale: 1.01, y: -1 } : undefined}
      transition={{ duration: 0.15 }}
      {...props}
    >
      {children}
    </motion.div>
  )
}
