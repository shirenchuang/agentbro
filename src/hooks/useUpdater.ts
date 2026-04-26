/* Agent Island — Auto-Update Hook
 * Checks for updates on app start (with delay) and exposes manual check.
 */
import { useState, useEffect, useCallback } from 'react'
import { isTauri } from '../services/tauriApi'
import { useConfigStore } from '../stores/configStore'

export type UpdateStatus = 'idle' | 'checking' | 'available' | 'downloading' | 'ready' | 'error' | 'up-to-date'

interface UpdateState {
  status: UpdateStatus
  version: string | null
  error: string | null
}

export function useUpdater() {
  const betaUpdates = useConfigStore((s) => s.betaUpdates)
  const [state, setState] = useState<UpdateState>({
    status: 'idle',
    version: null,
    error: null,
  })

  const checkForUpdate = useCallback(async () => {
    if (!isTauri()) return

    setState(prev => ({ ...prev, status: 'checking', error: null }))

    try {
      const { check } = await import('@tauri-apps/plugin-updater')
      const update = await check({
        headers: betaUpdates ? { 'X-Update-Channel': 'beta' } : { 'X-Update-Channel': 'stable' },
      })

      if (update) {
        setState({ status: 'available', version: update.version, error: null })

        // Auto-download and install
        setState(prev => ({ ...prev, status: 'downloading' }))
        await update.downloadAndInstall()
        setState(prev => ({ ...prev, status: 'ready' }))
      } else {
        setState({ status: 'up-to-date', version: null, error: null })
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e)
      console.error('[updater] check failed:', message)
      setState({ status: 'error', version: null, error: message })
    }
  }, [betaUpdates])

  // Check for updates on app start with a 5s delay
  useEffect(() => {
    if (!isTauri()) return

    const timer = setTimeout(() => {
      checkForUpdate()
    }, 5000)

    return () => clearTimeout(timer)
  }, [checkForUpdate])

  return { ...state, checkForUpdate }
}
