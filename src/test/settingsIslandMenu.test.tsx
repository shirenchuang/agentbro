import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { SettingsApp } from '../components/settings'
import { useConfigStore } from '../stores/configStore'

const tauriMocks = vi.hoisted(() => ({
  setIslandFeatureFlags: vi.fn(() => Promise.resolve()),
}))

vi.mock('../services/tauriApi', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../services/tauriApi')>()
  return {
    ...actual,
    setIslandFeatureFlags: tauriMocks.setIslandFeatureFlags,
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

  it('preserves follow focus when persisting visual island feature flags', async () => {
    useConfigStore.setState({ followFocus: true, tipsEnabled: true })

    render(<SettingsApp onClose={vi.fn()} />)

    fireEvent.click(screen.getByText('settings.island.title'))
    fireEvent.click(screen.getByRole('button', { name: /Advanced/ }))

    await waitFor(() => expect(screen.getByText('settings.tipsEnabled')).toBeInTheDocument())
    const tipsRow = screen.getByText('settings.tipsEnabled').closest('.setting-row')
    fireEvent.click(tipsRow!.querySelector('[role="switch"]')!)

    expect(tauriMocks.setIslandFeatureFlags).toHaveBeenCalledWith(expect.objectContaining({
      tipsEnabled: false,
      followFocus: true,
    }))
  })
})
