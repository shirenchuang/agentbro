import { describe, expect, it } from 'vitest'
import type { OverlayItem, SessionState } from '../types/agent'
import { deriveIslandInteraction, getFollowFocusVisibleSessions } from '../utils/islandInteraction'

function session(overrides: Partial<SessionState>): SessionState {
  return {
    id: 's1',
    agentType: 'codex',
    project: 'agent-island',
    terminal: 'Terminal',
    phase: 'idle',
    startedAt: 0,
    duration: 0,
    tokens: { input: 0, output: 0, cacheRead: 0, cacheCreate: 0 },
    chatHistory: [],
    subagents: [],
    activeTools: [],
    ...overrides,
  }
}

function overlay(type: OverlayItem['type']): OverlayItem {
  return {
    id: `${type}-1`,
    sessionId: 's1',
    type,
    data: {},
    createdAt: 0,
  }
}

describe('deriveIslandInteraction', () => {
  it('keeps minimal mode hidden during ordinary processing', () => {
    const state = deriveIslandInteraction({
      sessions: [session({ phase: 'processing' })],
      panelState: 'collapsed',
      activeOverlay: null,
      interactionMode: 'minimal',
      persistentIdleHidden: false,
      wakeSilenced: false,
    })

    expect(state.outerState).toBe('hidden')
    expect(state.hasRunningSession).toBe(true)
  })

  it('shows compact state in minimal mode for blocking requests', () => {
    const state = deriveIslandInteraction({
      sessions: [session({ phase: 'waiting_approval' })],
      panelState: 'collapsed',
      activeOverlay: overlay('permission'),
      interactionMode: 'minimal',
      persistentIdleHidden: false,
      wakeSilenced: false,
    })

    expect(state.outerState).toBe('compact')
    expect(state.hasBlockingSignal).toBe(true)
  })

  it('shows micro state in persistent mode for idle sessions', () => {
    const state = deriveIslandInteraction({
      sessions: [session({ phase: 'idle' })],
      panelState: 'collapsed',
      activeOverlay: null,
      interactionMode: 'persistent',
      persistentIdleHidden: false,
      wakeSilenced: false,
    })

    expect(state.outerState).toBe('micro')
    expect(state.isMicro).toBe(true)
  })

  it('uses compact state in persistent mode while work is running', () => {
    const state = deriveIslandInteraction({
      sessions: [session({ phase: 'processing' })],
      panelState: 'collapsed',
      activeOverlay: null,
      interactionMode: 'persistent',
      persistentIdleHidden: false,
      wakeSilenced: false,
    })

    expect(state.outerState).toBe('compact')
  })

  it('lets ESC silence hide persistent mode while work is running', () => {
    const state = deriveIslandInteraction({
      sessions: [session({ phase: 'processing' })],
      panelState: 'collapsed',
      activeOverlay: null,
      interactionMode: 'persistent',
      persistentIdleHidden: false,
      wakeSilenced: true,
    })

    expect(state.outerState).toBe('hidden')
    expect(state.isHidden).toBe(true)
  })

  it('keeps blocking requests visible while ESC silence is active', () => {
    const state = deriveIslandInteraction({
      sessions: [session({ phase: 'waiting_input' })],
      panelState: 'collapsed',
      activeOverlay: overlay('question'),
      interactionMode: 'persistent',
      persistentIdleHidden: false,
      wakeSilenced: true,
    })

    expect(state.outerState).toBe('compact')
    expect(state.hasBlockingSignal).toBe(true)
  })

  it('hides persistent mode after the idle hide timer fires', () => {
    const state = deriveIslandInteraction({
      sessions: [session({ phase: 'done' })],
      panelState: 'collapsed',
      activeOverlay: null,
      interactionMode: 'persistent',
      persistentIdleHidden: true,
      wakeSilenced: false,
    })

    expect(state.outerState).toBe('hidden')
    expect(state.isHidden).toBe(true)
  })

  it('treats an expanded panel as expanded regardless of mode', () => {
    const state = deriveIslandInteraction({
      sessions: [],
      panelState: 'hover',
      activeOverlay: null,
      interactionMode: 'minimal',
      persistentIdleHidden: true,
      wakeSilenced: true,
    })

    expect(state.outerState).toBe('expanded')
  })
})

describe('getFollowFocusVisibleSessions', () => {
  it('returns all sessions when follow focus is disabled or not ready', () => {
    const sessions = [
      session({ id: 'focused', pid: 100, terminal: 'iTerm2' }),
      session({ id: 'other', pid: 200, terminal: 'Terminal' }),
    ]

    expect(getFollowFocusVisibleSessions(sessions, false, new Set(['focused'])).map((s) => s.id)).toEqual(['focused', 'other'])
    expect(getFollowFocusVisibleSessions(sessions, true, null).map((s) => s.id)).toEqual(['focused', 'other'])
  })

  it('keeps sessions that cannot be associated with a terminal focus target', () => {
    const sessions = [
      session({ id: 'focused', pid: 100, terminal: 'iTerm2' }),
      session({ id: 'unfocused', pid: 200, terminal: 'Terminal' }),
      session({ id: 'internal', pid: undefined, terminal: 'AgentBro' }),
      session({ id: 'unknown-terminal', pid: 300, terminal: '' }),
    ]

    expect(getFollowFocusVisibleSessions(sessions, true, new Set(['focused'])).map((s) => s.id)).toEqual([
      'focused',
      'internal',
      'unknown-terminal',
    ])
  })
})
