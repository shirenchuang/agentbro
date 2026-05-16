import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useThemeStore } from '../stores/themeStore'
import type { ThemeConfig } from '../types/theme'

const petTheme: ThemeConfig = {
  name: 'codex-pet:test',
  version: '1.0.0',
  author: 'user',
  provider: 'codex',
  isCodexPet: true,
  displayName: 'Test Pet',
  pixelGrid: { cols: 5, rows: 5 },
  priorityColors: {},
  prioritySpeeds: {},
  priorityPatterns: {},
  character: {
    spriteSheet: 'data:image/webp;base64,AAAA',
    frameSize: { width: 192, height: 208 },
    scale: 1,
    animations: { idle: { row: 0, frames: 1, fps: 1 } },
  },
  sounds: { pack: '8bit' },
}

describe('themeStore', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    localStorage.clear()
    useThemeStore.getState().loadThemes([])
    useThemeStore.getState().setActiveTheme('default')
    useThemeStore.getState().setColorTheme('midnight')
  })

  it('does not persist again when selecting the active role theme', () => {
    const setItem = vi.spyOn(Storage.prototype, 'setItem')

    useThemeStore.getState().setActiveTheme('default')

    expect(setItem).not.toHaveBeenCalled()
  })

  it('does not persist again when selecting the active color theme', () => {
    const setItem = vi.spyOn(Storage.prototype, 'setItem')

    useThemeStore.getState().setColorTheme('midnight')

    expect(setItem).not.toHaveBeenCalled()
  })

  it('deduplicates role themes loaded from repeated syncs', () => {
    useThemeStore.getState().loadThemes([petTheme, petTheme])

    const names = useThemeStore.getState().themes.map((theme) => theme.name)
    expect(names.filter((name) => name === petTheme.name)).toHaveLength(1)
  })
})
