/* ChatView — Conversation-style message list for active session */
import { useEffect, useRef, useState, useCallback } from 'react'
import { useTranslation } from 'react-i18next'
import { answerDisplayedPermission, useSessionStore, selectActiveSession } from '../../stores/sessionStore'
import { useConfigStore } from '../../stores/configStore'
import { getChatHistoryTail, getSubagentChatHistory } from '../../services/tauriApi'
import { mapParsedMessages } from '../../hooks/useTauri'
import { ChatHeader } from './ChatHeader'
import { SubagentList } from './SubagentList'
import { MessageBubble } from './MessageBubble'
import { CollapsedGroup } from './CollapsedGroup'
import { ApprovalBar } from './ApprovalBar'
import { TokenBar } from './TokenBar'
import type { ChatMessage, SessionState, SubagentInfo, ToolStatus } from '../../types/agent'
import { respondQuestion, respondPlan, sendMessage, jumpToTerminal, setNotchFocusable } from '../../services/tauriApi'
import { getAgentDisplayName } from '../../utils/sessionDisplay'
import './ChatView.css'

interface MessageGroup {
  type: 'collapsed' | 'single'
  messages: ChatMessage[]
}

function isCollapsible(msg: ChatMessage): boolean {
  return msg.role === 'thinking' || msg.role === 'tool_use'
}

function groupMessages(messages: ChatMessage[]): MessageGroup[] {
  const groups: MessageGroup[] = []
  let buffer: ChatMessage[] = []

  for (const msg of messages) {
    if (isCollapsible(msg)) {
      buffer.push(msg)
    } else {
      if (buffer.length >= 3) {
        groups.push({ type: 'collapsed', messages: buffer })
      } else {
        for (const b of buffer) groups.push({ type: 'single', messages: [b] })
      }
      buffer = []
      groups.push({ type: 'single', messages: [msg] })
    }
  }

  // Flush remaining buffer
  if (buffer.length >= 3) {
    groups.push({ type: 'collapsed', messages: buffer })
  } else {
    for (const b of buffer) groups.push({ type: 'single', messages: [b] })
  }

  return groups
}

function remoteDisplayMessages(session: SessionState): ChatMessage[] {
  const messages = [...session.chatHistory]

  if (!messages.some((message) => message.role === 'user')) {
    const text = session.lastUserMessage?.trim()
    if (text) {
      messages.push({
        role: 'user',
        content: text,
        timestamp: session.lastUserMessageAt || session.lastActivityAt || Date.now(),
      })
    }
  }

  if (!messages.some((message) => message.role === 'assistant')) {
    const text = (session.responseText || session.description || '').trim()
    if (text) {
      messages.push({
        role: 'assistant',
        content: text,
        timestamp: session.taskCompletedAt || session.lastActivityAt || Date.now(),
      })
    }
  }

  for (const tool of (session.activeTools || [])) {
    if (!messages.some((m) => m.role === 'tool_use' && m.toolName === tool.toolName && m.toolUseId === tool.toolUseId)) {
      messages.push({
        role: 'tool_use',
        toolName: tool.toolName,
        toolInput: tool.toolInput,
        status: tool.status as ToolStatus,
        timestamp: tool.startedAt || tool.completedAt || Date.now(),
        toolUseId: tool.toolUseId,
      })
    }
  }

  return messages
}

interface ChatViewProps {
  onBack: () => void
  initialSubagentId?: string
  onInitialSubagentHandled?: () => void
  onInputDraftStateChange?: (hasDraft: boolean) => void
}

export function ChatView({ onBack, initialSubagentId, onInitialSubagentHandled, onInputDraftStateChange }: ChatViewProps) {
  const activeSession = useSessionStore(selectActiveSession)
  const contentFontSize = useConfigStore((s) => s.contentFontSize)
  const showAgentActivityDetails = useConfigStore((s) => s.showAgentActivityDetails)
  const islandMonitorSubagents = useConfigStore((s) => s.islandMonitorSubagents)
  const contentRef = useRef<HTMLDivElement>(null)
  const [userScrolled, setUserScrolled] = useState(false)
  const [loadingHistory, setLoadingHistory] = useState(false)
  const [sendError, setSendError] = useState<string | null>(null)
  const [subagentHistory, setSubagentHistory] = useState<{
    sessionId: string
    agentId: string
    title: string
    subtitle?: string
    messages: ChatMessage[]
    loading: boolean
    error?: string
  } | null>(null)
  const activeSessionId = activeSession?.id
  const activeSessionChatLength = activeSession?.chatHistory.length ?? 0
  const subagentHistoryLength = subagentHistory?.sessionId === activeSessionId ? subagentHistory?.messages.length : undefined

  // Auto-load chat history when ChatView mounts, and refresh it as hook
  // metadata changes so detail view does not look empty while a run is active.
  // Only loads the tail (~50 messages) to keep the floating detail view light.
  useEffect(() => {
    if (!activeSessionId) return

    let cancelled = false
    queueMicrotask(() => {
      if (cancelled) return
      setLoadingHistory(activeSessionChatLength === 0)
      getChatHistoryTail(activeSessionId, { limit: 50 })
        .then((slice) => {
          if (cancelled) return
          if (slice.messages.length > 0) {
            const messages = mapParsedMessages(slice.messages)
            useSessionStore.getState().setChatHistory(activeSessionId, messages, {
              hasMore: slice.hasMore,
              firstMessageId: slice.firstMessageId ?? undefined,
              totalCount: slice.totalCount,
              transcriptPath: slice.transcriptPath ?? undefined,
            })
          }
        })
        .catch((e) => console.warn('[ChatView] getChatHistoryTail:', e))
        .finally(() => {
          if (!cancelled) setLoadingHistory(false)
        })
    })
    return () => {
      cancelled = true
    }
  }, [
    activeSessionId,
    activeSessionChatLength,
    activeSession?.lastUserMessage,
    activeSession?.responseText,
    activeSession?.description,
    activeSession?.lastToolName,
    activeSession?.phase,
  ])

  // Auto-scroll to bottom
  useEffect(() => {
    if (!userScrolled && contentRef.current) {
      contentRef.current.scrollTop = contentRef.current.scrollHeight
    }
  }, [activeSessionChatLength, subagentHistoryLength, userScrolled])

  const handleScroll = useCallback(() => {
    if (!contentRef.current) return
    const { scrollTop, scrollHeight, clientHeight } = contentRef.current
    setUserScrolled(scrollHeight - scrollTop - clientHeight > 30)
  }, [])

  // Navigate back if session disappears (e.g. session ended)
  useEffect(() => {
    if (!activeSession) {
      onBack()
    }
  }, [activeSession, onBack])

  const { t } = useTranslation()

  const handleAllow = () => {
    if (!activeSession) return
    if (activeSession.planContent) {
      respondPlan(activeSession.id, 'acceptEdits')
      useSessionStore.getState().clearPlan(activeSession.id)
      return
    }
    answerDisplayedPermission(activeSession.id, activeSession.pendingPermission, true, false)
  }

  const handleAllowAlways = () => {
    if (!activeSession) return
    if (activeSession.planContent) {
      respondPlan(activeSession.id, 'bypassPermissions')
      useSessionStore.getState().clearPlan(activeSession.id)
      return
    }
    answerDisplayedPermission(activeSession.id, activeSession.pendingPermission, true, true)
  }

  const handleDeny = () => {
    if (!activeSession) return
    if (activeSession.planContent) {
      respondPlan(activeSession.id, 'manual')
      useSessionStore.getState().clearPlan(activeSession.id)
      return
    }
    answerDisplayedPermission(activeSession.id, activeSession.pendingPermission, false)
  }

  const handleAutoApprove = () => {
    if (!activeSession) return
    if (activeSession.planContent) {
      respondPlan(activeSession.id, 'bypassPermissions')
      useSessionStore.getState().clearPlan(activeSession.id)
      return
    }
    answerDisplayedPermission(activeSession.id, activeSession.pendingPermission, 'auto')
  }

  const handleSend = async (msg: string) => {
    if (!activeSession) return
    if (!msg.trim()) return
    setSendError(null)
    try {
      if (activeSession.pendingQuestion) {
        await respondQuestion(activeSession.id, msg)
        useSessionStore.getState().clearQuestion(activeSession.id)
      } else if (activeSession.planContent) {
        await respondPlan(activeSession.id, 'feedback', msg)
        useSessionStore.getState().clearPlan(activeSession.id)
      } else {
        await sendMessage(activeSession.id, msg)
      }
      // Add user message to local chat
      useSessionStore.getState().updateSession({
        type: 'user_message',
        sessionId: activeSession.id,
        content: msg,
      })
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      setSendError(message || t('notch.sendFailed', '发送失败'))
    }
  }

  const handleJump = () => {
    if (!activeSession) return
    setNotchFocusable(false).catch(() => {})
    jumpToTerminal(activeSession.id).catch((error) => console.warn('[ChatView] jumpToTerminal:', error))
  }

  const handleBack = useCallback(() => {
    onBack()
  }, [onBack])

  const handleOpenSubagentHistory = useCallback((subagent: SubagentInfo) => {
    const historyPath = subagent.agentTranscriptPath
      || (subagent.transcriptPath ? `${subagent.transcriptPath}#agentbro-subagent=${encodeURIComponent(subagent.agentId)}` : undefined)
    if (!activeSessionId || !historyPath) return
    const title = subagent.name
      ? `@${subagent.name}`
      : (subagent.description || subagent.agentType || 'Subagent')
    const subtitle = subagent.name
      ? (subagent.description || subagent.agentType)
      : (subagent.description ? subagent.agentType : undefined)
    setSubagentHistory({
      sessionId: activeSessionId,
      agentId: subagent.agentId,
      title,
      subtitle,
      messages: [],
      loading: true,
    })
    getSubagentChatHistory(activeSessionId, historyPath)
      .then((parsed) => {
        setSubagentHistory((current) => {
          if (!current || current.sessionId !== activeSessionId || current.agentId !== subagent.agentId) return current
          return {
            ...current,
            messages: mapParsedMessages(parsed),
            loading: false,
          }
        })
      })
      .catch((e) => {
        setSubagentHistory((current) => {
          if (!current || current.sessionId !== activeSessionId || current.agentId !== subagent.agentId) return current
          return {
            ...current,
            loading: false,
            error: e instanceof Error ? e.message : String(e),
          }
        })
      })
  }, [activeSessionId])

  useEffect(() => {
    if (!initialSubagentId || !activeSession) return
    const subagent = activeSession.subagents.find((item) => item.agentId === initialSubagentId)
    if (!subagent || !(subagent.agentTranscriptPath || subagent.transcriptPath)) return
    const id = window.setTimeout(() => {
      handleOpenSubagentHistory(subagent)
      onInitialSubagentHandled?.()
    }, 0)
    return () => window.clearTimeout(id)
  }, [activeSession, handleOpenSubagentHistory, initialSubagentId, onInitialSubagentHandled])

  if (!activeSession) {
    return null
  }

  const currentSubagentHistory = subagentHistory?.sessionId === activeSession.id ? subagentHistory : null
  const displayedMessages = currentSubagentHistory?.messages ?? remoteDisplayMessages(activeSession)
  const agentName = getAgentDisplayName(activeSession)
  const showHistoryLimitNotice = !currentSubagentHistory && activeSession.chatHistoryMeta?.hasMore

  return (
    <div className="chat-view">
      <ChatHeader session={activeSession} onBack={handleBack} onJump={handleJump} />

      {islandMonitorSubagents && showAgentActivityDetails && activeSession.subagents && activeSession.subagents.length > 0 && (
        <SubagentList subagents={activeSession.subagents} onOpenHistory={handleOpenSubagentHistory} />
      )}

      <div className="chat-view__messages glass-scroll" ref={contentRef} onScroll={handleScroll} style={{ fontSize: contentFontSize }}>
        {showHistoryLimitNotice && (
          <div className="chat-view__history-limit">
            <span>{t('notch.historyLimitNotice', '性能优化，只加载最近几条消息。')}</span>
          </div>
        )}
        {currentSubagentHistory && (
          <div className="chat-view__subagent-history">
            <button className="chat-view__subagent-back" onClick={() => setSubagentHistory(null)}>
              ←
            </button>
            <div className="chat-view__subagent-copy">
              <span className="chat-view__subagent-title">{currentSubagentHistory.title}</span>
              {currentSubagentHistory.subtitle && (
                <span className="chat-view__subagent-subtitle">{currentSubagentHistory.subtitle}</span>
              )}
            </div>
            <span className="chat-view__subagent-badge">readonly</span>
          </div>
        )}

        {/* Error state banner */}
        {activeSession.phase === 'error' && (
          <div className="chat-view__error-banner">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" aria-hidden>
              <circle cx="12" cy="12" r="10" stroke="var(--red, #ff453a)" strokeWidth="2" />
              <path d="M12 8v5M12 16h.01" stroke="var(--red, #ff453a)" strokeWidth="2" strokeLinecap="round" />
            </svg>
            <div className="chat-view__error-body">
              <span className="chat-view__error-title">{t('notch.sessionError')}</span>
              <span className="chat-view__error-message">
                {activeSession.description || t('notch.unexpectedError')}
              </span>
              <span className="chat-view__error-hint">
                {t('notch.checkTerminal')}
              </span>
            </div>
            <button className="chat-view__error-jump" onClick={handleJump}>
              {t('notch.openTerminal')}
            </button>
          </div>
        )}

        {currentSubagentHistory?.loading ? (
          <div className="chat-view__empty">
            <span>{t('notch.loadingHistory', 'Loading history...')}</span>
          </div>
        ) : currentSubagentHistory?.error ? (
          <div className="chat-view__empty">
            <span>{currentSubagentHistory.error}</span>
          </div>
        ) : displayedMessages.length === 0 ? (
          <div className="chat-view__empty">
            <span className="chat-view__empty-icon">💬</span>
            <span>{loadingHistory ? t('notch.loadingHistory', 'Loading history...') : t('notch.waitingMessages')}</span>
          </div>
        ) : (
          groupMessages(displayedMessages).map((group, i) =>
            group.type === 'collapsed' ? (
              <CollapsedGroup key={`g-${i}`} messages={group.messages} />
            ) : (
              <MessageBubble
                key={`m-${i}`}
                message={group.messages[0]}
                agentType={activeSession.agentType}
                agentName={agentName}
              />
            )
          )
        )}
      </div>

      {/* Scroll-to-bottom button when user has scrolled up */}
      {userScrolled && (
        <button
          className="chat-view__scroll-btn"
          onClick={() => {
            setUserScrolled(false)
            contentRef.current?.scrollTo({ top: contentRef.current.scrollHeight, behavior: 'smooth' })
          }}
        >
          ↓
        </button>
      )}

      {!currentSubagentHistory && sendError && (
        <div className="chat-view__send-error">
          <span>{sendError}</span>
          <button type="button" onClick={() => setSendError(null)}>
            {t('notch.dismiss', '关闭')}
          </button>
        </div>
      )}

      {!currentSubagentHistory && (
        <ApprovalBar
          session={activeSession}
          onAllow={handleAllow}
          onAllowAlways={handleAllowAlways}
          onDeny={handleDeny}
          onAutoApprove={handleAutoApprove}
          onSendMessage={handleSend}
          onDraftStateChange={onInputDraftStateChange}
          onJumpToHostApp={() => jumpToTerminal(activeSession.id).catch((error) => console.warn('[ChatView] jumpToTerminal:', error))}
        />
      )}

      <TokenBar tokens={activeSession.tokens} />
    </div>
  )
}
