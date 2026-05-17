import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { ClaudeHookUiLab } from '../components/dev/ClaudeHookUiLab'
import { useSessionStore } from '../stores/sessionStore'

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
})
