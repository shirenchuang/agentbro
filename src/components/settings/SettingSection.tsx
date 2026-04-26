import type { ReactNode } from 'react'

interface SettingSectionProps {
  title: string
  description?: string
  children: ReactNode
}

export function SettingSection({ title, description, children }: SettingSectionProps) {
  return (
    <div className="setting-section">
      <div className="setting-section__header">
        <h2 className="setting-section__title">{title}</h2>
        {description && <p className="setting-section__description">{description}</p>}
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-lg)' }}>
        {children}
      </div>
    </div>
  )
}
