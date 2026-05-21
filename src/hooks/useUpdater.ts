import { useState, useEffect, useCallback, useRef } from 'react'
import { isTauri } from '../services/tauriApi'
import { useConfigStore } from '../stores/configStore'

export type UpdateStatus = 'idle' | 'checking' | 'available' | 'downloading' | 'ready' | 'error' | 'up-to-date'

interface UpdateState {
  status: UpdateStatus
  version: string | null
  notes: string | null
  date: string | null
  error: string | null
}

export function useUpdater() {
  const betaUpdates = useConfigStore((s) => s.betaUpdates)
  const updateRef = useRef<any>(null)
  const [state, setState] = useState<UpdateState>({
    status: 'idle',
    version: null,
    notes: null,
    date: null,
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
        updateRef.current = update
        setState({
          status: 'available',
          version: update.version,
          notes: update.body ?? null,
          date: update.date ?? null,
          error: null,
        })
      } else {
        updateRef.current = null
        setState({ status: 'up-to-date', version: null, notes: null, date: null, error: null })
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e)
      console.error('[updater] check failed:', message)
      setState({ status: 'error', version: null, notes: null, date: null, error: message })
    }
  }, [betaUpdates])

  const installUpdate = useCallback(async () => {
    const update = updateRef.current
    if (!update) return

    setState(prev => ({ ...prev, status: 'downloading' }))
    try {
      await update.downloadAndInstall()
      setState(prev => ({ ...prev, status: 'ready' }))
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e)
      console.error('[updater] install failed:', message)
      setState(prev => ({ ...prev, status: 'error', error: message }))
    }
  }, [])

  const dismissUpdate = useCallback(() => {
    updateRef.current = null
    setState({ status: 'idle', version: null, notes: null, date: null, error: null })
  }, [])

  useEffect(() => {
    if (!isTauri()) return

    const timer = setTimeout(() => {
      checkForUpdate()
    }, 5000)

    return () => clearTimeout(timer)
  }, [checkForUpdate])

  return { ...state, checkForUpdate, installUpdate, dismissUpdate }
}
