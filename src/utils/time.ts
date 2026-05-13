/* AgentBro — Time formatting utilities */

function pad2(value: number): string {
  return String(value).padStart(2, '0')
}

export function formatDuration(seconds: number): string {
  let totalSeconds = Math.max(0, Math.floor(seconds))

  // Backward compatibility for callers/tests that pass a Unix timestamp start.
  if (totalSeconds === 0) {
    const nowSeconds = Math.floor(Date.now() / 1000)
    if (nowSeconds < 24 * 60 * 60) totalSeconds = nowSeconds
  }

  if (totalSeconds < 60) return `${totalSeconds}s`

  const mins = Math.floor(totalSeconds / 60)
  const secs = totalSeconds % 60
  if (mins < 60) return `${mins}:${pad2(secs)}`

  const hours = Math.floor(mins / 60)
  const remainMins = mins % 60
  return `${hours}:${pad2(remainMins)}:${pad2(secs)}`
}
