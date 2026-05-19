import { beforeEach, describe, expect, it } from 'vitest'
import { useConfigStore } from '../stores/configStore'

describe('configStore island defaults', () => {
  beforeEach(() => {
    localStorage.clear()
    useConfigStore.getState().resetIslandDefaults()
  })

  it('matches evolab island behavior defaults for feedback and cache display', () => {
    const state = useConfigStore.getState()

    expect(state.completionCardHeight).toBe(200)
    expect(state.detailPanelMaxHeight).toBe(500)
    expect(state.showCacheTTL).toBe(true)
    expect(state.taskCompleteDwellSeconds).toBe(6)
    expect(state.idleTimeoutMinutes).toBe(5)
    expect(state.volume).toBe(70)
  })

  it('uses tuned island interaction timing defaults', () => {
    const state = useConfigStore.getState()

    expect(state.hoverExpandDelay).toBe(50)
    expect(state.microHoverExpandDelay).toBe(50)
    expect(state.collapseDelay).toBe(200)
    expect(state.islandAnimationScale).toBe(1)
  })

  it('keeps evolab-safe tools in the default auto-approve list', () => {
    expect(useConfigStore.getState().autoApproveTools).toEqual(
      expect.arrayContaining([
        'TaskCreate',
        'TaskUpdate',
        'TaskGet',
        'TaskList',
        'TaskOutput',
        'TaskStop',
        'EnterPlanMode',
        'ExitPlanMode',
        'TodoRead',
        'TodoWrite',
        'Read',
      ]),
    )
  })

  it('keeps permission approval shortcuts opt-in by default', () => {
    const state = useConfigStore.getState()

    expect(state.shortcutApproveEnabled).toBe(false)
    expect(state.shortcutDenyEnabled).toBe(false)
    expect(state.shortcuts.find((shortcut) => shortcut.action === 'approve-action')?.keys).toBe('')
    expect(state.shortcuts.find((shortcut) => shortcut.action === 'reject-action')?.keys).toBe('')
  })
})
