import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { ChatMessage } from '../../types/agent'
import { StatusDot } from '../shared'
import { DiffView } from './DiffView'
import { getToolActivityLabel } from '../../utils/toolLabels'
import './CollapsedGroup.css'

interface CollapsedGroupProps {
  messages: ChatMessage[]
}

export function CollapsedGroup({ messages }: CollapsedGroupProps) {
  const [expanded, setExpanded] = useState(false)
  const { t } = useTranslation()

  const thinkingCount = messages.filter((m) => m.role === 'thinking').length
  const toolMessages = messages.filter((m) => m.role === 'tool_use')
  const toolCount = toolMessages.length

  return (
    <div className="collapsed-group">
      <button
        className="collapsed-group__summary"
        onClick={() => setExpanded(!expanded)}
      >
        <span className="collapsed-group__chevron">{expanded ? '▼' : '▶'}</span>
        {thinkingCount > 0 && (
          <span className="collapsed-group__badge collapsed-group__badge--thinking">
            thinking × {thinkingCount}
          </span>
        )}
        {toolCount > 0 && (
          <span className="collapsed-group__badge collapsed-group__badge--tool">
            tool calls × {toolCount}
          </span>
        )}
      </button>

      {expanded && (
        <div className="collapsed-group__detail">
          {messages.map((msg, i) => {
            if (msg.role === 'thinking') {
              return (
                <div key={i} className="collapsed-group__item collapsed-group__item--thinking">
                  <span className="collapsed-group__dot" />
                  <span className="collapsed-group__text">
                    {msg.content.length > 120 ? msg.content.slice(0, 120) + '...' : msg.content}
                  </span>
                </div>
              )
            }
            if (msg.role === 'tool_use') {
              const displayName = getToolActivityLabel(t, msg.toolName)
              const isEditTool = ['Edit', 'Write', 'NotebookEdit'].includes(msg.toolName)

              return (
                <div key={i} className="collapsed-group__item collapsed-group__item--tool">
                  <StatusDot phase={msg.status === 'running' ? 'processing' : msg.status === 'success' ? 'done' : 'error'} size={6} />
                  <span className="collapsed-group__tool-name">{displayName}</span>
                  {msg.status === 'success' && <span className="collapsed-group__check">✓</span>}
                  {msg.status === 'error' && <span className="collapsed-group__x">✗</span>}
                  {msg.status === 'running' && <span className="collapsed-group__spinner" />}
                  {isEditTool && msg.diff && (
                    <div className="collapsed-group__diff">
                      <DiffView diff={msg.diff} />
                    </div>
                  )}
                </div>
              )
            }
            return null
          })}
        </div>
      )}
    </div>
  )
}
