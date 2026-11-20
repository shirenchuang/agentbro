import { useContext } from 'react'
import { motion } from 'framer-motion'
import { SettingRowLabelContext } from './settingRowLabel'

interface ToggleProps {
  checked: boolean
  onChange: (checked: boolean) => void
  disabled?: boolean
  label?: string
}

export function Toggle({ checked, onChange, disabled, label }: ToggleProps) {
  const rowLabel = useContext(SettingRowLabelContext)
  return (
    <button
      role="switch"
      aria-checked={checked}
      aria-labelledby={label ? undefined : rowLabel?.labelId}
      aria-describedby={rowLabel?.descriptionId}
      aria-label={label}
      className={`toggle ${checked ? 'toggle--on' : 'toggle--off'}`}
      onClick={() => !disabled && onChange(!checked)}
      disabled={disabled}
      style={{ opacity: disabled ? 0.4 : 1 }}
    >
      <motion.div
        className="toggle__knob"
        animate={{ x: checked ? 20 : 2 }}
        transition={{ type: 'spring', stiffness: 500, damping: 30 }}
      />
    </button>
  )
}
