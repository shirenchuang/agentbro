import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { ThemeConfig } from '../types/theme'

const DEFAULT_THEME: ThemeConfig = {
  name: 'default',
  version: '1.0.0',
  author: 'builtin',
  pixelGrid: { cols: 5, rows: 5 },
  priorityColors: {
    dormant: '#666666',
    idle: '#30D158',
    done: '#30D158',
    thinking: '#007AFF',
    working: '#FF9500',
    compacting: '#9C27B0',
    attention: '#FF3B30',
  },
  prioritySpeeds: {
    dormant: 0,
    idle: 2000,
    done: 1500,
    thinking: 800,
    working: 600,
    compacting: 500,
    attention: 300,
  },
  priorityPatterns: {
    dormant: { activePixels: [{ row: 2, col: 2 }], animation: 'pulse', fps: 1 },
    idle: { activePixels: [{ row: 1, col: 1 }, { row: 1, col: 3 }, { row: 3, col: 2 }], animation: 'breath', fps: 2 },
    done: { activePixels: [{ row: 1, col: 1 }, { row: 2, col: 2 }, { row: 3, col: 3 }], animation: 'wave', fps: 3 },
    thinking: { activePixels: [{ row: 0, col: 2 }, { row: 2, col: 0 }, { row: 2, col: 4 }, { row: 4, col: 2 }], animation: 'spin', fps: 4 },
    working: { activePixels: [{ row: 1, col: 1 }, { row: 1, col: 3 }, { row: 3, col: 1 }, { row: 3, col: 3 }], animation: 'wave', fps: 5 },
    compacting: { activePixels: [{ row: 0, col: 0 }, { row: 0, col: 4 }, { row: 4, col: 0 }, { row: 4, col: 4 }, { row: 2, col: 2 }], animation: 'spin', fps: 6 },
    attention: { activePixels: [{ row: 0, col: 2 }, { row: 1, col: 1 }, { row: 1, col: 3 }, { row: 2, col: 0 }, { row: 2, col: 4 }, { row: 3, col: 1 }, { row: 3, col: 3 }, { row: 4, col: 2 }], animation: 'blink', fps: 8 },
  },
  sounds: { pack: '8bit' },
}

interface ThemeStore {
  themes: ThemeConfig[]
  activeThemeName: string
  activeTheme: ThemeConfig
  setActiveTheme: (name: string) => void
  loadThemes: (themes: ThemeConfig[]) => void
}

export const useThemeStore = create<ThemeStore>()(
  persist(
    (set, get) => ({
      themes: [DEFAULT_THEME],
      activeThemeName: 'default',
      activeTheme: DEFAULT_THEME,

      setActiveTheme: (name) => {
        const theme = get().themes.find((t) => t.name === name)
        if (theme) {
          set({ activeThemeName: name, activeTheme: theme })
        }
      },

      loadThemes: (themes) => {
        const all = [DEFAULT_THEME, ...themes.filter((t) => t.name !== 'default')]
        const activeThemeName = get().activeThemeName
        const activeTheme = all.find((t) => t.name === activeThemeName) ?? DEFAULT_THEME
        set({ themes: all, activeTheme })
      },
    }),
    { name: 'agent-island-theme', partialize: (state) => ({ activeThemeName: state.activeThemeName }) }
  )
)
