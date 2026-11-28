import { act, cleanup, render, screen } from '@testing-library/react'
import type { PropsWithChildren } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import App from '../App'
import { useConfigStore } from '../stores/configStore'
import { usePetStore } from '../stores/petStore'
import { useThemeStore } from '../stores/themeStore'

const { desktop } = vi.hoisted(() => ({ desktop: vi.fn(() => false) }))
vi.mock('../services/tauriApi', async (importOriginal) => ({
  ...await importOriginal<typeof import('../services/tauriApi')>(),
  isTauri: desktop,
}))
vi.mock('../hooks/useTauri', () => ({ useTauriInit: vi.fn() }))
vi.mock('../hooks/useAutoHide', () => ({ useAutoHide: vi.fn() }))
vi.mock('../components/dev/ClaudeHookUiLab', () => ({
  ClaudeHookUiLab: ({ children }: PropsWithChildren) => children,
}))
vi.mock('../components/notch/NotchPanel', () => ({
  NotchPanel: () => <div data-testid="notch-panel" />,
}))
vi.mock('../components/BackgroundUpdater', () => ({ BackgroundUpdater: () => null }))

const configKey = 'agentbro-config'

async function renderApp() {
  render(<App />)
  await screen.findByTestId('notch-panel')
}

function receiveConfig(value: string) {
  window.dispatchEvent(new StorageEvent('storage', {
    key: configKey,
    newValue: value,
    storageArea: window.localStorage,
  }))
}

describe('configuration notifications between windows', () => {
  beforeEach(() => {
    window.localStorage.clear()
    desktop.mockReturnValue(false)
    useConfigStore.setState({ panelHorizontalOffset: 0, volume: 70 })
  })

  afterEach(cleanup)

  it('keeps the released position when an older notification arrives', async () => {
    const older = window.localStorage.getItem(configKey)!
    await renderApp()

    await act(async () => useConfigStore.getState().updateConfig('panelHorizontalOffset', 28))
    const saved = window.localStorage.getItem(configKey)
    await act(async () => receiveConfig(older))

    expect(useConfigStore.getState().panelHorizontalOffset).toBe(28)
    expect(window.localStorage.getItem(configKey)).toBe(saved)
  })

  it('applies a current window preference while preserving backend settings', async () => {
    await renderApp()
    const snapshot = JSON.stringify({
      state: { panelHorizontalOffset: 35, volume: 8 },
      version: 0,
    })

    window.localStorage.setItem(configKey, snapshot)
    await act(async () => receiveConfig(snapshot))

    expect(useConfigStore.getState().panelHorizontalOffset).toBe(35)
    expect(useConfigStore.getState().volume).toBe(70)
    expect(window.localStorage.getItem(configKey)).toBe(snapshot)

    await act(async () => useConfigStore.getState().updateConfig('panelHorizontalOffset', 42))
    expect(JSON.parse(window.localStorage.getItem(configKey)!).state.panelHorizontalOffset).toBe(42)
  })

  it('keeps desktop pet, debug, and role theme state when focus replays window storage', async () => {
    desktop.mockReturnValue(true)
    const [cachedTheme, backendTheme] = useThemeStore.getState().themes
    useThemeStore.getState().setActiveTheme(backendTheme.name)
    useConfigStore.setState({ petVitalsDebugOpen: false })
    usePetStore.getState().hydrateFromConfig('backend-pet')
    await renderApp()

    window.localStorage.setItem(configKey, JSON.stringify({ state: { petVitalsDebugOpen: true, panelHorizontalOffset: 26 } }))
    window.localStorage.setItem('agentbro-pet', JSON.stringify({ state: { activePetId: 'cached-pet' } }))
    window.localStorage.setItem('agentbro-theme', JSON.stringify({ version: 3, state: { activeThemeName: cachedTheme.name } }))
    await act(async () => window.dispatchEvent(new Event('focus')))

    expect(useConfigStore.getState().petVitalsDebugOpen).toBe(false)
    expect(usePetStore.getState().activePetId).toBe('backend-pet')
    expect(useThemeStore.getState().activeThemeName).toBe(backendTheme.name)
    expect(useConfigStore.getState().panelHorizontalOffset).toBe(26)
  })
})
