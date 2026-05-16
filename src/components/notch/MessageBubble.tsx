/* MessageBubble — Renders a single chat message by type */
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import Markdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import type { ChatMessage, ChatToolCall } from '../../types/agent'
import { DiffView } from './DiffView'
import { StatusDot } from '../shared'
import { parseMcpTool } from '../../utils/mcp'
import { getToolActivityLabel } from '../../utils/toolLabels'
import { openImage } from '../../services/tauriApi'
import './MessageBubble.css'

interface MessageBubbleProps {
  message: ChatMessage
}

export function MessageBubble({ message }: MessageBubbleProps) {
  switch (message.role) {
    case 'user':
      return (
        <div className="msg msg--user">
          <div className="msg__pill">
            {message.content && (
              <div>
                <span className="msg__prefix">You:</span> {message.content}
              </div>
            )}
            {message.images && message.images.length > 0 && (
              <div className="msg__images">
                {message.images.map((src, index) => (
                  <img
                    key={`${src}-${index}`}
                    className="msg__image"
                    src={src}
                    alt=""
                    title="Open image"
                    onClick={() => openImage(src).catch((e) => console.warn('[MessageBubble] openImage:', e))}
                  />
                ))}
              </div>
            )}
          </div>
        </div>
      )

    case 'assistant':
      return <AssistantMessage message={message} />

    case 'tool_use':
      return <ToolMessage message={message} />

    case 'permission':
      return <PermissionMessage message={message} />

    case 'thinking':
      return <ThinkingMessage content={message.content} />

    case 'error':
      return (
        <div className="msg msg--error">
          <span className="msg__error-dot" />
          <span>{message.message}</span>
        </div>
      )

    default:
      return null
  }
}

function AssistantMessage({ message }: { message: Extract<ChatMessage, { role: 'assistant' }> }) {
  const [expanded, setExpanded] = useState(false)
  const { t } = useTranslation()
  const toolCalls = message.toolCalls ?? []
  const hasThinking = !!message.thinking
  const hasIntermediateText = !!message.content && !!message.trailingContent
  const hasProcess = hasThinking || toolCalls.length > 0 || hasIntermediateText
  const finalContent = message.trailingContent ?? (!hasProcess ? message.content : '')
  const summary = buildProcessSummary({
    messageCount: message.messageCount ?? 0,
    thinkingCount: message.thinkingCount ?? (hasThinking ? 1 : 0),
    toolCount: toolCalls.length,
    t,
  })

  return (
    <div className="msg msg--assistant">
      {hasProcess && (
        <div className="msg__process">
          <button className="msg__process-summary" onClick={() => setExpanded(!expanded)}>
            <span className="msg__process-chevron">{expanded ? '▼' : '▶'}</span>
            <span>{summary}</span>
            {toolCalls.some((tool) => tool.status === 'running') && <span className="msg__tool-spinner" />}
          </button>
          {expanded && (
            <div className="msg__process-detail selectable">
              {message.thinking && (
                <div className="msg__process-section msg__process-section--thinking">
                  <div className="msg__process-label">{t('notch.chat.thinking', '思考过程')}</div>
                  <Markdown remarkPlugins={[remarkGfm]}>{message.thinking}</Markdown>
                </div>
              )}
              {hasIntermediateText && (
                <div className="msg__process-section markdown-body">
                  <Markdown remarkPlugins={[remarkGfm]}>{message.content}</Markdown>
                </div>
              )}
              {toolCalls.length > 0 && (
                <div className="msg__process-tools">
                  {toolCalls.map((tool, index) => (
                    <AssistantToolCall key={tool.toolUseId ?? `${tool.toolName}-${index}`} tool={tool} />
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {finalContent && (
        <div className="msg__content selectable markdown-body">
          <Markdown remarkPlugins={[remarkGfm]}>{finalContent}</Markdown>
        </div>
      )}
      {message.images && message.images.length > 0 && (
        <div className="msg__images">
          {message.images.map((src, index) => (
            <ImageThumb key={`${src}-${index}`} src={src} />
          ))}
        </div>
      )}
    </div>
  )
}

function buildProcessSummary({
  messageCount,
  thinkingCount,
  toolCount,
  t,
}: {
  messageCount: number
  thinkingCount: number
  toolCount: number
  t: ReturnType<typeof useTranslation>['t']
}): string {
  const parts: string[] = []
  if (thinkingCount > 0) parts.push(t('notch.chat.summaryThinking', '{{count}} 次思考', { count: thinkingCount }))
  if (toolCount > 0) parts.push(t('notch.chat.summaryTools', '{{count}} 次工具调用', { count: toolCount }))
  if (messageCount > 0) parts.push(t('notch.chat.summaryMessages', '{{count}} 条消息', { count: messageCount }))
  return parts.join(t('notch.chat.summarySeparator', '，')) || t('notch.chat.summaryProcess', '处理过程')
}

function AssistantToolCall({ tool }: { tool: ChatToolCall }) {
  const { t } = useTranslation()
  const displayName = getToolActivityLabel(t, tool.toolName)

  return (
    <div className="msg__process-tool">
      <div className="msg__tool-header">
        <StatusDot phase={tool.status === 'running' ? 'processing' : tool.status === 'success' ? 'done' : 'error'} size={6} />
        <span className="msg__tool-name">{displayName}</span>
        {tool.status === 'running' && <span className="msg__tool-spinner" />}
        {tool.status === 'success' && <span className="msg__tool-check">✓</span>}
        {tool.status === 'error' && <span className="msg__tool-x">✗</span>}
      </div>
      {tool.toolInput && <pre className="msg__process-pre">{tool.toolInput}</pre>}
      {tool.result && <pre className="msg__process-pre">{tool.result}</pre>}
      {tool.diff && <DiffView diff={tool.diff} />}
    </div>
  )
}

function ImageThumb({ src }: { src: string }) {
  return (
    <img
      className="msg__image"
      src={src}
      alt=""
      title="Open image"
      onClick={() => openImage(src).catch((e) => console.warn('[MessageBubble] openImage:', e))}
    />
  )
}

function ToolMessage({ message }: { message: ChatMessage & { role: 'tool_use' } }) {
  const [expanded, setExpanded] = useState(false)
  const { t } = useTranslation()
  const mcp = parseMcpTool(message.toolName)
  const displayName = getToolActivityLabel(t, message.toolName)

  return (
    <div className="msg msg--tool" onClick={() => setExpanded(!expanded)}>
      <div className="msg__tool-header">
        <StatusDot phase={message.status === 'running' ? 'processing' : message.status === 'success' ? 'done' : 'error'} size={6} />
        <span className="msg__tool-name">{displayName}</span>
        {mcp.isMcp && <span className="msg__mcp-badge">MCP</span>}
        {message.status === 'running' && <span className="msg__tool-spinner" />}
        {message.status === 'success' && <span className="msg__tool-check">✓</span>}
        {message.status === 'error' && <span className="msg__tool-x">✗</span>}
      </div>
      {expanded && message.toolInput && (
        <div className="msg__tool-detail selectable">
          <pre>{message.toolInput}</pre>
        </div>
      )}
      {expanded && message.result && (
        <div className="msg__tool-detail selectable">
          <pre>{message.result}</pre>
        </div>
      )}
      {message.diff && <DiffView diff={message.diff} />}
    </div>
  )
}

function PermissionMessage({ message }: { message: ChatMessage & { role: 'permission' } }) {
  const { t } = useTranslation()
  const displayName = getToolActivityLabel(t, message.toolName)

  return (
    <div className="msg msg--permission">
      <div className="msg__perm-header">
        <span className="msg__perm-icon">⚠</span>
        <span className="msg__perm-tool">{displayName}</span>
      </div>
      {message.toolInput && (
        <div className="msg__perm-input selectable">
          <pre>{message.toolInput}</pre>
        </div>
      )}
      {message.diff && <DiffView diff={message.diff} />}
    </div>
  )
}

function ThinkingMessage({ content }: { content: string }) {
  const [expanded, setExpanded] = useState(false)
  const isLong = content.length > 80

  return (
    <div className="msg msg--thinking" onClick={() => isLong && setExpanded(!expanded)}>
      <span className="msg__thinking-dot" />
      <span className="msg__thinking-text">
        {expanded || !isLong ? content : content.slice(0, 80) + '...'}
      </span>
    </div>
  )
}
