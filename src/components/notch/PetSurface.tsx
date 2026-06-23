import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type MouseEvent,
  type PointerEvent,
} from 'react'
import { useTranslation } from 'react-i18next'
import type { OverlayItem, SessionState } from '../../types/agent'
import { PRIORITY, computePriority, type Priority } from '../../types/priority'
import { useSessionStore, selectActiveOverlay } from '../../stores/sessionStore'
import {
  endPetDrag,
  isCursorInWindowZones,
  isTauri,
  jumpToTerminal,
  openSettingsWindow,
  respondAutoApprove,
  respondPermission,
  respondPlan,
  respondQuestion,
  sendMessage,
  setNotchIgnoreCursorEvents,
  startPetDrag,
} from '../../services/tauriApi'
import { useConfigStore } from '../../stores/configStore'
import { selectActivePet, usePetStore } from '../../stores/petStore'
import { useUpdateStore } from '../../stores/updateStore'
import { usePetVitalsDebug } from '../../stores/petVitalsDebugStore'
import { getSessionTitle } from '../../utils/sessionDisplay'
import { isBlockingOverlay, isNonBlockingOverlay } from '../../utils/islandInteraction'
import { MascotRouter } from './mascots'
import { SpriteCanvas } from './SpriteCanvas'
import { PetVitals } from './PetVitals'
import { PetEmote } from './PetEmote'
import { HoverList } from './HoverList'
import { ChatView } from './ChatView'
import { PermissionCard } from '../overlay/PermissionCard'
import { PlanApprovalCard } from '../overlay/PlanApprovalCard'
import { QuestionCard } from '../overlay/QuestionCard'
import { OverlayResponseCard } from '../overlay/OverlayResponseCard'
import { OverlayCompletionCard } from '../overlay/OverlayCompletionCard'
import { OverlayCompactingCard } from '../overlay/OverlayCompactingCard'
import { usePetSummon } from './usePetSummon'
import { buildTips, shuffleTips } from './tips'
import { isWindowsPlatform } from '../../utils/platform'
import {
  DEFAULT_PET_STAGE_ANCHOR,
  PET_STAGE_WIDTH,
  PET_STAGE_HEIGHT,
  PET_SLOT_SIZE,
  PET_ANCHOR_RIGHT,
  PET_ANCHOR_BOTTOM,
  type PetStageAnchor,
} from './petStageAnchor'
import './PetSurface.css'

type DragDirection = 'left' | 'right' | 'running' | null
type PetPanelHoverHandlers = {
  onPointerEnter: () => void
  onPointerLeave: () => void
}

interface PetSurfaceProps {
  sessions: SessionState[]
  scale: number
  hidden: boolean
  activeOverlay?: OverlayItem | null
  expanded?: boolean
  onCollapse?: () => void
  onDismissOverlay?: (overlayId: string) => void
}

const PET_DRAG_THRESHOLD = 4
const CODEX_PET_DONE_ANIMATION_MS = 1800
const PET_PANEL_AUTO_HIDE_DELAY_MS = 650
const PET_PANEL_GAP = 14
const PET_PANEL_MARGIN = 8
const PET_TIP_WIDTH = 260
const PET_TIP_HEIGHT = 64
const PET_ACTION_TOAST_WIDTH = 268
const PET_ACTION_TOAST_HEIGHT = 84
const PET_MESSAGE_TOAST_WIDTH = 460
const PET_MESSAGE_TOAST_HEIGHT = 316
const PET_SESSION_LIST_WIDTH = 520
const PET_SESSION_LIST_HEIGHT = 316
const PET_EMPTY_PANEL_WIDTH = 218
const PET_EMPTY_PANEL_HEIGHT = 64
const PET_DETAIL_PANEL_WIDTH = 520
const PET_DETAIL_PANEL_HEIGHT = 328
const PET_IDLE_TIP_DELAY_MS = 1200
const PET_IDLE_TIP_VISIBLE_MS = 8000
const PET_IDLE_TIP_INTERVAL_MS = 15000
const PET_IDLE_TIPS_REQUIRE_QUIET = true

function configAnchorToStage(anchor: { left: boolean; top: boolean } | null | undefined): PetStageAnchor | null {
  if (!anchor) return null
  return {
    x: anchor.left ? 'left' : 'right',
    y: anchor.top ? 'top' : 'bottom',
  }
}

function clearPermissionAfter(sessionId: string, work: Promise<void>) {
  work
    .then(() => useSessionStore.getState().clearPermission(sessionId))
    .catch((error) => console.warn('[PetSurface] permission response failed:', error))
}

/**
 * Evolab-style pet companion for AgentBro's dedicated transparent Tauri window.
 * The pet remains a draggable desktop sprite, while short HUD surfaces bloom
 * around it for sessions, blocking actions, and lightweight completion notices.
 */
export function PetSurface({ sessions, scale, hidden }: PetSurfaceProps) {
  const { t } = useTranslation()
  const [dragging, setDragging] = useState(false)
  const [dragDirection, setDragDirection] = useState<DragDirection>(null)
  const [hudOpen, setHudOpen] = useState(false)
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null)
  const [codexPetDoneUntil, setCodexPetDoneUntil] = useState(0)
  const [hasInputDraft, setHasInputDraft] = useState(false)
  const [petHovered, setPetHovered] = useState(false)
  const [suppressedOverlayId, setSuppressedOverlayId] = useState<string | null>(null)
  const dragCandidateRef = useRef<{ pointerId: number; startX: number; startY: number } | null>(null)
  const dragPointerIdRef = useRef<number | null>(null)
  const dragLastScreenXRef = useRef<number | null>(null)
  const draggingRef = useRef(false)
  const suppressClickRef = useRef(false)
  const lastNoticeKeyRef = useRef<string | null>(null)
  const panelLeaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const hasInputDraftRef = useRef(false)
  const messageToastInsideRef = useRef(false)
  const messageDismissPendingRef = useRef<string | null>(null)

  const updateConfig = useConfigStore((s) => s.updateConfig)
  const updateAvailable = useUpdateStore((s) => s.availableVersion)
  const updateBadgeLabel = t('notch.updateBadgeLabel', { defaultValue: 'Update' })
  const taskCompleteDwellSeconds = useConfigStore((s) => s.taskCompleteDwellSeconds)
  const petVitalsEnabled = useConfigStore((s) => s.petVitalsEnabled)
  const tipsEnabled = useConfigStore((s) => s.tipsEnabled)
  const globalShortcut = useConfigStore((s) => s.globalShortcut)
  const shortcutApprove = useConfigStore((s) => s.shortcutApprove)
  const shortcutApproveEnabled = useConfigStore((s) => s.shortcutApproveEnabled)
  const shortcutDeny = useConfigStore((s) => s.shortcutDeny)
  const shortcutDenyEnabled = useConfigStore((s) => s.shortcutDenyEnabled)
  const shortcutSkip = useConfigStore((s) => s.shortcutSkip)
  const shortcutSkipEnabled = useConfigStore((s) => s.shortcutSkipEnabled)
  const petRegistry = usePetStore((s) => s.registry)
  const activePetId = usePetStore((s) => s.activePetId)
  const loadPetRegistry = usePetStore((s) => s.loadRegistry)
  const petLoading = usePetStore((s) => s.loading)
  const petError = usePetStore((s) => s.error)
  const savedPetWindowAnchor = useConfigStore((s) => s.islandPetWindowAnchor)
  const activeOverlay = useSessionStore(selectActiveOverlay)
  const dismissOverlay = useSessionStore((s) => s.dismissOverlay)

  useDefensiveRegistryLoad(petRegistry.length, petLoading, petError, loadPetRegistry)

  const sortedSessions = useMemo(
    () => [...sessions].sort((a, b) => computePriority(b) - computePriority(a)),
    [sessions],
  )
  const selectedSession = useMemo(
    () => sortedSessions.find((session) => session.id === selectedSessionId) ?? null,
    [selectedSessionId, sortedSessions],
  )
  const topSession = sortedSessions[0]
  const agentPetMap = useConfigStore((s) => s.islandAgentPetMap)
  const activePet = useMemo(
    () => selectActivePet(petRegistry, activePetId, sessions, agentPetMap),
    [petRegistry, activePetId, sessions, agentPetMap],
  )
  const visibleActiveOverlay = activeOverlay && activeOverlay.id !== suppressedOverlayId ? activeOverlay : null
  const displayScale = Math.min(1.2, Math.max(0.1, scale / 100))
  const savedStageAnchor = useMemo(() => configAnchorToStage(savedPetWindowAnchor), [savedPetWindowAnchor])
  const [stageAnchor, setStageAnchor] = usePetStageAnchor(
    !hidden,
    dragging,
    savedStageAnchor,
  )
  const spriteSize = Math.round(PET_SLOT_SIZE * displayScale)
  const actionCount = useMemo(() => getPetActionCount(sessions, visibleActiveOverlay), [sessions, visibleActiveOverlay])
  const activeSessionCount = useMemo(
    () => sessions.filter((session) => session.phase !== 'idle' && session.phase !== 'done').length,
    [sessions],
  )
  const actionSession = useMemo(
    () => sortedSessions.find(sessionNeedsPetPrompt) ?? (isBlockingOverlay(visibleActiveOverlay) ? getOverlaySession(visibleActiveOverlay, sessions) : null),
    [sessions, sortedSessions, visibleActiveOverlay],
  )
  const blockingOverlaySession = useMemo(
    () => (visibleActiveOverlay && isBlockingOverlay(visibleActiveOverlay) ? getOverlaySession(visibleActiveOverlay, sessions) : null),
    [sessions, visibleActiveOverlay],
  )
  const showBlockingOverlay = !hidden && !hudOpen && Boolean(visibleActiveOverlay && isBlockingOverlay(visibleActiveOverlay) && blockingOverlaySession)
  const showActionToast = !hidden && !hudOpen && Boolean(actionSession || (visibleActiveOverlay && isBlockingOverlay(visibleActiveOverlay)))
  const showMessageToast = !hidden && !hudOpen && shouldShowPetMessageToast(visibleActiveOverlay, taskCompleteDwellSeconds)
  const messageToastPlacement = useMemo(
    () => getPetPanelPlacement(getPetSidePanelWidth(displayScale, PET_MESSAGE_TOAST_WIDTH, stageAnchor), PET_MESSAGE_TOAST_HEIGHT, displayScale, stageAnchor),
    [displayScale, stageAnchor],
  )
  const showHud = !hidden && hudOpen
  const sessionPanelPlacement = useMemo(() => getPetPanelPlacement(
    selectedSession
      ? getPetSidePanelWidth(displayScale, PET_DETAIL_PANEL_WIDTH, stageAnchor)
      : sortedSessions.length > 0
        ? getPetSidePanelWidth(displayScale, PET_SESSION_LIST_WIDTH, stageAnchor)
        : PET_EMPTY_PANEL_WIDTH,
    selectedSession
      ? PET_DETAIL_PANEL_HEIGHT
      : sortedSessions.length > 0
        ? PET_SESSION_LIST_HEIGHT
        : PET_EMPTY_PANEL_HEIGHT,
    displayScale,
    stageAnchor,
  ), [displayScale, selectedSession, sortedSessions.length, stageAnchor])
  const blockingOverlayPlacement = useMemo(
    () => getPetPanelPlacement(getPetSidePanelWidth(displayScale, PET_DETAIL_PANEL_WIDTH, stageAnchor), PET_DETAIL_PANEL_HEIGHT, displayScale, stageAnchor),
    [displayScale, stageAnchor],
  )
  const actionToastPlacement = useMemo(
    () => getPetPanelPlacement(PET_ACTION_TOAST_WIDTH, PET_ACTION_TOAST_HEIGHT, displayScale, stageAnchor),
    [displayScale, stageAnchor],
  )
  const idleTipPlacement = useMemo(
    () => getPetPanelPlacement(PET_TIP_WIDTH, PET_TIP_HEIGHT, displayScale, stageAnchor),
    [displayScale, stageAnchor],
  )
  const allSessionsQuiet = sessions.length === 0 || sessions.every(sessionIsQuiet)
  const petTips = useMemo(() => buildTips({
    globalShortcut,
    shortcutApprove,
    shortcutApproveEnabled,
    shortcutDeny,
    shortcutDenyEnabled,
    shortcutSkip,
    shortcutSkipEnabled,
  }, 'pet'), [
    globalShortcut,
    shortcutApprove,
    shortcutApproveEnabled,
    shortcutDeny,
    shortcutDenyEnabled,
    shortcutSkip,
    shortcutSkipEnabled,
  ])
  const idleTip = usePetIdleTip(
    tipsEnabled
    && !hidden
    && (!PET_IDLE_TIPS_REQUIRE_QUIET || allSessionsQuiet)
    && !hudOpen
    && !dragging
    && !visibleActiveOverlay,
    petTips,
  )

  useEffect(() => {
    hasInputDraftRef.current = hasInputDraft
  }, [hasInputDraft])

  useEffect(() => {
    if (hidden) {
      if (panelLeaveTimerRef.current) {
        window.clearTimeout(panelLeaveTimerRef.current)
        panelLeaveTimerRef.current = null
      }
      setHudOpen(false)
      setSelectedSessionId(null)
      setHasInputDraft(false)
      useSessionStore.getState().setActiveSession(null)
    }
  }, [hidden])

  useEffect(() => {
    if (selectedSessionId && !sortedSessions.some((session) => session.id === selectedSessionId)) {
      setSelectedSessionId(null)
      useSessionStore.getState().setActiveSession(null)
    }
  }, [selectedSessionId, sortedSessions])

  const releaseMessageDismissHold = useCallback(() => {
    const overlayId = messageDismissPendingRef.current
    if (!overlayId) return
    if (messageToastInsideRef.current || hasInputDraftRef.current || petMessageToastHasFocusedEditable()) return

    messageDismissPendingRef.current = null
    const currentOverlay = useSessionStore.getState().activeOverlay
    if (currentOverlay?.id === overlayId && isNonBlockingOverlay(currentOverlay)) {
      dismissOverlay(overlayId)
    }
  }, [dismissOverlay])

  useEffect(() => {
    messageDismissPendingRef.current = null
    messageToastInsideRef.current = false
    if (!activeOverlay || !isNonBlockingOverlay(activeOverlay)) return
    const overlayId = activeOverlay.id
    const deadline = activeOverlay.createdAt + Math.max(1, taskCompleteDwellSeconds) * 1000
    const delay = Math.max(0, deadline - Date.now())
    const timer = window.setTimeout(() => {
      const currentOverlay = useSessionStore.getState().activeOverlay
      if (currentOverlay?.id === overlayId && isNonBlockingOverlay(currentOverlay)) {
        if (messageToastInsideRef.current || hasInputDraftRef.current || petMessageToastHasFocusedEditable()) {
          messageDismissPendingRef.current = overlayId
          return
        }
        useSessionStore.getState().dismissOverlay(overlayId)
      }
    }, delay)
    return () => window.clearTimeout(timer)
  }, [activeOverlay, dismissOverlay, taskCompleteDwellSeconds])

  useEffect(() => {
    const noticeKey = activeOverlay && isNonBlockingOverlay(activeOverlay)
      ? `${activeOverlay.type}:${activeOverlay.id}:${activeOverlay.createdAt}`
      : topSession?.phase === 'done'
        ? `done:${topSession.id}:${topSession.taskCompletedAt ?? topSession.duration}`
        : null
    if (!noticeKey || noticeKey === lastNoticeKeyRef.current) return
    lastNoticeKeyRef.current = noticeKey
    setCodexPetDoneUntil(Date.now() + CODEX_PET_DONE_ANIMATION_MS)
    const timer = window.setTimeout(() => {
      setCodexPetDoneUntil((current) => (current <= Date.now() ? 0 : current))
    }, CODEX_PET_DONE_ANIMATION_MS)
    return () => window.clearTimeout(timer)
  }, [activeOverlay, topSession?.duration, topSession?.id, topSession?.phase, topSession?.taskCompletedAt])

  useEffect(() => {
    if (!isTauri()) return
    if (hidden) {
      void setNotchIgnoreCursorEvents(true, 'pet').catch(() => {})
      return
    }

    let cancelled = false
    let inFlight = false
    let lastApplied: boolean | null = null
    let probeFailed = false

    const apply = (ignore: boolean) => {
      if (lastApplied === ignore) return
      lastApplied = ignore
      void setNotchIgnoreCursorEvents(ignore, 'pet').catch(() => {})
    }

    apply(false)

    const tick = async () => {
      if (cancelled || inFlight) return
      inFlight = true
      try {
        if (draggingRef.current) {
          apply(false)
          return
        }
        const zones = Array.from(document.querySelectorAll<HTMLElement>('.pet-surface__interactive'))
          .map((node) => node.getBoundingClientRect())
          .filter((rect) => rect.width > 0 && rect.height > 0)
          .map((rect) => ({ left: rect.left, top: rect.top, width: rect.width, height: rect.height }))
        if (zones.length === 0) {
          apply(true)
          return
        }
        const isOver = await isCursorInWindowZones(zones, 'pet')
        probeFailed = false
        apply(!isOver)
      } catch (err) {
        if (!probeFailed) {
          probeFailed = true
          console.warn('[PetSurface] click-through probe failed, staying interactive:', err)
        }
        apply(false)
      } finally {
        inFlight = false
      }
    }

    const hasInteractivePanel = hudOpen || showActionToast || showBlockingOverlay || showMessageToast
    const probeIntervalMs = isWindowsPlatform() && !hasInteractivePanel ? 1000 : 250
    const id = window.setInterval(tick, probeIntervalMs)
    void tick()
    return () => {
      cancelled = true
      window.clearInterval(id)
      void setNotchIgnoreCursorEvents(false, 'pet').catch(() => {})
    }
  }, [hidden, hudOpen, showActionToast, showBlockingOverlay, showMessageToast])

  const summon = usePetSummon({ activeOverlay, topSession })
  const dragAnimationOverride = useMemo(() => {
    switch (dragDirection) {
      case 'left':
        return ['running-left', 'runningLeft', 'running'] as const
      case 'right':
        return ['running-right', 'runningRight', 'running'] as const
      case 'running':
        return ['running', 'running-right', 'runningRight', 'running-left', 'runningLeft'] as const
      default:
        return null
    }
  }, [dragDirection])
  const hoverAnimationOverride = petHovered && !dragging
    ? (['jumping', 'waving', 'stretch'] as const)
    : null
  const animationOverride = dragAnimationOverride ?? summon.summonAnimationOverride ?? hoverAnimationOverride
  const animationOverrideMode = dragAnimationOverride ? 'continuous' : 'transient'
  const realPetPriority = getPetPriority({
    actionCount,
    activePetProvider: activePet?.provider,
    doneUntil: codexPetDoneUntil,
    topSession,
  })
  const vitalsDebug = usePetVitalsDebug()
  const petPriority = vitalsDebug.enabled && vitalsDebug.phaseOverride
    ? debugPhaseToPriority(vitalsDebug.phaseOverride)
    : realPetPriority
  const realContextPressure = topSession?.contextWindow?.usedPercentage ?? 0
  const realEnergyLevel = useMemo(() => {
    const limits = sessions.map((s) => s.rateLimits?.fiveHourUsage ?? 0)
    return limits.length > 0 ? Math.max(...limits) : 0
  }, [sessions])
  const realIsWorking = topSession?.phase === 'processing'
  const realIsIdle = !topSession || topSession.phase === 'idle' || topSession.phase === 'done'

  const contextPressure = vitalsDebug.enabled ? vitalsDebug.contextPressure : realContextPressure
  const energyLevel = vitalsDebug.enabled ? vitalsDebug.energyLevel : realEnergyLevel
  const isWorking = vitalsDebug.enabled && vitalsDebug.phaseOverride
    ? (vitalsDebug.phaseOverride === 'working' || vitalsDebug.phaseOverride === 'thinking')
    : realIsWorking
  const isSessionIdle = vitalsDebug.enabled && vitalsDebug.phaseOverride
    ? (vitalsDebug.phaseOverride === 'idle' || vitalsDebug.phaseOverride === 'done')
    : realIsIdle

  const finishDrag = useCallback(
    async (pointerId?: number) => {
      if (dragPointerIdRef.current == null) {
        dragCandidateRef.current = null
        return
      }
      if (pointerId != null && dragPointerIdRef.current !== pointerId) return
      dragPointerIdRef.current = null
      dragCandidateRef.current = null
      dragLastScreenXRef.current = null
      draggingRef.current = false
      setDragDirection(null)
      try {
        const result = await endPetDrag()
        if (result) {
          setStageAnchor({
            x: result.anchorLeft ? 'left' : 'right',
            y: result.anchorTop ? 'top' : 'bottom',
          })
          updateConfig('islandPetWindowOrigin', result.origin)
          updateConfig('islandPetWindowAnchor', {
            left: result.anchorLeft,
            top: result.anchorTop,
          })
        }
      } catch (err) {
        console.warn('[PetSurface] endPetDrag:', err)
      }
      setDragging(false)
      window.setTimeout(() => {
        suppressClickRef.current = false
      }, 0)
    },
    [updateConfig, setStageAnchor],
  )

  useEffect(() => {
    const finish = () => void finishDrag()
    window.addEventListener('pointerup', finish)
    window.addEventListener('pointercancel', finish)
    window.addEventListener('blur', finish)
    return () => {
      window.removeEventListener('pointerup', finish)
      window.removeEventListener('pointercancel', finish)
      window.removeEventListener('blur', finish)
    }
  }, [finishDrag])

  useEffect(() => {
    return () => {
      if (panelLeaveTimerRef.current) {
        window.clearTimeout(panelLeaveTimerRef.current)
        panelLeaveTimerRef.current = null
      }
    }
  }, [])

  const clearPanelLeaveTimer = useCallback(() => {
    if (!panelLeaveTimerRef.current) return
    window.clearTimeout(panelLeaveTimerRef.current)
    panelLeaveTimerRef.current = null
  }, [])

  const closeSessionPanel = useCallback(() => {
    if (draggingRef.current || hasInputDraftRef.current || petPanelHasFocusedEditable()) return false
    clearPanelLeaveTimer()
    setHudOpen(false)
    setSelectedSessionId(null)
    setHasInputDraft(false)
    useSessionStore.getState().setActiveSession(null)
    return true
  }, [clearPanelLeaveTimer])

  const hideCurrentPetSurface = useCallback(() => {
    if (draggingRef.current) return false
    if (hudOpen) return closeSessionPanel()
    if (!visibleActiveOverlay) return false

    if (isNonBlockingOverlay(visibleActiveOverlay)) {
      dismissOverlay(visibleActiveOverlay.id)
      return true
    }

    if (isBlockingOverlay(visibleActiveOverlay)) {
      setSuppressedOverlayId(visibleActiveOverlay.id)
      return true
    }

    return false
  }, [closeSessionPanel, dismissOverlay, hudOpen, visibleActiveOverlay])

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return
      if (!hideCurrentPetSurface()) return
      event.preventDefault()
      event.stopPropagation()
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [hideCurrentPetSurface])

  const panelHoverHandlers = useMemo<PetPanelHoverHandlers>(() => ({
    onPointerEnter: clearPanelLeaveTimer,
    onPointerLeave: () => {
      clearPanelLeaveTimer()
      panelLeaveTimerRef.current = window.setTimeout(() => {
        panelLeaveTimerRef.current = null
        closeSessionPanel()
      }, PET_PANEL_AUTO_HIDE_DELAY_MS)
    },
  }), [clearPanelLeaveTimer, closeSessionPanel])

  const handlePointerDown = (event: PointerEvent<HTMLButtonElement>) => {
    if (event.button !== 0) return
    dragCandidateRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
    }
    try {
      event.currentTarget.setPointerCapture(event.pointerId)
    } catch {
      // Pointer capture can throw on synthetic events in tests.
    }
  }

  const handlePointerMove = (event: PointerEvent<HTMLButtonElement>) => {
    if (dragPointerIdRef.current === event.pointerId) {
      const lastScreenX = dragLastScreenXRef.current
      if (lastScreenX != null) {
        updateDragDirection(event.screenX - lastScreenX, setDragDirection)
      }
      dragLastScreenXRef.current = event.screenX
      return
    }
    const candidate = dragCandidateRef.current
    if (!candidate || candidate.pointerId !== event.pointerId) return
    const signedDx = event.clientX - candidate.startX
    if (Math.hypot(signedDx, event.clientY - candidate.startY) < PET_DRAG_THRESHOLD) return
    suppressClickRef.current = true
    dragPointerIdRef.current = event.pointerId
    dragLastScreenXRef.current = event.screenX
    setDragging(true)
    draggingRef.current = true
    if (Math.abs(signedDx) < 2) {
      setDragDirection('running')
    } else {
      updateDragDirection(signedDx, setDragDirection)
    }
    startPetDrag(
      stageAnchor.x === 'left',
      stageAnchor.y === 'top',
    )
      .then((started) => {
        if (!started) {
          console.warn('[PetSurface] startPetDrag returned false - Rust drag loop did not arm')
        }
        if (!started && dragPointerIdRef.current === event.pointerId) {
          dragPointerIdRef.current = null
          dragLastScreenXRef.current = null
          setDragging(false)
          draggingRef.current = false
          setDragDirection(null)
        }
      })
      .catch((err) => {
        console.warn('[PetSurface] startPetDrag failed:', err)
        if (dragPointerIdRef.current === event.pointerId) {
          dragPointerIdRef.current = null
          dragLastScreenXRef.current = null
          setDragging(false)
          draggingRef.current = false
          setDragDirection(null)
        }
      })
  }

  const handlePetClick = (event: MouseEvent<HTMLButtonElement>) => {
    clearPanelLeaveTimer()
    if (suppressClickRef.current || dragging) {
      event.preventDefault()
      suppressClickRef.current = false
      return
    }
    setSuppressedOverlayId(null)
    setHudOpen((current) => {
      const next = !current
      if (!next) {
        setSelectedSessionId(null)
        setHasInputDraft(false)
        useSessionStore.getState().setActiveSession(null)
      }
      return next
    })
  }

  const openSessionInTerminal = (sessionId: string) => {
    void jumpToTerminal(sessionId).catch((err) => console.warn('[PetSurface] jumpToTerminal:', err))
  }

  const openSessionDetail = (sessionId: string) => {
    clearPanelLeaveTimer()
    setSelectedSessionId(sessionId)
    useSessionStore.getState().setActiveSession(sessionId)
    setHudOpen(true)
  }

  return (
    <div
      className="pet-surface"
      data-hidden={hidden ? 'true' : 'false'}
      data-hud-open={showHud ? 'true' : 'false'}
      data-anchor-x={stageAnchor.x}
      data-anchor-y={stageAnchor.y}
      style={{ '--pet-scale': displayScale } as CSSProperties}
    >
      <div className="pet-surface__stage">
        {showHud && (
          <PetSessionPanel
            sessions={sortedSessions}
            selectedSession={selectedSession}
            placement={sessionPanelPlacement}
            onBack={() => {
              setSelectedSessionId(null)
              setHasInputDraft(false)
              useSessionStore.getState().setActiveSession(null)
            }}
            onClose={() => {
              clearPanelLeaveTimer()
              setHudOpen(false)
              setSelectedSessionId(null)
              setHasInputDraft(false)
              useSessionStore.getState().setActiveSession(null)
            }}
            onJumpToTerminal={openSessionInTerminal}
            onSelectSession={openSessionDetail}
            onInputDraftStateChange={setHasInputDraft}
            hoverHandlers={panelHoverHandlers}
          />
        )}

        {showBlockingOverlay && visibleActiveOverlay && blockingOverlaySession && (
          <PetBlockingOverlay
            overlay={visibleActiveOverlay}
            session={blockingOverlaySession}
            placement={blockingOverlayPlacement}
            sessionCount={sessions.length}
            onClose={() => dismissOverlay(visibleActiveOverlay.id)}
            onShowSessions={() => setHudOpen(true)}
          />
        )}

        {!showBlockingOverlay && showActionToast && (
          <PetActionToast
            actionCount={actionCount}
            overlay={visibleActiveOverlay}
            session={actionSession}
            placement={actionToastPlacement}
            onOpen={() => {
              if (actionSession) openSessionDetail(actionSession.id)
              else setHudOpen(true)
            }}
          />
        )}

        {showMessageToast && visibleActiveOverlay && (
          <PetMessageToast
            overlay={visibleActiveOverlay}
            session={getOverlaySession(visibleActiveOverlay, sessions) ?? topSession}
            onDismiss={() => dismissOverlay(visibleActiveOverlay.id)}
            onInputDraftStateChange={setHasInputDraft}
            onPointerEnter={() => {
              messageToastInsideRef.current = true
            }}
            onPointerLeave={() => {
              messageToastInsideRef.current = false
              releaseMessageDismissHold()
            }}
            onShowSessions={() => setHudOpen(true)}
            placement={messageToastPlacement}
            sessionCount={sessions.length}
          />
        )}

        <PetIdleTip tip={idleTip} placement={idleTipPlacement} />

        <button
          type="button"
          className="pet-surface__pet pet-surface__interactive"
          data-dragging={dragging ? 'true' : 'false'}
          aria-label="Pet companion"
          onClick={handlePetClick}
          onContextMenu={(event) => {
            event.preventDefault()
            event.stopPropagation()
            setHudOpen(false)
            void openSettingsWindow().catch((err) => {
              console.warn('[PetSurface] openSettingsWindow:', err)
            })
          }}
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={(event) => void finishDrag(event.pointerId)}
          onPointerCancel={(event) => void finishDrag(event.pointerId)}
          onPointerEnter={() => setPetHovered(true)}
          onPointerLeave={() => setPetHovered(false)}
        >
          <span className="pet-surface__sprite-shell" style={{ position: 'relative' }}>
            {activePet ? (
              <>
                <SpriteCanvas
                  pet={activePet}
                  priority={petPriority}
                  size={spriteSize}
                  animationOverride={animationOverride}
                  animationOverrideMode={animationOverrideMode}
                  contextPressure={petVitalsEnabled ? contextPressure : 0}
                  energyLevel={petVitalsEnabled ? energyLevel : 0}
                />
                {petVitalsEnabled && (
                  <PetVitals
                    contextPressure={contextPressure}
                    energyLevel={energyLevel}
                    isWorking={isWorking}
                    isIdle={isSessionIdle}
                    size={spriteSize}
                  />
                )}
              </>
            ) : petLoading ? (
              <span className="pet-surface__loading" aria-hidden="true">
                ...
              </span>
            ) : (
              <MascotRouter
                toolType={topSession?.agentType ?? 'claude-code'}
                phase={topSession?.phase ?? 'idle'}
                size={spriteSize}
              />
            )}
            <PetStatusBadges actionCount={actionCount} sessionCount={activeSessionCount} />
            {updateAvailable && (
              <span className="pet-surface__update-badge" aria-label={updateBadgeLabel}>
                <span className="pet-surface__update-dot" aria-hidden="true" />
                <span className="pet-surface__update-label">{updateBadgeLabel}</span>
              </span>
            )}
          </span>
        </button>

        <PetEmote
          emote={summon.summonEmote}
          anchorTop={154}
          anchorLeft={448}
          key={summon.summonNonce}
        />
      </div>
    </div>
  )
}

function PetIdleTip({ tip, placement }: { tip: string | null; placement: PetPanelPlacement }) {
  if (!tip) return null
  return (
    <aside
      className="pet-surface__idle-tip"
      data-placement={placement.placement}
      style={petPanelStyle(placement)}
      aria-live="polite"
    >
      <span className="pet-surface__idle-tip-label">Tips</span>
      <span className="pet-surface__idle-tip-text">{tip}</span>
    </aside>
  )
}

function PetStatusBadges({ actionCount, sessionCount }: { actionCount: number; sessionCount: number }) {
  if (actionCount <= 0 && sessionCount <= 0) return null
  return (
    <span className="pet-surface__badges" aria-hidden="true">
      {sessionCount > 0 && <PetBadge count={sessionCount} tone="session" />}
      {actionCount > 0 && <PetBadge count={actionCount} tone="action" />}
    </span>
  )
}

function PetBadge({ count, tone }: { count: number; tone: 'action' | 'session' }) {
  return (
    <span className={`pet-surface__badge pet-surface__badge--${tone}`}>
      {count > 9 ? '9+' : count}
    </span>
  )
}

function PetSessionPanel({
  sessions,
  selectedSession,
  placement,
  onBack,
  onClose,
  onJumpToTerminal,
  onSelectSession,
  onInputDraftStateChange,
  hoverHandlers,
}: {
  sessions: SessionState[]
  selectedSession: SessionState | null
  placement: PetPanelPlacement
  onBack: () => void
  onClose: () => void
  onJumpToTerminal: (sessionId: string) => void
  onSelectSession: (sessionId: string) => void
  onInputDraftStateChange: (hasDraft: boolean) => void
  hoverHandlers: PetPanelHoverHandlers
}) {
  if (selectedSession) {
    return (
      <PetSessionDetail
        onBack={onBack}
        onClose={onClose}
        onInputDraftStateChange={onInputDraftStateChange}
        hoverHandlers={hoverHandlers}
        placement={placement}
      />
    )
  }

  if (sessions.length === 0) {
    return (
      <button
        type="button"
        className="pet-surface__drawer pet-surface__drawer--empty pet-surface__interactive"
        data-placement={placement.placement}
        style={petPanelStyle(placement)}
        onClick={onClose}
        {...hoverHandlers}
      >
        <span className="pet-surface__drawer-title">All quiet</span>
        <span className="pet-surface__drawer-preview">No active sessions.</span>
      </button>
    )
  }

  return (
    <section
      className="pet-surface__drawer pet-surface__drawer--sessions pet-surface__interactive"
      data-placement={placement.placement}
      style={petPanelStyle(placement)}
      aria-label="Pet sessions"
      {...hoverHandlers}
    >
      <button type="button" className="pet-surface__icon-button pet-surface__icon-button--floating" aria-label="Close sessions" onClick={onClose}>
        x
      </button>
      <div className="pet-surface__hover-list">
        <HoverList
          sessions={sessions}
          onSessionClick={onSelectSession}
          onJumpToTerminal={onJumpToTerminal}
          onInputDraftStateChange={onInputDraftStateChange}
          hideBrandFooter
        />
      </div>
    </section>
  )
}

function PetSessionDetail({
  onBack,
  onClose,
  onInputDraftStateChange,
  hoverHandlers,
  placement,
}: {
  onBack: () => void
  onClose: () => void
  onInputDraftStateChange: (hasDraft: boolean) => void
  hoverHandlers: PetPanelHoverHandlers
  placement: PetPanelPlacement
}) {
  return (
    <section
      className="pet-surface__drawer pet-surface__drawer--detail pet-surface__interactive"
      data-placement={placement.placement}
      style={petPanelStyle(placement)}
      aria-label="Pet session detail"
      {...hoverHandlers}
    >
      <button type="button" className="pet-surface__icon-button pet-surface__icon-button--floating" aria-label="Close session detail" onClick={onClose}>
        x
      </button>
      <ChatView onBack={onBack} onInputDraftStateChange={onInputDraftStateChange} />
    </section>
  )
}

function PetBlockingOverlay({
  overlay,
  session,
  placement,
  sessionCount,
  onClose,
  onShowSessions,
}: {
  overlay: OverlayItem
  session: SessionState
  placement: PetPanelPlacement
  sessionCount: number
  onClose: () => void
  onShowSessions: () => void
}) {
  const showSessions = sessionCount > 1 ? onShowSessions : undefined

  return (
    <section
      className="pet-surface__overlay pet-surface__interactive"
      data-placement={placement.placement}
      data-overlay-type={overlay.type}
      style={petPanelStyle(placement)}
      aria-label="Pet action prompt"
    >
      {overlay.type === 'permission' && (
        <PermissionCard
          overlay={overlay}
          session={session}
          onAllow={() => { clearPermissionAfter(session.id, respondPermission(session.id, true)) }}
          onAllowAlways={() => { clearPermissionAfter(session.id, respondPermission(session.id, true, true)) }}
          onAutoApprove={() => { clearPermissionAfter(session.id, respondAutoApprove(session.id)) }}
          onDeny={(message?: string) => {
            if (message) void sendMessage(session.id, message).catch((error) => console.warn('[PetSurface] deny feedback:', error))
            clearPermissionAfter(session.id, respondPermission(session.id, false))
          }}
          onDismiss={onClose}
          onShowSessions={showSessions}
          sessionCount={sessionCount}
        />
      )}

      {overlay.type === 'question' && (
        <QuestionCard
          overlay={overlay}
          session={session}
          onAnswer={(answer) => {
            respondQuestion(session.id, answer)
              .then(() => useSessionStore.getState().clearQuestion(session.id))
              .catch((error) => console.warn('[PetSurface] respondQuestion:', error))
          }}
          onDismiss={onClose}
          onShowSessions={showSessions}
          sessionCount={sessionCount}
        />
      )}

      {overlay.type === 'plan' && (
        <PlanApprovalCard
          overlay={overlay}
          session={session}
          onSendFeedback={(message) => {
            respondPlan(session.id, 'feedback', message)
            useSessionStore.getState().clearPlan(session.id)
            onClose()
          }}
          onManualReview={() => {
            respondPlan(session.id, 'manual')
            useSessionStore.getState().clearPlan(session.id)
            onClose()
          }}
          onAcceptEdits={() => {
            respondPlan(session.id, 'acceptEdits')
            useSessionStore.getState().clearPlan(session.id)
            onClose()
          }}
          onAutoApprove={() => {
            respondPlan(session.id, 'bypassPermissions')
            useSessionStore.getState().clearPlan(session.id)
            onClose()
          }}
          onJumpToTerminal={() => jumpToTerminal(session.id).catch((error) => console.warn('[PetSurface] jumpToTerminal:', error))}
          onDismiss={onClose}
          onShowSessions={showSessions}
          sessionCount={sessionCount}
        />
      )}
    </section>
  )
}

function PetActionToast({
  actionCount,
  overlay,
  session,
  placement,
  onOpen,
}: {
  actionCount: number
  overlay: OverlayItem | null
  session: SessionState | null | undefined
  placement: PetPanelPlacement
  onOpen: () => void
}) {
  const kind = overlay && isBlockingOverlay(overlay) ? overlay.type : getSessionPendingKind(session)
  return (
    <button
      type="button"
      className="pet-surface__toast pet-surface__toast--action pet-surface__interactive"
      data-placement={placement.placement}
      style={petPanelStyle(placement)}
      onClick={onOpen}
    >
      <span className="pet-surface__toast-kicker">{formatActionKind(kind)}{actionCount > 1 ? ` · ${actionCount}` : ''}</span>
      <span className="pet-surface__toast-title">{session ? getSessionTitle(session) : 'Needs attention'}</span>
      <span className="pet-surface__toast-preview">{session ? getSessionPreview(session) : getOverlayPreview(overlay)}</span>
    </button>
  )
}

function PetMessageToast({
  overlay,
  session,
  onDismiss,
  onInputDraftStateChange,
  onPointerEnter,
  onPointerLeave,
  onShowSessions,
  placement,
  sessionCount,
}: {
  overlay: OverlayItem
  session: SessionState | null | undefined
  onDismiss: () => void
  onInputDraftStateChange: (hasDraft: boolean) => void
  onPointerEnter: () => void
  onPointerLeave: () => void
  onShowSessions: () => void
  placement: PetPanelPlacement
  sessionCount: number
}) {
  if (!session) return null

  const showSessions = sessionCount > 1 ? onShowSessions : undefined
  const jumpToSessionTerminal = () => {
    void jumpToTerminal(session.id).catch((error) => console.warn('[PetSurface] jumpToTerminal:', error))
  }

  return (
    <section
      className="pet-surface__toast pet-surface__toast--message pet-surface__interactive"
      data-placement={placement.placement}
      style={petPanelStyle(placement)}
      aria-label="Pet message notification"
      onPointerEnter={onPointerEnter}
      onPointerLeave={onPointerLeave}
    >
      {overlay.type === 'response' && (
        <OverlayResponseCard
          overlay={overlay}
          session={session}
          onJumpToTerminal={jumpToSessionTerminal}
          onShowSessions={showSessions}
          onDismiss={onDismiss}
          onDraftStateChange={onInputDraftStateChange}
          sessionCount={sessionCount}
        />
      )}
      {overlay.type === 'completion' && (
        <OverlayCompletionCard
          overlay={overlay}
          session={session}
          onJumpToTerminal={jumpToSessionTerminal}
          onShowSessions={showSessions}
          onDismiss={onDismiss}
          onDraftStateChange={onInputDraftStateChange}
          sessionCount={sessionCount}
        />
      )}
      {overlay.type === 'compacting' && (
        <OverlayCompactingCard
          overlay={overlay}
          session={session}
          onJumpToTerminal={jumpToSessionTerminal}
          onShowSessions={showSessions}
          onDismiss={onDismiss}
          sessionCount={sessionCount}
        />
      )}
    </section>
  )
}

type PetPanelPlacementName = 'top-left' | 'top-right' | 'left' | 'right' | 'bottom-left' | 'bottom-right'

type PetPanelPlacement = {
  placement: PetPanelPlacementName
  left: number
  top: number
}

function petPanelStyle(placement: PetPanelPlacement): CSSProperties {
  return {
    '--pet-panel-left': `${placement.left}px`,
    '--pet-panel-top': `${placement.top}px`,
  } as CSSProperties
}

function getPetSidePanelWidth(scale: number, preferredWidth: number, anchor: PetStageAnchor): number {
  const petSize = PET_SLOT_SIZE * scale
  const petLeft = getPetLeft(scale, anchor)
  const petRight = petLeft + petSize
  const availableWidth = anchor.x === 'left'
    ? PET_STAGE_WIDTH - petRight - PET_PANEL_GAP - PET_PANEL_MARGIN
    : petLeft - PET_PANEL_GAP - PET_PANEL_MARGIN
  return Math.max(240, Math.min(preferredWidth, availableWidth))
}

function getPetPanelPlacement(width: number, height: number, scale: number, anchor: PetStageAnchor): PetPanelPlacement {
  const petSize = PET_SLOT_SIZE * scale
  const petLeft = getPetLeft(scale, anchor)
  const petRight = petLeft + petSize
  const petTop = getPetTop(scale, anchor)
  const petBottom = petTop + petSize
  const headX = petLeft + petSize * 0.58
  const top = petTop - height - PET_PANEL_GAP
  const bottom = petBottom + PET_PANEL_GAP
  const aboveLeft = headX - width + petSize * 0.24
  const aboveRight = headX - petSize * 0.12
  const sideTop = clamp(
    petTop - PET_PANEL_GAP,
    PET_PANEL_MARGIN,
    PET_STAGE_HEIGHT - height - PET_PANEL_MARGIN,
  )
  const sideLeft = petLeft - width - PET_PANEL_GAP
  const sideRight = petRight + PET_PANEL_GAP
  const primaryCorner: PetPanelPlacementName = anchor.y === 'top'
    ? anchor.x === 'left' ? 'bottom-right' : 'bottom-left'
    : anchor.x === 'left' ? 'top-right' : 'top-left'
  const secondaryCorner: PetPanelPlacementName = anchor.y === 'top'
    ? anchor.x === 'left' ? 'top-right' : 'top-left'
    : anchor.x === 'left' ? 'bottom-right' : 'bottom-left'
  const primarySide: PetPanelPlacementName = anchor.x === 'left' ? 'right' : 'left'
  const secondarySide: PetPanelPlacementName = anchor.x === 'left' ? 'left' : 'right'

  const candidates: PetPanelPlacement[] = [
    { placement: primaryCorner, left: anchor.x === 'left' ? aboveRight : aboveLeft, top: anchor.y === 'top' ? bottom : top },
    { placement: primarySide, left: anchor.x === 'left' ? sideRight : sideLeft, top: sideTop },
    { placement: secondaryCorner, left: anchor.x === 'left' ? aboveRight : aboveLeft, top: anchor.y === 'top' ? top : bottom },
    { placement: secondarySide, left: anchor.x === 'left' ? sideLeft : sideRight, top: sideTop },
  ]

  const fitted = candidates.find((candidate) => placementFits(candidate, width, height))
  if (fitted) return fitted

  const nearest = candidates.find((candidate) => candidate.placement === (anchor.x === 'left' ? 'right' : 'left')) ?? candidates[0]
  return {
    placement: nearest.placement,
    left: clamp(nearest.left, PET_PANEL_MARGIN, PET_STAGE_WIDTH - width - PET_PANEL_MARGIN),
    top: clamp(nearest.top, PET_PANEL_MARGIN, PET_STAGE_HEIGHT - height - PET_PANEL_MARGIN),
  }
}

function getPetLeft(scale: number, anchor: PetStageAnchor): number {
  const petSize = PET_SLOT_SIZE * scale
  return anchor.x === 'left'
    ? PET_ANCHOR_RIGHT
    : PET_STAGE_WIDTH - PET_ANCHOR_RIGHT - petSize
}

function getPetTop(scale: number, anchor: PetStageAnchor): number {
  const petSize = PET_SLOT_SIZE * scale
  return anchor.y === 'top'
    ? PET_ANCHOR_BOTTOM
    : PET_STAGE_HEIGHT - PET_ANCHOR_BOTTOM - petSize
}

function placementFits(placement: PetPanelPlacement, width: number, height: number): boolean {
  return placement.left >= PET_PANEL_MARGIN
    && placement.top >= PET_PANEL_MARGIN
    && placement.left + width <= PET_STAGE_WIDTH - PET_PANEL_MARGIN
    && placement.top + height <= PET_STAGE_HEIGHT - PET_PANEL_MARGIN
}

function getPetPriority({
  actionCount,
  activePetProvider,
  doneUntil,
  topSession,
}: {
  actionCount: number
  activePetProvider: string | undefined
  doneUntil: number
  topSession: SessionState | undefined
}): Priority {
  const base = topSession ? computePriority(topSession) : PRIORITY.idle
  if (activePetProvider !== 'codex') return base
  if (actionCount > 0 || topSession?.phase === 'waiting_approval' || topSession?.phase === 'waiting_input') {
    return PRIORITY.attention
  }
  if (topSession?.phase === 'error') return PRIORITY.error
  if (doneUntil > Date.now()) return PRIORITY.done
  return PRIORITY.idle
}

function getPetActionCount(sessions: SessionState[], activeOverlay: OverlayItem | null): number {
  const sessionCount = sessions.filter(sessionNeedsPetPrompt).length
  const overlayAddsAction = Boolean(
    activeOverlay
    && isBlockingOverlay(activeOverlay)
    && !sessions.some((session) => session.id === activeOverlay.sessionId && sessionNeedsPetPrompt(session)),
  )
  return sessionCount + (overlayAddsAction ? 1 : 0)
}

function petPanelHasFocusedEditable(): boolean {
  const active = document.activeElement
  if (!(active instanceof HTMLElement)) return false
  const panel = active.closest('.pet-surface__drawer--sessions, .pet-surface__drawer--detail')
  return Boolean(panel && active.closest('input, textarea, select, [contenteditable="true"]'))
}

function petMessageToastHasFocusedEditable(): boolean {
  const active = document.activeElement
  if (!(active instanceof HTMLElement)) return false
  const toast = active.closest('.pet-surface__toast--message')
  return Boolean(toast && active.closest('input, textarea, select, [contenteditable="true"]'))
}

function sessionNeedsPetPrompt(session: SessionState): boolean {
  return session.phase === 'waiting_approval'
    || session.phase === 'waiting_input'
    || session.phase === 'error'
    || Boolean(session.pendingPermission)
    || Boolean(session.pendingQuestion)
    || Boolean(session.planTitle || session.planContent)
}

function shouldShowPetMessageToast(overlay: OverlayItem | null, dwellSeconds: number): boolean {
  if (!overlay || !isNonBlockingOverlay(overlay) || overlay.suppressed) return false
  return Date.now() - overlay.createdAt <= Math.max(1, dwellSeconds) * 1000
}

function sessionIsQuiet(session: SessionState): boolean {
  return session.phase === 'idle' || session.phase === 'done' || session.phase === 'ready'
}

function usePetStageAnchor(
  active: boolean,
  frozen: boolean,
  savedAnchor: PetStageAnchor | null,
): [PetStageAnchor, (a: PetStageAnchor) => void] {
  const [anchor, setAnchor] = useState<PetStageAnchor>(savedAnchor ?? DEFAULT_PET_STAGE_ANCHOR)

  useEffect(() => {
    if (!active) {
      const timer = window.setTimeout(() => setAnchor(DEFAULT_PET_STAGE_ANCHOR), 0)
      return () => window.clearTimeout(timer)
    }
    if (frozen || !savedAnchor) return
    const timer = window.setTimeout(() => {
      setAnchor((current) => {
        if (current.x === savedAnchor.x && current.y === savedAnchor.y) return current
        return savedAnchor
      })
    }, 0)
    return () => window.clearTimeout(timer)
  }, [active, frozen, savedAnchor])

  return [anchor, setAnchor]
}


function usePetIdleTip(active: boolean, tips: string[]): string | null {
  const [tip, setTip] = useState<string | null>(null)
  const shuffledRef = useRef<string[]>([])
  const indexRef = useRef(0)

  const nextTip = useCallback(() => {
    if (tips.length === 0) return null
    if (shuffledRef.current.length === 0 || indexRef.current >= shuffledRef.current.length) {
      shuffledRef.current = shuffleTips(tips)
      indexRef.current = 0
    }
    const next = shuffledRef.current[indexRef.current] ?? tips[0]
    indexRef.current += 1
    return next
  }, [tips])

  useEffect(() => {
    shuffledRef.current = []
    indexRef.current = 0
    const timer = window.setTimeout(() => setTip(null), 0)
    return () => window.clearTimeout(timer)
  }, [tips])

  useEffect(() => {
    let cancelled = false
    const timers: ReturnType<typeof window.setTimeout>[] = []
    const schedule = (callback: () => void, delay: number) => {
      const timer = window.setTimeout(callback, delay)
      timers.push(timer)
    }
    const hide = () => {
      if (cancelled) return
      setTip(null)
    }
    const show = () => {
      if (cancelled) return
      setTip(nextTip())
      schedule(hide, PET_IDLE_TIP_VISIBLE_MS)
      schedule(show, PET_IDLE_TIP_INTERVAL_MS)
    }

    if (!active) {
      schedule(hide, 0)
      return () => {
        cancelled = true
        timers.forEach((timer) => window.clearTimeout(timer))
      }
    }

    schedule(show, PET_IDLE_TIP_DELAY_MS)
    return () => {
      cancelled = true
      timers.forEach((timer) => window.clearTimeout(timer))
    }
  }, [active, nextTip])

  return tip
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value))
}

function getOverlaySession(overlay: OverlayItem | null, sessions: SessionState[]): SessionState | null {
  if (!overlay) return null
  return sessions.find((session) => session.id === overlay.sessionId) ?? null
}

function getSessionPendingKind(session: SessionState | null | undefined): 'permission' | 'question' | 'plan' | null {
  if (!session) return null
  if (session.pendingPermission) return 'permission'
  if (session.pendingQuestion) return 'question'
  if (session.planTitle || session.planContent) return 'plan'
  return null
}

function getSessionPreview(session: SessionState): string {
  if (session.pendingPermission?.toolName) return `${session.pendingPermission.toolName} approval`
  if (session.pendingQuestion?.question) return session.pendingQuestion.question
  if (session.planTitle) return session.planTitle
  if (session.lastToolName) return session.lastToolTarget ? `${session.lastToolName}: ${session.lastToolTarget}` : session.lastToolName
  if (session.description) return session.description.split('\n')[0]
  if (session.lastUserMessage) return session.lastUserMessage
  if (session.responseText) return session.responseText
  return formatPhase(session.phase)
}

function getOverlayPreview(overlay: OverlayItem | null): string {
  const data = overlay?.data as Record<string, unknown> | undefined
  const fields = ['summary', 'message', 'title', 'question', 'text', 'content']
  const value = fields.map((field) => data?.[field]).find((item): item is string => typeof item === 'string' && item.trim().length > 0)
  if (value) return value
  return overlay ? formatActionKind(overlay.type) : ''
}

function formatActionKind(kind: string | null | undefined): string {
  switch (kind) {
    case 'permission':
      return 'Approval'
    case 'question':
      return 'Question'
    case 'plan':
      return 'Plan'
    case 'completion':
      return 'Done'
    case 'response':
      return 'Response'
    case 'compacting':
      return 'Compacting'
    default:
      return 'Attention'
  }
}

function formatPhase(phase: SessionState['phase']): string {
  switch (phase) {
    case 'ready':
      return 'Ready'
    case 'idle':
      return 'Idle'
    case 'processing':
      return 'Working'
    case 'waiting_approval':
      return 'Needs approval'
    case 'waiting_input':
      return 'Waiting for input'
    case 'compacting':
      return 'Compacting'
    case 'done':
      return 'Done'
    case 'error':
      return 'Error'
    case 'interrupted':
      return 'Interrupted'
    default:
      return phase
  }
}

function updateDragDirection(deltaX: number, setDirection: (direction: DragDirection) => void): void {
  if (Math.abs(deltaX) < 2) {
    return
  }
  setDirection(deltaX > 0 ? 'right' : 'left')
}

function useDefensiveRegistryLoad(
  registrySize: number,
  loading: boolean,
  error: string | null,
  loadRegistry: () => Promise<void>,
) {
  const triggered = useRef(false)
  useEffect(() => {
    if (triggered.current || registrySize !== 0 || loading || error) return
    triggered.current = true
    void loadRegistry()
  }, [error, loadRegistry, loading, registrySize])
}

function debugPhaseToPriority(phase: string): Priority {
  switch (phase) {
    case 'working': return PRIORITY.working
    case 'thinking': return PRIORITY.thinking
    case 'done': return PRIORITY.done
    case 'error': return PRIORITY.error
    case 'attention': return PRIORITY.attention
    case 'compacting': return PRIORITY.compacting
    default: return PRIORITY.idle
  }
}
