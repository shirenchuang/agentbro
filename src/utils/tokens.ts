/* AgentBro — Token formatting and cost estimation utilities */
import type { TokenUsage } from '../types/agent'

export function formatTokens(n: number): string {
  if (n === 0) return '0'
  if (n < 1000) return `${n}`
  if (n < 1_000_000) return `${(n / 1000).toFixed(1)}K`
  return `${(n / 1_000_000).toFixed(2)}M`
}

export interface ModelPricing {
  label: string
  inputPer1M: number
  outputPer1M: number
  cacheReadPer1M: number
}

export const MODEL_PRICING: Record<string, ModelPricing> = {
  'claude-sonnet-4': {
    label: 'Claude Sonnet 4',
    inputPer1M: 3.0,
    outputPer1M: 15.0,
    cacheReadPer1M: 0.3,
  },
  'claude-opus-4': {
    label: 'Claude Opus 4',
    inputPer1M: 15.0,
    outputPer1M: 75.0,
    cacheReadPer1M: 1.5,
  },
  'gpt-4o': {
    label: 'GPT-4o',
    inputPer1M: 2.5,
    outputPer1M: 10.0,
    cacheReadPer1M: 1.25,
  },
  'gemini-2.5-pro': {
    label: 'Gemini 2.5 Pro',
    inputPer1M: 1.25,
    outputPer1M: 10.0,
    cacheReadPer1M: 0.315,
  },
}

export function estimateCost(tokens: TokenUsage, modelId: string): number {
  const pricing = MODEL_PRICING[modelId] ?? MODEL_PRICING['claude-sonnet-4']
  return (
    (tokens.input * pricing.inputPer1M) / 1_000_000 +
    (tokens.output * pricing.outputPer1M) / 1_000_000 +
    (tokens.cacheRead * pricing.cacheReadPer1M) / 1_000_000
  )
}

export function formatCost(cost: number): string {
  if (cost === 0) return '$0.00'
  if (cost < 0.01) return '<$0.01'
  return `$${cost.toFixed(2)}`
}
