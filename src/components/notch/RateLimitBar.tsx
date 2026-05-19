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

  const windows = rateLimits.windows && rateLimits.windows.length > 0
    ? rateLimits.windows.slice(0, 2)
    : [
      {
        id: 'five_hour',
        title: '5h',
        usedPercent: rateLimits.fiveHourUsage,
        remainingLabel: rateLimits.fiveHourRemaining,
      },
      {
        id: 'seven_day',
        title: '7d',
        usedPercent: rateLimits.sevenDayUsage,
        remainingLabel: rateLimits.sevenDayRemaining,
      },
    ]

  const source = rateLimits.providerLabel || rateLimits.provider || 'Usage'
  const sourceDetail = [rateLimits.source, rateLimits.updatedAt ? new Date(rateLimits.updatedAt).toLocaleTimeString() : null]
    .filter(Boolean)
    .join(' · ')
  const title = `${source}: ${windows.map((window) => `${window.title}: ${Math.round(window.usedPercent)}% used${window.remainingLabel ? ` (${window.remainingLabel} left)` : ''}`).join(' | ')}${sourceDetail ? ` · ${sourceDetail}` : ''}`

  return (
    <div className="rate-limit" title={title}>
      {windows.map((window, index) => (
        <span key={window.id} className={`rate-limit__segment ${usageClass(window.usedPercent)}`} style={{ color: usageColor(window.usedPercent) }}>
          {index > 0 && <span className="rate-limit__divider">|</span>}
          {window.title} {Math.round(window.usedPercent)}% {window.remainingLabel || ''}
        </span>
      ))}
    </div>
  )
}
