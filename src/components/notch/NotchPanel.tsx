/* Agent Island — Notch Panel (Layered Dynamic Island) */
import { useEffect, useRef, useState, useMemo } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import { useSessionStore, selectSessionList, selectPanelState, selectRateLimits, selectActiveOverlay } from '../../stores/sessionStore'
import { useConfigStore } from '../../stores/configStore'
import { respondPermission, sendMessage, jumpToTerminal, resizeNotch, setNotchOpacity, getChatHistory } from '../../services/tauriApi'
import { mapParsedMessages } from '../../hooks/useTauri'
import { computePriority, PRIORITY } from '../../types/priority'
import type { OverlayItem } from '../../types/agent'
import { CollapsedBar } from './CollapsedBar'
import { HoverList } from './HoverList'
import { ChatView } from './ChatView'
import { PermissionCard } from '../overlay/PermissionCard'
import { PlanApprovalCard } from '../overlay/PlanApprovalCard'
import { QuestionCard } from '../overlay/QuestionCard'
import { OverlayResponseCard } from '../overlay/OverlayResponseCard'
import { OverlayCompletionCard } from '../overlay/OverlayCompletionCard'
import './NotchPanel.css'

const springTransition = {
  type: 'spring' as const,
  stiffness: 400,
  damping: 30,
  mass: 0.8,
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
          onDeny={() => { respondPermission(session.id, false); useSessionStore.getState().clearPermission(session.id) }}
          onDismiss={onDismiss}
        />
      )
    case 'plan':
      return (
        <PlanApprovalCard
          overlay={overlay}
          session={session}
          onSendFeedback={(msg) => sendMessage(session.id, msg)}
          onAcceptEdits={() => { sendMessage(session.id, 'acceptEdits'); onDismiss() }}
          onAutoApprove={() => { sendMessage(session.id, 'bypassPermissions'); onDismiss() }}
          onDismiss={onDismiss}
        />
      )
    case 'question':
      return (
        <QuestionCard
          overlay={overlay}
          session={session}
          onAnswer={(answer) => { sendMessage(session.id, answer); useSessionStore.getState().clearQuestion(session.id) }}
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
  const leaveTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const [bouncing, setBouncing] = useState(false)

  // Track sessions needing attention via overlay queue
  const attentionCount = useMemo(
    () => sessions.filter(s => computePriority(s) === PRIORITY.attention).length,
    [sessions],
  )

  // Bounce animation + auto-expand on new attention event
  const prevAttentionRef = useRef(0)
  useEffect(() => {
    if (attentionCount > 0) {
      setBouncing(true)
      if (attentionCount > prevAttentionRef.current && panelState === 'collapsed') {
        setPanelState('hover')
      }
    }
    prevAttentionRef.current = attentionCount
  }, [attentionCount, panelState, setPanelState])

  // Auto-collapse after all attention resolved
  const collapseTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  useEffect(() => {
    if (attentionCount === 0 && prevAttentionRef.current > 0 && panelState === 'hover') {
      collapseTimerRef.current = setTimeout(() => {
        const current = useSessionStore.getState().panelState
        if (current === 'hover') setPanelState('collapsed')
      }, 800)
    }
    return () => {
      if (collapseTimerRef.current) clearTimeout(collapseTimerRef.current)
    }
  }, [attentionCount, panelState, setPanelState])

  // Auto-hide when no sessions
  const autoHideNoSessions = useConfigStore((s) => s.autoHideNoSessions)
  const autoHideTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  useEffect(() => {
    if (!autoHideNoSessions) return

    if (sessions.length === 0) {
      autoHideTimerRef.current = setTimeout(() => {
        setNotchOpacity(0)
      }, 1000)
    } else {
      if (autoHideTimerRef.current) {
        clearTimeout(autoHideTimerRef.current)
        autoHideTimerRef.current = undefined
      }
      setNotchOpacity(1)
    }

    return () => {
      if (autoHideTimerRef.current) {
        clearTimeout(autoHideTimerRef.current)
      }
    }
  }, [sessions.length, autoHideNoSessions])

  // Mouse enter
  const handleMouseEnter = () => {
    if (leaveTimerRef.current) {
      clearTimeout(leaveTimerRef.current)
      leaveTimerRef.current = undefined
    }
    if (panelState === 'collapsed') {
      setPanelState('hover')
    }
  }

  // Mouse leave
  const handleMouseLeave = () => {
    if (!autoCollapse) return
    if (panelState === 'hover') {
      leaveTimerRef.current = setTimeout(() => {
        const current = useSessionStore.getState().panelState
        if (current === 'hover') setPanelState('collapsed')
      }, dwellDuration)
    }
  }

  useEffect(() => {
    return () => {
      if (leaveTimerRef.current) clearTimeout(leaveTimerRef.current)
    }
  }, [])

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
            setPanelState('collapsed')
          }
        } else if (store.panelState === 'expanded') {
          setPanelState('hover')
        } else if (store.panelState === 'hover') {
          setPanelState('collapsed')
        }
        return
      }

      // Collapse panel shortcut
      const collapseBinding = findShortcut('collapse-panel')
      if (collapseBinding && matchesShortcut(e, collapseBinding)) {
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
  }, [setPanelState, shortcuts])

  const handleSessionClick = (sessionId: string) => {
    useSessionStore.getState().setActiveSession(sessionId)
    setPanelState('expanded')

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
    setPanelState('collapsed')
  }

  // Sizing
  const isCompact = notchStyle === 'compact'
  const hasOverlay = activeOverlay !== null
  const panelWidth = panelState === 'collapsed' ? (isCompact ? 280 : 340) : (isCompact ? 560 : 620)

  const statusBarHeight = panelState !== 'collapsed' ? 32 : 0
  const overlayExtraHeight = hasOverlay ? 120 : 0
  const panelHeight =
    panelState === 'collapsed'
      ? 36
      : panelState === 'hover'
        ? Math.min(statusBarHeight + 36 + Math.max(sessions.length, 1) * 72 + 16 + overlayExtraHeight, 480)
        : (maxPanelHeight || 560)

  useEffect(() => {
    const windowHeight = panelHeight + 16
    const windowWidth = panelWidth + 20
    resizeNotch(windowWidth, windowHeight)
  }, [panelWidth, panelHeight])

  return (
    <div className="notch-container">
      <div
        className={`notch-bounce ${bouncing ? 'notch-bounce--active' : ''}`}
        onAnimationEnd={() => setBouncing(false)}
      >
        <motion.div
          className="notch-panel"
          role="region"
          aria-label="Agent Island"
          aria-expanded={panelState !== 'collapsed'}
          layout
          animate={{
            width: panelWidth,
            height: panelHeight,
          }}
          transition={springTransition}
          onMouseEnter={handleMouseEnter}
          onMouseLeave={handleMouseLeave}
          style={{ overflow: 'hidden' }}
        >
          {/* Header — always visible */}
          <CollapsedBar
            sessions={sessions}
            panelState={panelState}
            rateLimits={rateLimits}
            onCollapse={handleCollapse}
          />

          <AnimatePresence mode="wait">
            {/* Base layer: session list */}
            {panelState === 'hover' && (
              <motion.div
                key="hover"
                initial={{ opacity: 0, y: -10 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -10 }}
                transition={{ type: 'spring', stiffness: 500, damping: 35 }}
              >
                <HoverList
                  sessions={sessions}
                  onSessionClick={handleSessionClick}
                  onJumpToTerminal={(id) => jumpToTerminal(id)}
                />
              </motion.div>
            )}

            {/* Base layer: detail view */}
            {panelState === 'expanded' && (
              <motion.div
                key="expanded"
                initial={{ opacity: 0, y: -12 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -12 }}
                transition={{ type: 'spring', stiffness: 400, damping: 30 }}
                style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}
              >
                <ChatView onBack={() => setPanelState('hover')} />
              </motion.div>
            )}
          </AnimatePresence>

          {/* Overlay layer — renders on top of base layer */}
          <AnimatePresence>
            {activeOverlay && panelState !== 'collapsed' && (
              <motion.div
                key={`overlay-${activeOverlay.id}`}
                className="notch-panel__overlay"
                initial={{ opacity: 0, y: 8 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: 8 }}
                transition={{ type: 'spring', stiffness: 500, damping: 35 }}
              >
                <OverlayRenderer overlay={activeOverlay} onDismiss={() => dismissOverlay(activeOverlay.id)} />
              </motion.div>
            )}
          </AnimatePresence>
        </motion.div>
      </div>
    </div>
  )
}
