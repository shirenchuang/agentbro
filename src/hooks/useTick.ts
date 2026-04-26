import { useState, useEffect } from 'react'

export function useTick(intervalMs = 1000, enabled = true) {
  const [, setTick] = useState(0)
  useEffect(() => {
    if (!enabled) return
    const id = setInterval(() => setTick(t => t + 1), intervalMs)
    return () => clearInterval(id)
  }, [intervalMs, enabled])
}
