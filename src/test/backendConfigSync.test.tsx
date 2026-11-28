import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useConfigSync } from '../hooks/useTauri'
import type { BackendConfig, BackendConfigSnapshot } from '../services/tauriApi'
import { useConfigStore } from '../stores/configStore'
import { usePetStore } from '../stores/petStore'
import { useThemeStore } from '../stores/themeStore'
import type { ThemeConfig } from '../types/theme'

const api = vi.hoisted(() => ({
  getConfig: vi.fn<() => Promise<BackendConfigSnapshot>>(),
  listen: vi.fn<(name: string, callback: (event: { payload: BackendConfigSnapshot }) => void) => Promise<() => void>>(),
  listThemes: vi.fn(async () => []),
  setLanguage: vi.fn(async () => {}),
  getActiveThemeBundle: vi.fn<(name: string) => Promise<ThemeConfig>>(),
}))

vi.mock('../services/tauriApi', async (importOriginal) => ({
  ...await importOriginal<typeof import('../services/tauriApi')>(),
  isTauri: () => true,
  getConfig: api.getConfig,
  listThemes: api.listThemes,
  setLanguage: api.setLanguage,
  getActiveThemeBundle: api.getActiveThemeBundle,
}))

vi.mock('@tauri-apps/api/event', async (importOriginal) => ({
  ...await importOriginal<typeof import('@tauri-apps/api/event')>(),
  listen: api.listen,
}))

vi.mock('i18next', () => ({ default: { language: 'en', changeLanguage: vi.fn() } }))

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => { resolve = done })
  return { promise, resolve }
}

async function makeSnapshot(revision: number, patch: Partial<BackendConfig> = {}): Promise<BackendConfigSnapshot> {
  const actual = await vi.importActual<typeof import('../services/tauriApi')>('../services/tauriApi')
  return { ...await actual.getConfig(), ...patch, revision }
}

describe('Tauri configuration synchronization', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    api.listen.mockResolvedValue(vi.fn())
    useConfigStore.setState({ language: 'en', idleInteractionRoutingMinutes: 5 })
    usePetStore.getState().hydrateFromConfig(null)
  })

  it('keeps a newer event when the initial read or older and duplicate events arrive late', async () => {
    const [oldTheme, newTheme] = useThemeStore.getState().themes
    const initial = await makeSnapshot(0, { language: 'zh', theme: oldTheme.name, islandActivePetId: 'old-pet' })
    const saved = { ...initial, revision: 2, language: 'en' as const, theme: newTheme.name, islandActivePetId: 'new-pet', idleInteractionRoutingMinutes: 15 }
    const read = deferred<BackendConfigSnapshot>()
    api.getConfig.mockReturnValue(read.promise)
    const view = renderHook(() => useConfigSync())
    await waitFor(() => expect(api.listen).toHaveBeenCalledWith('config-changed', expect.any(Function)))
    await waitFor(() => expect(api.getConfig).toHaveBeenCalledOnce())
    const receive = api.listen.mock.calls[0][1]

    await act(async () => receive({ payload: saved }))
    expect(useConfigStore.getState().idleInteractionRoutingMinutes).toBe(15)
    await act(async () => {
      read.resolve(initial)
      receive({ payload: { ...initial, revision: 1 } })
      receive({ payload: { ...initial, revision: 2 } })
    })

    expect(useConfigStore.getState().idleInteractionRoutingMinutes).toBe(15)
    expect(useThemeStore.getState().activeThemeName).toBe(newTheme.name)
    expect(usePetStore.getState().activePetId).toBe('new-pet')
    expect(api.setLanguage).not.toHaveBeenCalled()
    expect(api.listThemes).toHaveBeenCalledOnce()
    view.unmount()
  })

  it('waits for the listener before reading the initial configuration', async () => {
    const listener = deferred<() => void>()
    const unlisten = vi.fn()
    api.listen.mockReturnValue(listener.promise)
    api.getConfig.mockResolvedValue(await makeSnapshot(0, {
      idleInteractionRoutingMinutes: 10,
      language: 'zh',
      soundRules: { boot: { enabled: false, sound: 'default' } },
    }))
    const view = renderHook(() => useConfigSync())

    await waitFor(() => expect(api.listen).toHaveBeenCalledOnce())
    expect(api.getConfig).not.toHaveBeenCalled()
    await act(async () => listener.resolve(unlisten))
    await waitFor(() => expect(useConfigStore.getState().idleInteractionRoutingMinutes).toBe(10))
    expect(api.setLanguage).toHaveBeenCalledOnce()
    expect(api.setLanguage).toHaveBeenCalledWith('en')
    view.unmount()
    expect(unlisten).toHaveBeenCalledOnce()
  })

  it('releases a late listener without reading configuration after unmount', async () => {
    const listener = deferred<() => void>()
    const unlisten = vi.fn()
    api.listen.mockReturnValue(listener.promise)
    const view = renderHook(() => useConfigSync())

    await waitFor(() => expect(api.listen).toHaveBeenCalledOnce())
    view.unmount()
    await act(async () => listener.resolve(unlisten))

    expect(unlisten).toHaveBeenCalledOnce()
    expect(api.getConfig).not.toHaveBeenCalled()
  })

  it('keeps the current role theme when an earlier theme bundle arrives late', async () => {
    const [baseTheme, currentTheme] = useThemeStore.getState().themes
    useThemeStore.getState().setActiveTheme(baseTheme.name)
    const oldTheme = { ...baseTheme, name: 'older-theme' }
    const bundle = deferred<ThemeConfig>()
    api.getActiveThemeBundle.mockReturnValue(bundle.promise)
    const initial = await makeSnapshot(0, { theme: baseTheme.name })
    api.getConfig.mockResolvedValue(initial)
    const view = renderHook(() => useConfigSync(true, false))
    await waitFor(() => expect(api.listThemes).toHaveBeenCalledOnce())
    const receive = api.listen.mock.calls[0][1]

    await act(async () => receive({ payload: { ...initial, revision: 1, theme: oldTheme.name } }))
    expect(api.getActiveThemeBundle).toHaveBeenCalledWith(oldTheme.name)
    await act(async () => receive({ payload: { ...initial, revision: 2, theme: currentTheme.name } }))
    await act(async () => bundle.resolve(oldTheme))

    expect(useThemeStore.getState().activeThemeName).toBe(currentTheme.name)
    view.unmount()
  })
})
