/* AgentBro — Notch Panel (Layered Dynamic Island) */
import { useCallback, useEffect, useRef, useState, useMemo, type CSSProperties, type PointerEvent } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import { useSessionStore, selectSessionList, selectPanelState, selectRateLimits, selectActiveOverlay } from '../../stores/sessionStore'
import { useConfigStore } from '../../stores/configStore'
import { respondPermission, respondQuestion, respondPlan, sendMessage, jumpToTerminal, resizeNotch, setNotchOpacity, getChatHistory, performHaptic, setNotchFocusable, startNotchDrag, endNotchDrag, isCursorOverNotch, isTerminalFocused, isTauri } from '../../services/tauriApi'
import { mapParsedMessages } from '../../hooks/useTauri'
import { computePriority } from '../../types/priority'
import type { OverlayItem, PanelState } from '../../types/agent'
import { deriveIslandInteraction, getFollowFocusVisibleSessions, isBlockingOverlay, isNonBlockingOverlay } from '../../utils/islandInteraction'
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

function sameStringSet(a: Set<string> | null, b: Set<string>): boolean {
  if (!a || a.size !== b.size) return false
  for (const item of b) {
    if (!a.has(item)) return false
  }
  return true
}

const morphTransition = {
  type: 'spring' as const,
  duration: 0.58,
  bounce: 0,
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
          onSendFeedback={(msg) => { respondPlan(session.id, 'feedback', msg); onDismiss() }}
          onManualReview={() => { respondPlan(session.id, 'manual'); onDismiss() }}
          onAcceptEdits={() => { respondPlan(session.id, 'acceptEdits'); onDismiss() }}
          onAutoApprove={() => { respondPlan(session.id, 'bypassPermissions'); onDismiss() }}
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
          onDismiss={onDismiss}
        />
      )
    default:
      return null
  }
}

type IslandLayoutPreview = {
  mode: 'micro' | 'compact' | 'expanded' | 'completion'
}

function LayoutPreviewBody({ mode }: { mode: IslandLayoutPreview['mode'] }) {
  if (mode === 'completion') {
    return (
      <div className="layout-preview layout-preview--completion">
        <div className="layout-preview__eyebrow">Task Complete</div>
        <div className="layout-preview__title">Codex finished running tests</div>
        <div className="layout-preview__meta">agentBro · npm run test:run · now</div>
      </div>
    )
  }

  if (mode === 'expanded') {
    return (
      <div className="layout-preview layout-preview--expanded">
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
  const dismissOnOutsideClick = useConfigStore((s) => s.dismissOnOutsideClick)
  const autoHideNoSessions = useConfigStore((s) => s.autoHideNoSessions)
  const noSessionsHideDelay = useConfigStore((s) => s.noSessionsHideDelay)
  const idleTimeoutMinutes = useConfigStore((s) => s.idleTimeoutMinutes)
  const escSilenceDuration = useConfigStore((s) => s.escSilenceDuration)
  const interactionMode = useConfigStore((s) => s.interactionMode)
  const taskCompleteDwellSeconds = useConfigStore((s) => s.taskCompleteDwellSeconds)
  const confettiEnabled = useConfigStore((s) => s.confettiEnabled)
  const pixelCursorEnabled = useConfigStore((s) => s.pixelCursorEnabled)
  const followFocus = useConfigStore((s) => s.followFocus)
  const islandSurfaceMode = useConfigStore((s) => s.islandSurfaceMode)
  const islandPetScale = useConfigStore((s) => s.islandPetScale)
  const wakeSilencedUntil = useSessionStore((s) => s.wakeSilencedUntil)
  const setWakeSilencedUntil = useSessionStore((s) => s.setWakeSilencedUntil)
  const setFocusedTerminal = useSessionStore((s) => s.setFocusedTerminal)
  const applyIdleTimeout = useSessionStore((s) => s.applyIdleTimeout)
  const leaveTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const idleHideTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const overlayDismissTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const [persistentIdleHidden, setPersistentIdleHidden] = useState(false)
  const [displayChanging, setDisplayChanging] = useState(false)
  const [isDragging, setIsDragging] = useState(false)
  const [layoutPreview, setLayoutPreview] = useState<IslandLayoutPreview | null>(null)
  const [focusedSessionIds, setFocusedSessionIds] = useState<Set<string> | null>(null)
  const dragPointerIdRef = useRef<number | null>(null)
  const dragCandidateRef = useRef<{ pointerId: number; startX: number; startY: number } | null>(null)
  const focusSessionKey = useMemo(
    () => sessions.map((session) => `${session.id}:${session.pid ?? ''}:${session.terminal}`).join('|'),
    [sessions],
  )

  useEffect(() => {
    if (!followFocus || !isTauri()) {
      setFocusedSessionIds(null)
      if (useSessionStore.getState().focusedTerminal !== null) setFocusedTerminal(null)
      return
    }

    let disposed = false
    let pollInFlight = false
    const pollFocusedSessions = async () => {
      if (pollInFlight) return
      pollInFlight = true
      try {
        const snapshot = useSessionStore.getState().sessionList
        if (snapshot.length === 0) {
          if (!disposed) {
            setFocusedSessionIds((prev) => (prev && prev.size === 0 ? prev : new Set()))
            if (useSessionStore.getState().focusedTerminal !== null) setFocusedTerminal(null)
          }
          return
        }

        const results = await Promise.all(snapshot.map(async (session) => {
          if (!session.pid) return [session.id, false] as const
          try {
            return [session.id, await isTerminalFocused(session.id)] as const
          } catch (err) {
            console.warn('[notch] followFocus check failed:', err)
            return [session.id, false] as const
          }
        }))

        if (disposed) return
        const nextFocusedIds = new Set(results.filter(([, focused]) => focused).map(([id]) => id))
        setFocusedSessionIds((prev) => sameStringSet(prev, nextFocusedIds) ? prev : nextFocusedIds)

        const focusedSession = snapshot.find((session) => nextFocusedIds.has(session.id))
        const focusedTerminal = focusedSession?.terminal ?? null
        if (useSessionStore.getState().focusedTerminal !== focusedTerminal) {
          setFocusedTerminal(focusedTerminal)
        }
      } finally {
        pollInFlight = false
      }
    }

    pollFocusedSessions()
    const timer = window.setInterval(pollFocusedSessions, 1000)
    return () => {
      disposed = true
      window.clearInterval(timer)
    }
  }, [followFocus, focusSessionKey, setFocusedTerminal])

  useEffect(() => {
    if (idleTimeoutMinutes <= 0) return
    applyIdleTimeout()
    const timer = window.setInterval(() => applyIdleTimeout(), 2000)
    return () => window.clearInterval(timer)
  }, [applyIdleTimeout, idleTimeoutMinutes])

  const visibleSessions = useMemo(() => {
    return getFollowFocusVisibleSessions(sessions, followFocus, focusedSessionIds)
  }, [followFocus, focusedSessionIds, sessions])
  const focusFilteredEmpty = followFocus && focusedSessionIds !== null && sessions.length > 0 && visibleSessions.length === 0

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

  // Track sessions needing blocking attention.
  const blockingAttentionCount = useMemo(
    () => sessions.filter((session) => session.phase === 'waiting_approval' || session.phase === 'waiting_input').length,
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
    wakeSilenced: Date.now() < wakeSilencedUntil,
  }), [sessions, panelState, activeOverlay, interactionMode, persistentIdleHidden, wakeSilencedUntil])

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
    if ((hasNewAttention || hasNewBlockingOverlay) && panelState === 'collapsed') {
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
  }, [activeOverlay, autoHideNoSessions, interaction.hasActiveSession, interactionMode, noSessionsHideDelay, panelState])

  useEffect(() => {
    // Keep the native transparent window alive for hover hit-testing. Minimal
    // mode hides the visual shell with CSS opacity; setting the whole NSWindow
    // to alpha 0 makes later pointer entry unreliable on macOS.
    setNotchOpacity(1).catch(() => {})
  }, [])

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
  const nativeHoverInsideRef = useRef(false)
  const interactionLockUntilRef = useRef(0)
  const detailModeRef = useRef(false)
  const detailBackGuardUntilRef = useRef(0)
  const pendingDetailOpenTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  // Mouse enter
  const handleMouseEnter = () => {
    const wakeSilenced = useSessionStore.getState().isWakeSilenced()
    if (leaveTimerRef.current) {
      clearTimeout(leaveTimerRef.current)
      leaveTimerRef.current = undefined
    }
    if (wakeSilenced) return
    nativeHoverInsideRef.current = true
    if (panelState === 'collapsed') {
      const delay = interaction.isMicro ? microHoverExpandDelay : hoverExpandDelay
      if (delay > 0) {
        expandTimerRef.current = setTimeout(() => {
          const current = useSessionStore.getState().panelState
          const silenced = useSessionStore.getState().isWakeSilenced()
          if (current === 'collapsed' && !silenced) {
            setPanelState('hover')
            if (hapticOnHover) performHaptic(hapticIntensity).catch(() => {})
          }
        }, delay)
      } else {
        setPanelState('hover')
        if (hapticOnHover) performHaptic(hapticIntensity).catch(() => {})
      }
    }
  }

  // Mouse leave
  const handleMouseLeave = () => {
    if (detailModeRef.current) return
    if (Date.now() < interactionLockUntilRef.current) return
    if (useSessionStore.getState().panelState === 'expanded') return
    nativeHoverInsideRef.current = false
    if (isDragging) return
    if (expandTimerRef.current) {
      clearTimeout(expandTimerRef.current)
      expandTimerRef.current = undefined
    }
    if (!autoCollapse) return
    if (panelState === 'hover') {
      const delay = collapseDelay > 0 ? collapseDelay : dwellDuration
      leaveTimerRef.current = setTimeout(() => {
        const current = useSessionStore.getState().panelState
        if (current === 'hover') {
          setNotchFocusable(false).catch(() => {})
          setPanelState('collapsed')
        }
      }, delay)
    }
  }

  useEffect(() => {
    return () => {
      if (leaveTimerRef.current) clearTimeout(leaveTimerRef.current)
      if (expandTimerRef.current) clearTimeout(expandTimerRef.current)
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
        const isOver = await isCursorOverNotch()
        if (cancelled) return
        const wasOver = nativeHoverInsideRef.current
        if (isOver && !wasOver) {
          handleMouseEnter()
        } else if (!isOver && wasOver) {
          if (!detailModeRef.current && useSessionStore.getState().panelState !== 'expanded') {
            handleMouseLeave()
          }
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
      window.clearInterval(interval)
    }
  }, [isDragging, interaction.isMicro, hoverExpandDelay, microHoverExpandDelay, hapticOnHover, hapticIntensity, panelState, autoCollapse, collapseDelay, dwellDuration])

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
        const escDuration = useConfigStore.getState().escSilenceDuration
        const silenceUntil = Date.now() + escDuration * 1000
        useSessionStore.getState().setWakeSilencedUntil(silenceUntil)
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
      return
    }
    if (pendingDetailOpenTimerRef.current) {
      clearTimeout(pendingDetailOpenTimerRef.current)
      pendingDetailOpenTimerRef.current = undefined
    }
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
  const allowHorizontalDrag = useConfigStore((s) => s.allowHorizontalDrag)
  const panelHorizontalOffset = useConfigStore((s) => s.panelHorizontalOffset)
  const displayMonitor = useConfigStore((s) => s.displayMonitor)
  const updateConfig = useConfigStore((s) => s.updateConfig)
  const effectiveHorizontalOffset = allowHorizontalDrag ? panelHorizontalOffset : 0
  const isPetMode = islandSurfaceMode === 'pet'

  // Sizing
  const isCompact = notchStyle === 'compact'
  const hasOverlay = !layoutPreview && activeOverlay !== null
  const previewMode = layoutPreview?.mode
  const isMicro = previewMode === 'micro' || (!previewMode && panelState === 'collapsed' && interaction.isMicro)
  const effectivePanelState: PanelState = previewMode === 'expanded'
    ? 'expanded'
    : previewMode === 'completion'
      ? 'hover'
      : panelState
  const collapsedHeight = notchHeightMode === 'custom'
    ? customNotchHeight
    : notchHeightMode === 'matchMenuBar'
      ? 28
      : 32
  const contentWidth = isPetMode
    ? 820
    : isDragging
    ? microPillWidth
    : previewMode === 'micro'
      ? microPillWidth
      : previewMode === 'compact'
        ? Math.round(compactPillWidth * (collapsedWidthScale / 100))
        : previewMode === 'expanded' || previewMode === 'completion'
          ? (isCompact ? panelMaxWidth : Math.min(760, panelMaxWidth + 50))
          : panelState === 'collapsed'
            ? isMicro
              ? microPillWidth
              : Math.round(compactPillWidth * (collapsedWidthScale / 100))
            : (isCompact ? panelMaxWidth : Math.min(760, panelMaxWidth + 50))

  const statusBarHeight = effectivePanelState !== 'collapsed' ? 32 : 0
  const overlayExtraHeight = hasOverlay ? 120 : 0
  const projectCount = new Set(visibleSessions.map((session) => session.project)).size
  const hoverListHeight = 96 + Math.max(visibleSessions.length, 1) * 76 + Math.max(projectCount, 1) * 32
  const panelHeight =
    isPetMode
      ? 360
      : isDragging
      ? collapsedHeight
      : previewMode === 'micro' || previewMode === 'compact'
        ? collapsedHeight
        : previewMode === 'completion'
          ? Math.min(Math.max(statusBarHeight + completionCardHeight + 72, 220), maxPanelHeight || 600)
          : previewMode === 'expanded'
            ? (maxPanelHeight || 560)
            : panelState === 'collapsed'
              ? collapsedHeight
              : panelState === 'hover'
                ? Math.min(Math.max(statusBarHeight + hoverListHeight + overlayExtraHeight, 260), maxPanelHeight || 600)
                : (maxPanelHeight || 560)

  const visualState = isPetMode
    ? 'pet'
    : isDragging
    ? 'micro'
    : previewMode === 'micro'
      ? 'micro'
      : previewMode === 'compact'
        ? 'compact'
        : previewMode === 'completion'
          ? 'feedback'
          : previewMode === 'expanded'
            ? 'expanded'
            : activeOverlay?.type === 'permission'
      ? 'alert_permission'
      : activeOverlay?.type === 'question'
        ? 'alert_question'
        : activeOverlay?.type === 'plan'
          ? 'alert_plan'
          : activeOverlay?.type === 'completion' || activeOverlay?.type === 'response'
            ? 'feedback'
            : panelState === 'collapsed'
              ? (isMicro ? 'micro' : 'compact')
              : panelState === 'expanded'
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
  const hitSlopX = panelState === 'collapsed' || isDragging
    ? NOTCH_HIT_SLOP_X_COLLAPSED
    : NOTCH_HIT_SLOP_X_EXPANDED
  const hitSlopY = panelState === 'collapsed' || isDragging
    ? NOTCH_HIT_SLOP_Y_COLLAPSED
    : NOTCH_HIT_SLOP_Y_EXPANDED
  const hitboxWidth = shellWidth + hitSlopX * 2
  const hitboxHeight = panelHeight + hitSlopY

  // Debounce IPC resize to avoid jitter during spring animation
  useEffect(() => {
    if (isDragging) return
    const timer = setTimeout(() => {
      resizeNotch(hitboxWidth, hitboxHeight, effectiveHorizontalOffset, displayMonitor)
    }, 50)
    return () => clearTimeout(timer)
  }, [hitboxWidth, hitboxHeight, effectiveHorizontalOffset, displayMonitor, isDragging])

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
    event.currentTarget.setPointerCapture(event.pointerId)
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
    setIsDragging(true)
    const dragShellWidth = microPillWidth + NOTCH_SHELL_SIDE_EXTENSION * 2
    const dragWindowWidth = dragShellWidth + NOTCH_HIT_SLOP_X_COLLAPSED * 2
    const dragWindowHeight = collapsedHeight + NOTCH_HIT_SLOP_Y_COLLAPSED
    startNotchDrag(panelHorizontalOffset, dragWindowWidth, dragWindowHeight, displayMonitor).then((started) => {
      if (!started && dragPointerIdRef.current === event.pointerId) {
        dragPointerIdRef.current = null
        setIsDragging(false)
        if (event.currentTarget.hasPointerCapture(event.pointerId)) {
          event.currentTarget.releasePointerCapture(event.pointerId)
        }
      }
    }).catch(() => {
      if (dragPointerIdRef.current === event.pointerId) {
        dragPointerIdRef.current = null
        setIsDragging(false)
        if (event.currentTarget.hasPointerCapture(event.pointerId)) {
          event.currentTarget.releasePointerCapture(event.pointerId)
        }
      }
    })
  }

  const finishPointerDrag = (event: PointerEvent<HTMLDivElement>) => {
    if (dragCandidateRef.current?.pointerId === event.pointerId) {
      dragCandidateRef.current = null
      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
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
        '--notch-hitbox-width': `${hitboxWidth}px`,
        '--notch-hitbox-height': `${hitboxHeight}px`,
        '--notch-hitbox-pad-x': `${hitSlopX}px`,
      } as CSSProperties}
    >
      <div
        className="notch-hitbox"
        data-island-hidden={!layoutPreview && interaction.isHidden ? 'true' : 'false'}
        onPointerEnter={handleMouseEnter}
        onPointerLeave={handleMouseLeave}
      >
        <motion.div
          className="notch-panel"
          data-island-state={visualState}
          data-dragging={isDragging ? 'true' : 'false'}
          role="region"
          aria-label="AgentBro"
          aria-expanded={effectivePanelState !== 'collapsed'}
          animate={{
            width: shellWidth,
            height: panelHeight,
            opacity: layoutPreview ? 1 : displayChanging || interaction.isHidden ? 0 : 1,
          }}
          transition={morphTransition}
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
            '--notch-shell-path': notchShellClipPath,
          } as CSSProperties & { '--notch-shell-path': string }}
        >
          {isPetMode ? (
            <PetSurface
              activeOverlay={activeOverlay}
              hidden={!layoutPreview && interaction.isHidden}
              onCollapse={handleCollapse}
              onDismissOverlay={dismissOverlay}
              scale={islandPetScale}
              sessions={visibleSessions}
            />
          ) : (
            <>
              <Confetti trigger={confettiEnabled && activeOverlay?.type === 'completion'} />
              <PixelCursor priority={activePriority} visible={pixelCursorEnabled && panelState !== 'collapsed'} />

              {/* Header — always visible */}
              <CollapsedBar
                sessions={visibleSessions}
                panelState={effectivePanelState}
                rateLimits={rateLimits}
                onCollapse={handleCollapse}
                isMicro={isDragging || isMicro}
                focusFilteredEmpty={focusFilteredEmpty}
              />

              <AnimatePresence mode="wait">
                {!isDragging && layoutPreview && (previewMode === 'expanded' || previewMode === 'completion') && (
                  <motion.div
                    key={`layout-preview-${previewMode}`}
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={contentTransition}
                  >
                    <LayoutPreviewBody mode={previewMode} />
                  </motion.div>
                )}

                {/* Base layer: session list */}
                {!isDragging && !layoutPreview && panelState === 'hover' && (
                  <motion.div
                    key="hover"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={contentTransition}
                  >
                    <HoverList
                      sessions={visibleSessions}
                      onSessionClick={handleSessionClick}
                      onJumpToTerminal={(id) => {
                        setNotchFocusable(false).catch(() => {})
                        jumpToTerminal(id)
                      }}
                      focusFilteredEmpty={focusFilteredEmpty}
                    />
                  </motion.div>
                )}

                {/* Base layer: detail view */}
                {!isDragging && !layoutPreview && panelState === 'expanded' && (
                  <motion.div
                    key="expanded"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={contentTransition}
                    style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}
                  >
                    <ChatView onBack={() => {
                      if (Date.now() < detailBackGuardUntilRef.current) return
                      detailModeRef.current = false
                      detailBackGuardUntilRef.current = 0
                      setPanelState('hover')
                    }} />
                  </motion.div>
                )}
              </AnimatePresence>

              {!isDragging && !layoutPreview && activeOverlay && isNonBlockingOverlay(activeOverlay) && dismissOnOutsideClick && panelState !== 'collapsed' && (
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
                {!isDragging && !layoutPreview && activeOverlay && panelState !== 'collapsed' && (
                  <motion.div
                    key={`overlay-${activeOverlay.id}`}
                    className="notch-panel__overlay"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={contentTransition}
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
