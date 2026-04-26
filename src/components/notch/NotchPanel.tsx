/* Agent Island — Notch Panel (3-state Dynamic Island) */
import { useEffect, useRef, useState, useMemo } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import { useSessionStore, selectSessionList, selectPanelState, selectRateLimits } from '../../stores/sessionStore'
import { useConfigStore } from '../../stores/configStore'
import { respondPermission, sendMessage, jumpToTerminal, resizeNotch, setNotchOpacity, getChatHistory } from '../../services/tauriApi'
import { mapParsedMessages } from '../../hooks/useTauri'
import { CollapsedBar } from './CollapsedBar'
import { HoverList } from './HoverList'
import { ChatView } from './ChatView'
import './NotchPanel.css'

const springTransition = {
  type: 'spring' as const,
  stiffness: 400,
  damping: 30,
  mass: 0.8,
}

export function NotchPanel() {
  const panelState = useSessionStore(selectPanelState)
  const setPanelState = useSessionStore((s) => s.setPanelState)
  const sessions = useSessionStore(selectSessionList)
  const rateLimits = useSessionStore(selectRateLimits)
  const dwellDuration = useConfigStore((s) => s.dwellDuration)
  const notchStyle = useConfigStore((s) => s.notchStyle)
  const maxPanelHeight = useConfigStore((s) => s.maxPanelHeight)
  const autoCollapse = useConfigStore((s) => s.autoCollapse)
  const shortcuts = useConfigStore((s) => s.shortcuts)
  const leaveTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const [bouncing, setBouncing] = useState(false)

  // Track sessions needing attention for bounce animation
  const attentionCount = useMemo(
    () => sessions.filter(s => s.phase === 'waiting_approval').length,
    [sessions],
  )

  // Bounce animation + auto-expand on new permission request
  const prevAttentionRef = useRef(0)
  useEffect(() => {
    if (attentionCount > 0) {
      setBouncing(true)
      // Auto-expand to hover when a new permission request arrives
      if (attentionCount > prevAttentionRef.current && panelState === 'collapsed') {
        setPanelState('hover')
      }
    }
    prevAttentionRef.current = attentionCount
  }, [attentionCount, panelState, setPanelState])

  // Auto-collapse after permission is resolved (attentionCount drops to 0)
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

  // Auto-hide when no sessions: fade out after 1 second, restore on new session
  const autoHideNoSessions = useConfigStore((s) => s.autoHideNoSessions)
  const autoHideTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  useEffect(() => {
    if (!autoHideNoSessions) return

    if (sessions.length === 0) {
      // Start 1-second timer before hiding
      autoHideTimerRef.current = setTimeout(() => {
        setNotchOpacity(0)
      }, 1000)
    } else {
      // Sessions appeared — cancel timer and restore visibility
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

  // Mouse enter — clear pending leave timer and expand
  const handleMouseEnter = () => {
    if (leaveTimerRef.current) {
      clearTimeout(leaveTimerRef.current)
      leaveTimerRef.current = undefined
    }
    if (panelState === 'collapsed') {
      setPanelState('hover')
    }
  }

  // Mouse leave — configurable dwell delay before collapsing
  const handleMouseLeave = () => {
    if (!autoCollapse) return
    if (panelState === 'hover') {
      leaveTimerRef.current = setTimeout(() => {
        const current = useSessionStore.getState().panelState
        if (current === 'hover') setPanelState('collapsed')
      }, dwellDuration)
    }
  }

  // Cleanup leave timer on unmount
  useEffect(() => {
    return () => {
      if (leaveTimerRef.current) clearTimeout(leaveTimerRef.current)
    }
  }, [])

  // Keyboard shortcuts (configurable via settings)
  useEffect(() => {
    /** Check if a keyboard event matches a shortcut binding */
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
      // Collapse panel
      const collapseBinding = findShortcut('collapse-panel')
      if (collapseBinding && matchesShortcut(e, collapseBinding)) {
        setPanelState('collapsed')
        return
      }

      const store = useSessionStore.getState()
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

      // Jump to terminal (Cmd+J default)
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

    // Fetch full chat history from the JSONL file when expanding
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

  // Vibe Island sizing — wider panel
  const isCompact = notchStyle === 'compact'
  const panelWidth = panelState === 'collapsed' ? (isCompact ? 280 : 340) : (isCompact ? 560 : 620)

  // Height: status bar (32px) + main bar (36px) + session cards
  const statusBarHeight = panelState !== 'collapsed' ? 32 : 0
  const panelHeight =
    panelState === 'collapsed'
      ? 36
      : panelState === 'hover'
        ? Math.min(statusBarHeight + 36 + Math.max(sessions.length, 1) * 72 + 16, 480)
        : (maxPanelHeight || 560)

  // Resize the Tauri window to match panel content (with padding for position offset)
  useEffect(() => {
    // Add some padding: 8px top offset + 8px bottom breathing room
    const windowHeight = panelHeight + 16
    const windowWidth = panelWidth + 20 // 10px each side for shadow
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
            {/* Hover state: session list */}
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

            {/* Expanded state: full detail */}
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
        </motion.div>
      </div>
    </div>
  )
}
