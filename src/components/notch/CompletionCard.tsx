/* CompletionCard — Brief notification when a session completes */
import { useEffect, useState } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import { useConfigStore } from '../../stores/configStore'
import type { SessionState } from '../../types/agent'

interface CompletionCardProps {
  session: SessionState
  onDismiss: (sessionId: string) => void
}

const CHECKMARK_ICON = (
  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" aria-hidden>
    <circle cx="12" cy="12" r="11" stroke="var(--green)" strokeWidth="2" opacity="0.3" />
    <path d="M7 12.5l3 3 7-7" stroke="var(--green)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
)

function parseCompletionTimeout(raw: string): number {
  if (raw === 'persistent') return 0
  const match = raw.match(/^(\d+)s$/)
  return match ? parseInt(match[1], 10) * 1000 : 5000
}

export function CompletionCard({ session, onDismiss }: CompletionCardProps) {
  const completionPopupDuration = useConfigStore((s) => s.completionPopupDuration)
  const completionCardHeight = useConfigStore((s) => s.completionCardHeight)
  const [visible, setVisible] = useState(true)

  useEffect(() => {
    const timeout = parseCompletionTimeout(completionPopupDuration)
    if (timeout <= 0) return // persistent — no auto-dismiss

    const timer = setTimeout(() => {
      setVisible(false)
    }, timeout)

    return () => clearTimeout(timer)
  }, [completionPopupDuration])

  return (
    <AnimatePresence onExitComplete={() => onDismiss(session.id)}>
      {visible && (
        <motion.div
          className="completion-card glass-card"
          style={{ maxHeight: completionCardHeight }}
          initial={{ opacity: 0, y: -8, scale: 0.96 }}
          animate={{ opacity: 1, y: 0, scale: 1 }}
          exit={{ opacity: 0, y: -8, scale: 0.96 }}
          transition={{ type: 'spring', stiffness: 500, damping: 35 }}
        >
          <span className="completion-card__icon">{CHECKMARK_ICON}</span>
          <div className="completion-card__body">
            <span className="completion-card__project">{session.project}</span>
            <span className="completion-card__summary">
              {session.description?.split('\n')[0] || 'Task complete'}
            </span>
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  )
}
