/* Agent Icon — Per-agent colored icon */
import type { AgentType } from '../../types/agent'

const agentColors: Record<string, string> = {
  'claude-code': '#D97706',
  'codex': '#10B981',
  'deepseek': '#2563EB',
  'gemini-cli': '#6366F1',
  'cursor': '#3B82F6',
  'opencode': '#8B5CF6',
  'droid': '#EF4444',
  'qoder': '#EC4899',
  'traecli': '#22C55E',
  'codebuddy': '#14B8A6',
  'copilot': '#6B7280',
  'kiro': '#F59E0B',
}

const agentLabels: Record<string, string> = {
  'claude-code': 'C',
  'codex': 'X',
  'deepseek': 'DS',
  'gemini-cli': 'G',
  'cursor': 'Cu',
  'opencode': 'O',
  'droid': 'D',
  'qoder': 'Q',
  'traecli': 'T',
  'codebuddy': 'B',
  'copilot': 'Co',
  'kiro': 'K',
}

interface AgentIconProps {
  agentType: AgentType
  size?: number
}

export function AgentIcon({ agentType, size = 20 }: AgentIconProps) {
  const color = agentColors[agentType] || '#6B7280'
  const label = agentLabels[agentType] || '?'

  return (
    <div
      style={{
        width: size,
        height: size,
        borderRadius: size * 0.3,
        background: `${color}33`,
        border: `1px solid ${color}66`,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        fontSize: size * 0.5,
        fontWeight: 700,
        color,
        flexShrink: 0,
      }}
    >
      {label}
    </div>
  )
}
