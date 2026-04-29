import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { ThemeConfig } from '../types/theme'

export interface ColorThemeInfo {
  id: string
  label: string
  labelZh: string
  tag: string
  isDark: boolean
  bg: string
  card: string
  accent: string
}

export const COLOR_THEMES: ColorThemeInfo[] = [
  { id: 'classic', label: 'Classic', labelZh: '经典', tag: 'Default', isDark: true, bg: '#0a0a0a', card: '#111111', accent: '#3b82f6' },
  { id: 'forest', label: 'Forest', labelZh: '森林', tag: 'Nature', isDark: true, bg: '#1a2e1a', card: '#243524', accent: '#22c55e' },
  { id: 'neon-tokyo', label: 'Neon Tokyo', labelZh: '霓虹东京', tag: 'Cyber', isDark: true, bg: '#0a0a0f', card: '#12121a', accent: '#ff00ff' },
  { id: 'sunset', label: 'Sunset', labelZh: '落日', tag: 'Warm', isDark: true, bg: '#1f1020', card: '#2a1a2e', accent: '#fbbf24' },
  { id: 'retro-arcade', label: 'Retro Arcade', labelZh: '复古街机', tag: '8-bit', isDark: true, bg: '#0f0f23', card: '#1a1a30', accent: '#ff0040' },
  { id: 'high-contrast', label: 'High Contrast', labelZh: '高对比', tag: 'A11y', isDark: true, bg: '#000000', card: '#111111', accent: '#00ffff' },
  { id: 'sakura', label: 'Sakura', labelZh: '樱花', tag: 'Soft', isDark: true, bg: '#1a0f14', card: '#24131c', accent: '#ec4899' },
]

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

function applyColorTheme(id: string) {
  document.documentElement.setAttribute('data-island-color-theme', id)
}

interface ThemeStore {
  themes: ThemeConfig[]
  activeThemeName: string
  activeTheme: ThemeConfig
  colorTheme: string
  setActiveTheme: (name: string) => void
  setColorTheme: (id: string) => void
  loadThemes: (themes: ThemeConfig[]) => void
}

export const useThemeStore = create<ThemeStore>()(
  persist(
    (set, get) => ({
      themes: [DEFAULT_THEME],
      activeThemeName: 'default',
      activeTheme: DEFAULT_THEME,
      colorTheme: 'classic',

      setActiveTheme: (name) => {
        const theme = get().themes.find((t) => t.name === name)
        if (theme) {
          set({ activeThemeName: name, activeTheme: theme })
        }
      },

      setColorTheme: (id) => {
        if (COLOR_THEMES.some((t) => t.id === id)) {
          applyColorTheme(id)
          set({ colorTheme: id })
        }
      },

      loadThemes: (themes) => {
        const all = [DEFAULT_THEME, ...themes.filter((t) => t.name !== 'default')]
        const activeThemeName = get().activeThemeName
        const activeTheme = all.find((t) => t.name === activeThemeName) ?? DEFAULT_THEME
        set({ themes: all, activeTheme })
      },
    }),
    {
      name: 'agent-island-theme',
      partialize: (state) => ({ activeThemeName: state.activeThemeName, colorTheme: state.colorTheme }),
      onRehydrateStorage: () => {
        return (state) => {
          if (state?.colorTheme) {
            applyColorTheme(state.colorTheme)
          }
        }
      },
    }
  )
)
