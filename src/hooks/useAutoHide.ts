/* Agent Island — Auto-hide notch when no sessions are active.
 * Uses opacity instead of window.hide() because macOS breaks
 * transparent window compositing after a hide()/show() cycle.
 */
import { useEffect, useRef } from 'react'
import { useSessionStore, selectSessionList } from '../stores/sessionStore'
import { useConfigStore } from '../stores/configStore'
import { setNotchOpacity } from '../services/tauriApi'

/**
 * When `autoHideNoSessions` is enabled and there are zero sessions,
 * fade the notch out after a 1-second delay. Fade back in immediately
 * when sessions become non-empty.
 */
export function useAutoHide() {
  const sessions = useSessionStore(selectSessionList)
  const autoHideNoSessions = useConfigStore((s) => s.autoHideNoSessions)
  const timerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const hiddenRef = useRef(false)

  useEffect(() => {
    // Clear any pending timer on dependency change
    if (timerRef.current) {
      clearTimeout(timerRef.current)
      timerRef.current = undefined
    }

    if (!autoHideNoSessions) {
      // Feature disabled — make sure notch is visible
      if (hiddenRef.current) {
        setNotchOpacity(1)
        hiddenRef.current = false
      }
      return
    }

    if (sessions.length === 0) {
      // Start 1-second timer before fading out
      timerRef.current = setTimeout(() => {
        setNotchOpacity(0)
        hiddenRef.current = true
      }, 1000)
    } else {
      // Sessions exist — show immediately
      if (hiddenRef.current) {
        setNotchOpacity(1)
        hiddenRef.current = false
      }
    }

    return () => {
      if (timerRef.current) {
        clearTimeout(timerRef.current)
        timerRef.current = undefined
      }
    }
  }, [sessions.length, autoHideNoSessions])
}
