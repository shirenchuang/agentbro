import { fireEvent, render, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { CollapsedBar } from '../components/notch/CollapsedBar'

const tauriMocks = vi.hoisted(() => ({
  openSettingsWindow: vi.fn(() => Promise.resolve()),
  setSoundEnabled: vi.fn(() => Promise.resolve()),
}))

vi.mock('../services/tauriApi', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../services/tauriApi')>()
  return {
    ...actual,
    openSettingsWindow: tauriMocks.openSettingsWindow,
    setSoundEnabled: tauriMocks.setSoundEnabled,
  }
})

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}))

describe('collapsed bar settings button', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    tauriMocks.openSettingsWindow.mockResolvedValue(undefined)
  })

  it('opens native settings from the expanded header gear without collapsing', async () => {
    const onCollapse = vi.fn()
    const { container } = render(
      <CollapsedBar sessions={[]} panelState="expanded" onCollapse={onCollapse} />,
    )

    const settingsButton = container.querySelector<HTMLButtonElement>(
      '.collapsed-bar__status-row button[title="notch.settings"]',
    )
    expect(settingsButton).toBeInTheDocument()

    fireEvent.click(settingsButton!)

    await waitFor(() => expect(tauriMocks.openSettingsWindow).toHaveBeenCalledTimes(1))
    expect(onCollapse).not.toHaveBeenCalled()
  })
})
