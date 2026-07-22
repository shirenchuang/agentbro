/* PetApp — entry point for the dedicated 'pet' Tauri window.
   Renders only the pet companion (sprite + drag + emote + summon).
   The island UI lives in the separate 'notch' window driven by App. */

import { useEffect } from 'react'
import { PetSurface } from './components/notch/PetSurface'
import { BackgroundUpdater } from './components/BackgroundUpdater'
import { useSessionStore } from './stores/sessionStore'
import { useConfigStore } from './stores/configStore'
import { isTauri } from './services/tauriApi'
import { PetVitalsLab } from './components/dev/PetVitalsLab'
import './styles/globals.css'

export function PetApp() {
  // Make the pet window's web view transparent — the underlying Tauri window
  // is already configured transparent, but globals.css sets a background.
  useEffect(() => {
    document.body.classList.add('pet-window-body')
    document.documentElement.classList.add('pet-window-body')
    return () => {
      document.body.classList.remove('pet-window-body')
      document.documentElement.classList.remove('pet-window-body')
    }
  }, [])

  const sessions = useSessionStore((s) => s.sessionList)
  const scale = useConfigStore((s) => s.islandPetScale)
  const surfaceMode = useConfigStore((s) => s.islandSurfaceMode)
  const petVitalsDebugOpen = useConfigStore((s) => s.petVitalsDebugOpen)

  return (
    <div className="pet-window-root">
      <BackgroundUpdater />
      <PetSurface sessions={sessions} scale={scale} hidden={isTauri() ? surfaceMode !== 'pet' : false} />
      {import.meta.env.DEV && petVitalsDebugOpen && <PetVitalsLab />}
    </div>
  )
}
