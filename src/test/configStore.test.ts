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
    expect(state.dismissOnOutsideClick).toBe(true)
    expect(state.showCacheTTL).toBe(true)
    expect(state.taskCompleteDwellSeconds).toBe(10)
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
})
