import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { ThemeConfig } from '../types/theme'
import inkAmberThemeConfig from '../themes/ink-amber/theme.json'

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
  { id: 'ink-amber', label: 'AgentBro Classic', labelZh: 'AgentBro 经典', tag: 'Classic', isDark: false, bg: '#fff7ec', card: '#fffdf8', accent: '#f8a400' },
  { id: 'midnight', label: 'Midnight', labelZh: '午夜', tag: 'Default', isDark: true, bg: '#000000', card: '#0a0a0a', accent: '#7b78ff' },
  { id: 'frosted-glass', label: 'Frosted Glass', labelZh: '磨砂玻璃', tag: 'Light', isDark: false, bg: '#eef0f4', card: '#f6f7f9', accent: '#352eff' },
  { id: 'apple', label: 'Apple', labelZh: '苹果', tag: 'Clean', isDark: false, bg: '#f5f5f7', card: '#ffffff', accent: '#007aff' },
  { id: 'smoke', label: 'Smoke', labelZh: '烟灰', tag: 'Neutral', isDark: false, bg: '#e8e8ec', card: '#f4f4f6', accent: '#64748b' },
  { id: 'ocean-mist', label: 'Ocean Mist', labelZh: '海雾', tag: 'Cool', isDark: false, bg: '#e8eef5', card: '#f2f6fb', accent: '#0284c7' },
  { id: 'warm-paper', label: 'Warm Paper', labelZh: '暖纸', tag: 'Warm', isDark: false, bg: '#f2efe8', card: '#faf7f0', accent: '#d97706' },
  { id: 'soft-lavender', label: 'Soft Lavender', labelZh: '柔薰衣草', tag: 'Soft', isDark: false, bg: '#eeedf6', card: '#f8f7fc', accent: '#6366f1' },
  { id: 'system', label: 'System', labelZh: '跟随系统', tag: 'Auto', isDark: false, bg: 'linear-gradient(90deg, #0b0c0f 0 50%, #f5f5f7 50% 100%)', card: 'linear-gradient(90deg, #15171c 0 50%, #ffffff 50% 100%)', accent: '#007aff' },
]

export function isDarkColorTheme(id: string) {
  return COLOR_THEMES.find((theme) => theme.id === id)?.isDark ?? false
}

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

const INK_AMBER_THEME = inkAmberThemeConfig as ThemeConfig
const BUILTIN_THEMES = [INK_AMBER_THEME, DEFAULT_THEME]
const DEFAULT_ROLE_THEME_NAME = INK_AMBER_THEME.name
const DEFAULT_COLOR_THEME_ID = 'midnight'

type PersistedThemeState = {
  activeThemeName?: string
  colorTheme?: string
}

function applyColorTheme(id: string) {
  document.documentElement.setAttribute('data-island-color-theme', id)
}

function mergeThemes(themes: ThemeConfig[]) {
  const seen = new Set(BUILTIN_THEMES.map((theme) => theme.name))
  const all = [...BUILTIN_THEMES]

  for (const theme of themes) {
    if (seen.has(theme.name)) continue
    seen.add(theme.name)
    all.push(theme)
  }

  return all
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
      themes: BUILTIN_THEMES,
      activeThemeName: DEFAULT_ROLE_THEME_NAME,
      activeTheme: INK_AMBER_THEME,
      colorTheme: DEFAULT_COLOR_THEME_ID,

      setActiveTheme: (name) => {
        const theme = get().themes.find((t) => t.name === name)
        if (theme) {
          const current = get()
          if (current.activeThemeName === name && current.activeTheme === theme) return
          set({ activeThemeName: name, activeTheme: theme })
        }
      },

      setColorTheme: (id) => {
        if (COLOR_THEMES.some((t) => t.id === id)) {
          applyColorTheme(id)
          if (get().colorTheme === id) return
          set({ colorTheme: id })
        }
      },

      loadThemes: (themes) => {
        const builtInThemeNames = new Set(BUILTIN_THEMES.map((theme) => theme.name))
        const all = mergeThemes(themes.filter((theme) => !builtInThemeNames.has(theme.name)))
        const activeThemeName = get().activeThemeName
        const activeTheme = all.find((t) => t.name === activeThemeName) ?? INK_AMBER_THEME
        set({ themes: all, activeTheme })
      },
    }),
    {
      name: 'agentbro-theme',
      version: 3,
      partialize: (state) => ({ activeThemeName: state.activeThemeName, colorTheme: state.colorTheme }),
      migrate: (persistedState, version) => {
        const state = persistedState as PersistedThemeState | undefined
        if (!state) return persistedState
        if (version < 2 && (!state.activeThemeName || state.activeThemeName === DEFAULT_THEME.name)) {
          return {
            ...state,
            activeThemeName: DEFAULT_ROLE_THEME_NAME,
            colorTheme: !state.colorTheme || state.colorTheme === 'ink-amber' ? DEFAULT_COLOR_THEME_ID : state.colorTheme,
          }
        }
        if (version < 3 && (!state.colorTheme || state.colorTheme === 'ink-amber')) {
          return { ...state, colorTheme: DEFAULT_COLOR_THEME_ID }
        }
        return persistedState
      },
      onRehydrateStorage: () => {
        return (state) => {
          if (state?.colorTheme && COLOR_THEMES.some((theme) => theme.id === state.colorTheme)) {
            applyColorTheme(state.colorTheme)
          } else if (state) {
            state.colorTheme = DEFAULT_COLOR_THEME_ID
            applyColorTheme(DEFAULT_COLOR_THEME_ID)
          }
          if (state && !state.activeThemeName) {
            state.activeThemeName = DEFAULT_ROLE_THEME_NAME
            state.activeTheme = INK_AMBER_THEME
          }
        }
      },
    }
  )
)
