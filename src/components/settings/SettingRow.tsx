import { useId, type ReactNode } from 'react'
import { SettingRowLabelContext } from './settingRowLabel'

interface SettingRowProps {
  label: string
  description?: string
  children: ReactNode
}

export function SettingRow({ label, description, children }: SettingRowProps) {
  const id = useId()
  const labelId = `${id}-label`
  const descriptionId = description ? `${id}-description` : undefined

  return (
    <div className="setting-row">
      <div className="setting-row__info">
        <div className="setting-row__label" id={labelId}>{label}</div>
        {description && <div className="setting-row__description" id={descriptionId}>{description}</div>}
      </div>
      <div className="setting-row__control">
        <SettingRowLabelContext value={{ labelId, descriptionId }}>{children}</SettingRowLabelContext>
      </div>
    </div>
  )
}
