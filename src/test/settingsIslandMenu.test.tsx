import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { SettingsApp } from '../components/settings'
import type { BackendDisplayInfo } from '../services/tauriApi'
import { useConfigStore } from '../stores/configStore'

const tauriMocks = vi.hoisted(() => ({
  listDisplays: vi.fn(() => Promise.resolve([] as BackendDisplayInfo[])),
  repositionNotch: vi.fn(() => Promise.resolve()),
  setDisplayId: vi.fn(() => Promise.resolve()),
  setIslandFeatureFlags: vi.fn(() => Promise.resolve()),
  previewIslandLayout: vi.fn(() => Promise.resolve()),
  clearIslandLayoutPreview: vi.fn(() => Promise.resolve()),
}))

vi.mock('../services/tauriApi', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../services/tauriApi')>()
  return {
    ...actual,
    listDisplays: tauriMocks.listDisplays,
    repositionNotch: tauriMocks.repositionNotch,
    setDisplayId: tauriMocks.setDisplayId,
    setIslandFeatureFlags: tauriMocks.setIslandFeatureFlags,
    previewIslandLayout: tauriMocks.previewIslandLayout,
    clearIslandLayoutPreview: tauriMocks.clearIslandLayoutPreview,
  }
})

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: { defaultValue?: string }) => options?.defaultValue ?? key,
    i18n: { language: 'en' },
  }),
}))

describe('settings island menu', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    tauriMocks.listDisplays.mockResolvedValue([])
    useConfigStore.setState({ displayMonitor: 'auto', followFocus: false, tipsEnabled: true })
  })

  it('uses the left settings menu for island pages instead of top tabs', async () => {
    const { container } = render(<SettingsApp onClose={vi.fn()} />)

    fireEvent.click(screen.getByText('settings.island.title'))

    await waitFor(() => expect(screen.getByRole('button', { name: /Overview/ })).toHaveClass('active'))
    expect(screen.getByRole('button', { name: /Display/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Behavior/ })).toBeInTheDocument()
    expect(container.querySelector('.island-tabs')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /Display/ }))

    await waitFor(() => expect(screen.getByRole('button', { name: /Display/ })).toHaveClass('active'))
    await waitFor(() => expect(screen.getByText('settings.colorTheme')).toBeInTheDocument())
    expect(screen.getByRole('radiogroup', { name: '展示模式' })).toBeInTheDocument()
    expect(screen.getByRole('radio', { name: '灵动岛' })).toHaveAttribute('aria-checked', 'true')
    expect(container.querySelector('.island-tabs')).not.toBeInTheDocument()
  })

  it('shows tips toggle in island overview and preserves follow focus when persisting it', async () => {
    useConfigStore.setState({ followFocus: true, tipsEnabled: true })

    render(<SettingsApp onClose={vi.fn()} />)

    fireEvent.click(screen.getByText('settings.island.title'))

    await waitFor(() => expect(screen.getByText('settings.tipsEnabled')).toBeInTheDocument())
    const tipsRow = screen.getByText('settings.tipsEnabled').closest('.setting-row')
    fireEvent.click(tipsRow!.querySelector('[role="switch"]')!)

    expect(tauriMocks.setIslandFeatureFlags).toHaveBeenCalledWith(expect.objectContaining({
      tipsEnabled: false,
      followFocus: true,
    }))
  })

  it('shows the primary display label instead of a stale raw display id', async () => {
    useConfigStore.setState({ displayMonitor: '14035' })
    tauriMocks.listDisplays.mockResolvedValue([
      {
        id: 'Color LCD',
        name: 'Color LCD',
        label: 'Color LCD (1728x1117)',
        width: 3456,
        height: 2234,
        scaleFactor: 2,
        isPrimary: true,
      },
    ])

    render(<SettingsApp onClose={vi.fn()} />)

    fireEvent.click(screen.getByText('settings.island.title'))
    fireEvent.click(screen.getByRole('button', { name: /Display/ }))

    await waitFor(() => {
      expect(screen.getByText('settings.mainDisplay · Color LCD (1728x1117)')).toBeInTheDocument()
    })
    expect(screen.queryByText('14035')).not.toBeInTheDocument()
  })

  it('passes custom notch height through the layout preview event', async () => {
    useConfigStore.setState({ notchHeightMode: 'custom', customNotchHeight: 40 })
    const { container } = render(<SettingsApp onClose={vi.fn()} />)

    fireEvent.click(screen.getByText('settings.island.title'))
    fireEvent.click(screen.getByRole('button', { name: /Display/ }))

    await waitFor(() => expect(screen.getByText('settings.customNotchHeight')).toBeInTheDocument())
    const customRow = screen.getByText('settings.customNotchHeight').closest('.setting-row')
    const slider = customRow!.querySelector<HTMLInputElement>('input[type="range"]')!
    fireEvent.change(slider, { target: { value: '55' } })

    expect(tauriMocks.previewIslandLayout).toHaveBeenCalledWith('compact', expect.objectContaining({
      notchHeightMode: 'custom',
      customNotchHeight: 55,
    }))
    expect(container.querySelector('.island-tabs')).not.toBeInTheDocument()
  })
})
