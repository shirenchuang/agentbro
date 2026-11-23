import { act, cleanup, render } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import App from '../App'
import { useConfigStore } from '../stores/configStore'

vi.mock('../hooks/useTauri', () => ({ useTauriInit: vi.fn() }))
vi.mock('../hooks/useAutoHide', () => ({ useAutoHide: vi.fn() }))
vi.mock('../components/notch/NotchPanel', () => ({ NotchPanel: () => null }))
vi.mock('../components/BackgroundUpdater', () => ({ BackgroundUpdater: () => null }))

const configKey = 'agentbro-config'

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
    useConfigStore.setState({ panelHorizontalOffset: 0, volume: 70 })
  })

  afterEach(cleanup)

  it('keeps the released position when an older notification arrives', async () => {
    const older = window.localStorage.getItem(configKey)!
    render(<App />)

    await act(async () => useConfigStore.getState().updateConfig('panelHorizontalOffset', 28))
    const saved = window.localStorage.getItem(configKey)
    await act(async () => receiveConfig(older))

    expect(useConfigStore.getState().panelHorizontalOffset).toBe(28)
    expect(window.localStorage.getItem(configKey)).toBe(saved)
  })

  it('applies a current window preference while preserving backend settings', async () => {
    render(<App />)
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
})
