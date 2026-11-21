import { act, fireEvent, render, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { PetSurface } from '../components/notch/PetSurface'
import { useSessionStore } from '../stores/sessionStore'
import { useThemeStore } from '../stores/themeStore'
import { usePetStore } from '../stores/petStore'
import { useConfigStore } from '../stores/configStore'
import { useUpdateStore } from '../stores/updateStore'
import type { SessionState } from '../types/agent'
import type { PetOption } from '../types/pet'

const tauriMocks = vi.hoisted(() => ({
  openSettingsWindow: vi.fn(() => Promise.resolve()),
  jumpToTerminal: vi.fn(() => Promise.resolve()),
  respondPermission: vi.fn(() => Promise.resolve()),
  respondPlan: vi.fn(() => Promise.resolve()),
  respondQuestion: vi.fn(() => Promise.resolve()),
  respondAutoApprove: vi.fn(() => Promise.resolve()),
  sendMessage: vi.fn(() => Promise.resolve()),
  getChatHistory: vi.fn(() => Promise.resolve([])),
  getChatHistoryTail: vi.fn(() => Promise.resolve({ messages: [], hasMore: false, firstMessageId: null, totalCount: 0, transcriptPath: null })),
  startPetDrag: vi.fn(() => Promise.resolve(true)),
  endPetDrag: vi.fn(() => Promise.resolve(null)),
}))

vi.mock('../services/tauriApi', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../services/tauriApi')>()
  return {
    ...actual,
    openSettingsWindow: tauriMocks.openSettingsWindow,
    jumpToTerminal: tauriMocks.jumpToTerminal,
    respondPermission: tauriMocks.respondPermission,
    respondPlan: tauriMocks.respondPlan,
    respondQuestion: tauriMocks.respondQuestion,
    respondAutoApprove: tauriMocks.respondAutoApprove,
    sendMessage: tauriMocks.sendMessage,
    getChatHistory: tauriMocks.getChatHistory,
    getChatHistoryTail: tauriMocks.getChatHistoryTail,
    startPetDrag: tauriMocks.startPetDrag,
    endPetDrag: tauriMocks.endPetDrag,
  }
})

function openPetSessionDetail(container: HTMLElement, title: string) {
  fireEvent.click(container.querySelector('.pet-surface__pet') as HTMLElement)
  const titleElement = within(container).getByText(title)
  fireEvent.keyDown(titleElement.closest('[role="button"]') as HTMLElement, { key: 'Enter' })
}

function session(overrides: Partial<SessionState> = {}): SessionState {
  return {
    id: 's1',
    agentType: 'claude-code',
    project: 'agentbro',
    terminal: 'iTerm',
    phase: 'processing',
    startedAt: Date.now() - 10_000,
    duration: 10_000,
    tokens: { input: 0, output: 0, cacheRead: 0, cacheCreate: 0 },
    chatHistory: [],
    subagents: [],
    activeTools: [],
    sessionTitle: 'Build pet surface',
    description: 'Running implementation',
    pid: 1234,
    tty: '/dev/ttys001',
    ...overrides,
  }
}

function makePet(): PetOption {
  return {
    id: 'codex:test',
    displayName: 'Test Pet',
    provider: 'codex',
    builtin: true,
    spritesheetPath: '/tmp/test-pets/codex-test/spritesheet.webp',
    spritesheetUrl: 'asset://localhost/tmp/test-pets/codex-test/spritesheet.webp',
    frameSize: { width: 192, height: 208 },
    animations: {
      idle: { row: 0, frames: 1, fps: 1 },
      running: { row: 7, frames: 1, fps: 1 },
      'running-left': { row: 2, frames: 1, fps: 1 },
      'running-right': { row: 1, frames: 1, fps: 1 },
      jumping: { row: 4, frames: 1, fps: 1 },
    },
    stateMapping: { working: 'running' },
  }
}

describe('PetSurface (companion)', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useSessionStore.setState({
      sessions: {},
      sessionList: [],
      activeSessionId: null,
      panelState: 'collapsed',
      overlayQueue: [],
      activeOverlay: null,
      wakeSilencedUntil: 0,
    })
    useThemeStore.getState().loadThemes([])
    useThemeStore.getState().setActiveTheme('default')
    usePetStore.setState({ registry: [], activePetId: null })
    useConfigStore.setState({ tipsEnabled: true })
    useUpdateStore.setState({ availableVersion: null })
  })

  it('renders the pet button with sprite when an active pet is selected', () => {
    usePetStore.setState({ registry: [makePet()], activePetId: 'codex:test' })

    const { container } = render(
      <PetSurface hidden={false} scale={72} sessions={[session()]} />,
    )

    expect(container.querySelector('.pet-surface__pet')).toBeInTheDocument()
    expect(container.querySelector('.pet-surface__pet .sprite-canvas')).toBeInTheDocument()
  })

  it('shows an update badge on the pet when a newer version is available', () => {
    usePetStore.setState({ registry: [makePet()], activePetId: 'codex:test' })

    const { container, rerender } = render(
      <PetSurface hidden={false} scale={72} sessions={[session()]} />,
    )
    expect(container.querySelector('.pet-surface__update-dot')).toBeNull()

    act(() => {
      useUpdateStore.setState({ availableVersion: '0.3.0' })
    })
    rerender(<PetSurface hidden={false} scale={72} sessions={[session()]} />)
    expect(container.querySelector('.pet-surface__update-dot')).toBeInTheDocument()
    expect(container.querySelector('.pet-surface__update-label')).toBeInTheDocument()
  })

  it('falls back to MascotRouter when no pet registry is loaded', () => {
    // Mark as already attempted so the defensive loader doesn't switch into
    // loading state (which would render the placeholder instead of the mascot).
    usePetStore.setState({ registry: [], activePetId: null, loading: false, error: 'mock-no-tauri' })

    const { container } = render(
      <PetSurface hidden={false} scale={72} sessions={[session()]} />,
    )

    // mascot-image is the wrapper rendered by MascotRouter / MascotCanvas
    expect(container.querySelector('.mascot-image')).toBeInTheDocument()
  })

  it('opens settings on right-click', () => {
    const { container } = render(
      <PetSurface hidden={false} scale={72} sessions={[session()]} />,
    )

    const button = container.querySelector('.pet-surface__pet') as HTMLElement
    fireEvent.contextMenu(button)
    expect(tauriMocks.openSettingsWindow).toHaveBeenCalledOnce()
  })

  it('marks data-hidden when hidden prop is true', () => {
    const { container } = render(
      <PetSurface hidden={true} scale={72} sessions={[session()]} />,
    )
    expect(container.querySelector('.pet-surface')?.getAttribute('data-hidden')).toBe('true')
  })

  it('shows a pet tip after dwell and then hides it', () => {
    vi.useFakeTimers()
    try {
      const { container } = render(
        <PetSurface hidden={false} scale={72} sessions={[]} />,
      )

      expect(container.querySelector('.pet-surface__idle-tip')).not.toBeInTheDocument()

      act(() => {
        vi.advanceTimersByTime(1200)
      })

      expect(container.querySelector('.pet-surface__idle-tip')).toBeInTheDocument()
      expect(container.querySelector('.pet-surface__idle-tip-label')).toHaveTextContent('Tips')

      act(() => {
        vi.advanceTimersByTime(8000)
      })

      expect(container.querySelector('.pet-surface__idle-tip')).not.toBeInTheDocument()
    } finally {
      vi.useRealTimers()
    }
  })

  it('does not show pet tips when tips are disabled', () => {
    vi.useFakeTimers()
    try {
      useConfigStore.setState({ tipsEnabled: false })
      const disabled = render(
        <PetSurface hidden={false} scale={72} sessions={[]} />,
      )

      act(() => {
        vi.advanceTimersByTime(1200)
      })

      expect(disabled.container.querySelector('.pet-surface__idle-tip')).not.toBeInTheDocument()
      disabled.unmount()
    } finally {
      vi.useRealTimers()
    }
  })

  it('does not show pet tips while a session is active', () => {
    vi.useFakeTimers()
    try {
      const { container } = render(
        <PetSurface hidden={false} scale={72} sessions={[session({ phase: 'processing' })]} />,
      )

      act(() => {
        vi.advanceTimersByTime(1200)
      })

      expect(container.querySelector('.pet-surface__idle-tip')).not.toBeInTheDocument()
    } finally {
      vi.useRealTimers()
    }
  })

  it('respects a non-default islandPetScale via the --pet-scale CSS variable', () => {
    usePetStore.setState({ registry: [makePet()], activePetId: 'codex:test' })

    const { container } = render(
      <PetSurface hidden={false} scale={100} sessions={[session()]} />,
    )
    const root = container.querySelector('.pet-surface') as HTMLElement
    const sprite = container.querySelector('.pet-surface__pet .sprite-canvas') as HTMLElement

    // 100 / 100 = 1.0; clamp keeps it ≤ 1.2
    expect(root.style.getPropertyValue('--pet-scale')).toBe('1')
    expect(sprite.style.width).toBe('160px')
  })

  it('allows islandPetScale down to 10 percent', () => {
    usePetStore.setState({ registry: [makePet()], activePetId: 'codex:test' })

    const { container } = render(
      <PetSurface hidden={false} scale={10} sessions={[session()]} />,
    )
    const root = container.querySelector('.pet-surface') as HTMLElement
    const sprite = container.querySelector('.pet-surface__pet .sprite-canvas') as HTMLElement

    expect(root.style.getPropertyValue('--pet-scale')).toBe('0.1')
    expect(sprite.style.width).toBe('16px')
  })

  it('shows session badges and opens the side session drawer on pet click', () => {
    usePetStore.setState({ registry: [makePet()], activePetId: 'codex:test' })

    const { container, getByText } = render(
      <PetSurface hidden={false} scale={72} sessions={[session({ project: 'agentbro', sessionTitle: 'Build pet HUD' })]} />,
    )

    expect(container.querySelector('.pet-surface__badge--session')).toHaveTextContent('1')

    const button = container.querySelector('.pet-surface__pet') as HTMLElement
    fireEvent.click(button)

    expect(container.querySelector('.pet-surface__drawer')).toBeInTheDocument()
    expect(container.querySelector('.pet-surface__drawer')).toHaveAttribute('data-placement', 'left')
    expect(getByText('agentbro · Build pet HUD')).toBeInTheDocument()
  })

  it('lets the pet session drawer expand to every session', () => {
    useConfigStore.setState({ maxVisibleSessions: 4 })
    const sessions = Array.from({ length: 6 }, (_, index) => session({
      id: `s${index + 1}`,
      sessionTitle: `Session ${index + 1}`,
      pid: 2000 + index,
      tty: `/dev/ttys00${index}`,
    }))

    const { container, getByText, queryByText } = render(
      <PetSurface hidden={false} scale={72} sessions={sessions} />,
    )

    fireEvent.click(container.querySelector('.pet-surface__pet') as HTMLElement)

    expect(getByText('agentbro · Session 1')).toBeInTheDocument()
    expect(queryByText('agentbro · Session 6')).not.toBeInTheDocument()

    const showAll = Array.from(container.querySelectorAll('button'))
      .find((button) => /showAllSessions|Show all|显示全部/.test(button.textContent ?? ''))
    expect(showAll).toBeInTheDocument()

    fireEvent.click(showAll as HTMLElement)

    expect(getByText('agentbro · Session 6')).toBeInTheDocument()
  })

  it('keeps the last drag direction while the pointer is held still', () => {
    usePetStore.setState({ registry: [makePet()], activePetId: 'codex:test' })

    const { container } = render(
      <PetSurface hidden={false} scale={72} sessions={[session()]} />,
    )

    const button = container.querySelector('.pet-surface__pet') as HTMLElement
    fireEvent.pointerDown(button, { button: 0, pointerId: 8, clientX: 100, clientY: 100, screenX: 100 })
    fireEvent.pointerMove(button, { pointerId: 8, clientX: 90, clientY: 100, screenX: 90 })

    const sprite = container.querySelector('.pet-surface__pet .sprite-canvas') as HTMLElement
    expect(sprite).toHaveAttribute('data-pet-animation', 'running-left')
    expect(sprite).toHaveAttribute('data-pet-animation-mode', 'continuous')

    fireEvent.pointerMove(button, { pointerId: 8, clientX: 90, clientY: 100, screenX: 90 })

    expect(sprite).toHaveAttribute('data-pet-animation', 'running-left')
  })

  it('auto hides the session panel on pointer leave without hiding the pet', () => {
    vi.useFakeTimers()
    try {
      const target = session({ sessionTitle: 'Auto hide panel' })
      const { container } = render(
        <PetSurface hidden={false} scale={72} sessions={[target]} />,
      )

      fireEvent.click(container.querySelector('.pet-surface__pet') as HTMLElement)
      const panel = container.querySelector('.pet-surface__drawer--sessions') as HTMLElement
      expect(panel).toBeInTheDocument()

      fireEvent.pointerLeave(panel)
      act(() => {
        vi.advanceTimersByTime(650)
      })

      expect(container.querySelector('.pet-surface__drawer--sessions')).not.toBeInTheDocument()
      expect(container.querySelector('.pet-surface__pet')).toBeInTheDocument()
    } finally {
      vi.useRealTimers()
    }
  })

  it('closes the pet session panel on Escape without hiding the pet', () => {
    usePetStore.setState({ registry: [makePet()], activePetId: 'codex:test' })

    const { container } = render(
      <PetSurface hidden={false} scale={72} sessions={[session()]} />,
    )

    fireEvent.click(container.querySelector('.pet-surface__pet') as HTMLElement)
    expect(container.querySelector('.pet-surface__drawer--sessions')).toBeInTheDocument()

    fireEvent.keyDown(window, { key: 'Escape' })

    expect(container.querySelector('.pet-surface__drawer--sessions')).not.toBeInTheDocument()
    expect(container.querySelector('.pet-surface__pet')).toBeInTheDocument()
    expect(useSessionStore.getState().activeSessionId).toBeNull()
  })

  it('opens pet-local session detail instead of the island panel when a session row is clicked', () => {
    const target = session({ sessionTitle: 'Stay in pet mode' })
    useSessionStore.setState({ sessions: { s1: target }, sessionList: [target] })

    const { container, getByRole } = render(
      <PetSurface hidden={false} scale={72} sessions={[target]} />,
    )

    openPetSessionDetail(container, 'agentbro · Stay in pet mode')

    expect(getByRole('region', { name: 'Pet session detail' })).toBeInTheDocument()
    expect(getByRole('textbox')).toHaveAttribute('placeholder', 'notch.typeMessage')
    expect(useSessionStore.getState().panelState).toBe('collapsed')
    expect(tauriMocks.jumpToTerminal).not.toHaveBeenCalled()
  })

  it('can jump to terminal from pet-local session detail', () => {
    const target = session({ sessionTitle: 'Open me' })
    useSessionStore.setState({ sessions: { s1: target }, sessionList: [target] })

    const { container, getByLabelText } = render(
      <PetSurface hidden={false} scale={72} sessions={[target]} />,
    )

    openPetSessionDetail(container, 'agentbro · Open me')
    fireEvent.click(getByLabelText('notch.jumpToTerminal'))

    expect(tauriMocks.jumpToTerminal).toHaveBeenCalledWith('s1')
  })

  it('sends a quick reply from pet session detail', () => {
    const target = session({ sessionTitle: 'Reply target' })
    useSessionStore.setState({ sessions: { s1: target }, sessionList: [target] })

    const { container, getByLabelText, getByPlaceholderText } = render(
      <PetSurface hidden={false} scale={72} sessions={[target]} />,
    )

    openPetSessionDetail(container, 'agentbro · Reply target')
    fireEvent.change(getByPlaceholderText('notch.typeMessage'), { target: { value: 'continue please' } })
    fireEvent.mouseDown(getByLabelText('notch.send'))

    expect(tauriMocks.sendMessage).toHaveBeenCalledWith('s1', 'continue please')
  })

  it('answers pending questions through quick reply', () => {
    const pending = session({
      phase: 'waiting_input',
      pendingQuestion: {
        question: 'Pick a direction?',
        options: ['A', 'B'],
      },
    })
    useSessionStore.setState({
      sessions: { s1: pending },
      sessionList: [pending],
    })

    const { container, getByText } = render(
      <PetSurface hidden={false} scale={72} sessions={[pending]} />,
    )

    openPetSessionDetail(container, 'agentbro · Build pet surface')
    fireEvent.mouseDown(getByText('B').closest('button') as HTMLElement)

    expect(tauriMocks.respondQuestion).toHaveBeenCalledWith('s1', 'B')
  })

  it('surfaces a compact action toast for pending sessions', () => {
    const pending = session({
      phase: 'waiting_input',
      pendingQuestion: {
        question: 'Pick a direction?',
        options: ['A', 'B'],
      },
    })

    const { container, getByText } = render(
      <PetSurface hidden={false} scale={72} sessions={[pending]} />,
    )

    expect(container.querySelector('.pet-surface__toast--action')).toBeInTheDocument()
    expect(container.querySelector('.pet-surface__badge--action')).toHaveTextContent('1')
    expect(getByText('Pick a direction?')).toBeInTheDocument()
  })

  it('does not treat unfinished task summaries as pet action prompts', () => {
    const working = session({
      phase: 'processing',
      tasks: [
        {
          id: 'task-1',
          name: 'Fix content offset',
          status: 'pending',
        },
      ],
    })

    const { container } = render(
      <PetSurface hidden={false} scale={72} sessions={[working]} />,
    )

    expect(container.querySelector('.pet-surface__toast--action')).not.toBeInTheDocument()
    expect(container.querySelector('.pet-surface__badge--action')).not.toBeInTheDocument()
  })

  it('shows response overlays with the same notification panel used by island mode', () => {
    const base = session({ responseText: 'Response' })
    useSessionStore.setState({
      sessions: { s1: base },
      sessionList: [base],
      activeOverlay: {
        id: 'response-s1',
        sessionId: 's1',
        type: 'response',
        data: { responseText: 'Response' },
        createdAt: Date.now(),
      },
      overlayQueue: [],
    })

    const { container } = render(
      <PetSurface hidden={false} scale={72} sessions={[base]} />,
    )

    const toast = container.querySelector('.pet-surface__toast--message') as HTMLElement
    expect(toast).toBeInTheDocument()
    expect(toast).toHaveAttribute('data-placement', 'left')
    expect(toast.style.getPropertyValue('--pet-panel-top')).not.toBe('')
    expect(container.querySelector('.overlay-feedback--response')).toBeInTheDocument()
    expect(container.querySelector('.overlay-feedback__input')).toBeInTheDocument()
    expect(toast).toHaveTextContent('Response')
  })

  it('dismisses non-blocking pet message overlays on Escape', () => {
    const base = session({ responseText: 'Response' })
    useSessionStore.setState({
      sessions: { s1: base },
      sessionList: [base],
      activeOverlay: {
        id: 'response-s1',
        sessionId: 's1',
        type: 'response',
        data: { responseText: 'Response' },
        createdAt: Date.now(),
      },
      overlayQueue: [],
    })

    const { container } = render(
      <PetSurface hidden={false} scale={72} sessions={[base]} />,
    )

    expect(container.querySelector('.pet-surface__toast--message')).toBeInTheDocument()

    fireEvent.keyDown(window, { key: 'Escape' })

    expect(container.querySelector('.pet-surface__toast--message')).not.toBeInTheDocument()
    expect(useSessionStore.getState().activeOverlay).toBeNull()
  })

  it('renders blocking permission overlays as actionable pet prompts', () => {
    const target = session({ phase: 'processing' })
    useSessionStore.setState({
      sessions: { s1: target },
      sessionList: [target],
      activeOverlay: {
        id: 'perm-s1',
        sessionId: 's1',
        type: 'permission',
        data: { toolName: 'Bash', toolInput: '{"command":"pnpm test"}' },
        createdAt: Date.now(),
      },
      overlayQueue: [],
    })

    const { container, getByText } = render(
      <PetSurface hidden={false} scale={72} sessions={[target]} />,
    )

    expect(container.querySelector('.pet-surface__overlay')).toBeInTheDocument()
    expect(getByText('Bash')).toBeInTheDocument()

    fireEvent.click(getByText('notch.allowOnce').closest('button') as HTMLElement)

    expect(tauriMocks.respondPermission).toHaveBeenCalledWith('s1', true, undefined, undefined)
  })

  it('hides blocking pet prompts on Escape without dismissing or approving them', () => {
    const target = session({ phase: 'processing' })
    useSessionStore.setState({
      sessions: { s1: target },
      sessionList: [target],
      activeOverlay: {
        id: 'perm-s1',
        sessionId: 's1',
        type: 'permission',
        data: { toolName: 'Bash', toolInput: '{"command":"pnpm test"}' },
        createdAt: Date.now(),
      },
      overlayQueue: [],
    })

    const { container } = render(
      <PetSurface hidden={false} scale={72} sessions={[target]} />,
    )

    expect(container.querySelector('.pet-surface__overlay')).toBeInTheDocument()

    fireEvent.keyDown(window, { key: 'Escape' })

    expect(container.querySelector('.pet-surface__overlay')).not.toBeInTheDocument()
    expect(useSessionStore.getState().activeOverlay?.id).toBe('perm-s1')
    expect(tauriMocks.respondPermission).not.toHaveBeenCalled()
  })

  it('triggers pet permission approval on mouse down so desktop click-through cannot swallow it', () => {
    const target = session({ phase: 'processing' })
    useSessionStore.setState({
      sessions: { s1: target },
      sessionList: [target],
      activeOverlay: {
        id: 'perm-s1',
        sessionId: 's1',
        type: 'permission',
        data: { toolName: 'Bash', toolInput: '{"command":"pnpm test"}' },
        createdAt: Date.now(),
      },
      overlayQueue: [],
    })

    const { getByText } = render(
      <PetSurface hidden={false} scale={72} sessions={[target]} />,
    )

    fireEvent.mouseDown(getByText('notch.allowOnce').closest('button') as HTMLElement)
    fireEvent.click(getByText('notch.allowOnce').closest('button') as HTMLElement)

    expect(tauriMocks.respondPermission).toHaveBeenCalledTimes(1)
    expect(tauriMocks.respondPermission).toHaveBeenCalledWith('s1', true, undefined, undefined)
  })
})
