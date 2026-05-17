import { describe, expect, it } from 'vitest'
import {
  CUSTOM_NOTCH_HEIGHT_DEFAULT,
  getCollapsedIslandHeight,
  MATCH_MENU_BAR_HEIGHT,
  MATCH_NOTCH_HEIGHT,
} from '../utils/islandLayout'

describe('island layout sizing', () => {
  it('keeps preset collapsed heights at least as tall as the menu bar preset', () => {
    expect(MATCH_MENU_BAR_HEIGHT).toBeGreaterThanOrEqual(38)
    expect(MATCH_NOTCH_HEIGHT).toBeGreaterThanOrEqual(MATCH_MENU_BAR_HEIGHT)
  })

  it('resolves collapsed height modes consistently', () => {
    expect(getCollapsedIslandHeight('matchMenuBar', 52)).toBe(MATCH_MENU_BAR_HEIGHT)
    expect(getCollapsedIslandHeight('matchNotch', 52)).toBe(MATCH_NOTCH_HEIGHT)
    expect(getCollapsedIslandHeight('custom', 52)).toBe(52)
    expect(CUSTOM_NOTCH_HEIGHT_DEFAULT).toBe(MATCH_NOTCH_HEIGHT)
  })
})
