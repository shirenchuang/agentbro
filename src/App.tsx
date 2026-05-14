/* AgentBro — Main App */
import { useEffect, useRef, useState } from 'react'
import { NotchPanel } from './components/notch/NotchPanel'
import { SettingsApp } from './components/settings'
import { useSessionStore } from './stores/sessionStore'
import { COLOR_THEMES, useThemeStore } from './stores/themeStore'
import { useConfigStore } from './stores/configStore'
import { useTauriInit } from './hooks/useTauri'
import { useAutoHide } from './hooks/useAutoHide'
import { getActiveThemeBundle, isTauri } from './services/tauriApi'
import type { AgentType, DiffLine, ChatMessage, TaskInfo } from './types/agent'
import './styles/globals.css'

// ── Mock Data ──────────────────────────────────────────────────

const mockDiffLines: DiffLine[] = [
  { type: 'context', lineNumber: 10, content: 'import { getToken } from "./auth"' },
  { type: 'remove', lineNumber: 12, content: 'const token = getToken()' },
  { type: 'add', lineNumber: 12, content: 'const token = await refreshToken()' },
  { type: 'context', lineNumber: 13, content: 'if (!token) return null' },
  { type: 'remove', lineNumber: 15, content: 'return { Authorization: token }' },
  { type: 'add', lineNumber: 15, content: 'return { Authorization: `Bearer ${token}` }' },
]

const mockChat1: ChatMessage[] = [
  { role: 'user', content: 'Fix the auth middleware to refresh expired tokens', timestamp: Date.now() - 120000 },
  { role: 'assistant', content: 'I\'ll fix the auth middleware. Let me read the current implementation first.', timestamp: Date.now() - 115000 },
  { role: 'tool_use', toolName: 'Read', toolInput: 'src/auth/middleware.ts', status: 'success', timestamp: Date.now() - 110000 },
  { role: 'assistant', content: 'I can see the issue. The `getToken()` call doesn\'t handle expired tokens. I\'ll update it to use `refreshToken()` with proper Bearer prefix.', timestamp: Date.now() - 100000 },
  { role: 'permission', toolName: 'Edit', diff: { filePath: 'src/auth/middleware.ts', lines: mockDiffLines }, timestamp: Date.now() - 90000 },
]

const mockChat2: ChatMessage[] = [
  { role: 'user', content: '\u4ECA\u5929\u4EC0\u4E48\u5929\u6C14\uFF1F', timestamp: Date.now() - 300000 },
  { role: 'assistant', content: 'I\'ll search for today\'s weather information.', timestamp: Date.now() - 295000 },
  { role: 'tool_use', toolName: 'WebSearch', toolInput: '\u4ECA\u5929\u5929\u6C14 2026\u5E744\u670815\u65E5', status: 'running', timestamp: Date.now() - 5000 },
]

const mockTasks1: TaskInfo[] = [
  { id: 't1', name: 'Move plugin ActivityBar items into NavRail', status: 'completed' },
  { id: 't2', name: 'Update behaviors.ts fixed width references', status: 'completed' },
  { id: 't3', name: 'Verify with bun check and bun test:run', status: 'in_progress' },
  { id: 't4', name: 'Update layout constants', status: 'completed' },
  { id: 't5', name: 'Fix responsive breakpoints', status: 'completed' },
]

const mockTasks2: TaskInfo[] = [
  { id: 't6', name: 'Analyze query patterns', status: 'completed' },
  { id: 't7', name: 'Find N+1 in UserRepository', status: 'completed' },
  { id: 't8', name: 'Refactor with eager loading', status: 'completed' },
  { id: 't9', name: 'Fix OrderRepository queries', status: 'completed' },
  { id: 't10', name: 'Query Context7 docs', status: 'completed' },
  { id: 't11', name: 'Add batch loading', status: 'completed' },
  { id: 't12', name: 'Update integration tests', status: 'completed' },
]

const mockPlanContent = `Context
The current layout has accumulated redundant wrappers
from the multi-sidebar era.

Current Layout (outer \u2192 inner)
App.tsx:
  [NavRail 56px] | [PageRouter \u2192 children]

Proposed Changes
1. Merge ActivityBar into NavRail
2. Remove redundant PageWrapper
3. Update fixed width references`

function loadMockSessions() {
  const store = useSessionStore.getState()

  // Session 1: Claude with permission request + plan + tasks
  store.updateSession({ type: 'session_start', sessionId: 's1', project: 'app-layout', terminal: 'iTerm\u00B7tmux', agentType: 'claude-code' as AgentType })
  store.updateSession({ type: 'token_usage', sessionId: 's1', input: 12500, output: 3200, cacheRead: 8000, cacheCreate: 0 })

  // Session 2: Claude processing with weather query + response
  store.updateSession({ type: 'session_start', sessionId: 's2', project: 'assistant', terminal: 'iTerm\u00B7tmux', agentType: 'claude-code' as AgentType })
  store.updateSession({ type: 'token_usage', sessionId: 's2', input: 45000, output: 12000, cacheRead: 20000, cacheCreate: 0 })

  // Session 3: Gemini idle
  store.updateSession({ type: 'session_start', sessionId: 's3', project: 'optimize-queries', terminal: 'Ghostty', agentType: 'gemini-cli' as AgentType })

  useSessionStore.setState((state) => {
    const sessions = { ...state.sessions }

    if (sessions.s1) {
      sessions.s1 = {
        ...sessions.s1,
        chatHistory: mockChat1,
        phase: 'waiting_approval',
        pendingPermission: { toolName: 'Edit', toolInput: 'src/components/NavRail.tsx', diff: { filePath: 'src/components/NavRail.tsx', lines: mockDiffLines } },
        tasks: mockTasks1,
        lastUserMessage: 'Edit: app-layout.tsx',
        sessionTitle: 'Edit: app-layout.tsx',
        lastToolName: 'Edit',
        lastToolStatus: 'running',
        planTitle: 'Layout Optimization: Merge NavRail...',
        planContent: mockPlanContent,
      }
    }

    if (sessions.s2) {
      sessions.s2 = {
        ...sessions.s2,
        chatHistory: mockChat2,
        phase: 'done',
        description: 'Weather query completed',
        tasks: mockTasks2,
        lastUserMessage: '\u4ECA\u5929\u4EC0\u4E48\u5929\u6C14\uFF1F',
        sessionTitle: '\u4ECA\u5929\u4EC0\u4E48\u5929\u6C14\uFF1F',
        lastToolName: 'WebSearch',
        lastToolStatus: 'success',
        responseText: '\u4ECA\u5929\u662F 2026\u5E744\u670815\u65E5 (\u661F\u671F\u4E09), \u4E3B\u8981\u57CE\u5E02\u5929\u6C14:\n- \u5317\u4EAC: \u6674, 14\u2103 ~ 26\u2103\n- \u4E0A\u6D77: \u591A\u4E91, 16\u2103 ~ 22\u2103\n- \u6DF1\u5733: \u96F7\u9635\u96E8, 22\u2103 ~ 28\u2103',
      }
    }

    if (sessions.s3) {
      sessions.s3 = {
        ...sessions.s3,
        lastUserMessage: 'Optimize the SQL queries for the dashboard',
        sessionTitle: 'Dashboard SQL optimization',
        lastToolName: 'Bash',
        lastToolStatus: 'success',
      }
    }

    return { sessions, sessionList: Object.values(sessions) }
  })
  store.setPanelState('collapsed')
}

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
          useThemeStore.setState({ colorTheme: nextTheme })
        }
        const activeThemeName = persisted.state?.activeThemeName
        if (activeThemeName) {
          const store = useThemeStore.getState()
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

  // Load mocks only for notch in browser dev mode
  useEffect(() => {
    if (!isTauri() && windowLabel === 'notch') loadMockSessions()
  }, [windowLabel])

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

  // Notch window (default)
  return (
    <div style={{
      width: '100vw',
      height: '100vh',
      background: 'transparent',
      position: 'relative',
    }}>
      <NotchPanel />
    </div>
  )
}

export default App
