import type { ButtonHTMLAttributes, ReactNode } from 'react'

interface GlassButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'danger' | 'ghost'
  children: ReactNode
  shortcut?: string
}

export function GlassButton({ variant = 'secondary', children, shortcut, className = '', ...props }: GlassButtonProps) {
  return (
    <button className={`glass-btn glass-btn--${variant} ${className}`} {...props}>
      {children}
      {shortcut && <kbd style={{ opacity: 0.5, fontSize: 'var(--font-size-xs)' }}>{shortcut}</kbd>}
    </button>
  )
}
