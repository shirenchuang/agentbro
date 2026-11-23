import { createContext } from 'react'

export const SettingRowLabelContext = createContext<{
  labelId: string
  descriptionId?: string
} | null>(null)
