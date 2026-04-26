import type { ReactNode } from 'react'

interface SettingGroupProps {
  label?: string
  children: ReactNode
}

export function SettingGroup({ label, children }: SettingGroupProps) {
  return (
    <div>
      {label && <div className="setting-group__label">{label}</div>}
      <div className="settings-card">
        <div className="setting-group">
          {children}
        </div>
      </div>
    </div>
  )
}
