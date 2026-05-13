import { useCallback, useMemo, useRef, useState, type CSSProperties, type PointerEvent } from 'react'
import { useTranslation } from 'react-i18next'
import type { OverlayItem, SessionState } from '../../types/agent'
import { PRIORITY, computePriority } from '../../types/priority'
import { useSessionStore } from '../../stores/sessionStore'
import {
  getChatHistory,
  jumpToTerminal,
  respondPermission,
  respondPlan,
  respondQuestion,
  sendMessage,
  setNotchFocusable,
  startPetDrag,
  endPetDrag,
} from '../../services/tauriApi'
import { mapParsedMessages } from '../../hooks/useTauri'
import { getToolActivityLabel } from '../../utils/toolLabels'
import { useConfigStore } from '../../stores/configStore'
import { useThemeStore } from '../../stores/themeStore'
import { MascotRouter } from './mascots'
import { SpriteCanvas } from './SpriteCanvas'
import './PetSurface.css'

interface PetSurfaceProps {
  sessions: SessionState[]
  activeOverlay: OverlayItem | null
  scale: number
  hidden: boolean
  onCollapse: () => void
  onDismissOverlay: (id: string) => void
}

type PetActionKind = 'permission' | 'question' | 'plan' | null

function enablePetTextInput() {
  setNotchFocusable(true).catch(() => {})
}

function disablePetTextInput() {
  setNotchFocusable(false).catch(() => {})
}

export function PetSurface({
  sessions,
  activeOverlay,
  scale,
  hidden,
  onCollapse,
  onDismissOverlay,
}: PetSurfaceProps) {
  const [expandedSessionId, setExpandedSessionId] = useState<string | null>(null)
  const [dragging, setDragging] = useState(false)
  const dragCandidateRef = useRef<{ pointerId: number; startX: number; startY: number } | null>(null)
  const dragPointerIdRef = useRef<number | null>(null)
  const suppressClickRef = useRef(false)
  const updateConfig = useConfigStore((s) => s.updateConfig)
  const activeTheme = useThemeStore((s) => s.activeTheme)
  const sortedSessions = useMemo(
    () => [...sessions].sort((a, b) => computePriority(b) - computePriority(a)).slice(0, 4),
    [sessions],
  )
  const topSession = sortedSessions[0]
  const actionKind = getActionKind(activeOverlay)
  const activeSession = activeOverlay ? sessions.find((session) => session.id === activeOverlay.sessionId) : undefined
  const displayScale = Math.min(1.2, Math.max(0.5, scale / 100))

  const finishDrag = useCallback(async (pointerId?: number) => {
    if (dragPointerIdRef.current == null) {
      dragCandidateRef.current = null
      return
    }
    if (pointerId != null && dragPointerIdRef.current !== pointerId) return
    dragPointerIdRef.current = null
    dragCandidateRef.current = null
    setDragging(false)
    try {
      const origin = await endPetDrag()
      if (origin) updateConfig('islandPetWindowOrigin', origin)
    } catch (err) {
      console.warn('[PetSurface] endPetDrag:', err)
    }
    window.setTimeout(() => {
      suppressClickRef.current = false
    }, 0)
  }, [updateConfig])

  const handlePointerDown = (event: PointerEvent<HTMLButtonElement>) => {
    if (event.button !== 0) return
    dragCandidateRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
    }
  }

  const handlePointerMove = (event: PointerEvent<HTMLButtonElement>) => {
    if (dragPointerIdRef.current === event.pointerId) return
    const candidate = dragCandidateRef.current
    if (!candidate || candidate.pointerId !== event.pointerId) return
    if (Math.hypot(event.clientX - candidate.startX, event.clientY - candidate.startY) < 4) return
    suppressClickRef.current = true
    dragPointerIdRef.current = event.pointerId
    setDragging(true)
    startPetDrag().then((started) => {
      if (!started && dragPointerIdRef.current === event.pointerId) {
        dragPointerIdRef.current = null
        setDragging(false)
      }
    }).catch((err) => {
      console.warn('[PetSurface] startPetDrag:', err)
      if (dragPointerIdRef.current === event.pointerId) {
        dragPointerIdRef.current = null
        setDragging(false)
      }
    })
  }

  return (
    <div
      className="pet-surface"
      data-hidden={hidden ? 'true' : 'false'}
      style={{ '--pet-scale': displayScale } as CSSProperties}
    >
      <div className="pet-surface__stage">
        <SessionFan
          expandedSessionId={expandedSessionId}
          sessions={sortedSessions}
          onCollapse={onCollapse}
          onOpenSession={setExpandedSessionId}
        />

        {activeOverlay && activeSession && (
          <ActionToast
            kind={actionKind}
            overlay={activeOverlay}
            session={activeSession}
            onDismiss={() => onDismissOverlay(activeOverlay.id)}
          />
        )}

        {!activeOverlay && topSession && topSession.phase === 'done' && (
          <MessageToast session={topSession} onDismiss={onCollapse} />
        )}

        <button
          type="button"
          className="pet-surface__pet"
          data-dragging={dragging ? 'true' : 'false'}
          aria-label="Open pet status"
          onClick={(event) => {
            if (suppressClickRef.current) {
              event.preventDefault()
              suppressClickRef.current = false
              return
            }
            if (expandedSessionId || activeOverlay) {
              setExpandedSessionId(null)
              onCollapse()
            } else if (topSession) {
              setExpandedSessionId(topSession.id)
            }
          }}
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={(event) => void finishDrag(event.pointerId)}
          onPointerCancel={(event) => void finishDrag(event.pointerId)}
        >
          {activeTheme.character ? (
            <SpriteCanvas
              theme={activeTheme}
              priority={topSession ? computePriority(topSession) : PRIORITY.idle}
              size={112}
            />
          ) : (
            <MascotRouter
              toolType={topSession?.agentType ?? 'claude-code'}
              phase={topSession?.phase ?? 'idle'}
              size={112}
            />
          )}
          <PetBadges
            actionCount={activeOverlay ? 1 : 0}
            sessionCount={sessions.filter((session) => session.phase !== 'done').length}
          />
        </button>
      </div>
    </div>
  )
}

function SessionFan({
  expandedSessionId,
  onCollapse,
  onOpenSession,
  sessions,
}: {
  expandedSessionId: string | null
  onCollapse: () => void
  onOpenSession: (id: string | null) => void
  sessions: SessionState[]
}) {
  if (expandedSessionId) {
    const session = sessions.find((item) => item.id === expandedSessionId)
    if (session) {
      return <SessionDetail session={session} onBack={() => onOpenSession(null)} />
    }
  }

  if (sessions.length === 0) {
    return (
      <button type="button" className="pet-card pet-card--quiet" onClick={onCollapse}>
        <strong>All quiet</strong>
        <span>No active sessions.</span>
      </button>
    )
  }

  return (
    <div className="pet-surface__fan">
      {sessions.map((session, index) => (
        <button
          type="button"
          key={session.id}
          className="pet-card pet-session"
          style={{ '--fan-index': index, '--fan-count': sessions.length } as CSSProperties}
          onClick={() => onOpenSession(session.id)}
        >
          <span className={`pet-dot pet-dot--${toneForPhase(session.phase)}`} />
          <span className="pet-session__body">
            <span className="pet-session__title">{session.sessionTitle || session.project || session.id}</span>
            <span className="pet-session__preview">{getSessionPreview(session)}</span>
          </span>
          <span className="pet-session__meta">{session.terminal || session.agentType}</span>
        </button>
      ))}
    </div>
  )
}

function SessionDetail({ session, onBack }: { session: SessionState; onBack: () => void }) {
  const [reply, setReply] = useState('')
  const [loading, setLoading] = useState(false)
  const isComposingRef = useRef(false)
  const setChatHistory = useSessionStore((s) => s.setChatHistory)
  const messages = session.chatHistory.slice(-3)

  const refreshHistory = async () => {
    setLoading(true)
    try {
      const parsed = await getChatHistory(session.id)
      setChatHistory(session.id, mapParsedMessages(parsed))
    } catch (err) {
      console.warn('[PetSurface] refresh history:', err)
    } finally {
      setLoading(false)
    }
  }

  const submitReply = async () => {
    const text = reply.trim()
    if (!text) return
    await sendMessage(session.id, text)
    useSessionStore.getState().updateSession({
      type: 'user_message',
      sessionId: session.id,
      content: text,
    })
    setReply('')
    await refreshHistory()
  }

  return (
    <div className="pet-detail">
      <div className="pet-detail__header">
        <button type="button" onClick={onBack}>返回</button>
        <strong>{session.sessionTitle || session.project || session.id}</strong>
        <button type="button" onClick={() => jumpToTerminal(session.id)}>跳转</button>
      </div>
      <div className="pet-detail__messages">
        {messages.length === 0 ? (
          <button type="button" className="pet-detail__empty" onClick={refreshHistory}>
            {loading ? '加载中...' : '加载历史'}
          </button>
        ) : messages.map((message, index) => (
          <div key={index} className={`pet-detail__message pet-detail__message--${message.role}`}>
            <span>{message.role === 'user' ? 'You' : 'AI'}</span>
            <p>{message.role === 'error' ? message.message : getMessageText(message)}</p>
          </div>
        ))}
      </div>
      <div className="pet-detail__reply">
        <input
          value={reply}
          placeholder="回复..."
          onChange={(event) => setReply(event.target.value)}
          onBlur={disablePetTextInput}
          onCompositionEnd={() => { isComposingRef.current = false }}
          onCompositionStart={() => { isComposingRef.current = true }}
          onFocus={enablePetTextInput}
          onKeyDown={(event) => {
            const nativeEvent = event.nativeEvent as KeyboardEvent & { isComposing?: boolean }
            if (nativeEvent.isComposing || isComposingRef.current || nativeEvent.keyCode === 229) return
            if (event.key === 'Enter') void submitReply()
          }}
        />
        <button type="button" disabled={!reply.trim()} onClick={() => void submitReply()}>发送</button>
      </div>
    </div>
  )
}

function ActionToast({
  kind,
  onDismiss,
  overlay,
  session,
}: {
  kind: PetActionKind
  onDismiss: () => void
  overlay: OverlayItem
  session: SessionState
}) {
  if (kind === 'permission') {
    return <PermissionToast overlay={overlay} session={session} onDismiss={onDismiss} />
  }
  if (kind === 'question') {
    return <QuestionToast overlay={overlay} session={session} onDismiss={onDismiss} />
  }
  if (kind === 'plan') {
    return <PlanToast overlay={overlay} session={session} onDismiss={onDismiss} />
  }
  return <MessageToast session={session} overlay={overlay} onDismiss={onDismiss} />
}

function PermissionToast({ overlay, onDismiss, session }: { overlay: OverlayItem; onDismiss: () => void; session: SessionState }) {
  const { t } = useTranslation()
  const data = overlay.data as { toolName?: string; toolInput?: string }
  const toolName = data.toolName || session.pendingPermission?.toolName || 'Tool'
  const label = getToolActivityLabel(t, toolName)

  const answer = async (allowed: boolean, always = false) => {
    await respondPermission(session.id, allowed, always)
    useSessionStore.getState().clearPermission(session.id)
    onDismiss()
  }

  return (
    <div className="pet-toast pet-toast--action">
      <div className="pet-toast__title">需要授权</div>
      <div className="pet-toast__text">{label}</div>
      {data.toolInput && <pre>{data.toolInput}</pre>}
      <div className="pet-toast__actions">
        <button type="button" onClick={() => void answer(false)}>拒绝</button>
        <button type="button" onClick={() => void answer(true)}>允许</button>
        <button type="button" className="pet-danger" onClick={() => void answer(true, true)}>始终允许</button>
      </div>
    </div>
  )
}

function QuestionToast({ overlay, onDismiss, session }: { overlay: OverlayItem; onDismiss: () => void; session: SessionState }) {
  const data = overlay.data as {
    question?: string
    options?: Array<{ label: string; description?: string }>
    multiSelect?: boolean
    questions?: Array<{
      question: string
      header?: string | null
      options: Array<{ label: string; description?: string | null }>
      multiSelect?: boolean
    }>
  }

  const answer = async (value: string) => {
    await respondQuestion(session.id, value)
    useSessionStore.getState().clearQuestion(session.id)
    onDismiss()
  }

  if (data.questions && data.questions.length > 1) {
    return <MultiQuestionToast data={data} onAnswer={answer} />
  }

  if (data.multiSelect) {
    return <MultiSelectQuestionToast data={data} onAnswer={answer} />
  }

  return (
    <div className="pet-toast pet-toast--question">
      <div className="pet-toast__title">需要回答</div>
      <div className="pet-toast__text">{data.question || session.pendingQuestion?.question || 'Choose an option'}</div>
      <div className="pet-toast__options">
        {(data.options || []).slice(0, 4).map((option) => (
          <button type="button" key={option.label} onClick={() => void answer(option.label)}>
            <strong>{option.label}</strong>
            {option.description && <span>{option.description}</span>}
          </button>
        ))}
      </div>
    </div>
  )
}

function MultiSelectQuestionToast({
  data,
  onAnswer,
}: {
  data: { question?: string; options?: Array<{ label: string; description?: string }> }
  onAnswer: (value: string) => Promise<void>
}) {
  const [selected, setSelected] = useState<Set<number>>(new Set())
  const options = data.options || []

  return (
    <div className="pet-toast pet-toast--question">
      <div className="pet-toast__title">多选问题</div>
      <div className="pet-toast__text">{data.question || 'Choose one or more options'}</div>
      <div className="pet-toast__options">
        {options.slice(0, 6).map((option, index) => (
          <button
            type="button"
            key={option.label}
            data-selected={selected.has(index) ? 'true' : 'false'}
            onClick={() => {
              setSelected((current) => {
                const next = new Set(current)
                if (next.has(index)) next.delete(index)
                else next.add(index)
                return next
              })
            }}
          >
            <strong>{selected.has(index) ? `✓ ${option.label}` : option.label}</strong>
            {option.description && <span>{option.description}</span>}
          </button>
        ))}
      </div>
      <div className="pet-toast__actions">
        <button
          type="button"
          disabled={selected.size === 0}
          onClick={() => void onAnswer([...selected].map((index) => options[index].label).join(', '))}
        >
          提交
        </button>
      </div>
    </div>
  )
}

function MultiQuestionToast({
  data,
  onAnswer,
}: {
  data: {
    question?: string
    questions?: Array<{
      question: string
      header?: string | null
      options: Array<{ label: string; description?: string | null }>
      multiSelect?: boolean
    }>
  }
  onAnswer: (value: string) => Promise<void>
}) {
  const questions = data.questions || []
  const [answers, setAnswers] = useState<Record<number, string | string[]>>({})
  const complete = questions.every((_, index) => answers[index] !== undefined)

  const toggleAnswer = (questionIndex: number, optionLabel: string, multiSelect?: boolean) => {
    setAnswers((current) => {
      if (!multiSelect) return { ...current, [questionIndex]: optionLabel }
      const existing = Array.isArray(current[questionIndex]) ? current[questionIndex] as string[] : []
      const next = existing.includes(optionLabel)
        ? existing.filter((item) => item !== optionLabel)
        : [...existing, optionLabel]
      const updated = { ...current }
      if (next.length === 0) delete updated[questionIndex]
      else updated[questionIndex] = next
      return updated
    })
  }

  const submit = () => {
    const payload: Record<string, string> = {}
    questions.forEach((question, index) => {
      const value = answers[index]
      payload[question.question] = Array.isArray(value) ? value.join(', ') : value
    })
    void onAnswer(JSON.stringify(payload))
  }

  return (
    <div className="pet-toast pet-toast--question pet-toast--multi-question">
      <div className="pet-toast__title">{questions.length} 个问题</div>
      <div className="pet-toast__multi-list">
        {questions.slice(0, 4).map((question, questionIndex) => {
          const current = answers[questionIndex]
          const selected = new Set(Array.isArray(current) ? current : current ? [current] : [])
          return (
            <div className="pet-toast__multi-item" key={question.question}>
              <div className="pet-toast__multi-question">
                {question.header && <span>[{question.header}] </span>}
                {question.question}
              </div>
              <div className="pet-toast__multi-options">
                {question.options.slice(0, 4).map((option) => (
                  <button
                    type="button"
                    key={option.label}
                    data-selected={selected.has(option.label) ? 'true' : 'false'}
                    onClick={() => toggleAnswer(questionIndex, option.label, question.multiSelect)}
                  >
                    {selected.has(option.label) ? `✓ ${option.label}` : option.label}
                  </button>
                ))}
              </div>
            </div>
          )
        })}
      </div>
      <div className="pet-toast__actions">
        <button type="button" disabled={!complete} onClick={submit}>全部提交</button>
      </div>
    </div>
  )
}

function PlanToast({ overlay, onDismiss, session }: { overlay: OverlayItem; onDismiss: () => void; session: SessionState }) {
  const data = overlay.data as { planTitle?: string; planContent?: string; requestedPermissions?: string[] }
  const respond = async (mode: 'manual' | 'acceptEdits' | 'bypassPermissions' | 'feedback') => {
    await respondPlan(session.id, mode)
    onDismiss()
  }

  return (
    <div className="pet-toast pet-toast--plan">
      <div className="pet-toast__title">{data.planTitle || '计划确认'}</div>
      <div className="pet-toast__text">{data.planContent || session.planContent || 'Review the plan before continuing.'}</div>
      {data.requestedPermissions && data.requestedPermissions.length > 0 && (
        <div className="pet-toast__chips">{data.requestedPermissions.map((item) => <span key={item}>{item}</span>)}</div>
      )}
      <div className="pet-toast__actions">
        <button type="button" onClick={() => void respond('manual')}>手动</button>
        <button type="button" onClick={() => void respond('acceptEdits')}>接受编辑</button>
        <button type="button" className="pet-danger" onClick={() => void respond('bypassPermissions')}>自动批准</button>
      </div>
    </div>
  )
}

function MessageToast({ overlay, onDismiss, session }: { overlay?: OverlayItem; onDismiss: () => void; session: SessionState }) {
  const data = overlay?.data as { responseText?: string; summary?: string; userMessage?: string } | undefined
  const text = data?.responseText || data?.summary || session.responseText || session.description || 'There is a new update.'
  const [replyOpen, setReplyOpen] = useState(false)
  return (
    <div className="pet-toast pet-toast--message">
      <div className="pet-toast__title">{session.sessionTitle || session.project || '新的回复'}</div>
      <div className="pet-toast__text">{text}</div>
      <div className="pet-toast__actions">
        <button type="button" onClick={() => jumpToTerminal(session.id)}>查看终端</button>
        <button type="button" onClick={() => setReplyOpen((open) => !open)}>{replyOpen ? '收起' : '回复'}</button>
        <button type="button" onClick={onDismiss}>关闭</button>
      </div>
      {replyOpen && <PetReplyInline sessionId={session.id} />}
    </div>
  )
}

function PetReplyInline({ sessionId }: { sessionId: string }) {
  const [reply, setReply] = useState('')
  const [sending, setSending] = useState(false)
  const [sent, setSent] = useState(false)
  const [error, setError] = useState(false)
  const isComposingRef = useRef(false)

  const send = async () => {
    const text = reply.trim()
    if (!text || sending) return
    setSending(true)
    setError(false)
    try {
      await sendMessage(sessionId, text)
      useSessionStore.getState().updateSession({
        type: 'user_message',
        sessionId,
        content: text,
      })
      setReply('')
      setSent(true)
      window.setTimeout(() => setSent(false), 900)
    } catch {
      setError(true)
      window.setTimeout(() => setError(false), 1400)
    } finally {
      setSending(false)
    }
  }

  return (
    <form
      className="pet-reply-inline"
      onClick={(event) => event.stopPropagation()}
      onKeyDown={(event) => event.stopPropagation()}
      onSubmit={(event) => {
        event.preventDefault()
        void send()
      }}
    >
      <input
        value={reply}
        disabled={sending}
        placeholder={sent ? '已发送' : '快速回复...'}
        onBlur={disablePetTextInput}
        onChange={(event) => setReply(event.target.value)}
        onCompositionEnd={() => { isComposingRef.current = false }}
        onCompositionStart={() => { isComposingRef.current = true }}
        onFocus={enablePetTextInput}
        onKeyDown={(event) => {
          const nativeEvent = event.nativeEvent as KeyboardEvent & { isComposing?: boolean }
          if (nativeEvent.isComposing || isComposingRef.current || nativeEvent.keyCode === 229) return
          if (event.key === 'Enter' && !event.shiftKey) {
            event.preventDefault()
            void send()
          }
        }}
      />
      {error && <span className="pet-reply-inline__error">失败</span>}
      <button type="submit" disabled={sending || !reply.trim()}>{sending ? '...' : '发送'}</button>
    </form>
  )
}

function PetBadges({ actionCount, sessionCount }: { actionCount: number; sessionCount: number }) {
  if (actionCount <= 0 && sessionCount <= 0) return null
  return (
    <div className="pet-badges">
      {sessionCount > 0 && <span className="pet-badge pet-badge--session">{sessionCount > 9 ? '9+' : sessionCount}</span>}
      {actionCount > 0 && <span className="pet-badge pet-badge--action">{actionCount}</span>}
    </div>
  )
}

function getActionKind(overlay: OverlayItem | null): PetActionKind {
  if (!overlay) return null
  if (overlay.type === 'permission') return 'permission'
  if (overlay.type === 'question') return 'question'
  if (overlay.type === 'plan') return 'plan'
  return null
}

function toneForPhase(phase: SessionState['phase']): string {
  if (phase === 'waiting_approval' || phase === 'waiting_input') return 'attention'
  if (phase === 'error') return 'error'
  if (phase === 'done') return 'done'
  if (phase === 'processing' || phase === 'compacting') return 'active'
  return 'idle'
}

function getSessionPreview(session: SessionState): string {
  if (session.pendingPermission) return `需要授权：${session.pendingPermission.toolName}`
  if (session.pendingQuestion) return session.pendingQuestion.question
  if (session.responseText) return session.responseText
  if (session.description) return session.description
  if (session.lastToolName) return session.lastToolTarget ? `${session.lastToolName}: ${session.lastToolTarget}` : session.lastToolName
  return session.phase === 'done' ? '任务已完成' : '等待新的活动'
}

function getMessageText(message: SessionState['chatHistory'][number]): string {
  if (message.role === 'assistant') return message.trailingContent || message.content || message.thinking || ''
  if (message.role === 'tool_use') return message.toolName
  if (message.role === 'thinking') return message.content
  if (message.role === 'permission') return message.toolName
  if (message.role === 'user') return message.content
  return ''
}
