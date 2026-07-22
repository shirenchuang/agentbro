import { useEffect, useMemo, useRef, useState } from 'react'
import { convertFileSrc } from '@tauri-apps/api/core'
import type { PetOption } from '../../types/pet'
import type { ThemeConfig } from '../../types/theme'
import type { Priority } from '../../types/priority'
import { priorityName } from '../../types/priority'

interface SpriteCanvasProps {
  pet?: PetOption | null
  theme?: ThemeConfig
  priority: Priority
  size: number
  /** Forces a specific animation row regardless of priority/idle state. Arrays are tried in order. */
  animationOverride?: string | readonly string[] | null
  /** Whether an override should keep looping instead of settling back to idle. */
  animationOverrideMode?: 'transient' | 'continuous'
  /** Toggle the idle "personality" scheduler (blink/yawn/stretch). Default true. */
  enableIdleBehaviors?: boolean
  /**
   * Override how long the surface has been idle. If unset/0, the canvas tracks
   * its own idle-since timestamp from priority transitions.
   */
  idleSinceMs?: number
  /** Context window usage 0-100. When >75 slows FPS to show strain. */
  contextPressure?: number
  /** 5h token usage 0-100. When >75 and idle, reduces idle behavior frequency. */
  energyLevel?: number
}

/** Candidate one-shot animations the idle scheduler will pick from, in priority order. */
const IDLE_BEHAVIORS = ['blink', 'stretch', 'yawn', 'waving'] as const
const IDLE_TRIGGER_DELAY_MS = 8000
const IDLE_INTERVAL_MIN_MS = 4000
const IDLE_INTERVAL_MAX_MS = 7000
const SLEEP_THRESHOLD_MS = 120000
const SLEEP_FPS_FLOOR = 2
const ACTIVE_ANIMATION_LOOPS = 3

export function SpriteCanvas({
  pet,
  theme,
  priority,
  size,
  animationOverride,
  animationOverrideMode = 'transient',
  enableIdleBehaviors = true,
  idleSinceMs,
  contextPressure = 0,
  energyLevel = 0,
}: SpriteCanvasProps) {
  const [renderedStep, setRenderedStep] = useState<RenderedStep | null>(null)
  const [atlasGrid, setAtlasGrid] = useState<AtlasGrid | null>(null)
  const [idleBehavior, setIdleBehavior] = useState<string | null>(null)
  const prefersReducedMotion = usePrefersReducedMotion()
  const pageVisible = usePageVisibility()

  const trackedIdleMs = useTrackedIdleMs(priority, pageVisible)
  const effectiveIdleSinceMs = idleSinceMs && idleSinceMs > 0 ? idleSinceMs : trackedIdleMs
  const activePet = useMemo(() => pet ?? themeToPet(theme), [pet, theme])

  const pName = priorityName(priority)
  const isIdle = pName === 'idle'
  const isSleeping = isIdle && effectiveIdleSinceMs > SLEEP_THRESHOLD_MS
  const baseAnimName = activePet ? (activePet.stateMapping[pName] ?? 'idle') : 'idle'

  const overrideAnimName = pickAnimationOverride(animationOverride, activePet)
  const activeAnimName = pickActiveAnimName({ overrideAnimName, idleBehavior, baseAnimName, pet: activePet })
  const anim = activePet?.animations[activeAnimName] ?? activePet?.animations['idle']
  const idleAnim = activePet?.animations['idle']
  const atlasKey = activePet
    ? makeAtlasKey(activePet.id, activePet.frameSize.width, activePet.frameSize.height)
    : null
  const effectiveAtlasGrid = atlasGrid?.key === atlasKey ? atlasGrid : inferAtlasGrid(activePet)
  const shouldSettleToIdle = Boolean(
    anim
    && idleAnim
    && activeAnimName !== 'idle'
    && idleBehavior === null
    && !(overrideAnimName && animationOverrideMode === 'continuous'),
  )

  useEffect(() => {
    if (!activePet?.spritesheetUrl || !activePet.frameSize.width || !activePet.frameSize.height) {
      return
    }
    const key = makeAtlasKey(activePet.id, activePet.frameSize.width, activePet.frameSize.height)
    let cancelled = false
    const img = new Image()
    img.onload = () => {
      if (cancelled) return
      const width = img.naturalWidth || img.width
      const height = img.naturalHeight || img.height
      if (!width || !height) return
      setAtlasGrid({
        key,
        columns: Math.max(1, Math.round(width / activePet.frameSize.width)),
        rows: Math.max(1, Math.round(height / activePet.frameSize.height)),
      })
    }
    img.src = activePet.spritesheetUrl
    return () => {
      cancelled = true
    }
    // Deps key off pet.id rather than the (potentially MB-long) URL so React's
    // dep comparison stays cheap and doesn't pin the string in memory.
  }, [activePet?.id, activePet?.frameSize.height, activePet?.frameSize.width, activePet?.spritesheetUrl])

  useEffect(() => {
    if (!anim || !activePet || !pageVisible) return

    let cancelled = false
    let timer: ReturnType<typeof window.setTimeout> | null = null
    let frameIndex = 0

    const isOneShot = idleBehavior !== null && activeAnimName === idleBehavior
    const render = () => {
      if (cancelled) return
      const step = getRenderStep({
        activeAnimName,
        activeAnim: anim,
        frameIndex,
        idleAnim,
        prefersReducedMotion,
        shouldSettleToIdle,
      })
      setRenderedStep(step)

      if (prefersReducedMotion) return

      frameIndex += 1
      if (isOneShot && frameIndex >= anim.frames) {
        setIdleBehavior(null)
        return
      }

      const fps = computeEffectiveFps({
        anim: step.anim,
        activeAnimName: step.animName,
        baseAnimName,
        contextPressure,
        energyLevel,
        isIdle,
        isSleeping,
      })
      timer = window.setTimeout(render, 1000 / Math.max(1, fps))
    }

    render()
    return () => {
      cancelled = true
      if (timer) window.clearTimeout(timer)
    }
  }, [
    activeAnimName,
    activePet,
    animationOverrideMode,
    anim,
    baseAnimName,
    contextPressure,
    energyLevel,
    idleAnim,
    idleBehavior,
    isIdle,
    isSleeping,
    overrideAnimName,
    pageVisible,
    prefersReducedMotion,
    shouldSettleToIdle,
  ])

  // Idle behavior scheduler — picks a random one-shot animation when idle.
  useEffect(() => {
    if (!pageVisible || !enableIdleBehaviors || !isIdle || isSleeping || animationOverride) {
      if (idleBehavior !== null) {
        const timer = window.setTimeout(() => setIdleBehavior(null), 0)
        return () => window.clearTimeout(timer)
      }
      return
    }
    if (idleBehavior !== null) return
    if (effectiveIdleSinceMs < IDLE_TRIGGER_DELAY_MS) return

    const available = IDLE_BEHAVIORS.filter((name) => activePet?.animations[name])
    if (available.length === 0) return

    const energyMultiplier = energyLevel > 90 ? 3 : energyLevel > 75 ? 2 : energyLevel > 50 ? 1.3 : 1
    const delay = (IDLE_INTERVAL_MIN_MS + Math.random() * (IDLE_INTERVAL_MAX_MS - IDLE_INTERVAL_MIN_MS)) * energyMultiplier
    const timer = window.setTimeout(() => {
      const pick = available[Math.floor(Math.random() * available.length)]
      setIdleBehavior(pick)
    }, delay)
    return () => window.clearTimeout(timer)
  }, [
    enableIdleBehaviors,
    isIdle,
    isSleeping,
    effectiveIdleSinceMs,
    animationOverride,
    idleBehavior,
    pageVisible,
    energyLevel,
    activePet,
  ])

  if (!activePet || !anim) return null

  const step = renderedStep ?? getRenderStep({
    activeAnimName,
    activeAnim: anim ?? activePet.animations['idle'],
    frameIndex: 0,
    idleAnim,
    prefersReducedMotion,
    shouldSettleToIdle,
  })
  const columns = Math.max(effectiveAtlasGrid?.columns ?? 1, step.frame + 1)
  const rows = Math.max(effectiveAtlasGrid?.rows ?? 1, step.anim.row + 1)
  const backgroundPosition = `${toBackgroundPercent(step.frame, columns)}% ${toBackgroundPercent(step.anim.row, rows)}%`
  const aspectRatio = `${activePet.frameSize.width} / ${activePet.frameSize.height}`

  return (
    <div
      className="sprite-canvas"
      data-testid="sprite-canvas"
      data-pet-animation={activeAnimName}
      data-pet-animation-mode={overrideAnimName ? animationOverrideMode : undefined}
      data-pet-rendered-animation={step.animName}
      data-pet-rendered-frame={step.frame}
      style={{
        width: size,
        aspectRatio,
        backgroundImage: `url(${activePet.spritesheetUrl})`,
        backgroundPosition,
        backgroundRepeat: 'no-repeat',
        backgroundSize: `${columns * 100}% ${rows * 100}%`,
      }}
      aria-hidden
    />
  )
}

function themeToPet(theme: ThemeConfig | undefined): PetOption | null {
  if (!theme?.character) return null
  // Theme character.spriteSheet now carries an absolute filesystem path; the
  // legacy `spriteSheetDataUrl` field is no longer populated. Convert to an
  // asset URL so the WebView can stream it instead of holding base64 bytes.
  const spriteSheet = theme.character.spriteSheetUrl ?? theme.character.spriteSheet
  if (!spriteSheet) return null
  const spritesheetUrl = spriteSheet.startsWith('data:') || spriteSheet.startsWith('asset:') || spriteSheet.startsWith('http')
    ? spriteSheet
    : convertFileSrc(spriteSheet)
  return {
    id: theme.name,
    displayName: theme.displayName ?? theme.name,
    description: theme.description,
    provider: theme.provider ?? 'agentbro',
    builtin: theme.author === 'builtin',
    spritesheetPath: spriteSheet,
    spritesheetUrl,
    frameSize: theme.character.frameSize,
    animations: theme.character.animations,
    stateMapping: theme.stateMapping ?? {},
  }
}

function pickActiveAnimName({
  overrideAnimName,
  idleBehavior,
  baseAnimName,
  pet,
}: {
  overrideAnimName: string | null
  idleBehavior: string | null
  baseAnimName: string
  pet: PetOption | null
}): string {
  if (overrideAnimName) return overrideAnimName
  if (idleBehavior && pet?.animations[idleBehavior]) return idleBehavior
  return baseAnimName
}

function pickAnimationOverride(
  animationOverride: string | readonly string[] | null | undefined,
  pet: PetOption | null,
): string | null {
  const overrides = Array.isArray(animationOverride)
    ? animationOverride
    : animationOverride
      ? [animationOverride]
      : []
  return overrides.find((name) => pet?.animations[name]) ?? null
}

function getRenderStep({
  activeAnimName,
  activeAnim,
  frameIndex,
  idleAnim,
  prefersReducedMotion,
  shouldSettleToIdle,
}: {
  activeAnimName: string
  activeAnim: NonNullable<PetOption['animations'][string]>
  frameIndex: number
  idleAnim: PetOption['animations'][string] | undefined
  prefersReducedMotion: boolean
  shouldSettleToIdle: boolean
}): RenderedStep {
  if (prefersReducedMotion) {
    return { anim: activeAnim, animName: activeAnimName, frame: 0 }
  }
  const settleAfterFrames = activeAnim.frames * ACTIVE_ANIMATION_LOOPS
  if (shouldSettleToIdle && idleAnim && frameIndex >= settleAfterFrames) {
    return {
      anim: idleAnim,
      animName: 'idle',
      frame: (frameIndex - settleAfterFrames) % idleAnim.frames,
    }
  }
  return {
    anim: activeAnim,
    animName: activeAnimName,
    frame: frameIndex % activeAnim.frames,
  }
}

function computeEffectiveFps({
  anim,
  activeAnimName,
  baseAnimName,
  contextPressure,
  energyLevel,
  isIdle,
  isSleeping,
}: {
  anim: NonNullable<PetOption['animations'][string]>
  activeAnimName: string
  baseAnimName: string
  contextPressure: number
  energyLevel: number
  isIdle: boolean
  isSleeping: boolean
}): number {
  const baseFps = anim.fps ?? 6
  const vitalsFpsFactor = computeVitalsFpsFactor(contextPressure, energyLevel, isIdle)
  if (isSleeping && activeAnimName === baseAnimName) {
    return Math.max(SLEEP_FPS_FLOOR, baseFps / 2)
  }
  return Math.max(SLEEP_FPS_FLOOR, baseFps * vitalsFpsFactor)
}

type AtlasGrid = {
  key: string
  columns: number
  rows: number
}

type RenderedStep = {
  anim: NonNullable<PetOption['animations'][string]>
  animName: string
  frame: number
}

function inferAtlasGrid(pet: PetOption | null): AtlasGrid | null {
  if (!pet) return null
  const animations = Object.values(pet.animations)
  if (animations.length === 0) return null
  return {
    key: makeAtlasKey(pet.id, pet.frameSize.width, pet.frameSize.height),
    columns: Math.max(1, ...animations.map((anim) => anim.frames)),
    rows: Math.max(1, ...animations.map((anim) => anim.row + 1)),
  }
}

// Atlas cache key — short and stable so it can sit in React deps without
// pinning large strings. Pet id + frame size uniquely identifies the layout.
function makeAtlasKey(petId: string, frameWidth: number, frameHeight: number): string {
  return `${petId}:${frameWidth}x${frameHeight}`
}

function toBackgroundPercent(index: number, count: number): number {
  if (count <= 1) return 0
  return (index / (count - 1)) * 100
}

function usePrefersReducedMotion(): boolean {
  const [prefersReducedMotion, setPrefersReducedMotion] = useState(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return false
    return window.matchMedia('(prefers-reduced-motion: reduce)').matches
  })

  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return
    const query = window.matchMedia('(prefers-reduced-motion: reduce)')
    const onChange = () => setPrefersReducedMotion(query.matches)
    query.addEventListener?.('change', onChange)
    return () => query.removeEventListener?.('change', onChange)
  }, [])

  return prefersReducedMotion
}

function usePageVisibility(): boolean {
  const [visible, setVisible] = useState(() => (
    typeof document === 'undefined' || document.visibilityState !== 'hidden'
  ))

  useEffect(() => {
    if (typeof document === 'undefined') return
    const update = () => setVisible(document.visibilityState !== 'hidden')
    document.addEventListener('visibilitychange', update)
    return () => document.removeEventListener('visibilitychange', update)
  }, [])

  return visible
}

/**
 * Tracks how long the surface has been in `idle` priority. Resets to 0 the
 * moment priority changes away from idle. Updates at 1 Hz while idle.
 */
function useTrackedIdleMs(priority: Priority, pageVisible: boolean): number {
  const startRef = useRef<number | null>(null)
  const [idleMs, setIdleMs] = useState(0)
  const isIdle = priorityName(priority) === 'idle'

  useEffect(() => {
    if (!isIdle || !pageVisible) {
      startRef.current = null
      const timer = window.setTimeout(() => setIdleMs(0), 0)
      return () => window.clearTimeout(timer)
    }
    startRef.current = Date.now()
    const resetTimer = window.setTimeout(() => setIdleMs(0), 0)
    const id = window.setInterval(() => {
      const start = startRef.current
      if (start === null) return
      setIdleMs(Date.now() - start)
    }, 1000)
    return () => {
      window.clearTimeout(resetTimer)
      window.clearInterval(id)
    }
  }, [isIdle, pageVisible])

  return idleMs
}

function computeVitalsFpsFactor(contextPressure: number, energyLevel: number, isIdle: boolean): number {
  let factor = 1
  if (contextPressure > 75) {
    factor *= 1 - (Math.min(contextPressure, 100) - 75) / 62.5
  }
  if (isIdle && energyLevel > 75) {
    factor *= 1 - (Math.min(energyLevel, 100) - 75) / 83
  }
  return Math.max(0.6, factor)
}
