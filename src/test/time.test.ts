import { describe, it, expect, vi } from 'vitest'
import { formatDuration, formatDurationShort } from '../utils/time'

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

describe('formatDurationShort', () => {
  it('formats sub-minute durations like Evolab', () => {
    expect(formatDurationShort(1)).toBe('<1m')
    expect(formatDurationShort(59)).toBe('<1m')
  })

  it('formats minutes and hours compactly', () => {
    expect(formatDurationShort(60)).toBe('1m')
    expect(formatDurationShort(125)).toBe('2m')
    expect(formatDurationShort(3900)).toBe('1h5m')
  })
})
