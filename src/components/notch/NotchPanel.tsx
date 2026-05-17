/* AgentBro — Notch Panel (Layered Dynamic Island) */
import { useCallback, useEffect, useLayoutEffect, useRef, useState, useMemo, type CSSProperties, type PointerEvent } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import { useSessionStore, selectSessionList, selectPanelState, selectRateLimits, selectActiveOverlay } from '../../stores/sessionStore'
import { useConfigStore } from '../../stores/configStore'
import { respondPermission, respondQuestion, respondPlan, sendMessage, jumpToTerminal, resizeNotch, setNotchOpacity, getChatHistory, performHaptic, setNotchFocusable, setNotchIgnoreCursorEvents, openSettingsWindow, startNotchDrag, endNotchDrag, isCursorOverNotch, isTerminalFocused, isFrontmostAppFullscreen, isTauri } from '../../services/tauriApi'
import { mapParsedMessages } from '../../hooks/useTauri'
import { computePriority } from '../../types/priority'
import type { OverlayItem, PanelState } from '../../types/agent'
import { deriveIslandInteraction, getFollowFocusVisibleSessions, isBlockingOverlay, isNonBlockingOverlay, sessionNeedsAttention } from '../../utils/islandInteraction'
import { getCollapsedIslandHeight } from '../../utils/islandLayout'
import { CollapsedBar } from './CollapsedBar'
import { HoverList } from './HoverList'
import { ChatView } from './ChatView'
import { PermissionCard } from '../overlay/PermissionCard'
import { PlanApprovalCard } from '../overlay/PlanApprovalCard'
import { QuestionCard } from '../overlay/QuestionCard'
import { OverlayResponseCard } from '../overlay/OverlayResponseCard'
import { OverlayCompletionCard } from '../overlay/OverlayCompletionCard'
import { Confetti } from './Confetti'
import { PixelCursor } from './PixelCursor'
import { PetSurface } from './PetSurface'
import './NotchPanel.css'

const openMorphTransition = {
  type: 'tween' as const,
  duration: 0.5,
  ease: [0.16, 1, 0.3, 1] as const,
}

const closeMorphTransition = {
  type: 'tween' as const,
  duration: 0.5,
  ease: [0.2, 0, 0, 1] as const,
}

const contentTransition = {
  duration: 0.2,
  ease: 'easeOut' as const,
}

const NOTCH_SHELL_SIDE_EXTENSION = 14
const NOTCH_HIT_SLOP_X_COLLAPSED = 48
const NOTCH_HIT_SLOP_Y_COLLAPSED = 24
const NOTCH_HIT_SLOP_X_EXPANDED = 14
const NOTCH_HIT_SLOP_Y_EXPANDED = 12
const DRAG_START_THRESHOLD_PX = 4
const OPEN_NATIVE_PREPARE_FALLBACK_MS = 120
const CLOSE_NATIVE_RESIZE_DELAY_MS = 520

function nativeHostResizeKey(width: number, height: number, horizontalOffset: number, displayId?: string): string {
  return `${width.toFixed(2)}:${height.toFixed(2)}:${horizontalOffset.toFixed(2)}:${displayId ?? ''}`
}

function scaleTransitionDuration<T extends { duration?: number }>(transition: T, scale: number): T {
  return typeof transition.duration === 'number'
    ? { ...transition, duration: transition.duration * scale }
    : transition
}

function isDragIgnoredTarget(target: EventTarget | null): boolean {
  return target instanceof Element && Boolean(
    target.closest('button, a, input, select, textarea, [role="button"], [data-no-drag]'),
  )
}

function buildNotchShellClipPath(width: number, height: number, state: string, sideExtension: number): string {
  const micro = state === 'micro'
  const compact = state === 'compact'
  const shoulderDepth = micro ? 5 : compact ? 6 : 14
  const baseRadius = micro ? 14 : compact ? 14 : 22
  const left = sideExtension
  const right = width - sideExtension
  const bottomRadius = Math.min(baseRadius, (right - left) / 4, height / 2)
  const k = 0.62
  const r = (n: number) => Math.round(n * 10) / 10

  return `path('M0,0 L${r(width)},0 C${r(width - sideExtension * 0.65)},0 ${r(right)},${r(shoulderDepth * 0.35)} ${r(right)},${shoulderDepth} L${r(right)},${r(height - bottomRadius)} C${r(right)},${r(height - bottomRadius * (1 - k))} ${r(right - bottomRadius * (1 - k))},${height} ${r(right - bottomRadius)},${height} L${r(left + bottomRadius)},${height} C${r(left + bottomRadius * (1 - k))},${height} ${left},${r(height - bottomRadius * (1 - k))} ${left},${r(height - bottomRadius)} L${left},${shoulderDepth} C${left},${r(shoulderDepth * 0.35)} ${r(sideExtension * 0.65)},0 0,0 Z')`
}

function OverlayRenderer({ overlay, onDismiss }: { overlay: OverlayItem; onDismiss: () => void }) {
  const session = useSessionStore((s) => s.sessions[overlay.sessionId])
  if (!session) return null

  switch (overlay.type) {
    case 'permission':
      return (
        <PermissionCard
          overlay={overlay}
          session={session}
          onAllow={() => { respondPermission(session.id, true); useSessionStore.getState().clearPermission(session.id) }}
          onAllowAlways={() => { respondPermission(session.id, true, true); useSessionStore.getState().clearPermission(session.id) }}
          onDeny={(message?: string) => {
            if (message) sendMessage(session.id, message)
            respondPermission(session.id, false)
            useSessionStore.getState().clearPermission(session.id)
          }}
          onDismiss={onDismiss}
        />
      )
    case 'plan':
      return (
        <PlanApprovalCard
          overlay={overlay}
          session={session}
          onSendFeedback={(msg) => { respondPlan(session.id, 'feedback', msg); useSessionStore.getState().clearPlan(session.id); onDismiss() }}
          onManualReview={() => { respondPlan(session.id, 'manual'); useSessionStore.getState().clearPlan(session.id); onDismiss() }}
          onAcceptEdits={() => { respondPlan(session.id, 'acceptEdits'); useSessionStore.getState().clearPlan(session.id); onDismiss() }}
          onAutoApprove={() => { respondPlan(session.id, 'bypassPermissions'); useSessionStore.getState().clearPlan(session.id); onDismiss() }}
          onDismiss={onDismiss}
        />
      )
    case 'question':
      return (
        <QuestionCard
          overlay={overlay}
          session={session}
          onAnswer={(answer) => { respondQuestion(session.id, answer); useSessionStore.getState().clearQuestion(session.id) }}
          onDismiss={onDismiss}
        />
      )
    case 'response':
      return (
        <OverlayResponseCard
          overlay={overlay}
          session={session}
          onJumpToTerminal={() => jumpToTerminal(session.id)}
          onDismiss={onDismiss}
        />
      )
    case 'completion':
      return (
        <OverlayCompletionCard
          overlay={overlay}
          session={session}
          onJumpToTerminal={() => jumpToTerminal(session.id)}
          onDismiss={onDismiss}
        />
      )
    default:
      return null
  }
}

type IslandLayoutPreview = {
  mode: 'micro' | 'compact' | 'expanded' | 'completion'
  collapsedWidthScale?: number
  microPillWidth?: number
  compactPillWidth?: number
  panelMaxWidth?: number
  notchHeightMode?: 'matchNotch' | 'matchMenuBar' | 'custom'
  customNotchHeight?: number
  contentFontSize?: string
  completionCardHeight?: number
  maxPanelHeight?: number
}

function LayoutPreviewBody({ mode }: { mode: IslandLayoutPreview['mode'] }) {
  const contentFontSize = useConfigStore((s) => s.contentFontSize)

  if (mode === 'completion') {
    return (
      <div className="layout-preview layout-preview--completion" style={{ '--preview-content-font-size': contentFontSize } as CSSProperties}>
        <div className="layout-preview__eyebrow">Task Complete</div>
        <div className="layout-preview__title">Codex finished running tests</div>
        <div className="layout-preview__meta">agentBro · npm run test:run · now</div>
      </div>
    )
  }

  if (mode === 'expanded') {
    return (
      <div className="layout-preview layout-preview--expanded" style={{ '--preview-content-font-size': contentFontSize } as CSSProperties}>
        {Array.from({ length: 4 }).map((_, index) => (
          <div className="layout-preview__row" key={index}>
            <span className="layout-preview__dot" />
            <div className="layout-preview__copy">
              <strong>Preview session {index + 1}</strong>
              <span>Adjusting island dimensions updates this panel.</span>
            </div>
            <code>{index + 2}m</code>
          </div>
        ))}
      </div>
    )
  }

  return null
}

export function NotchPanel() {
  const panelState = useSessionStore(selectPanelState)
  const setPanelState = useSessionStore((s) => s.setPanelState)
  const sessions = useSessionStore(selectSessionList)
  const rateLimits = useSessionStore(selectRateLimits)
  const activeOverlay = useSessionStore(selectActiveOverlay)
  const dismissOverlay = useSessionStore((s) => s.dismissOverlay)
  const dwellDuration = useConfigStore((s) => s.dwellDuration)
  const notchStyle = useConfigStore((s) => s.notchStyle)
  const maxPanelHeight = useConfigStore((s) => s.maxPanelHeight)
  const autoCollapse = useConfigStore((s) => s.autoCollapse)
  const shortcuts = useConfigStore((s) => s.shortcuts)
  const hoverExpandDelay = useConfigStore((s) => s.hoverExpandDelay)
  const microHoverExpandDelay = useConfigStore((s) => s.microHoverExpandDelay)
  const collapseDelay = useConfigStore((s) => s.collapseDelay)
  const clickToDetail = useConfigStore((s) => s.clickToDetail)
  const islandEnabled = useConfigStore((s) => s.islandEnabled)
  const islandMonitorSubagents = useConfigStore((s) => s.islandMonitorSubagents)
  const autoHideNoSessions = useConfigStore((s) => s.autoHideNoSessions)
  const hideInFullscreen = useConfigStore((s) => s.hideInFullscreen)
  const idleCompactDwellSeconds = useConfigStore((s) => s.idleCompactDwellSeconds)
  const noSessionsHideDelay = useConfigStore((s) => s.noSessionsHideDelay)
  const idleTimeoutMinutes = useConfigStore((s) => s.idleTimeoutMinutes)
  const escSilenceDuration = useConfigStore((s) => s.escSilenceDuration)
  const interactionMode = useConfigStore((s) => s.interactionMode)
  const taskCompleteDwellSeconds = useConfigStore((s) => s.taskCompleteDwellSeconds)
  const confettiEnabled = useConfigStore((s) => s.confettiEnabled)
  const pixelCursorEnabled = useConfigStore((s) => s.pixelCursorEnabled)
  const islandSurfaceMode = useConfigStore((s) => s.islandSurfaceMode)
  const islandPetScale = useConfigStore((s) => s.islandPetScale)
  const islandAnimationScaleValue = useConfigStore((s) => s.islandAnimationScale)
  const islandAnimationScale = Math.max(0.1, islandAnimationScaleValue || 1)
  const followFocus = useConfigStore((s) => s.followFocus)
  const wakeSilencedUntil = useSessionStore((s) => s.wakeSilencedUntil)
  const setWakeSilencedUntil = useSessionStore((s) => s.setWakeSilencedUntil)
  const applyIdleTimeout = useSessionStore((s) => s.applyIdleTimeout)
  const leaveTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const idleHideTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const overlayDismissTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const alertContentRef = useRef<HTMLDivElement | null>(null)
  const feedbackContentRef = useRef<HTMLDivElement | null>(null)
  const [persistentIdleHidden, setPersistentIdleHidden] = useState(false)
  const [displayChanging, setDisplayChanging] = useState(false)
  const [isDragging, setIsDragging] = useState(false)
  const [layoutPreview, setLayoutPreview] = useState<IslandLayoutPreview | null>(null)
  const [measuredAlertContentHeight, setMeasuredAlertContentHeight] = useState(0)
  const [measuredFeedbackContentHeight, setMeasuredFeedbackContentHeight] = useState(0)
  const [focusedSessionIds, setFocusedSessionIds] = useState<Set<string> | null>(null)
  const [preparingOpen, setPreparingOpen] = useState(false)
  const [keepCompactAfterActive, setKeepCompactAfterActive] = useState(false)
  const dragPointerIdRef = useRef<number | null>(null)
  const dragCandidateRef = useRef<{ pointerId: number; startX: number; startY: number } | null>(null)
  const nativeHoverHitboxSizeRef = useRef({ width: 420, height: 52 })

  useEffect(() => {
    if (idleTimeoutMinutes <= 0) return
    applyIdleTimeout()
    const timer = window.setInterval(() => applyIdleTimeout(), 2000)
    return () => window.clearInterval(timer)
  }, [applyIdleTimeout, idleTimeoutMinutes])

  useEffect(() => {
    if (!followFocus) {
      setFocusedSessionIds(null)
      return
    }
    let cancelled = false
    let inFlight = false
    const refreshFocusedSessions = async () => {
      if (cancelled || inFlight) return
      inFlight = true
      try {
        const entries = await Promise.all(
          sessions.map(async (session) => {
            if (!session.pid || !session.terminal) return [session.id, true] as const
            try {
              return [session.id, await isTerminalFocused(session.id)] as const
            } catch {
              return [session.id, false] as const
            }
          }),
        )
        if (!cancelled) {
          const nextFocused = new Set(entries.filter(([, focused]) => focused).map(([id]) => id))
          setFocusedSessionIds(!isTauri() && nextFocused.size === 0 ? null : nextFocused)
        }
      } finally {
        inFlight = false
      }
    }

    refreshFocusedSessions()
    const interval = window.setInterval(refreshFocusedSessions, 1000)
    return () => {
      cancelled = true
      window.clearInterval(interval)
    }
  }, [followFocus, sessions])

  const visibleSessions = useMemo(
    () => getFollowFocusVisibleSessions(sessions, followFocus, focusedSessionIds),
    [focusedSessionIds, followFocus, sessions],
  )
  const displayedSessions = useMemo(() => (
    islandMonitorSubagents
      ? visibleSessions
      : visibleSessions.map((session) => ({ ...session, subagents: [] }))
  ), [islandMonitorSubagents, visibleSessions])
  const focusFilteredEmpty = followFocus && focusedSessionIds !== null && sessions.length > 0 && visibleSessions.length === 0

  const hasActiveSession = useMemo(
    () => sessions.some((session) => session.phase === 'processing' || session.phase === 'compacting' || sessionNeedsAttention(session)),
    [sessions],
  )

  useEffect(() => {
    if (hasActiveSession) {
      setPersistentIdleHidden(false)
      setKeepCompactAfterActive(true)
      return
    }

    if (idleCompactDwellSeconds <= 0) {
      setKeepCompactAfterActive(false)
      return
    }

    const timer = window.setTimeout(() => {
      setKeepCompactAfterActive(false)
    }, idleCompactDwellSeconds * 1000)

    return () => window.clearTimeout(timer)
  }, [hasActiveSession, idleCompactDwellSeconds])

  // Display-change listener: fade-out → pause → fade-in
  useEffect(() => {
    if (!isTauri()) return
    let unlisten: (() => void) | undefined
    let fadeInTimer: ReturnType<typeof setTimeout> | undefined

    import('@tauri-apps/api/event').then(({ listen }) => {
      listen('display-changed', () => {
        setDisplayChanging(true)
        fadeInTimer = setTimeout(() => setDisplayChanging(false), 600)
      }).then((fn) => { unlisten = fn })
    }).catch(() => {})

    return () => {
      unlisten?.()
      if (fadeInTimer) clearTimeout(fadeInTimer)
    }
  }, [])

  // Settings-window layout preview parity with evolab: temporarily morph the
  // island while size and mode controls are adjusted.
  useEffect(() => {
    if (!isTauri()) return
    let unlistenPreview: (() => void) | undefined
    let unlistenClear: (() => void) | undefined

    import('@tauri-apps/api/event').then(({ listen }) => {
      listen<IslandLayoutPreview>('island-layout-preview', (event) => {
        const store = useConfigStore.getState()
        const next = event.payload
        if (typeof next.collapsedWidthScale === 'number') store.updateConfig('collapsedWidthScale', next.collapsedWidthScale)
        if (typeof next.microPillWidth === 'number') store.updateConfig('microPillWidth', next.microPillWidth)
        if (typeof next.compactPillWidth === 'number') store.updateConfig('compactPillWidth', next.compactPillWidth)
        if (typeof next.panelMaxWidth === 'number') store.updateConfig('panelMaxWidth', next.panelMaxWidth)
        if (next.notchHeightMode === 'matchNotch' || next.notchHeightMode === 'matchMenuBar' || next.notchHeightMode === 'custom') {
          store.updateConfig('notchHeightMode', next.notchHeightMode)
        }
        if (typeof next.customNotchHeight === 'number') store.updateConfig('customNotchHeight', next.customNotchHeight)
        if (typeof next.contentFontSize === 'string') store.updateConfig('contentFontSize', next.contentFontSize)
        if (typeof next.completionCardHeight === 'number') store.updateConfig('completionCardHeight', next.completionCardHeight)
        if (typeof next.maxPanelHeight === 'number') store.updateConfig('maxPanelHeight', next.maxPanelHeight)
        setLayoutPreview(event.payload)
        setNotchOpacity(1).catch(() => {})
      }).then((fn) => { unlistenPreview = fn }).catch(() => {})
      listen('island-layout-preview-clear', () => {
        setLayoutPreview(null)
      }).then((fn) => { unlistenClear = fn }).catch(() => {})
    }).catch(() => {})

    return () => {
      unlistenPreview?.()
      unlistenClear?.()
    }
  }, [])

  useEffect(() => {
    if (!isTauri()) return
    let unlisten: (() => void) | undefined

    import('@tauri-apps/api/event').then(({ listen }) => {
      listen('tray-open-agentbro', () => {
        detailModeRef.current = false
        detailBackGuardUntilRef.current = 0
        nativeHoverInsideRef.current = true
        interactionLockUntilRef.current = Date.now() + 700
        if (leaveTimerRef.current) {
          clearTimeout(leaveTimerRef.current)
          leaveTimerRef.current = undefined
        }
        if (expandTimerRef.current) {
          clearTimeout(expandTimerRef.current)
          expandTimerRef.current = undefined
        }
        if (pendingDetailOpenTimerRef.current) {
          clearTimeout(pendingDetailOpenTimerRef.current)
          pendingDetailOpenTimerRef.current = undefined
        }
        setPersistentIdleHidden(false)
        setWakeSilencedUntil(0)
        setNotchOpacity(1).catch(() => {})
        useSessionStore.getState().setActiveSession(null)
        setPanelState('hover')
      }).then((fn) => { unlisten = fn }).catch(() => {})
    }).catch(() => {})

    return () => {
      unlisten?.()
    }
  }, [setPanelState, setWakeSilencedUntil])

  // Track sessions needing blocking attention.
  const blockingAttentionCount = useMemo(
    () => sessions.filter(sessionNeedsAttention).length,
    [sessions],
  )

  const activePriority = useMemo(
    () => visibleSessions.reduce((max, s) => Math.max(max, computePriority(s)), 0),
    [visibleSessions],
  )

  const interaction = useMemo(() => deriveIslandInteraction({
    sessions,
    panelState,
    activeOverlay,
    interactionMode,
    persistentIdleHidden,
    keepCompactAfterActive,
    wakeSilenced: Date.now() < wakeSilencedUntil,
  }), [sessions, panelState, activeOverlay, interactionMode, persistentIdleHidden, keepCompactAfterActive, wakeSilencedUntil])

  // Auto-expand on new blocking attention, and collapse shortly after it resolves.
  const prevAttentionRef = useRef(0)
  const prevBlockingOverlayIdRef = useRef<string | null>(null)
  const collapseTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  useEffect(() => {
    if (collapseTimerRef.current) {
      clearTimeout(collapseTimerRef.current)
      collapseTimerRef.current = undefined
    }

    const blockingOverlayId = activeOverlay && isBlockingOverlay(activeOverlay) ? activeOverlay.id : null
    const previousAttentionCount = prevAttentionRef.current
    const hasNewAttention = blockingAttentionCount > prevAttentionRef.current
    const hasNewBlockingOverlay = Boolean(blockingOverlayId && blockingOverlayId !== prevBlockingOverlayIdRef.current)
    const suppressedBlockingOverlay = Boolean(activeOverlay?.suppressed && activeOverlay && isBlockingOverlay(activeOverlay))
    if ((hasNewAttention || hasNewBlockingOverlay) && !suppressedBlockingOverlay && panelState === 'collapsed') {
      setNotchOpacity(1).catch(() => {})
      setPanelState('hover')
    }

    if (blockingAttentionCount === 0 && previousAttentionCount > 0 && panelState === 'hover') {
      collapseTimerRef.current = setTimeout(() => {
        const current = useSessionStore.getState().panelState
        if (current === 'hover') setPanelState('collapsed')
      }, 800)
    }

    prevAttentionRef.current = blockingAttentionCount
    prevBlockingOverlayIdRef.current = blockingOverlayId

    return () => {
      if (collapseTimerRef.current) clearTimeout(collapseTimerRef.current)
    }
  }, [activeOverlay, blockingAttentionCount, panelState, setPanelState])

  // Persistent mode hides only after the configured no-active-session delay.
  useEffect(() => {
    if (idleHideTimerRef.current) {
      clearTimeout(idleHideTimerRef.current)
      idleHideTimerRef.current = undefined
    }

    if (
      !autoHideNoSessions
      || interactionMode !== 'persistent'
      || interaction.hasActiveSession
      || keepCompactAfterActive
      || activeOverlay
      || panelState !== 'collapsed'
    ) {
      setPersistentIdleHidden(false)
      return
    }

    idleHideTimerRef.current = setTimeout(() => {
      setPersistentIdleHidden(true)
    }, noSessionsHideDelay * 60 * 1000)

    return () => {
      if (idleHideTimerRef.current) clearTimeout(idleHideTimerRef.current)
    }
  }, [activeOverlay, autoHideNoSessions, interaction.hasActiveSession, interactionMode, keepCompactAfterActive, noSessionsHideDelay, panelState])

  useEffect(() => {
    // Keep the native transparent window alive for hover hit-testing. Minimal
    // mode hides the visual shell with CSS opacity; setting the whole NSWindow
    // to alpha 0 makes later pointer entry unreliable on macOS.
    setNotchOpacity(islandEnabled ? 1 : 0).catch(() => {})
  }, [islandEnabled])

  useEffect(() => {
    if (!hideInFullscreen || !islandEnabled || !isTauri()) {
      setNotchOpacity(islandEnabled ? 1 : 0).catch(() => {})
      return
    }

    let cancelled = false
    let inFlight = false
    const refresh = async () => {
      if (cancelled || inFlight) return
      inFlight = true
      try {
        const fullscreen = await isFrontmostAppFullscreen()
        if (!cancelled) setNotchOpacity(fullscreen ? 0 : 1).catch(() => {})
      } catch {
        if (!cancelled) setNotchOpacity(1).catch(() => {})
      } finally {
        inFlight = false
      }
    }

    refresh()
    const timer = window.setInterval(refresh, 1000)
    return () => {
      cancelled = true
      window.clearInterval(timer)
    }
  }, [hideInFullscreen, islandEnabled])

  // Non-blocking overlays still need to expire while the island is collapsed.
  useEffect(() => {
    if (overlayDismissTimerRef.current) {
      clearTimeout(overlayDismissTimerRef.current)
      overlayDismissTimerRef.current = undefined
    }
    if (!activeOverlay || !isNonBlockingOverlay(activeOverlay) || panelState !== 'collapsed') return

    overlayDismissTimerRef.current = setTimeout(() => {
      useSessionStore.getState().dismissOverlay(activeOverlay.id)
    }, Math.max(1, taskCompleteDwellSeconds) * 1000)

    return () => {
      if (overlayDismissTimerRef.current) clearTimeout(overlayDismissTimerRef.current)
    }
  }, [activeOverlay, panelState, taskCompleteDwellSeconds])

  const hapticOnHover = useConfigStore((s) => s.hapticOnHover)
  const hapticIntensity = useConfigStore((s) => s.hapticIntensity)

  const expandTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const openPrepareTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const openPrepareFrameRef = useRef<number | undefined>(undefined)
  const nativeHoverInsideRef = useRef(false)
  const desiredIgnoreCursorEventsRef = useRef(false)
  const appliedIgnoreCursorEventsRef = useRef(false)
  const forceIgnoreCursorEventsRef = useRef(false)
  const ignoreCursorEventsInFlightRef = useRef(false)
  const lastNativeHostResizeKeyRef = useRef<string | null>(null)
  const inFlightNativeHostResizeKeysRef = useRef(new Set<string>())
  const nativeHostResizeWaitersRef = useRef(new Map<string, Array<(anchorOffsetX: number) => void>>())
  const interactionLockUntilRef = useRef(0)
  const detailModeRef = useRef(false)
  const detailBackGuardUntilRef = useRef(0)
  const pendingDetailOpenTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  const flushNativeIgnoreCursorEvents = useCallback(function flushNativeIgnoreCursorEvents() {
    if (!isTauri() || ignoreCursorEventsInFlightRef.current) return

    const desiredIgnore = desiredIgnoreCursorEventsRef.current
    const force = forceIgnoreCursorEventsRef.current
    if (!force && appliedIgnoreCursorEventsRef.current === desiredIgnore) return

    forceIgnoreCursorEventsRef.current = false
    ignoreCursorEventsInFlightRef.current = true
    setNotchIgnoreCursorEvents(desiredIgnore)
      .then(() => {
        appliedIgnoreCursorEventsRef.current = desiredIgnore
      })
      .catch(() => {
        // Leave the applied state unchanged so the next request can retry.
      })
      .finally(() => {
        ignoreCursorEventsInFlightRef.current = false
        if (
          forceIgnoreCursorEventsRef.current
          || appliedIgnoreCursorEventsRef.current !== desiredIgnoreCursorEventsRef.current
        ) {
          flushNativeIgnoreCursorEvents()
        }
      })
  }, [])

  const requestNativeIgnoreCursorEvents = useCallback((ignore: boolean, options?: { force?: boolean }) => {
    if (!isTauri()) return
    desiredIgnoreCursorEventsRef.current = ignore
    if (options?.force) {
      forceIgnoreCursorEventsRef.current = true
    }
    flushNativeIgnoreCursorEvents()
  }, [flushNativeIgnoreCursorEvents])

  const focusNotchForHover = useCallback(() => {
    setNotchFocusable(true).catch(() => {})
  }, [])

  const setAlertContentNode = useCallback((node: HTMLDivElement | null) => {
    alertContentRef.current = node
    if (!node) return
    window.requestAnimationFrame(() => {
      const nextHeight = Math.ceil(node.getBoundingClientRect().height || node.scrollHeight)
      if (nextHeight > 0) setMeasuredAlertContentHeight(nextHeight)
    })
  }, [])

  const setFeedbackContentNode = useCallback((node: HTMLDivElement | null) => {
    feedbackContentRef.current = node
    if (!node) return
    window.requestAnimationFrame(() => {
      const nextHeight = Math.ceil(node.getBoundingClientRect().height || node.scrollHeight)
      if (nextHeight > 0) setMeasuredFeedbackContentHeight(nextHeight)
    })
  }, [])

  const finishPreparedOpen = useCallback(() => {
    if (openPrepareTimerRef.current) {
      clearTimeout(openPrepareTimerRef.current)
      openPrepareTimerRef.current = undefined
    }
    if (openPrepareFrameRef.current != null) {
      window.cancelAnimationFrame(openPrepareFrameRef.current)
      openPrepareFrameRef.current = undefined
    }
    const current = useSessionStore.getState().panelState
    const silenced = useSessionStore.getState().isWakeSilenced()
    if (current === 'collapsed' && !silenced && nativeHoverInsideRef.current) {
      setPanelState('hover')
    }
    setPreparingOpen(false)
  }, [setPanelState])

  const showHoverPanel = useCallback(() => {
    focusNotchForHover()
    if (openPrepareTimerRef.current) {
      clearTimeout(openPrepareTimerRef.current)
      openPrepareTimerRef.current = undefined
    }
    if (openPrepareFrameRef.current != null) {
      window.cancelAnimationFrame(openPrepareFrameRef.current)
      openPrepareFrameRef.current = undefined
    }

    if (useSessionStore.getState().panelState !== 'collapsed') {
      setPreparingOpen(false)
      setPanelState('hover')
      return
    }

    setPreparingOpen(true)
    openPrepareTimerRef.current = setTimeout(finishPreparedOpen, OPEN_NATIVE_PREPARE_FALLBACK_MS * islandAnimationScale)
  }, [finishPreparedOpen, focusNotchForHover, islandAnimationScale, setPanelState])

  // Mouse enter
  const handleMouseEnter = useCallback(() => {
    if (!islandEnabled) return
    const wakeSilenced = useSessionStore.getState().isWakeSilenced()
    if (leaveTimerRef.current) {
      clearTimeout(leaveTimerRef.current)
      leaveTimerRef.current = undefined
    }
    if (wakeSilenced) return
    nativeHoverInsideRef.current = true
    if (panelState !== 'collapsed') {
      focusNotchForHover()
      return
    }
    if (panelState === 'collapsed') {
      const delay = interaction.isMicro ? microHoverExpandDelay : hoverExpandDelay
      if (delay > 0) {
        expandTimerRef.current = setTimeout(() => {
          const current = useSessionStore.getState().panelState
          const silenced = useSessionStore.getState().isWakeSilenced()
          if (current === 'collapsed' && !silenced) {
            showHoverPanel()
            if (hapticOnHover) performHaptic(hapticIntensity).catch(() => {})
          }
        }, delay)
      } else {
        showHoverPanel()
        if (hapticOnHover) performHaptic(hapticIntensity).catch(() => {})
      }
    }
  }, [focusNotchForHover, hapticIntensity, hapticOnHover, hoverExpandDelay, interaction.isMicro, islandEnabled, microHoverExpandDelay, panelState, showHoverPanel])

  // Mouse leave
  const handleMouseLeave = useCallback(() => {
    if (Date.now() < interactionLockUntilRef.current) return
    nativeHoverInsideRef.current = false
    if (isDragging) return
    if (expandTimerRef.current) {
      clearTimeout(expandTimerRef.current)
      expandTimerRef.current = undefined
    }
    if (openPrepareTimerRef.current) {
      clearTimeout(openPrepareTimerRef.current)
      openPrepareTimerRef.current = undefined
    }
    if (openPrepareFrameRef.current != null) {
      window.cancelAnimationFrame(openPrepareFrameRef.current)
      openPrepareFrameRef.current = undefined
    }
    setPreparingOpen(false)
    if (!autoCollapse) return
    const currentPanelState = useSessionStore.getState().panelState
    if (currentPanelState === 'hover' || currentPanelState === 'expanded') {
      const delay = collapseDelay > 0 ? collapseDelay : dwellDuration
      leaveTimerRef.current = setTimeout(() => {
        const current = useSessionStore.getState().panelState
        if (current === 'hover' || current === 'expanded') {
          detailModeRef.current = false
          detailBackGuardUntilRef.current = 0
          setNotchFocusable(false).catch(() => {})
          setPanelState('collapsed')
        }
      }, delay)
    }
  }, [autoCollapse, collapseDelay, dwellDuration, isDragging, setPanelState])

  useEffect(() => {
    return () => {
      if (leaveTimerRef.current) clearTimeout(leaveTimerRef.current)
      if (expandTimerRef.current) clearTimeout(expandTimerRef.current)
      if (openPrepareTimerRef.current) clearTimeout(openPrepareTimerRef.current)
      if (openPrepareFrameRef.current != null) window.cancelAnimationFrame(openPrepareFrameRef.current)
      if (pendingDetailOpenTimerRef.current) clearTimeout(pendingDetailOpenTimerRef.current)
    }
  }, [])

  useEffect(() => {
    if (!isTauri()) return

    let cancelled = false
    let inFlight = false

    const tick = async () => {
      if (cancelled || inFlight || isDragging) return
      inFlight = true
      try {
        const { width, height } = nativeHoverHitboxSizeRef.current
        const isOver = await isCursorOverNotch(width, height)
        if (cancelled) return
        const currentPanelState = useSessionStore.getState().panelState
        const overlayKeepsWindowInteractive = Boolean(activeOverlay && isNonBlockingOverlay(activeOverlay) && !interaction.isHidden)
        const ignoreTransparentHost = !islandEnabled
          || (
            islandSurfaceMode !== 'pet'
            && !isDragging
            && !isOver
            && !preparingOpen
            && currentPanelState === 'collapsed'
            && !overlayKeepsWindowInteractive
          )
        requestNativeIgnoreCursorEvents(ignoreTransparentHost)
        const wasOver = nativeHoverInsideRef.current
        if (isOver && !wasOver) {
          requestNativeIgnoreCursorEvents(false, { force: true })
          handleMouseEnter()
        } else if (!isOver && wasOver) {
          handleMouseLeave()
        }
      } catch {
        // Pointer events remain the primary path if the native probe is not
        // available during startup or on non-macOS platforms.
      } finally {
        inFlight = false
      }
    }

    const interval = window.setInterval(tick, 100)
    tick()

    return () => {
      cancelled = true
      requestNativeIgnoreCursorEvents(false)
      window.clearInterval(interval)
    }
  }, [activeOverlay, handleMouseEnter, handleMouseLeave, interaction.isHidden, islandEnabled, islandSurfaceMode, isDragging, preparingOpen, requestNativeIgnoreCursorEvents])

  useEffect(() => {
    if (!isTauri() || islandSurfaceMode === 'pet') return
    const overlayNeedsInteraction = Boolean(activeOverlay && !interaction.isHidden)
    const shouldForceInteractive = islandEnabled
      && (isDragging || preparingOpen || panelState !== 'collapsed' || overlayNeedsInteraction)
    if (!shouldForceInteractive) return
    requestNativeIgnoreCursorEvents(false, { force: true })
  }, [activeOverlay, interaction.isHidden, islandEnabled, islandSurfaceMode, isDragging, panelState, preparingOpen, requestNativeIgnoreCursorEvents])

  // Keyboard shortcuts
  useEffect(() => {
    function matchesShortcut(e: KeyboardEvent, shortcut: { keys: string }): boolean {
      const parts = shortcut.keys.split('+').map(p => p.trim())
      const needsMeta = parts.includes('\u2318')
      const needsShift = parts.includes('Shift')
      const key = parts.filter(p => p !== '\u2318' && p !== 'Shift')[0] || ''
      return e.key.toLowerCase() === key.toLowerCase()
        && !!e.metaKey === needsMeta
        && !!e.shiftKey === needsShift
    }

    function findShortcut(action: string) {
      return shortcuts.find(s => s.action === action)
    }

    const handler = (e: KeyboardEvent) => {
      const store = useSessionStore.getState()

      // Progressive ESC
      if (e.key === 'Escape') {
        const overlay = store.activeOverlay
        if (overlay) {
          if (overlay.type === 'completion' || overlay.type === 'response') {
            store.dismissOverlay(overlay.id)
          } else {
            detailModeRef.current = false
            setNotchFocusable(false).catch(() => {})
            setPanelState('collapsed')
          }
        } else if (store.panelState === 'expanded') {
          detailModeRef.current = false
          setNotchFocusable(false).catch(() => {})
          setPanelState('hover')
        } else if (store.panelState === 'hover') {
          detailModeRef.current = false
          setNotchFocusable(false).catch(() => {})
          setPanelState('collapsed')
        } else if (store.panelState === 'collapsed') {
          const escDuration = useConfigStore.getState().escSilenceDuration
          useSessionStore.getState().setWakeSilencedUntil(Date.now() + escDuration * 1000)
        }
        return
      }

      // Collapse panel shortcut
      const collapseBinding = findShortcut('collapse-panel')
      if (collapseBinding && matchesShortcut(e, collapseBinding)) {
        setWakeSilencedUntil(Date.now() + escSilenceDuration * 1000)
        detailModeRef.current = false
        setNotchFocusable(false).catch(() => {})
        setPanelState('collapsed')
        return
      }

      const active = store.activeSessionId ? store.sessions[store.activeSessionId] : null

      // Approve action
      const approveBinding = findShortcut('approve-action')
      if (approveBinding && matchesShortcut(e, approveBinding) && active?.phase === 'waiting_approval') {
        e.preventDefault()
        respondPermission(active.id, true)
        useSessionStore.getState().clearPermission(active.id)
        return
      }

      // Reject action
      const rejectBinding = findShortcut('reject-action')
      if (rejectBinding && matchesShortcut(e, rejectBinding) && active?.phase === 'waiting_approval') {
        e.preventDefault()
        respondPermission(active.id, false)
        useSessionStore.getState().clearPermission(active.id)
        return
      }

      // Open settings
      const settingsBinding = findShortcut('open-settings')
      if (settingsBinding && matchesShortcut(e, settingsBinding)) {
        e.preventDefault()
        openSettingsWindow().catch((error) => console.warn('[notch] openSettingsWindow:', error))
        return
      }

      // Jump to terminal
      if (e.metaKey && e.key === 'j') {
        e.preventDefault()
        const sid = store.activeSessionId
        if (sid) jumpToTerminal(sid)
        return
      }

      // Cmd+1/2/3 — select option
      const num = parseInt(e.key, 10)
      if (e.metaKey && num >= 1 && num <= 3) {
        const options = active?.pendingQuestion?.options
        if (options && options[num - 1]) {
          e.preventDefault()
          sendMessage(active.id, options[num - 1])
          useSessionStore.getState().clearQuestion(active.id)
        }
      }
    }

    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [escSilenceDuration, setPanelState, setWakeSilencedUntil, shortcuts])

  const handleSessionClick = (sessionId: string) => {
    if (!clickToDetail) {
      setNotchFocusable(false).catch(() => {})
      jumpToTerminal(sessionId).catch((error) => console.warn('[notch] jumpToTerminal:', error))
      return
    }
    if (pendingDetailOpenTimerRef.current) {
      clearTimeout(pendingDetailOpenTimerRef.current)
      pendingDetailOpenTimerRef.current = undefined
    }
    requestNativeIgnoreCursorEvents(false, { force: true })
    setNotchFocusable(true).catch(() => {})
    detailModeRef.current = true
    const now = Date.now()
    interactionLockUntilRef.current = now + 1200
    detailBackGuardUntilRef.current = now + 550
    nativeHoverInsideRef.current = true
    if (leaveTimerRef.current) {
      clearTimeout(leaveTimerRef.current)
      leaveTimerRef.current = undefined
    }
    if (expandTimerRef.current) {
      clearTimeout(expandTimerRef.current)
      expandTimerRef.current = undefined
    }

    pendingDetailOpenTimerRef.current = setTimeout(() => {
      pendingDetailOpenTimerRef.current = undefined
      detailModeRef.current = true
      nativeHoverInsideRef.current = true
      useSessionStore.getState().setActiveSession(sessionId)
      setPanelState('expanded')
    }, 0)

    getChatHistory(sessionId)
      .then((parsed) => {
        if (parsed.length > 0) {
          const messages = mapParsedMessages(parsed)
          useSessionStore.getState().setChatHistory(sessionId, messages)
        }
      })
      .catch((e) => console.warn('[notch] getChatHistory:', e))
  }

  const handleCollapse = () => {
    detailModeRef.current = false
    detailBackGuardUntilRef.current = 0
    if (pendingDetailOpenTimerRef.current) {
      clearTimeout(pendingDetailOpenTimerRef.current)
      pendingDetailOpenTimerRef.current = undefined
    }
    setNotchFocusable(false).catch(() => {})
    setPanelState('collapsed')
  }

  const collapsedWidthScale = useConfigStore((s) => s.collapsedWidthScale)
  const notchHeightMode = useConfigStore((s) => s.notchHeightMode)
  const customNotchHeight = useConfigStore((s) => s.customNotchHeight)
  const microPillWidth = useConfigStore((s) => s.microPillWidth)
  const compactPillWidth = useConfigStore((s) => s.compactPillWidth)
  const panelMaxWidth = useConfigStore((s) => s.panelMaxWidth)
  const completionCardHeight = useConfigStore((s) => s.completionCardHeight)
  const maxVisibleSessions = useConfigStore((s) => s.maxVisibleSessions)
  const allowHorizontalDrag = useConfigStore((s) => s.allowHorizontalDrag)
  const panelHorizontalOffset = useConfigStore((s) => s.panelHorizontalOffset)
  const displayMonitor = useConfigStore((s) => s.displayMonitor)
  const updateConfig = useConfigStore((s) => s.updateConfig)
  const effectiveHorizontalOffset = allowHorizontalDrag ? panelHorizontalOffset : 0
  const isPetMode = islandSurfaceMode === 'pet'
  const overlayPresentationOpen = Boolean(
    !layoutPreview
    && panelState === 'collapsed'
    && activeOverlay
    && isNonBlockingOverlay(activeOverlay)
    && !interaction.isHidden,
  )

  // Sizing
  const isCompact = notchStyle === 'compact'
  const previewMode = layoutPreview?.mode
  const isMicro = previewMode === 'micro' || (!previewMode && panelState === 'collapsed' && interaction.isMicro)
  const effectivePanelState: PanelState = previewMode === 'expanded'
    ? 'expanded'
    : previewMode === 'completion'
      ? 'hover'
      : overlayPresentationOpen
        ? 'hover'
        : panelState
  const hasBlockingOverlayContent = Boolean(
    !layoutPreview
    && activeOverlay
    && isBlockingOverlay(activeOverlay)
    && effectivePanelState !== 'collapsed',
  )
  const feedbackPresentationOpen = Boolean(
    !layoutPreview
    && activeOverlay
    && isNonBlockingOverlay(activeOverlay)
    && effectivePanelState !== 'collapsed',
  )
  const collapsedHeight = getCollapsedIslandHeight(notchHeightMode, customNotchHeight)
  const contentWidth = isPetMode
    ? 820
    : previewMode === 'micro'
      ? microPillWidth
      : previewMode === 'compact'
        ? Math.round(compactPillWidth * (collapsedWidthScale / 100))
        : previewMode === 'expanded' || previewMode === 'completion'
          ? (isCompact ? panelMaxWidth : Math.min(760, panelMaxWidth + 50))
          : effectivePanelState === 'collapsed'
            ? isMicro
              ? microPillWidth
              : Math.round(compactPillWidth * (collapsedWidthScale / 100))
            : (isCompact ? panelMaxWidth : Math.min(760, panelMaxWidth + 50))

  const statusBarHeight = effectivePanelState !== 'collapsed' ? 32 : 0
  const visibleHoverSessions = useMemo(() => {
    const sorted = [...displayedSessions].sort((a, b) => computePriority(b) - computePriority(a))
    return maxVisibleSessions > 0 ? sorted.slice(0, maxVisibleSessions) : sorted
  }, [displayedSessions, maxVisibleSessions])
  const projectCount = new Set(visibleHoverSessions.map((session) => session.project)).size
  const hoverListHeight = 96 + Math.max(visibleHoverSessions.length, 1) * 76 + Math.max(projectCount, 1) * 32
  const blockingOverlayFallbackHeight = activeOverlay?.type === 'plan'
    ? 460
    : activeOverlay?.type === 'question'
      ? 380
      : 420
  const panelHeight =
    isPetMode
      ? 360
      : previewMode === 'micro' || previewMode === 'compact'
        ? collapsedHeight
        : previewMode === 'completion'
          ? Math.min(Math.max(statusBarHeight + completionCardHeight + 72, 220), maxPanelHeight || 600)
          : previewMode === 'expanded'
            ? (maxPanelHeight || 560)
            : hasBlockingOverlayContent
              ? Math.min(Math.max(measuredAlertContentHeight || blockingOverlayFallbackHeight, 220), maxPanelHeight || 600)
            : feedbackPresentationOpen
              ? Math.min(Math.max(measuredFeedbackContentHeight || completionCardHeight + 168, 220), maxPanelHeight || 600)
            : overlayPresentationOpen
              ? Math.min(Math.max(completionCardHeight + 168, 260), maxPanelHeight || 600)
              : effectivePanelState === 'collapsed'
                ? collapsedHeight
                : effectivePanelState === 'hover'
                  ? Math.min(Math.max(statusBarHeight + hoverListHeight, 260), maxPanelHeight || 600)
                  : (maxPanelHeight || 560)

  const visualState = isPetMode
    ? 'pet'
    : previewMode === 'micro'
      ? 'micro'
      : previewMode === 'compact'
        ? 'compact'
        : previewMode === 'completion'
          ? 'feedback'
          : previewMode === 'expanded'
            ? 'expanded'
            : hasBlockingOverlayContent && activeOverlay?.type === 'permission'
              ? 'alert_permission'
              : hasBlockingOverlayContent && activeOverlay?.type === 'question'
                ? 'alert_question'
                : hasBlockingOverlayContent && activeOverlay?.type === 'plan'
                  ? 'alert_plan'
                  : activeOverlay?.type === 'completion' || activeOverlay?.type === 'response'
                    ? 'feedback'
                    : effectivePanelState === 'collapsed'
                      ? (isMicro ? 'micro' : 'compact')
                      : effectivePanelState === 'expanded'
                        ? 'expanded'
                        : 'hover'
  const usesNotchShell = !isPetMode
  const shellSideExtension = usesNotchShell ? NOTCH_SHELL_SIDE_EXTENSION : 0
  const shellWidth = contentWidth + shellSideExtension * 2
  const notchShellClipPath = buildNotchShellClipPath(
    shellWidth,
    panelHeight,
    visualState,
    shellSideExtension,
  )
  const hitSlopX = effectivePanelState === 'collapsed'
    ? NOTCH_HIT_SLOP_X_COLLAPSED
    : NOTCH_HIT_SLOP_X_EXPANDED
  const hitSlopY = effectivePanelState === 'collapsed'
    ? NOTCH_HIT_SLOP_Y_COLLAPSED
    : NOTCH_HIT_SLOP_Y_EXPANDED
  const sloppedHitboxWidth = shellWidth + hitSlopX * 2
  const sloppedHitboxHeight = panelHeight + hitSlopY
  const usesVisibleCollapsedHitbox = effectivePanelState === 'collapsed'
  const hitboxWidth = usesVisibleCollapsedHitbox ? shellWidth : sloppedHitboxWidth
  const hitboxHeight = usesVisibleCollapsedHitbox ? panelHeight : sloppedHitboxHeight
  const hitboxPadX = usesVisibleCollapsedHitbox ? 0 : hitSlopX
  nativeHoverHitboxSizeRef.current = { width: hitboxWidth, height: hitboxHeight }
  const maxHostSlopX = Math.max(NOTCH_HIT_SLOP_X_COLLAPSED, NOTCH_HIT_SLOP_X_EXPANDED)
  const maxHostSlopY = Math.max(NOTCH_HIT_SLOP_Y_COLLAPSED, NOTCH_HIT_SLOP_Y_EXPANDED)
  const expandedHostContentWidth = isPetMode
    ? 820
    : (isCompact ? panelMaxWidth : Math.min(760, panelMaxWidth + 50))
  const expandedHostPanelHeight = isPetMode ? 360 : (maxPanelHeight || 600)
  const stableHostHitboxWidth = expandedHostContentWidth + shellSideExtension * 2 + maxHostSlopX * 2
  const stableHostHitboxHeight = expandedHostPanelHeight + maxHostSlopY
  const islandHidden = !islandEnabled || (!layoutPreview && interaction.isHidden)
  const hostUsesStableCanvas = !isPetMode && islandEnabled
  const hostTargetHitboxWidth = hostUsesStableCanvas ? stableHostHitboxWidth : hitboxWidth
  const hostTargetHitboxHeight = hostUsesStableCanvas ? stableHostHitboxHeight : hitboxHeight
  const [hostHitboxSize, setHostHitboxSize] = useState(() => ({
    width: hostTargetHitboxWidth,
    height: hostTargetHitboxHeight,
  }))
  const hostHitboxSizeRef = useRef(hostHitboxSize)
  const hostAnchorOffsetXRef = useRef(0)
  const [shellAnchorOffsetX, setShellAnchorOffsetX] = useState(0)
  const lastCollapsedVisualRef = useRef({
    shellWidth,
    panelHeight,
    clipPath: notchShellClipPath,
  })

  useEffect(() => {
    if (!hasBlockingOverlayContent) {
      setMeasuredAlertContentHeight(0)
      return
    }

    const element = alertContentRef.current
    if (!element) return

    const measure = () => {
      const nextHeight = Math.ceil(element.getBoundingClientRect().height || element.scrollHeight)
      if (nextHeight > 0) setMeasuredAlertContentHeight(nextHeight)
    }

    measure()
    const frame = window.requestAnimationFrame(measure)
    if (typeof ResizeObserver === 'undefined') {
      return () => window.cancelAnimationFrame(frame)
    }

    const observer = new ResizeObserver(measure)
    observer.observe(element)
    return () => {
      window.cancelAnimationFrame(frame)
      observer.disconnect()
    }
  }, [activeOverlay?.id, hasBlockingOverlayContent])

  useEffect(() => {
    if (!feedbackPresentationOpen) {
      setMeasuredFeedbackContentHeight(0)
      return
    }

    const element = feedbackContentRef.current
    if (!element) return

    const measure = () => {
      const nextHeight = Math.ceil(element.getBoundingClientRect().height || element.scrollHeight)
      if (nextHeight > 0) setMeasuredFeedbackContentHeight(nextHeight)
    }

    measure()
    const frame = window.requestAnimationFrame(measure)
    if (typeof ResizeObserver === 'undefined') {
      return () => window.cancelAnimationFrame(frame)
    }

    const observer = new ResizeObserver(measure)
    observer.observe(element)
    return () => {
      window.cancelAnimationFrame(frame)
      observer.disconnect()
    }
  }, [activeOverlay?.id, feedbackPresentationOpen])

  const morphTransition = effectivePanelState === 'collapsed'
    ? scaleTransitionDuration(closeMorphTransition, islandAnimationScale)
    : scaleTransitionDuration(openMorphTransition, islandAnimationScale)
  const scaledContentTransition = scaleTransitionDuration(contentTransition, islandAnimationScale)
  const hostIsLargerThanTarget = hostHitboxSize.width > hitboxWidth || hostHitboxSize.height > hitboxHeight
  const effectiveShellAnchorOffsetX = usesNotchShell && (effectivePanelState !== 'collapsed' || hostIsLargerThanTarget)
    ? shellAnchorOffsetX
    : 0
  const shellX = effectivePanelState === 'collapsed' || isDragging
    ? effectiveShellAnchorOffsetX
    : effectiveShellAnchorOffsetX === 0
      ? 0
      : [effectiveShellAnchorOffsetX, 0]
  const panelTransition = effectivePanelState === 'collapsed' && !hostIsLargerThanTarget
    ? { ...morphTransition, x: { duration: 0 } }
    : morphTransition
  const renderedShellWidth = preparingOpen ? lastCollapsedVisualRef.current.shellWidth : shellWidth
  const renderedPanelHeight = preparingOpen ? lastCollapsedVisualRef.current.panelHeight : panelHeight
  const renderedClipPath = preparingOpen ? lastCollapsedVisualRef.current.clipPath : notchShellClipPath
  const renderedVisualState = preparingOpen
    ? (isMicro ? 'micro' : 'compact')
    : visualState

  useLayoutEffect(() => {
    if (effectivePanelState !== 'collapsed' || isDragging) return
    lastCollapsedVisualRef.current = {
      shellWidth,
      panelHeight,
      clipPath: notchShellClipPath,
    }
  }, [effectivePanelState, isDragging, notchShellClipPath, panelHeight, shellWidth])

  useEffect(() => {
    if (isDragging) return
    setShellAnchorOffsetX(effectivePanelState === 'collapsed' ? hostAnchorOffsetXRef.current : hostAnchorOffsetXRef.current)
  }, [effectivePanelState, isDragging])

  const updateHostAnchorOffset = useCallback((anchorOffsetX: number) => {
    hostAnchorOffsetXRef.current = anchorOffsetX
    if (useSessionStore.getState().panelState === 'collapsed') {
      setShellAnchorOffsetX(anchorOffsetX)
    }
  }, [])

  const requestNativeHostResize = useCallback((
    width: number,
    height: number,
    horizontalOffset: number,
    displayId: string | undefined,
    onComplete: (anchorOffsetX: number) => void,
    options?: { force?: boolean },
  ) => {
    const key = nativeHostResizeKey(width, height, horizontalOffset, displayId)
    if (!options?.force && lastNativeHostResizeKeyRef.current === key) {
      onComplete(hostAnchorOffsetXRef.current)
      return
    }

    const waiters = nativeHostResizeWaitersRef.current
    const existingWaiters = waiters.get(key)
    if (existingWaiters) {
      existingWaiters.push(onComplete)
    } else {
      waiters.set(key, [onComplete])
    }

    const inFlightKeys = inFlightNativeHostResizeKeysRef.current
    if (inFlightKeys.has(key)) return
    inFlightKeys.add(key)

    resizeNotch(width, height, horizontalOffset, displayId)
      .then((result) => {
        inFlightKeys.delete(key)
        lastNativeHostResizeKeyRef.current = key
        const callbacks = waiters.get(key) ?? []
        waiters.delete(key)
        callbacks.forEach((callback) => callback(result.anchorOffsetX))
      })
      .catch(() => {
        inFlightKeys.delete(key)
        const callbacks = waiters.get(key) ?? []
        waiters.delete(key)
        callbacks.forEach((callback) => callback(hostAnchorOffsetXRef.current))
      })
  }, [])

  // Keep the native transparent host at a stable max canvas. macOS can briefly
  // show the old WebView backing store when a transparent NSWindow is resized
  // during hover/collapse; fixed host geometry avoids that repaint path.
  // Cursor passthrough for the transparent area is handled with
  // set_ignore_cursor_events while collapsed.
  useLayoutEffect(() => {
    if (isDragging) return
    const current = hostHitboxSizeRef.current
    const next = { width: hostTargetHitboxWidth, height: hostTargetHitboxHeight }
    const sameSize = current.width === next.width && current.height === next.height
    let cancelled = false
    const commitHostSize = () => {
      if (cancelled) return false
      hostHitboxSizeRef.current = next
      setHostHitboxSize(next)
      return true
    }
    const completeResize = (anchorOffsetX = 0) => {
      if (cancelled) return
      updateHostAnchorOffset(anchorOffsetX)
      if (preparingOpen && openPrepareFrameRef.current == null) {
        openPrepareFrameRef.current = window.requestAnimationFrame(() => {
          openPrepareFrameRef.current = undefined
          finishPreparedOpen()
        })
      }
    }

    const resizeNativeHost = () => {
      requestNativeHostResize(next.width, next.height, effectiveHorizontalOffset, displayMonitor, completeResize)
    }

    if (sameSize) {
      resizeNativeHost()
      return () => { cancelled = true }
    }

    if (next.width > current.width || next.height > current.height) {
      if (commitHostSize()) resizeNativeHost()
      return () => { cancelled = true }
    }

    const timer = window.setTimeout(() => {
      if (commitHostSize()) resizeNativeHost()
    }, CLOSE_NATIVE_RESIZE_DELAY_MS * islandAnimationScale)

    return () => {
      cancelled = true
      window.clearTimeout(timer)
    }
  }, [hostTargetHitboxWidth, hostTargetHitboxHeight, effectiveHorizontalOffset, displayMonitor, finishPreparedOpen, islandAnimationScale, isDragging, preparingOpen, requestNativeHostResize, updateHostAnchorOffset])

  useEffect(() => {
    if (!isTauri() || displayMonitor !== 'auto' || isDragging) return
    let inFlight = false
    const repositionToCursorDisplay = () => {
      if (inFlight) return
      inFlight = true
      requestNativeHostResize(
        hostHitboxSize.width,
        hostHitboxSize.height,
        effectiveHorizontalOffset,
        'auto',
        (anchorOffsetX) => {
          updateHostAnchorOffset(anchorOffsetX)
          inFlight = false
        },
        { force: true },
      )
    }
    const interval = window.setInterval(repositionToCursorDisplay, 500)
    repositionToCursorDisplay()
    return () => window.clearInterval(interval)
  }, [displayMonitor, effectiveHorizontalOffset, hostHitboxSize.height, hostHitboxSize.width, isDragging, requestNativeHostResize, updateHostAnchorOffset])

  const finishActiveDrag = useCallback((pointerId?: number, captureTarget?: Element) => {
    const activePointerId = dragPointerIdRef.current
    if (activePointerId == null) return
    if (pointerId != null && activePointerId !== pointerId) return

    dragPointerIdRef.current = null
    dragCandidateRef.current = null
    setIsDragging(false)
    endNotchDrag().then((finalOffset) => {
      if (typeof finalOffset === 'number') updateConfig('panelHorizontalOffset', finalOffset)
    }).catch(() => {})

    if (captureTarget?.hasPointerCapture?.(activePointerId)) {
      captureTarget.releasePointerCapture(activePointerId)
    }
  }, [updateConfig])

  useEffect(() => {
    if (!isDragging) return

    const finishFromWindow = () => finishActiveDrag()
    const finishFromPointer = (event: globalThis.PointerEvent) => finishActiveDrag(event.pointerId)

    window.addEventListener('pointerup', finishFromPointer, true)
    window.addEventListener('pointercancel', finishFromPointer, true)
    window.addEventListener('mouseup', finishFromWindow, true)
    window.addEventListener('blur', finishFromWindow)

    return () => {
      window.removeEventListener('pointerup', finishFromPointer, true)
      window.removeEventListener('pointercancel', finishFromPointer, true)
      window.removeEventListener('mouseup', finishFromWindow, true)
      window.removeEventListener('blur', finishFromWindow)
    }
  }, [finishActiveDrag, isDragging])

  const handlePointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (!allowHorizontalDrag || event.button !== 0 || isDragIgnoredTarget(event.target)) return
    dragCandidateRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
    }
  }

  const handlePointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (dragPointerIdRef.current === event.pointerId) {
      event.preventDefault()
      event.stopPropagation()
      return
    }

    const candidate = dragCandidateRef.current
    if (!candidate || candidate.pointerId !== event.pointerId) return
    const movedX = event.clientX - candidate.startX
    const movedY = event.clientY - candidate.startY
    if (Math.hypot(movedX, movedY) < DRAG_START_THRESHOLD_PX) return

    event.preventDefault()
    event.stopPropagation()
    if (expandTimerRef.current) {
      clearTimeout(expandTimerRef.current)
      expandTimerRef.current = undefined
    }
    dragCandidateRef.current = null
    dragPointerIdRef.current = event.pointerId
    event.currentTarget.setPointerCapture?.(event.pointerId)
    setIsDragging(true)
    const dragWindowWidth = hostHitboxSizeRef.current.width
    const dragWindowHeight = hostHitboxSizeRef.current.height
    startNotchDrag(panelHorizontalOffset, dragWindowWidth, dragWindowHeight, displayMonitor).then((started) => {
      if (!started && dragPointerIdRef.current === event.pointerId) {
        dragPointerIdRef.current = null
        setIsDragging(false)
        if (event.currentTarget.hasPointerCapture?.(event.pointerId)) {
          event.currentTarget.releasePointerCapture(event.pointerId)
        }
      }
    }).catch(() => {
      if (dragPointerIdRef.current === event.pointerId) {
        dragPointerIdRef.current = null
        setIsDragging(false)
        if (event.currentTarget.hasPointerCapture?.(event.pointerId)) {
          event.currentTarget.releasePointerCapture(event.pointerId)
        }
      }
    })
  }

  const finishPointerDrag = (event: PointerEvent<HTMLDivElement>) => {
    if (dragCandidateRef.current?.pointerId === event.pointerId) {
      dragCandidateRef.current = null
      if (event.currentTarget.hasPointerCapture?.(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId)
      }
    }
    const pointerId = dragPointerIdRef.current
    if (pointerId == null || pointerId !== event.pointerId) return
    event.preventDefault()
    finishActiveDrag(pointerId, event.currentTarget)
  }

  return (
    <div
      className="notch-container"
      style={{
        '--notch-host-width': `${hostHitboxSize.width}px`,
        '--notch-host-height': `${hostHitboxSize.height}px`,
        '--notch-hitbox-width': `${hitboxWidth}px`,
        '--notch-hitbox-height': `${hitboxHeight}px`,
        '--notch-hitbox-pad-x': `${hitboxPadX}px`,
        pointerEvents: islandEnabled ? 'auto' : 'none',
      } as CSSProperties}
    >
      <div
        className="notch-hitbox"
        data-island-hidden={islandHidden ? 'true' : 'false'}
        onPointerEnter={handleMouseEnter}
        onPointerLeave={handleMouseLeave}
      >
        <motion.div
          className="notch-panel"
          data-island-state={renderedVisualState}
          data-dragging={isDragging ? 'true' : 'false'}
          role="region"
          aria-label="AgentBro"
          aria-expanded={effectivePanelState !== 'collapsed'}
          initial={false}
          animate={{
            width: renderedShellWidth,
            height: renderedPanelHeight,
            x: shellX,
            opacity: layoutPreview ? 1 : displayChanging || islandHidden ? 0 : 1,
            clipPath: renderedClipPath,
            WebkitClipPath: renderedClipPath,
          } as { width: number; height: number; x: number | number[]; opacity: number; clipPath: string; WebkitClipPath: string }}
          transition={panelTransition}
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={finishPointerDrag}
          onPointerCancel={finishPointerDrag}
          onContextMenu={(event) => {
            if (allowHorizontalDrag) event.preventDefault()
          }}
          style={{
            overflow: 'hidden',
            cursor: allowHorizontalDrag ? (isDragging ? 'grabbing' : 'grab') : 'default',
            paddingInline: shellSideExtension,
          } as CSSProperties}
        >
          {allowHorizontalDrag && (
            <div
              aria-hidden="true"
              className="notch-panel__drag-handle"
              data-testid="notch-drag-handle"
            />
          )}
          {isPetMode ? (
            <PetSurface
              activeOverlay={activeOverlay}
              expanded={effectivePanelState !== 'collapsed'}
              hidden={islandHidden}
              onCollapse={handleCollapse}
              onDismissOverlay={dismissOverlay}
              scale={islandPetScale}
              sessions={displayedSessions}
            />
          ) : (
            <>
              <Confetti trigger={confettiEnabled && activeOverlay?.type === 'completion'} />
              <PixelCursor priority={activePriority} visible={pixelCursorEnabled && panelState !== 'collapsed'} />

              {!hasBlockingOverlayContent && !feedbackPresentationOpen && (
                <CollapsedBar
                  sessions={displayedSessions}
                  panelState={preparingOpen ? 'collapsed' : effectivePanelState}
                  rateLimits={rateLimits}
                  onCollapse={handleCollapse}
                  isMicro={isMicro}
                  focusFilteredEmpty={focusFilteredEmpty}
                />
              )}

              <AnimatePresence mode="wait">
                {!preparingOpen && hasBlockingOverlayContent && activeOverlay && (
                  <motion.div
                    ref={setAlertContentNode}
                    key={`alert-${activeOverlay.id}`}
                    className="notch-panel__alert-content"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={scaledContentTransition}
                  >
                    <OverlayRenderer overlay={activeOverlay} onDismiss={() => dismissOverlay(activeOverlay.id)} />
                  </motion.div>
                )}

                {!preparingOpen && layoutPreview && (previewMode === 'expanded' || previewMode === 'completion') && (
                  <motion.div
                    key={`layout-preview-${previewMode}`}
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={scaledContentTransition}
                  >
                    <LayoutPreviewBody mode={previewMode} />
                  </motion.div>
                )}

                {!preparingOpen && feedbackPresentationOpen && activeOverlay && (
                  <motion.div
                    ref={setFeedbackContentNode}
                    key={`feedback-${activeOverlay.id}`}
                    className="notch-panel__feedback-content"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={scaledContentTransition}
                  >
                    <OverlayRenderer overlay={activeOverlay} onDismiss={() => dismissOverlay(activeOverlay.id)} />
                  </motion.div>
                )}

                {/* Base layer: session list */}
                {!preparingOpen && !layoutPreview && !hasBlockingOverlayContent && !feedbackPresentationOpen && panelState === 'hover' && (
                  <motion.div
                    key="hover"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={scaledContentTransition}
                  >
                    <HoverList
                      sessions={displayedSessions}
                      onSessionClick={handleSessionClick}
                      onJumpToTerminal={(id) => {
                        setNotchFocusable(false).catch(() => {})
                        jumpToTerminal(id).catch((error) => console.warn('[notch] jumpToTerminal:', error))
                      }}
                      focusFilteredEmpty={focusFilteredEmpty}
                    />
                  </motion.div>
                )}

                {/* Base layer: detail view */}
                {!preparingOpen && !layoutPreview && !hasBlockingOverlayContent && !feedbackPresentationOpen && panelState === 'expanded' && (
                  <motion.div
                    key="expanded"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={scaledContentTransition}
                    style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}
                  >
                    <ChatView onBack={() => {
                      if (Date.now() < detailBackGuardUntilRef.current) return
                      detailModeRef.current = false
                      detailBackGuardUntilRef.current = 0
                      useSessionStore.getState().setActiveSession(null)
                      setPanelState('hover')
                    }} />
                  </motion.div>
                )}
              </AnimatePresence>

              {!preparingOpen && !layoutPreview && activeOverlay && isNonBlockingOverlay(activeOverlay) && effectivePanelState !== 'collapsed' && !feedbackPresentationOpen && (
                <button
                  type="button"
                  className="notch-panel__outside-dismiss"
                  data-testid="notch-outside-dismiss"
                  aria-label="Dismiss feedback"
                  onMouseDown={(event) => {
                    event.preventDefault()
                    dismissOverlay(activeOverlay.id)
                  }}
                />
              )}

              {/* Overlay layer — renders on top of base layer */}
              <AnimatePresence>
                {!preparingOpen && !layoutPreview && activeOverlay && isNonBlockingOverlay(activeOverlay) && effectivePanelState !== 'collapsed' && !feedbackPresentationOpen && (
                  <motion.div
                    key={`overlay-${activeOverlay.id}`}
                    className="notch-panel__overlay"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={scaledContentTransition}
                  >
                    <OverlayRenderer overlay={activeOverlay} onDismiss={() => dismissOverlay(activeOverlay.id)} />
                  </motion.div>
                )}
              </AnimatePresence>
            </>
          )}
        </motion.div>
      </div>
    </div>
  )
}
