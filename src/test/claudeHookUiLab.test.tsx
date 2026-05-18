import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { ClaudeHookUiLab } from '../components/dev/ClaudeHookUiLab'
import { useSessionStore } from '../stores/sessionStore'
import { useThemeStore } from '../stores/themeStore'

function renderLab() {
  render(
    <ClaudeHookUiLab>
      <div data-testid="stage" />
    </ClaudeHookUiLab>,
  )
}

const RECORDED_SESSION_ID = 'e336326d-ab6f-4002-bc6a-3ba66892b469'

describe('ClaudeHookUiLab', () => {
  beforeEach(() => {
    useSessionStore.setState({
      sessions: {},
      sessionList: [],
      activeSessionId: null,
      panelState: 'collapsed',
      baseLayer: 'compact',
      overlayQueue: [],
      activeOverlay: null,
      rateLimits: undefined,
      hookNotification: null,
      wakeSilencedUntil: 0,
      focusedTerminal: null,
    })
    useThemeStore.getState().setColorTheme('ink-amber')
  })

  afterEach(() => {
    cleanup()
  })

  it('loads PermissionRequest as a backend-style pending permission snapshot', async () => {
    renderLab()

    fireEvent.click(screen.getByText('Permission').closest('button')!)

    await waitFor(() => {
      const state = useSessionStore.getState()
      expect(state.sessions['claude-hook-lab']?.pendingPermission?.toolName).toBe('Edit')
      expect(state.activeOverlay?.type).toBe('permission')
      expect(state.panelState).toBe('hover')
    })
  })

  it('switches to AskUserQuestion with pendingQuestion and a question overlay', async () => {
    renderLab()

    fireEvent.click(screen.getByText('Question'))

    await waitFor(() => {
      const state = useSessionStore.getState()
      expect(state.sessions['claude-hook-lab']?.phase).toBe('waiting_input')
      expect(state.sessions['claude-hook-lab']?.pendingQuestion?.question).toContain('Where should Claude')
      expect(state.activeOverlay?.type).toBe('question')
    })
  })

  it('detail mode clears only the overlay layer while preserving hook metadata', async () => {
    renderLab()

    fireEvent.click(screen.getByText('Permission').closest('button')!)
    fireEvent.click(screen.getByText('Detail'))

    await waitFor(() => {
      const state = useSessionStore.getState()
      expect(state.sessions['claude-hook-lab']?.pendingPermission).toBeDefined()
      expect(state.activeOverlay).toBeNull()
      expect(state.panelState).toBe('expanded')
    })
  })

  it('represents Stop as a done session with the same completion overlay path as Tauri updates', async () => {
    renderLab()

    fireEvent.click(screen.getAllByText('Stop')[0].closest('button')!)

    await waitFor(() => {
      const state = useSessionStore.getState()
      expect(state.sessions['claude-hook-lab']?.phase).toBe('done')
      expect(state.sessions['claude-hook-lab']?.responseText).toContain('Updated token refresh flow')
      expect(state.activeOverlay?.type).toBe('completion')
      expect(state.panelState).toBe('collapsed')
    })
  })

  it('replays the real Edit permission checkpoint from the recorded Claude Code session', async () => {
    renderLab()

    fireEvent.click(screen.getByText('Real Edit Wait').closest('button')!)

    await waitFor(() => {
      const state = useSessionStore.getState()
      const session = state.sessions[RECORDED_SESSION_ID]
      expect(session?.phase).toBe('waiting_approval')
      expect(session?.pendingPermission?.toolName).toBe('Edit')
      expect(session?.pendingPermission?.diff?.filePath).toContain('src/auth/middleware.ts')
      expect(state.activeOverlay?.type).toBe('permission')
    })
  })

  it('replays the real multi-select AskUserQuestion checkpoint', async () => {
    renderLab()

    fireEvent.click(screen.getByText('Real Multi Q').closest('button')!)

    await waitFor(() => {
      const state = useSessionStore.getState()
      const session = state.sessions[RECORDED_SESSION_ID]
      expect(session?.phase).toBe('waiting_input')
      expect(session?.pendingQuestion?.multiSelect).toBe(true)
      expect(session?.pendingQuestion?.options).toContain('权限详情卡')
      expect(state.activeOverlay?.type).toBe('question')
    })
  })

  it('replays the real compact checkpoint without inventing an overlay', async () => {
    renderLab()

    fireEvent.click(screen.getByText('Real Compact').closest('button')!)

    await waitFor(() => {
      const state = useSessionStore.getState()
      const session = state.sessions[RECORDED_SESSION_ID]
      expect(session?.phase).toBe('compacting')
      expect(session?.lastToolName).toBe('Compacting')
      expect(state.activeOverlay).toBeNull()
    })
  })

  it('switches the lab color appearance through the shared theme store', async () => {
    renderLab()

    fireEvent.click(screen.getByTitle('午夜 · Evolab'))

    await waitFor(() => {
      expect(useThemeStore.getState().colorTheme).toBe('midnight')
      expect(document.documentElement.getAttribute('data-island-color-theme')).toBe('midnight')
    })
  })

  it('loads the empty island state without sessions or overlays', async () => {
    renderLab()

    fireEvent.click(screen.getByText('Empty').closest('button')!)

    await waitFor(() => {
      const state = useSessionStore.getState()
      expect(state.sessionList).toHaveLength(0)
      expect(state.activeSessionId).toBeNull()
      expect(state.activeOverlay).toBeNull()
      expect(state.panelState).toBe('collapsed')
    })
  })

  it('loads a non-blocking response overlay scenario', async () => {
    renderLab()

    fireEvent.click(screen.getByText('Response').closest('button')!)

    await waitFor(() => {
      const state = useSessionStore.getState()
      expect(state.sessions['claude-hook-lab']?.phase).toBe('processing')
      expect(state.sessions['claude-hook-lab']?.responseText).toContain('response overlay')
      expect(state.activeOverlay?.type).toBe('response')
    })
  })

  it('loads the blocking overlay priority queue scenario', async () => {
    renderLab()

    fireEvent.click(screen.getByText('Queue').closest('button')!)

    await waitFor(() => {
      const state = useSessionStore.getState()
      expect(state.sessionList).toHaveLength(3)
      expect(state.overlayQueue.map((overlay) => overlay.type)).toEqual(['permission', 'plan', 'question'])
      expect(state.activeOverlay?.type).toBe('permission')
    })
  })

  it('loads a mixed multi-session island scenario', async () => {
    renderLab()

    fireEvent.click(screen.getByText('Multi Mix').closest('button')!)

    await waitFor(() => {
      const state = useSessionStore.getState()
      expect(state.sessionList).toHaveLength(5)
      expect(state.sessions['lab-multi-processing']?.phase).toBe('processing')
      expect(state.sessions['lab-multi-error']?.phase).toBe('error')
      expect(state.activeOverlay?.type).toBe('permission')
    })
  })
})
