import { describe, it, expect } from 'vitest'

// Isolated helper extracted from quietHours logic for unit testing
// (avoids needing a live Zustand store in tests)
function isInQuietWindow(
  currentH: number,
  currentM: number,
  startStr: string,
  endStr: string,
): boolean {
  const currentMinutes = currentH * 60 + currentM
  const [startH, startM] = startStr.split(':').map(Number)
  const [endH, endM] = endStr.split(':').map(Number)
  const startMinutes = startH * 60 + startM
  const endMinutes = endH * 60 + endM

  if (startMinutes <= endMinutes) {
    return currentMinutes >= startMinutes && currentMinutes < endMinutes
  }
  // Wraps midnight
  return currentMinutes >= startMinutes || currentMinutes < endMinutes
}

describe('quiet hours window logic', () => {
  it('returns true when current time is inside normal window', () => {
    expect(isInQuietWindow(10, 0, '09:00', '17:00')).toBe(true)
  })

  it('returns false when current time is outside normal window', () => {
    expect(isInQuietWindow(8, 59, '09:00', '17:00')).toBe(false)
  })

  it('returns false at exact end boundary (exclusive)', () => {
    expect(isInQuietWindow(17, 0, '09:00', '17:00')).toBe(false)
  })

  it('handles midnight-wrapping window — time after start', () => {
    expect(isInQuietWindow(23, 0, '22:00', '08:00')).toBe(true)
  })

  it('handles midnight-wrapping window — time before end', () => {
    expect(isInQuietWindow(7, 30, '22:00', '08:00')).toBe(true)
  })

  it('handles midnight-wrapping window — time outside', () => {
    expect(isInQuietWindow(10, 0, '22:00', '08:00')).toBe(false)
  })

  it('returns true at exact start boundary (inclusive)', () => {
    expect(isInQuietWindow(9, 0, '09:00', '17:00')).toBe(true)
  })
})
