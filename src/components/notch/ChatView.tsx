/* ChatView — Conversation-style message list for active session */
import { useEffect, useRef, useState, useCallback } from 'react'
import { useTranslation } from 'react-i18next'
import { useSessionStore, selectActiveSession } from '../../stores/sessionStore'
import { useConfigStore } from '../../stores/configStore'
import { getChatHistory, getSubagentChatHistory } from '../../services/tauriApi'
import { mapParsedMessages } from '../../hooks/useTauri'
import { ChatHeader } from './ChatHeader'
import { SubagentList } from './SubagentList'
import { MessageBubble } from './MessageBubble'
import { CollapsedGroup } from './CollapsedGroup'
import { ApprovalBar } from './ApprovalBar'
import { TokenBar } from './TokenBar'
import type { ChatMessage, SubagentInfo } from '../../types/agent'
import { respondPermission, sendMessage, jumpToTerminal, respondAutoApprove, setNotchFocusable } from '../../services/tauriApi'
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

interface ChatViewProps {
  onBack: () => void
}

export function ChatView({ onBack }: ChatViewProps) {
  const activeSession = useSessionStore(selectActiveSession)
  const contentFontSize = useConfigStore((s) => s.contentFontSize)
  const showAgentActivityDetails = useConfigStore((s) => s.showAgentActivityDetails)
  const contentRef = useRef<HTMLDivElement>(null)
  const [userScrolled, setUserScrolled] = useState(false)
  const [subagentHistory, setSubagentHistory] = useState<{
    agentId: string
    title: string
    subtitle?: string
    messages: ChatMessage[]
    loading: boolean
    error?: string
  } | null>(null)

  useEffect(() => {
    setSubagentHistory(null)
  }, [activeSession?.id])

  // Auto-load chat history when ChatView mounts and history is empty
  useEffect(() => {
    if (!activeSession) return
    if (activeSession.chatHistory.length > 0) return

    getChatHistory(activeSession.id)
      .then((parsed) => {
        if (parsed.length > 0) {
          const messages = mapParsedMessages(parsed)
          useSessionStore.getState().setChatHistory(activeSession.id, messages)
        }
      })
      .catch((e) => console.warn('[ChatView] getChatHistory:', e))
  }, [activeSession?.id])

  // Auto-scroll to bottom
  useEffect(() => {
    if (!userScrolled && contentRef.current) {
      contentRef.current.scrollTop = contentRef.current.scrollHeight
    }
  }, [activeSession?.chatHistory.length, subagentHistory?.messages.length, userScrolled])

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

  if (!activeSession) {
    return null
  }

  const handleAllow = () => {
    respondPermission(activeSession.id, true, false)
    useSessionStore.getState().clearPermission(activeSession.id)
  }

  const handleAllowAlways = () => {
    respondPermission(activeSession.id, true, true)
    useSessionStore.getState().clearPermission(activeSession.id)
  }

  const handleDeny = () => {
    respondPermission(activeSession.id, false)
    useSessionStore.getState().clearPermission(activeSession.id)
  }

  const handleAutoApprove = () => {
    respondAutoApprove(activeSession.id)
    useSessionStore.getState().clearPermission(activeSession.id)
  }

  const handleSend = (msg: string) => {
    if (!msg.trim()) return
    sendMessage(activeSession.id, msg)
    // Add user message to local chat
    useSessionStore.getState().updateSession({
      type: 'user_message',
      sessionId: activeSession.id,
      content: msg,
    })
    if (activeSession.pendingQuestion) {
      useSessionStore.getState().clearQuestion(activeSession.id)
    }
  }

  const handleJump = () => {
    setNotchFocusable(false).catch(() => {})
    jumpToTerminal(activeSession.id)
  }

  const handleBack = useCallback(() => {
    setNotchFocusable(false).catch(() => {})
    onBack()
  }, [onBack])

  const handleOpenSubagentHistory = useCallback((subagent: SubagentInfo) => {
    if (!activeSession || !subagent.agentTranscriptPath) return
    const title = subagent.agentType || 'Subagent'
    const subtitle = subagent.description || subagent.lastAssistantMessage
    setSubagentHistory({
      agentId: subagent.agentId,
      title,
      subtitle,
      messages: [],
      loading: true,
    })
    getSubagentChatHistory(activeSession.id, subagent.agentTranscriptPath)
      .then((parsed) => {
        setSubagentHistory((current) => {
          if (!current || current.agentId !== subagent.agentId) return current
          return {
            ...current,
            messages: mapParsedMessages(parsed),
            loading: false,
          }
        })
      })
      .catch((e) => {
        setSubagentHistory((current) => {
          if (!current || current.agentId !== subagent.agentId) return current
          return {
            ...current,
            loading: false,
            error: e instanceof Error ? e.message : String(e),
          }
        })
      })
  }, [activeSession?.id])

  const displayedMessages = subagentHistory?.messages ?? activeSession.chatHistory

  return (
    <div className="chat-view">
      <ChatHeader session={activeSession} onBack={handleBack} onJump={handleJump} />

      {showAgentActivityDetails && activeSession.subagents && activeSession.subagents.length > 0 && (
        <SubagentList subagents={activeSession.subagents} onOpenHistory={handleOpenSubagentHistory} />
      )}

      <div className="chat-view__messages glass-scroll" ref={contentRef} onScroll={handleScroll} style={{ fontSize: contentFontSize }}>
        {subagentHistory && (
          <div className="chat-view__subagent-history">
            <button className="chat-view__subagent-back" onClick={() => setSubagentHistory(null)}>
              ←
            </button>
            <div className="chat-view__subagent-copy">
              <span className="chat-view__subagent-title">{subagentHistory.title}</span>
              {subagentHistory.subtitle && (
                <span className="chat-view__subagent-subtitle">{subagentHistory.subtitle}</span>
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

        {subagentHistory?.loading ? (
          <div className="chat-view__empty">
            <span>{t('notch.loadingHistory', 'Loading history...')}</span>
          </div>
        ) : subagentHistory?.error ? (
          <div className="chat-view__empty">
            <span>{subagentHistory.error}</span>
          </div>
        ) : displayedMessages.length === 0 ? (
          <div className="chat-view__empty">
            <span className="chat-view__empty-icon">💬</span>
            <span>{t('notch.waitingMessages')}</span>
          </div>
        ) : (
          groupMessages(displayedMessages).map((group, i) =>
            group.type === 'collapsed' ? (
              <CollapsedGroup key={`g-${i}`} messages={group.messages} />
            ) : (
              <MessageBubble key={`m-${i}`} message={group.messages[0]} />
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

      <ApprovalBar
        session={activeSession}
        onAllow={handleAllow}
        onAllowAlways={handleAllowAlways}
        onDeny={handleDeny}
        onAutoApprove={handleAutoApprove}
        onSendMessage={handleSend}
      />

      <TokenBar tokens={activeSession.tokens} />
    </div>
  )
}
