import { useEffect, useRef, useState } from 'react'
import type { MouseEvent, ReactNode } from 'react'

interface InlineConfirmActionProps {
  label: string
  confirmLabel: string
  onConfirm: () => void | Promise<void>
  icon: ReactNode
  disabled?: boolean
  className?: string
}

export function InlineConfirmAction({
  label,
  confirmLabel,
  onConfirm,
  icon,
  disabled = false,
  className,
}: InlineConfirmActionProps) {
  const [phase, setPhase] = useState<'idle' | 'confirming' | 'loading'>('idle')
  const rootRef = useRef<HTMLDivElement>(null)
  const confirmRef = useRef<HTMLButtonElement>(null)

  useEffect(() => {
    if (phase !== 'confirming') return
    function handlePointerDown(e: PointerEvent) {
      if (!rootRef.current?.contains(e.target as Node)) {
        setPhase('idle')
      }
    }
    document.addEventListener('pointerdown', handlePointerDown)
    return () => document.removeEventListener('pointerdown', handlePointerDown)
  }, [phase])

  useEffect(() => {
    if (phase === 'confirming') confirmRef.current?.focus()
  }, [phase])

  function handleArm(e: MouseEvent) {
    e.stopPropagation()
    if (disabled || phase === 'loading') return
    setPhase('confirming')
  }

  async function handleConfirm(e: MouseEvent) {
    e.stopPropagation()
    if (disabled || phase === 'loading') return
    setPhase('loading')
    try {
      await onConfirm()
    } finally {
      setPhase('idle')
    }
  }

  const showConfirm = phase === 'confirming' || phase === 'loading'

  return (
    <div ref={rootRef} className={`inline-confirm ${className ?? ''}`}>
      {showConfirm ? (
        <button
          ref={confirmRef}
          type="button"
          className="inline-confirm__btn inline-confirm__btn--danger"
          onClick={handleConfirm}
          disabled={disabled || phase === 'loading'}
        >
          {phase === 'loading' && <span className="skills-spinner" style={{ width: 12, height: 12, borderWidth: 1.5 }} />}
          {confirmLabel}
        </button>
      ) : (
        <button
          type="button"
          className="inline-confirm__btn inline-confirm__btn--idle"
          onClick={handleArm}
          disabled={disabled}
          title={label}
          aria-label={label}
        >
          {icon}
        </button>
      )}
    </div>
  )
}
