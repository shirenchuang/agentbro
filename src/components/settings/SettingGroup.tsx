import type { ReactNode } from 'react'

interface SettingGroupProps {
  actions?: ReactNode
  label?: string
  children: ReactNode
}

export function SettingGroup({ actions, label, children }: SettingGroupProps) {
  return (
    <div>
      {(label || actions) && (
        <div className="setting-group__header">
          {label && <div className="setting-group__label">{label}</div>}
          {actions && <div className="setting-group__actions">{actions}</div>}
        </div>
      )}
      <div className="settings-card">
        <div className="setting-group">
          {children}
        </div>
      </div>
    </div>
  )
}
