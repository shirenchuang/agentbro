import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { SettingsApp } from '../components/settings'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: { defaultValue?: string }) => options?.defaultValue ?? key,
    i18n: { language: 'en' },
  }),
}))

describe('settings island menu', () => {
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
})
