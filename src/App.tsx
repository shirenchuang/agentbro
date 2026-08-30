/* AgentBro — Main App */
import { lazy, Suspense, useEffect, useRef, useState } from 'react'
import { COLOR_THEMES, useThemeStore } from './stores/themeStore'
import { useConfigStore } from './stores/configStore'
import { usePetStore } from './stores/petStore'
import { BackgroundUpdater } from './components/BackgroundUpdater'
import { useTauriInit } from './hooks/useTauri'
import { useAutoHide } from './hooks/useAutoHide'
import { getActiveThemeBundle, isTauri } from './services/tauriApi'
import { primaryModifierPressed } from './utils/platform'
import './styles/globals.css'

const ClaudeHookUiLab = lazy(() => import('./components/dev/ClaudeHookUiLab').then((module) => ({ default: module.ClaudeHookUiLab })))
const NotchPanel = lazy(() => import('./components/notch/NotchPanel').then((module) => ({ default: module.NotchPanel })))
const SettingsApp = lazy(() => import('./components/settings/SettingsApp').then((module) => ({ default: module.SettingsApp })))

// Fields whose source of truth lives in the Rust backend and is broadcast via
// the `config-changed` event. We must NOT replay stale values from another
// window's `storage` snapshot, or a notch window writing localStorage during a
// `island-layout-preview` race can clobber the settings window's just-changed
// `islandSurfaceMode`, causing the surface mode toggle to flip-flop.
const BACKEND_MANAGED_CONFIG_KEYS = new Set<keyof ReturnType<typeof useConfigStore.getState>>([
  'soundEnabled', 'volume', 'launchAtLogin', 'autoHide', 'smartSuppression',
  'showUsageQuota', 'usageQueryEnabled', 'language', 'autoHideNoSessions', 'displayMonitor',
  'codexAppServerSyncEnabled', 'codexAppServerSyncIntervalSeconds',
  'globalShortcut',
  'shortcutApprove', 'shortcutApproveEnabled',
  'shortcutDeny', 'shortcutDenyEnabled',
  'shortcutSkip', 'shortcutSkipEnabled',
  'soundEvents', 'soundRules', 'customSounds', 'soundPack',
  'probeSessionFilter',
  'excludedHookCwdSubstrings', 'sessionSilenceRules',
  'tipsEnabled', 'pixelCursorEnabled', 'confettiEnabled',
  'analyticsEnabled', 'analyticsConsentPromptCompleted',
  'islandSurfaceMode', 'islandPetScale', 'islandPetWindowOrigin', 'islandPetWindowAnchor', 'islandActivePetId', 'islandAgentPetMap',
  'followFocus', 'quietHours', 'idleTimeoutMinutes',
  'idleInteractionRoutingEnabled', 'idleInteractionRoutingMinutes',
])

function applyPersistedConfig(raw: string | null) {
  if (!raw) return
  try {
    const persisted = JSON.parse(raw) as { state?: Partial<ReturnType<typeof useConfigStore.getState>> }
    if (!persisted.state) return
    const filtered: Partial<ReturnType<typeof useConfigStore.getState>> = {}
    for (const key of Object.keys(persisted.state) as Array<keyof ReturnType<typeof useConfigStore.getState>>) {
      if (BACKEND_MANAGED_CONFIG_KEYS.has(key)) continue
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      ;(filtered as any)[key] = (persisted.state as any)[key]
    }
    useConfigStore.setState(filtered)
  } catch {
    // Ignore malformed persisted config payloads.
  }
}

// ── Detect which Tauri window we're in ────────────────────────

async function detectWindowLabel(): Promise<string> {
  // Check URL hash first (works in both Tauri and browser)
  if (window.location.hash === '#settings') return 'settings'
  if (window.location.hash === '#pet') return 'pet'
  if (window.location.hash === '#skill-pack-picker') return 'skill-pack-picker'

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

  useTauriInit(windowLabel)
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
        const persisted = JSON.parse(raw) as { version?: number; state?: { activeThemeName?: string; colorTheme?: string } }
        const nextTheme = persisted.state?.colorTheme
        if (nextTheme && COLOR_THEMES.some((theme) => theme.id === nextTheme)) {
          document.documentElement.setAttribute('data-island-color-theme', nextTheme)
          if (useThemeStore.getState().colorTheme !== nextTheme) {
            useThemeStore.setState({ colorTheme: nextTheme })
          }
        }
        const activeThemeName = (persisted.version ?? 0) < 2 && persisted.state?.activeThemeName === 'default'
          ? 'ink-amber'
          : persisted.state?.activeThemeName
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

  useEffect(() => {
    function applyPersistedPet(raw: string | null) {
      if (!raw) return
      try {
        const persisted = JSON.parse(raw) as { state?: { activePetId?: string | null } }
        usePetStore.getState().hydrateFromConfig(persisted.state?.activePetId ?? null)
      } catch { /* ignore */ }
    }
    const handleStorage = (event: StorageEvent) => {
      if (event.key === 'agentbro-pet') applyPersistedPet(event.newValue)
    }
    const handleFocus = () => applyPersistedPet(window.localStorage.getItem('agentbro-pet'))

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

  // Browser dev mode: primary modifier + , toggles settings view in same page
  useEffect(() => {
    if (isTauri()) return
    const handler = (e: KeyboardEvent) => {
      if (primaryModifierPressed(e) && e.key === ',') {
        e.preventDefault()
        setWindowLabel((v) => (v === 'settings' ? 'notch' : 'settings'))
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [])

  // Wait for detection
  if (windowLabel === null) return null

  // Product Slimming keeps the underlying windows available for later cleanup
  // but exposes no Pet or Skill Pack UI in the Control Tower shell.
  if (windowLabel === 'pet' || windowLabel === 'skill-pack-picker') return null

  // Settings window
  if (windowLabel === 'settings') {
    const handleClose = async () => {
      if (isTauri()) {
        try {
          const { getCurrentWindow } = await import('@tauri-apps/api/window')
          const { invoke } = await import('@tauri-apps/api/core')
          await invoke('set_dock_visible', { visible: false })
          // Destroy (not hide) so the WebView XPC processes actually exit.
          // The backend's `open_settings_window` rebuilds the window via
          // `build_settings_window` next time the user reopens settings.
          getCurrentWindow().destroy()
        } catch {
          // fallback: do nothing
        }
      } else {
        // Browser dev mode: switch back to notch
        setWindowLabel('notch')
      }
    }

    return (
      <div style={{ width: '100vw', height: '100vh', background: 'var(--settings-bg)' }}>
        <Suspense fallback={null}><SettingsApp onClose={handleClose} /></Suspense>
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
      <BackgroundUpdater />
      <Suspense fallback={null}><NotchPanel /></Suspense>
    </div>
  )

  if (!isTauri() && windowLabel === 'notch') {
    return <Suspense fallback={notchWindow}><ClaudeHookUiLab>{notchWindow}</ClaudeHookUiLab></Suspense>
  }

  // Notch window (default)
  return notchWindow
}

export default App
