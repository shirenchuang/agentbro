export const MATCH_MENU_BAR_HEIGHT = 38
export const MATCH_NOTCH_HEIGHT = 40
export const CUSTOM_NOTCH_HEIGHT_DEFAULT = MATCH_NOTCH_HEIGHT
export const CUSTOM_NOTCH_HEIGHT_MIN = 32
export const CUSTOM_NOTCH_HEIGHT_MAX = 72

export type NotchHeightMode = 'matchNotch' | 'matchMenuBar' | 'custom'

export function getCollapsedIslandHeight(mode: NotchHeightMode, customHeight: number): number {
  if (mode === 'custom') return customHeight
  if (mode === 'matchMenuBar') return MATCH_MENU_BAR_HEIGHT
  return MATCH_NOTCH_HEIGHT
}
