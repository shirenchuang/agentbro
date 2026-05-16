import { describe, expect, it } from 'vitest'
import { computePriority, PRIORITY } from '../types/priority'

const startedAt = Date.now()

describe('computePriority', () => {
  it('matches Evolab island session sorting order', () => {
    expect(computePriority({ phase: 'error', startedAt })).toBe(PRIORITY.error)
    expect(computePriority({ phase: 'waiting_approval', startedAt })).toBe(PRIORITY.attention)
    expect(computePriority({ phase: 'processing', lastToolName: 'Bash', startedAt })).toBe(PRIORITY.working)
    expect(computePriority({ phase: 'processing', startedAt })).toBe(PRIORITY.thinking)
    expect(computePriority({ phase: 'compacting', startedAt })).toBe(PRIORITY.compacting)
    expect(PRIORITY.error).toBeGreaterThan(PRIORITY.attention)
    expect(PRIORITY.working).toBeGreaterThan(PRIORITY.thinking)
    expect(PRIORITY.thinking).toBeGreaterThan(PRIORITY.compacting)
    expect(PRIORITY.compacting).toBeGreaterThan(PRIORITY.done)
  })
})
