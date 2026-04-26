import { useConfigStore } from '../stores/configStore'

export function isQuietHours(): boolean {
  const { quietHours } = useConfigStore.getState()
  if (!quietHours.enabled) return false

  const now = new Date()
  const currentMinutes = now.getHours() * 60 + now.getMinutes()

  const [startH, startM] = quietHours.start.split(':').map(Number)
  const [endH, endM] = quietHours.end.split(':').map(Number)
  const startMinutes = startH * 60 + startM
  const endMinutes = endH * 60 + endM

  if (startMinutes <= endMinutes) {
    return currentMinutes >= startMinutes && currentMinutes < endMinutes
  }
  // Wraps midnight (e.g., 22:00 → 08:00)
  return currentMinutes >= startMinutes || currentMinutes < endMinutes
}
