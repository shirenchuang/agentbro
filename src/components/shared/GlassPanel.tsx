import { motion, type HTMLMotionProps } from 'framer-motion'
import type { ReactNode } from 'react'

interface GlassPanelProps extends HTMLMotionProps<'div'> {
  variant?: 'light' | 'medium' | 'heavy'
  children: ReactNode
  className?: string
}

export function GlassPanel({ variant = 'medium', children, className = '', ...props }: GlassPanelProps) {
  const variantClass = variant === 'medium' ? '' : `glass-panel--${variant}`
  return (
    <motion.div className={`glass-panel ${variantClass} ${className}`} {...props}>
      {children}
    </motion.div>
  )
}
