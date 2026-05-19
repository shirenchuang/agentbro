/* SubagentList — Shows nested agents running under the main session */
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { SubagentInfo } from '../../types/agent'
import { StatusDot } from '../shared'
import './SubagentList.css'

interface SubagentListProps {
  subagents: SubagentInfo[]
  onOpenHistory?: (subagent: SubagentInfo) => void
}

export function SubagentList({ subagents, onOpenHistory }: SubagentListProps) {
  const { t } = useTranslation()
  const [collapsed, setCollapsed] = useState(false)

  if (subagents.length === 0) return null

  return (
    <div className="subagent-list">
      <button
        className="subagent-list__toggle"
        onClick={() => setCollapsed(!collapsed)}
        aria-label={collapsed ? 'Expand subagents' : 'Collapse subagents'}
      >
        <svg
          className={`subagent-list__chevron ${collapsed ? '' : 'subagent-list__chevron--open'}`}
          width="10"
          height="10"
          viewBox="0 0 16 16"
          fill="none"
        >
          <path d="M6 4l4 4-4 4" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/>
        </svg>
        <span className="subagent-list__label">
          {subagents.length} {t('notch.subagents', 'Subagents')}
        </span>
      </button>

      {!collapsed && (
        <div className="subagent-list__items">
          {subagents.map((agent) => (
            <SubagentRow key={agent.agentId} agent={agent} onOpenHistory={onOpenHistory} />
          ))}
        </div>
      )}
    </div>
  )
}

function SubagentRow({ agent, onOpenHistory }: { agent: SubagentInfo; onOpenHistory?: (subagent: SubagentInfo) => void }) {
  const phaseForDot = agent.status === 'running' ? 'processing'
    : agent.status === 'completed' ? 'done'
    : 'error'

  const lastTool = agent.tools.length > 0 ? agent.tools[agent.tools.length - 1] : null
  const canOpenHistory = Boolean(agent.agentTranscriptPath && onOpenHistory)
  const displayName = agent.name ? `@${agent.name}` : (agent.description || agent.agentType || `@${agent.agentId.slice(0, 8)}`)
  const detail = agent.name && agent.description ? agent.description : null
  const result = agent.status !== 'running' ? agent.lastAssistantMessage : null

  return (
    <button
      className={`subagent-row${canOpenHistory ? ' subagent-row--button' : ''}`}
      type="button"
      onClick={() => canOpenHistory && onOpenHistory?.(agent)}
      disabled={!canOpenHistory}
      title={canOpenHistory ? 'Open subagent history' : undefined}
    >
      <StatusDot phase={phaseForDot} size={6} />
      <span className="subagent-row__main">
        <span className="subagent-row__name">{displayName}</span>
        {detail && <span className="subagent-row__desc">{detail}</span>}
      </span>
      {result && <span className="subagent-row__result">{result}</span>}
      {agent.status === 'running' && lastTool && (
        <span className="subagent-row__tool">
          <span className="subagent-row__spinner" />
          {lastTool}
        </span>
      )}
      {agent.status === 'completed' && agent.agentTranscriptPath && (
        <span className="subagent-row__history">历史</span>
      )}
    </button>
  )
}
