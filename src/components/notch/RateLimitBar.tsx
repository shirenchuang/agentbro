/* RateLimitBar — Compact API rate limit display */
import type { RateLimitInfo } from '../../types/agent'
import './RateLimitBar.css'

interface RateLimitBarProps {
  rateLimits?: RateLimitInfo
}

function usageColor(pct: number): string {
  if (pct >= 80) return 'var(--rate-limit-red, #ef4444)'
  if (pct >= 50) return 'var(--rate-limit-amber, #f59e0b)'
  return 'var(--rate-limit-green, #22c55e)'
}

function usageClass(pct: number): string {
  if (pct >= 80) return 'rate-limit__segment--red'
  if (pct >= 50) return 'rate-limit__segment--amber'
  return 'rate-limit__segment--green'
}

export function RateLimitBar({ rateLimits }: RateLimitBarProps) {
  if (!rateLimits) return null

  const { fiveHourUsage, fiveHourRemaining, sevenDayUsage, sevenDayRemaining } = rateLimits

  return (
    <div className="rate-limit" title={`5h: ${fiveHourUsage}% used (${fiveHourRemaining} left) | 7d: ${sevenDayUsage}% used (${sevenDayRemaining} left)`}>
      <span className={`rate-limit__segment ${usageClass(fiveHourUsage)}`} style={{ color: usageColor(fiveHourUsage) }}>
        5h {fiveHourUsage}% {fiveHourRemaining}
      </span>
      <span className="rate-limit__divider">|</span>
      <span className={`rate-limit__segment ${usageClass(sevenDayUsage)}`} style={{ color: usageColor(sevenDayUsage) }}>
        7d {sevenDayUsage}% {sevenDayRemaining}
      </span>
    </div>
  )
}
