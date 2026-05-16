import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { PetSurface } from '../components/notch/PetSurface'
import { useSessionStore } from '../stores/sessionStore'
import { useThemeStore } from '../stores/themeStore'
import type { OverlayItem, SessionState } from '../types/agent'
import type { ThemeConfig } from '../types/theme'

const tauriMocks = vi.hoisted(() => ({
  respondPlan: vi.fn(() => Promise.resolve()),
}))

vi.mock('../services/tauriApi', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../services/tauriApi')>()
  return {
    ...actual,
    respondPlan: tauriMocks.respondPlan,
  }
})

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, fallback?: string | { defaultValue?: string }, opts?: Record<string, unknown>) => {
      if (typeof fallback === 'string') return fallback.replace('{{count}}', String(opts?.count ?? ''))
      return fallback?.defaultValue ?? key
    },
  }),
}))

function session(overrides: Partial<SessionState> = {}): SessionState {
  return {
    id: 's1',
    agentType: 'claude-code',
    project: 'agent-island',
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
    ...overrides,
  }
}

describe('PetSurface', () => {
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
  })

  it('renders the pet session fan and opens session detail', () => {
    render(
      <PetSurface
        activeOverlay={null}
        expanded
        hidden={false}
        onCollapse={vi.fn()}
        onDismissOverlay={vi.fn()}
        scale={72}
        sessions={[session()]}
      />,
    )

    fireEvent.click(screen.getByText('Build pet surface'))

    expect(screen.getByText('返回')).toBeInTheDocument()
    expect(screen.getByPlaceholderText('回复...')).toBeInTheDocument()
  })

  it('renders a permission action toast', () => {
    const overlay: OverlayItem = {
      id: 'o1',
      sessionId: 's1',
      type: 'permission',
      data: { toolName: 'Edit', toolInput: 'file_path: src/App.tsx' },
      createdAt: Date.now(),
    }

    render(
      <PetSurface
        activeOverlay={overlay}
        expanded={false}
        hidden={false}
        onCollapse={vi.fn()}
        onDismissOverlay={vi.fn()}
        scale={72}
        sessions={[session({ phase: 'waiting_approval' })]}
      />,
    )

    expect(screen.getByText('需要授权')).toBeInTheDocument()
    expect(screen.getByText('始终允许')).toBeInTheDocument()
  })

  it('does not render the session fan behind an active overlay toast', () => {
    const overlay: OverlayItem = {
      id: 'o1',
      sessionId: 's1',
      type: 'permission',
      data: { toolName: 'Edit' },
      createdAt: Date.now(),
    }

    render(
      <PetSurface
        activeOverlay={overlay}
        expanded={false}
        hidden={false}
        onCollapse={vi.fn()}
        onDismissOverlay={vi.fn()}
        scale={72}
        sessions={[session({ phase: 'waiting_approval' })]}
      />,
    )

    expect(screen.getByText('需要授权')).toBeInTheDocument()
    expect(screen.queryByText('Build pet surface')).not.toBeInTheDocument()
    expect(screen.queryByText('Running implementation')).not.toBeInTheDocument()
  })

  it('uses the active Codex pet theme sprite when selected', () => {
    const petTheme: ThemeConfig = {
      name: 'codex-pet:test',
      version: '1.0.0',
      author: 'user',
      provider: 'codex',
      isCodexPet: true,
      displayName: 'Test Pet',
      pixelGrid: { cols: 5, rows: 5 },
      priorityColors: {},
      prioritySpeeds: {},
      priorityPatterns: {},
      character: {
        spriteSheet: 'data:image/webp;base64,AAAA',
        frameSize: { width: 192, height: 208 },
        scale: 1,
        animations: { idle: { row: 0, frames: 1, fps: 1 }, running: { row: 7, frames: 1, fps: 1 } },
      },
      stateMapping: { working: 'running' },
      sounds: { pack: '8bit' },
    }
    useThemeStore.getState().loadThemes([petTheme])
    useThemeStore.getState().setActiveTheme('codex-pet:test')

    const { container } = render(
      <PetSurface
        activeOverlay={null}
        expanded
        hidden={false}
        onCollapse={vi.fn()}
        onDismissOverlay={vi.fn()}
        scale={72}
        sessions={[session()]}
      />,
    )

    expect(container.querySelector('.pet-surface__pet canvas')).toBeInTheDocument()
  })

  it('keeps session cards hidden until the island state expands', () => {
    render(
      <PetSurface
        activeOverlay={null}
        expanded={false}
        hidden={false}
        onCollapse={vi.fn()}
        onDismissOverlay={vi.fn()}
        scale={72}
        sessions={[session()]}
      />,
    )

    expect(screen.queryByText('Build pet surface')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Open pet status' }))
    expect(screen.getByText('返回')).toBeInTheDocument()
  })

  it('clears plan state after a pet plan approval action', async () => {
    const currentSession = session({
      phase: 'waiting_approval',
      planTitle: 'Implementation plan',
      planContent: '1. Fix pet approval',
      planPermissions: ['Edit'],
    })
    const overlay: OverlayItem = {
      id: 'plan-s1',
      sessionId: 's1',
      type: 'plan',
      data: {
        planTitle: 'Implementation plan',
        planContent: '1. Fix pet approval',
        requestedPermissions: [{ tool: 'Edit', prompt: 'Update source' }],
      },
      createdAt: Date.now(),
    }
    const onDismissOverlay = vi.fn()
    useSessionStore.setState({
      sessions: { s1: currentSession },
      sessionList: [currentSession],
      overlayQueue: [overlay],
      activeOverlay: overlay,
    })

    render(
      <PetSurface
        activeOverlay={overlay}
        expanded={false}
        hidden={false}
        onCollapse={vi.fn()}
        onDismissOverlay={onDismissOverlay}
        scale={72}
        sessions={[currentSession]}
      />,
    )

    fireEvent.click(screen.getByText('接受编辑'))

    expect(tauriMocks.respondPlan).toHaveBeenCalledWith('s1', 'acceptEdits')
    await waitFor(() => expect(useSessionStore.getState().sessions.s1.planContent).toBeUndefined())
    expect(onDismissOverlay).toHaveBeenCalledWith('plan-s1')
  })
})
