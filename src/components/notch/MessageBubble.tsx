/* MessageBubble — Renders a single chat message by type */
import { useState } from 'react'
import Markdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import type { ChatMessage } from '../../types/agent'
import { DiffView } from './DiffView'
import { StatusDot } from '../shared'
import { parseMcpTool } from '../../utils/mcp'
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
            <span className="msg__prefix">You:</span> {message.content}
          </div>
        </div>
      )

    case 'assistant':
      return (
        <div className="msg msg--assistant">
          <div className="msg__content selectable markdown-body">
            <Markdown remarkPlugins={[remarkGfm]}>{message.content}</Markdown>
          </div>
        </div>
      )

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

function ToolMessage({ message }: { message: ChatMessage & { role: 'tool_use' } }) {
  const [expanded, setExpanded] = useState(false)
  const mcp = parseMcpTool(message.toolName)
  const displayName = mcp.isMcp ? `${mcp.displayServer} — ${mcp.displayTool}` : mcp.displayTool

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
  const mcp = parseMcpTool(message.toolName)
  const displayName = mcp.isMcp ? `${mcp.displayServer} — ${mcp.displayTool}` : mcp.displayTool

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
