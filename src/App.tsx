/* AgentBro — Main App */
import { useEffect, useRef, useState } from 'react'
import { ClaudeHookUiLab } from './components/dev/ClaudeHookUiLab'
import { NotchPanel } from './components/notch/NotchPanel'
import { SettingsApp } from './components/settings'
import { COLOR_THEMES, useThemeStore } from './stores/themeStore'
import { useConfigStore } from './stores/configStore'
import { useTauriInit } from './hooks/useTauri'
import { useAutoHide } from './hooks/useAutoHide'
import { getActiveThemeBundle, isTauri } from './services/tauriApi'
import './styles/globals.css'

function applyPersistedConfig(raw: string | null) {
  if (!raw) return
  try {
    const persisted = JSON.parse(raw) as { state?: Partial<ReturnType<typeof useConfigStore.getState>> }
    if (persisted.state) {
      useConfigStore.setState({ ...persisted.state, followFocus: false })
    }
  } catch {
    // Ignore malformed persisted config payloads.
  }
}

// ── Detect which Tauri window we're in ────────────────────────

async function detectWindowLabel(): Promise<string> {
  // Check URL hash first (works in both Tauri and browser)
  if (window.location.hash === '#settings') return 'settings'

  // In Tauri, use the real window label
  if (isTauri()) {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      return getCurrentWindow().label
    } catch {
      // fallback
    }
  }

  // Browser dev mode defaults to notch
  return 'notch'
}

// ── App ────────────────────────────────────────────────────────

function App() {
  const [windowLabel, setWindowLabel] = useState<string | null>(null)

  useTauriInit()
  useAutoHide()

  // Apply color theme to DOM
  const colorTheme = useThemeStore((s) => s.colorTheme)
  useEffect(() => {
    const normalizedTheme = COLOR_THEMES.some((theme) => theme.id === colorTheme) ? colorTheme : 'midnight'
    document.documentElement.setAttribute('data-island-color-theme', normalizedTheme)
  }, [colorTheme])

  useEffect(() => {
    const applyPersistedTheme = (raw: string | null) => {
      if (!raw) return
      try {
        const persisted = JSON.parse(raw) as { state?: { activeThemeName?: string; colorTheme?: string } }
        const nextTheme = persisted.state?.colorTheme
        if (nextTheme && COLOR_THEMES.some((theme) => theme.id === nextTheme)) {
          document.documentElement.setAttribute('data-island-color-theme', nextTheme)
          if (useThemeStore.getState().colorTheme !== nextTheme) {
            useThemeStore.setState({ colorTheme: nextTheme })
          }
        }
        const activeThemeName = persisted.state?.activeThemeName
        if (activeThemeName) {
          const store = useThemeStore.getState()
          if (store.activeThemeName === activeThemeName && store.activeTheme.name === activeThemeName) return
          const theme = store.themes.find((candidate) => candidate.name === activeThemeName)
          if (theme) {
            store.setActiveTheme(activeThemeName)
          } else if (isTauri()) {
            getActiveThemeBundle(activeThemeName)
              .then((bundle) => {
                const latest = useThemeStore.getState()
                latest.loadThemes([...latest.themes, bundle])
                latest.setActiveTheme(activeThemeName)
              })
              .catch(() => {})
          }
        }
      } catch {
        // Ignore malformed persisted theme payloads.
      }
    }

    const handleStorage = (event: StorageEvent) => {
      if (event.key === 'agentbro-theme') applyPersistedTheme(event.newValue)
    }
    const handleFocus = () => applyPersistedTheme(window.localStorage.getItem('agentbro-theme'))

    handleFocus()
    window.addEventListener('storage', handleStorage)
    window.addEventListener('focus', handleFocus)
    return () => {
      window.removeEventListener('storage', handleStorage)
      window.removeEventListener('focus', handleFocus)
    }
  }, [])

  useEffect(() => {
    const handleStorage = (event: StorageEvent) => {
      if (event.key === 'agentbro-config') applyPersistedConfig(event.newValue)
    }
    const handleFocus = () => applyPersistedConfig(window.localStorage.getItem('agentbro-config'))

    handleFocus()
    window.addEventListener('storage', handleStorage)
    window.addEventListener('focus', handleFocus)
    return () => {
      window.removeEventListener('storage', handleStorage)
      window.removeEventListener('focus', handleFocus)
    }
  }, [])

  // Detect window on mount
  useEffect(() => {
    detectWindowLabel().then(setWindowLabel)
  }, [])

  // Boot sound on startup
  const bootSoundPlayed = useRef(false)
  const soundEnabled = useConfigStore((s) => s.soundEnabled)
  const soundEvents = useConfigStore((s) => s.soundEvents)
  useEffect(() => {
    if (bootSoundPlayed.current) return
    if (!isTauri() || windowLabel !== 'notch') return
    const bootEvent = soundEvents.find((e) => e.id === 'boot')
    if (soundEnabled && bootEvent?.enabled) {
      bootSoundPlayed.current = true
      import('@tauri-apps/api/core').then(({ invoke }) => {
        invoke('play_sound', { event: 'boot' }).catch(() => {})
      }).catch(() => {})
    }
  }, [windowLabel, soundEnabled, soundEvents])

  // Browser dev mode: Cmd+, toggles settings view in same page
  useEffect(() => {
    if (isTauri()) return
    const handler = (e: KeyboardEvent) => {
      if (e.metaKey && e.key === ',') {
        e.preventDefault()
        setWindowLabel((v) => (v === 'settings' ? 'notch' : 'settings'))
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [])

  // Wait for detection
  if (windowLabel === null) return null

  // Settings window
  if (windowLabel === 'settings') {
    const handleClose = async () => {
      if (isTauri()) {
        try {
          const { getCurrentWindow } = await import('@tauri-apps/api/window')
          const { invoke } = await import('@tauri-apps/api/core')
          await invoke('set_dock_visible', { visible: false })
          getCurrentWindow().hide()
        } catch {
          // fallback: do nothing
        }
      } else {
        // Browser dev mode: switch back to notch
        setWindowLabel('notch')
      }
    }

    return (
      <div style={{ width: '100vw', height: '100vh', background: '#f2f2f7' }}>
        <SettingsApp onClose={handleClose} />
      </div>
    )
  }

  const notchWindow = (
    <div style={{
      width: '100vw',
      height: '100vh',
      background: 'transparent',
      position: 'relative',
    }}>
      <NotchPanel />
    </div>
  )

  if (!isTauri() && windowLabel === 'notch') {
    return <ClaudeHookUiLab>{notchWindow}</ClaudeHookUiLab>
  }

  // Notch window (default)
  return notchWindow
}

export default App
