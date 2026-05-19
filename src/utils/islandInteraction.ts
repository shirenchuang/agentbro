import type { OverlayItem, PanelState, SessionState } from '../types/agent'

export type IslandOuterState = 'hidden' | 'micro' | 'compact' | 'expanded'
export type IslandInteractionMode = 'persistent' | 'minimal'

export interface IslandInteractionInput {
  sessions: SessionState[]
  panelState: PanelState
  activeOverlay: OverlayItem | null
  interactionMode: IslandInteractionMode
  persistentIdleHidden: boolean
  keepCompactAfterActive?: boolean
  wakeSilenced: boolean
}

export interface IslandInteractionSnapshot {
  outerState: IslandOuterState
  hasActiveSession: boolean
  hasBlockingSignal: boolean
  hasRunningSession: boolean
  hasErrorSession: boolean
  hasNonBlockingOverlay: boolean
  isMicro: boolean
  isHidden: boolean
}

const BLOCKING_OVERLAYS = new Set(['permission', 'question', 'plan'])

export function isBlockingOverlay(overlay: OverlayItem | null): boolean {
  return overlay ? BLOCKING_OVERLAYS.has(overlay.type) : false
}

export function isNonBlockingOverlay(overlay: OverlayItem | null): boolean {
  return overlay?.type === 'completion' || overlay?.type === 'response' || overlay?.type === 'compacting'
}

export function hasUnfinishedTasks(session: SessionState): boolean {
  return Boolean(session.tasks?.some((task) => task.status !== 'completed'))
}

export function sessionNeedsAttention(session: SessionState): boolean {
  return session.phase === 'waiting_approval'
    || session.phase === 'waiting_input'
    || session.phase === 'error'
    || Boolean(session.pendingPermission)
    || Boolean(session.pendingQuestion)
    || Boolean(session.planTitle || session.planContent)
    || hasUnfinishedTasks(session)
}

export function getFollowFocusVisibleSessions(
  sessions: SessionState[],
  followFocus: boolean,
  focusedSessionIds: Set<string> | null,
): SessionState[] {
  if (!followFocus || focusedSessionIds === null) return sessions
  return sessions.filter((session) => !session.pid || !session.terminal || focusedSessionIds.has(session.id))
}

export function deriveIslandInteraction(input: IslandInteractionInput): IslandInteractionSnapshot {
  const { sessions, panelState, activeOverlay, interactionMode, persistentIdleHidden, keepCompactAfterActive = false, wakeSilenced } = input
  const hasRunningSession = sessions.some((session) => session.phase === 'processing' || session.phase === 'compacting')
  const hasWaitingSession = sessions.some(sessionNeedsAttention)
  const hasErrorSession = sessions.some((session) => session.phase === 'error')
  const hasBlockingSignal = hasWaitingSession || hasErrorSession || isBlockingOverlay(activeOverlay)
  const hasNonBlockingOverlay = isNonBlockingOverlay(activeOverlay)
  const hasActiveSession = hasRunningSession || hasWaitingSession || hasErrorSession

  if (panelState !== 'collapsed') {
    return {
      outerState: 'expanded',
      hasActiveSession,
      hasBlockingSignal,
      hasRunningSession,
      hasErrorSession,
      hasNonBlockingOverlay,
      isMicro: false,
      isHidden: false,
    }
  }

  if (interactionMode === 'minimal') {
    const mayShowNotification = hasNonBlockingOverlay && !wakeSilenced
    const isHidden = !hasBlockingSignal && !mayShowNotification
    return {
      outerState: isHidden ? 'hidden' : 'compact',
      hasActiveSession,
      hasBlockingSignal,
      hasRunningSession,
      hasErrorSession,
      hasNonBlockingOverlay,
      isMicro: false,
      isHidden,
    }
  }

  if (wakeSilenced && !hasBlockingSignal) {
    return {
      outerState: 'hidden',
      hasActiveSession,
      hasBlockingSignal,
      hasRunningSession,
      hasErrorSession,
      hasNonBlockingOverlay,
      isMicro: false,
      isHidden: true,
    }
  }

  const isHidden = persistentIdleHidden && !hasActiveSession && !hasNonBlockingOverlay && !hasBlockingSignal && !keepCompactAfterActive
  const isMicro = !isHidden && !keepCompactAfterActive && !hasRunningSession && !hasErrorSession && !hasBlockingSignal && !hasNonBlockingOverlay

  return {
    outerState: isHidden ? 'hidden' : isMicro ? 'micro' : 'compact',
    hasActiveSession,
    hasBlockingSignal,
    hasRunningSession,
    hasErrorSession,
    hasNonBlockingOverlay,
    isMicro,
    isHidden,
  }
}
