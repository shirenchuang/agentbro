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
  previewSound: vi.fn(() => Promise.resolve()),
  setSoundEventRule: vi.fn(() => Promise.resolve()),
  registerGlobalShortcut: vi.fn(() => Promise.resolve()),
  setGlobalActionShortcuts: vi.fn(() => Promise.resolve()),
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
    previewSound: tauriMocks.previewSound,
    setSoundEventRule: tauriMocks.setSoundEventRule,
    registerGlobalShortcut: tauriMocks.registerGlobalShortcut,
    setGlobalActionShortcuts: tauriMocks.setGlobalActionShortcuts,
  }
})

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: { defaultValue?: string; message?: string; count?: number }) => {
      const translations: Record<string, string> = {
        'settings.soundEvents.session-start': '会话开始',
        'settings.soundEvents.permission-request': '权限请求',
        'settings.soundEvents.context-compact': '上下文压缩',
        'settings.probeFilterDesc': '静音很快结束的后台探测会话，避免连接检查、模型探测等短任务播放提示音。',
      }
      const value = translations[key] ?? options?.defaultValue ?? key
      return value
        .replace('{{message}}', options?.message ?? '')
        .replace('{{count}}', String(options?.count ?? ''))
    },
    i18n: { language: 'en' },
  }),
}))

describe('settings island menu', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    tauriMocks.listDisplays.mockResolvedValue([])
    tauriMocks.registerGlobalShortcut.mockResolvedValue(undefined)
    tauriMocks.setGlobalActionShortcuts.mockResolvedValue(undefined)
    useConfigStore.setState({
      displayMonitor: 'auto',
      followFocus: false,
      tipsEnabled: true,
      sshHosts: [],
    })
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

  it('places SSH Remote under Integration and keeps it separate from Advanced', async () => {
    const { container } = render(<SettingsApp onClose={vi.fn()} />)

    fireEvent.click(screen.getByText('settings.island.title'))

    await waitFor(() => expect(screen.getByRole('button', { name: /Overview/ })).toHaveClass('active'))
    const islandMenuLabels = Array.from(container.querySelectorAll('.settings-capability-nav button'))
      .map((button) => button.getAttribute('aria-label'))
    expect(islandMenuLabels.slice(3, 5)).toEqual(['Integration', 'SSH Remote'])

    fireEvent.click(screen.getByRole('button', { name: /SSH Remote/ }))

    await waitFor(() => expect(screen.getByText('settings.sshDescription')).toBeInTheDocument())

    fireEvent.click(screen.getByRole('button', { name: /Advanced/ }))

    await waitFor(() => expect(screen.getByText('Visual Signals')).toBeInTheDocument())
    expect(screen.queryByText('settings.sshDescription')).not.toBeInTheDocument()
  })

  it('keeps the SSH Remote host fallback working outside Tauri', async () => {
    render(<SettingsApp onClose={vi.fn()} />)

    fireEvent.click(screen.getByText('settings.island.title'))
    fireEvent.click(screen.getByRole('button', { name: /SSH Remote/ }))

    fireEvent.change(await screen.findByPlaceholderText('settings.name'), { target: { value: 'Dev Box' } })
    fireEvent.change(screen.getByPlaceholderText('user@host'), { target: { value: 'dev.example.com:2222' } })
    fireEvent.click(screen.getByRole('button', { name: 'settings.add' }))

    await waitFor(() => expect(screen.getByText('Dev Box')).toBeInTheDocument())
    expect(useConfigStore.getState().sshHosts).toEqual([
      expect.objectContaining({ name: 'Dev Box', host: 'dev.example.com:2222', enabled: true }),
    ])
  })

  it('places Hook diagnostics under Integration instead of Advanced', async () => {
    render(<SettingsApp onClose={vi.fn()} />)

    fireEvent.click(screen.getByText('settings.island.title'))
    fireEvent.click(screen.getByRole('button', { name: /Integration/ }))

    await waitFor(() => expect(screen.getByText('Hook Doctor')).toBeInTheDocument())
    fireEvent.click(screen.getByRole('button', { name: 'Run diagnostics' }))
    await waitFor(() => expect(screen.getByText('Browser mode')).toBeInTheDocument())

    fireEvent.click(screen.getByRole('button', { name: /Advanced/ }))

    await waitFor(() => expect(screen.getByText('Visual Signals')).toBeInTheDocument())
    expect(screen.queryByText('Hook Doctor')).not.toBeInTheDocument()
    expect(screen.queryByText('Session Launcher')).not.toBeInTheDocument()
    expect(screen.queryByText('Custom CLI Hook Templates')).not.toBeInTheDocument()
  })

  it('renders Advanced without removed debug and launcher controls', async () => {
    render(<SettingsApp onClose={vi.fn()} />)

    fireEvent.click(screen.getByText('settings.island.title'))
    fireEvent.click(screen.getByRole('button', { name: /Advanced/ }))

    await waitFor(() => expect(screen.getByText('Visual Signals')).toBeInTheDocument())
    expect(screen.getByText('settings.agentActivity')).toBeInTheDocument()
    expect(screen.getByText('settings.pixelCursor')).toBeInTheDocument()
    expect(screen.queryByText('settings.showCacheTTL')).not.toBeInTheDocument()
    expect(screen.queryByText('settings.aiMessageLines')).not.toBeInTheDocument()
    expect(screen.queryByText('settings.confettiOnComplete')).not.toBeInTheDocument()
    expect(screen.queryByText('Debug and Paths')).not.toBeInTheDocument()
    expect(screen.queryByText('settings.remoteHosts')).not.toBeInTheDocument()
  })

  it('keeps the island page active when collapsing the capability sidebar', async () => {
    render(<SettingsApp onClose={vi.fn()} />)

    fireEvent.click(screen.getByText('settings.island.title'))

    await waitFor(() => expect(screen.getByText('settings.tipsEnabled')).toBeInTheDocument())
    fireEvent.click(screen.getByRole('button', { name: 'Collapse sidebar' }))

    expect(screen.getByRole('button', { name: 'Expand sidebar' })).toBeInTheDocument()
    expect(screen.getByText('settings.tipsEnabled')).toBeInTheDocument()
  })

  it('uses the capability brand area to return to general settings', async () => {
    render(<SettingsApp onClose={vi.fn()} />)

    fireEvent.click(screen.getByText('settings.island.title'))

    await waitFor(() => expect(screen.getByText('settings.tipsEnabled')).toBeInTheDocument())
    fireEvent.click(screen.getByRole('button', { name: 'Back to Settings' }))

    await waitFor(() => expect(screen.getByText('settings.language')).toBeInTheDocument())
    expect(screen.queryByText('settings.tipsEnabled')).not.toBeInTheDocument()
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

  it('uses interaction mode cards as presets and marks manual changes as custom', async () => {
    useConfigStore.setState({
      interactionMode: 'persistent',
      smartSuppression: true,
      autoHideNoSessions: false,
      idleCompactDwellSeconds: 8,
      noSessionsHideDelay: 10,
    })

    render(<SettingsApp onClose={vi.fn()} />)

    fireEvent.click(screen.getByText('settings.island.title'))
    fireEvent.click(await screen.findByText('Quiet Assistant'))

    expect(useConfigStore.getState()).toEqual(expect.objectContaining({
      interactionMode: 'minimal',
      smartSuppression: true,
      autoHideNoSessions: true,
      idleCompactDwellSeconds: 8,
      noSessionsHideDelay: 10,
    }))
    expect(screen.queryByText('Custom visibility')).not.toBeInTheDocument()

    const autoHideRow = screen.getByText('settings.autoHideNoSessions').closest('.setting-row')
    fireEvent.click(autoHideRow!.querySelector('[role="switch"]')!)

    await waitFor(() => expect(screen.getByText('Custom visibility')).toBeInTheDocument())
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

  it('previews the notification card max height from display settings', async () => {
    useConfigStore.setState({ completionCardHeight: 200 })
    render(<SettingsApp onClose={vi.fn()} />)

    fireEvent.click(screen.getByText('settings.island.title'))
    fireEvent.click(screen.getByRole('button', { name: /Display/ }))

    await waitFor(() => expect(screen.getByText('settings.completionCardHeight')).toBeInTheDocument())
    const row = screen.getByText('settings.completionCardHeight').closest('.setting-row')
    const slider = row!.querySelector<HTMLInputElement>('input[type="range"]')!
    expect(slider).toHaveAttribute('max', '420')

    fireEvent.change(slider, { target: { value: '360' } })

    expect(tauriMocks.previewIslandLayout).toHaveBeenCalledWith('completion', expect.objectContaining({
      completionCardHeight: 360,
    }))
  })

  it('previews the detail view height from display settings', async () => {
    useConfigStore.setState({ detailPanelMaxHeight: 500 })
    render(<SettingsApp onClose={vi.fn()} />)

    fireEvent.click(screen.getByText('settings.island.title'))
    fireEvent.click(screen.getByRole('button', { name: /Display/ }))

    await waitFor(() => expect(screen.getByText('settings.detailPanelMaxHeight')).toBeInTheDocument())
    const row = screen.getByText('settings.detailPanelMaxHeight').closest('.setting-row')
    const slider = row!.querySelector<HTMLInputElement>('input[type="range"]')!
    expect(slider).toHaveAttribute('max', '1200')

    fireEvent.change(slider, { target: { value: '420' } })

    expect(tauriMocks.previewIslandLayout).toHaveBeenCalledWith('expanded', expect.objectContaining({
      detailPanelMaxHeight: 420,
    }))
  })

  it('localizes sound event labels and explains probe session filtering', async () => {
    render(<SettingsApp onClose={vi.fn()} />)

    fireEvent.click(screen.getByText('settings.island.title'))
    fireEvent.click(screen.getByRole('button', { name: /Notifications/ }))

    await waitFor(() => expect(screen.getByText('会话开始')).toBeInTheDocument())
    expect(screen.getByText('权限请求')).toBeInTheDocument()
    expect(screen.getByText('上下文压缩')).toBeInTheDocument()
    expect(screen.queryByText('Session Started')).not.toBeInTheDocument()
    expect(screen.getByText('静音很快结束的后台探测会话，避免连接检查、模型探测等短任务播放提示音。')).toBeInTheDocument()
  })

  it('previews notification sounds without rewriting the saved event rule', async () => {
    useConfigStore.setState({
      soundRules: {
        ...useConfigStore.getState().soundRules,
        'session-start': { enabled: false, sound: 'builtin:hero' },
      },
    })
    render(<SettingsApp onClose={vi.fn()} />)

    fireEvent.click(screen.getByText('settings.island.title'))
    fireEvent.click(screen.getByRole('button', { name: /Notifications/ }))

    const row = (await screen.findByText('会话开始')).closest('.sound-event-row')!
    fireEvent.click(row.querySelector('.sound-event-row__play')!)

    await waitFor(() => expect(tauriMocks.previewSound).toHaveBeenCalledWith('session-start', 'builtin:hero'))
    expect(tauriMocks.setSoundEventRule).not.toHaveBeenCalled()
  })

  it('records and clears in-window shortcuts from the shortcuts page', async () => {
    render(<SettingsApp onClose={vi.fn()} />)

    fireEvent.click(screen.getByText('settings.island.title'))
    fireEvent.click(screen.getByRole('button', { name: /Shortcuts/ }))

    await waitFor(() => expect(screen.getByText('Toggle Panel')).toBeInTheDocument())
    expect(screen.getByText('Collapse Panel')).toBeInTheDocument()
    expect(screen.getByText('Open Settings')).toBeInTheDocument()
    expect(screen.queryByText('Expand Panel')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /Show advanced shortcuts/ }))
    await waitFor(() => expect(screen.getByText('Expand Panel')).toBeInTheDocument())
    const row = screen.getByText('Expand Panel').closest('.shortcuts-row')!
    const editButtons = row.querySelectorAll('.shortcuts-row__edit')
    fireEvent.click(editButtons[editButtons.length - 1]!)
    fireEvent.keyDown(window, { key: 'k', metaKey: true, shiftKey: true })

    expect(useConfigStore.getState().shortcuts.find((shortcut) => shortcut.action === 'expand-panel')?.keys).toBe('⌘+⇧+K')
    expect(row).toHaveTextContent('⌘')
    expect(row).toHaveTextContent('⇧')
    expect(row).toHaveTextContent('K')

    fireEvent.click(row.querySelector('.shortcuts-row__clear')!)

    expect(useConfigStore.getState().shortcuts.find((shortcut) => shortcut.action === 'expand-panel')?.keys).toBe('')
    expect(row).toHaveTextContent('Off')
  })

  it('syncs global shortcut controls and rolls back failed native registration', async () => {
    tauriMocks.setGlobalActionShortcuts.mockRejectedValueOnce(new Error('already registered'))
    render(<SettingsApp onClose={vi.fn()} />)

    fireEvent.click(screen.getByText('settings.island.title'))
    fireEvent.click(screen.getByRole('button', { name: /Shortcuts/ }))

    await waitFor(() => expect(screen.getByText('Approve current permission')).toBeInTheDocument())
    const row = screen.getByText('Approve current permission').closest('.setting-row')!
    fireEvent.click(row.querySelector('[role="switch"]')!)

    expect(tauriMocks.setGlobalActionShortcuts).toHaveBeenCalledWith(expect.objectContaining({
      approve: 'CommandOrControl+Shift+A',
      approveEnabled: true,
    }))
    await waitFor(() => expect(useConfigStore.getState().shortcutApproveEnabled).toBe(false))
    expect(screen.getByRole('alert')).toHaveTextContent('already registered')
  })
})
