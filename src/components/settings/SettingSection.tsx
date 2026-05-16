import type { ReactNode } from 'react'

interface SettingSectionProps {
  title: string
  description?: string
  children: ReactNode
  className?: string
}

export function SettingSection({ title, description, children, className }: SettingSectionProps) {
  return (
    <div className={`setting-section${className ? ` ${className}` : ''}`}>
      <div className="setting-section__header">
        <h2 className="setting-section__title">{title}</h2>
        {description && <p className="setting-section__description">{description}</p>}
      </div>
      <div className="setting-section__body">
        {children}
      </div>
    </div>
  )
}
