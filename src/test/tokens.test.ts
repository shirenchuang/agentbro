import { describe, it, expect } from 'vitest'
import { formatTokens, estimateCost, formatCost } from '../utils/tokens'

describe('formatTokens', () => {
  it('returns "0" for zero', () => {
    expect(formatTokens(0)).toBe('0')
  })

  it('formats small numbers as-is', () => {
    expect(formatTokens(500)).toBe('500')
  })

  it('formats thousands with K suffix', () => {
    expect(formatTokens(1500)).toBe('1.5K')
  })

  it('formats millions with M suffix', () => {
    expect(formatTokens(2_500_000)).toBe('2.50M')
  })
})

describe('estimateCost', () => {
  it('calculates cost for claude-sonnet-4', () => {
    const tokens = { input: 1_000_000, output: 100_000, cacheRead: 0, cacheCreate: 0 }
    const cost = estimateCost(tokens, 'claude-sonnet-4')
    expect(cost).toBeCloseTo(3.0 + 1.5, 5)
  })

  it('falls back to sonnet pricing for unknown model', () => {
    const tokens = { input: 1_000_000, output: 0, cacheRead: 0, cacheCreate: 0 }
    expect(estimateCost(tokens, 'unknown-model')).toBeCloseTo(3.0, 5)
  })
})

describe('formatCost', () => {
  it('formats zero', () => {
    expect(formatCost(0)).toBe('$0.00')
  })

  it('shows <$0.01 for tiny costs', () => {
    expect(formatCost(0.005)).toBe('<$0.01')
  })

  it('formats normal costs', () => {
    expect(formatCost(1.234)).toBe('$1.23')
  })
})
