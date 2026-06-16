import { createPortal } from 'react-dom'
import { AnimatePresence, motion } from 'framer-motion'
import type { ReactNode } from 'react'

interface SlideOverProps {
  open: boolean
  onClose: () => void
  title: ReactNode
  subtitle?: ReactNode
  actions?: ReactNode
  children: ReactNode
  className?: string
  /** drawer width in px (default 640) */
  width?: number
}

export function SlideOver({
  open,
  onClose,
  title,
  subtitle,
  actions,
  children,
  className,
  width = 640,
}: SlideOverProps) {
  if (typeof document === 'undefined') return null
  return createPortal(
    <AnimatePresence>
      {open && (
        <motion.div
          className="sm2__slideover-overlay"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.15 }}
          onClick={onClose}
        >
          <motion.div
            className={`sm2__slideover${className ? ` ${className}` : ''}`}
            style={{ width: `min(${width}px, 100vw)` }}
            initial={{ x: '100%' }}
            animate={{ x: 0 }}
            exit={{ x: '100%' }}
            transition={{ type: 'spring', stiffness: 320, damping: 34 }}
            onClick={(e) => e.stopPropagation()}
          >
            <div className="sm2__slideover-header">
              <div className="sm2__slideover-titles">
                <div className="sm2__slideover-title">{title}</div>
                {subtitle && <div className="sm2__slideover-subtitle">{subtitle}</div>}
              </div>
              <div className="sm2__slideover-actions">
                {actions}
                <button className="sm2__slideover-close" onClick={onClose} aria-label="关闭">
                  ✕
                </button>
              </div>
            </div>
            <div className="sm2__slideover-body settings-scroll">{children}</div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>,
    document.body,
  )
}
