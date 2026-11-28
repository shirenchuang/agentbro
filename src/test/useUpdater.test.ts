import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { selectReleaseDownloadAsset, useUpdater } from '../hooks/useUpdater'
import { useConfigStore } from '../stores/configStore'

const mocks = vi.hoisted(() => ({
  check: vi.fn(),
  isHomebrewInstall: vi.fn(),
  isWindowsPlatform: vi.fn(),
  open: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-updater', () => ({ check: mocks.check }))
vi.mock('@tauri-apps/plugin-shell', () => ({ open: mocks.open }))
vi.mock('../utils/platform', async (importOriginal) => ({
  ...await importOriginal<typeof import('../utils/platform')>(),
  isWindowsPlatform: mocks.isWindowsPlatform,
}))
vi.mock('../services/tauriApi', async (importOriginal) => ({
  ...await importOriginal<typeof import('../services/tauriApi')>(),
  isTauri: () => true,
  isHomebrewInstall: mocks.isHomebrewInstall,
  getCurrentAppVersion: async () => '3.7.0',
}))

function uploaded(name: string) {
  return {
    name,
    state: 'uploaded',
    browser_download_url: `https://github.com/shirenchuang/agentbro/releases/download/v3.7.1/${name}`,
  }
}

const assets = [
  uploaded('AgentBro_latest_universal.dmg'),
  uploaded('AgentBro_latest_x64.msi'),
  uploaded('AgentBro_latest_x64-setup.exe'),
]

describe('selectReleaseDownloadAsset', () => {
  it('prefers the Windows setup executable on Windows', () => {
    expect(selectReleaseDownloadAsset(assets, 'windows')?.name).toBe('AgentBro_latest_x64-setup.exe')
  })

  it('falls back to the Windows MSI when the setup executable is absent', () => {
    expect(selectReleaseDownloadAsset(assets.slice(0, 2), 'windows')?.name).toBe('AgentBro_latest_x64.msi')
  })

  it('keeps the universal DMG for macOS', () => {
    expect(selectReleaseDownloadAsset(assets, 'macos')?.name).toBe('AgentBro_latest_universal.dmg')
  })

  it('uses the versioned MSI when the setup asset is still uploading', () => {
    const setup = { ...assets[2], state: 'starter' }
    const msi = uploaded('AgentBro_3.7.1_x64_en-US.msi')
    expect(selectReleaseDownloadAsset([setup, msi], 'windows')).toBe(msi)
  })
})

function releaseResponse(releaseAssets: typeof assets) {
  return new Response(JSON.stringify({
    tag_name: 'v3.7.1',
    body: 'Release notes',
    published_at: '2026-11-28T00:00:00Z',
    assets: releaseAssets,
  }))
}

describe('useUpdater checks for a published installer', () => {
  const fetchRelease = vi.fn()
  const preferences = {
    autoCheckUpdate: useConfigStore.getState().autoCheckUpdate,
    autoInstallUpdate: useConfigStore.getState().autoInstallUpdate,
  }

  beforeEach(() => {
    vi.resetAllMocks()
    vi.stubGlobal('fetch', fetchRelease)
    useConfigStore.setState({ autoCheckUpdate: false, autoInstallUpdate: false })
    mocks.isHomebrewInstall.mockResolvedValue(false)
    mocks.isWindowsPlatform.mockReturnValue(true)
    mocks.check.mockResolvedValue(null)
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    act(() => useConfigStore.setState(preferences))
  })

  it('waits for the Windows installer and opens the uploaded asset on a subsequent check', async () => {
    mocks.check.mockRejectedValue(new Error('Windows target missing from the signed manifest'))
    fetchRelease
      .mockResolvedValueOnce(releaseResponse([assets[0], { ...assets[2], state: 'starter' }]))
      .mockResolvedValueOnce(releaseResponse(assets))
    const { result } = renderHook(() => useUpdater())

    await act(() => result.current.checkForUpdate())
    expect(result.current).toMatchObject({ status: 'installer-pending', version: '3.7.1', manualDownloadUrl: null })
    await act(() => result.current.installUpdate())
    expect(mocks.open).not.toHaveBeenCalled()

    await act(() => result.current.checkForUpdate())
    expect(result.current.status).toBe('available')
    await act(() => result.current.installUpdate())
    expect(mocks.open).toHaveBeenCalledExactlyOnceWith(assets[2].browser_download_url)
  })

  it('checks the Windows release even when the signed feed returns no update', async () => {
    fetchRelease.mockResolvedValueOnce(releaseResponse(assets))
    const { result } = renderHook(() => useUpdater())

    await act(() => result.current.checkForUpdate())

    expect(result.current).toMatchObject({
      status: 'available',
      manualDownloadUrl: assets[2].browser_download_url,
    })
  })

  it('keeps a signed update available without checking the manual release', async () => {
    mocks.check.mockResolvedValue({ version: '3.7.1', body: 'Signed update' })
    const { result } = renderHook(() => useUpdater())

    await act(() => result.current.checkForUpdate())

    expect(result.current).toMatchObject({ status: 'available', version: '3.7.1', manualDownloadUrl: null })
    expect(fetchRelease).not.toHaveBeenCalled()
  })

  it('accepts a successful no-update result on macOS and keeps the DMG fallback for errors', async () => {
    mocks.isWindowsPlatform.mockReturnValue(false)
    const { result } = renderHook(() => useUpdater())

    await act(() => result.current.checkForUpdate())
    expect(result.current.status).toBe('up-to-date')
    expect(fetchRelease).not.toHaveBeenCalled()

    mocks.check.mockRejectedValue(new Error('Signed feed unavailable'))
    fetchRelease.mockResolvedValueOnce(releaseResponse(assets))
    await act(() => result.current.checkForUpdate())
    expect(result.current).toMatchObject({ status: 'available', manualDownloadUrl: assets[0].browser_download_url })
  })

  it('shows Homebrew updates even when the release lacks a manual installer', async () => {
    mocks.isWindowsPlatform.mockReturnValue(false)
    mocks.isHomebrewInstall.mockResolvedValue(true)
    fetchRelease.mockResolvedValueOnce(releaseResponse([]))
    const { result } = renderHook(() => useUpdater())

    await act(() => result.current.checkForUpdate())

    expect(result.current).toMatchObject({ status: 'available', installChannel: 'homebrew', version: '3.7.1' })
    expect(mocks.check).not.toHaveBeenCalled()
  })
})
