import { describe, it, expect, vi } from 'vitest'
import { formatDuration } from '../utils/time'

describe('formatDuration', () => {
  it('formats seconds only', () => {
    vi.spyOn(Date, 'now').mockReturnValue(1000 * 30)
    expect(formatDuration(0)).toBe('30s')
    vi.restoreAllMocks()
  })

  it('formats minutes and seconds', () => {
    vi.spyOn(Date, 'now').mockReturnValue(1000 * 125)
    expect(formatDuration(0)).toBe('2:05')
    vi.restoreAllMocks()
  })

  it('formats hours, minutes, and seconds', () => {
    vi.spyOn(Date, 'now').mockReturnValue(1000 * 3661)
    expect(formatDuration(0)).toBe('1:01:01')
    vi.restoreAllMocks()
  })
})
