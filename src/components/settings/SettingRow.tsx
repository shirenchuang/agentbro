import type { ReactNode } from 'react'

interface SettingRowProps {
  label: string
  description?: string
  children: ReactNode
}

export function SettingRow({ label, description, children }: SettingRowProps) {
  return (
    <div className="setting-row">
      <div className="setting-row__info">
        <div className="setting-row__label">{label}</div>
        {description && <div className="setting-row__description">{description}</div>}
      </div>
      <div className="setting-row__control">{children}</div>
    </div>
  )
}
