/* Agent Icon — Per-agent colored icon */
import type { AgentType } from '../../types/agent'
import workbuddyIcon from '../../assets/cli-icons/workbuddy.png'
import zcodeIcon from '../../assets/cli-icons/zcode.svg'
import doubaoIcon from '../../assets/cli-icons/doubao.svg'

const agentIcons: Partial<Record<AgentType, string>> = {
  workbuddy: workbuddyIcon,
  zcode: zcodeIcon,
  doubao: doubaoIcon,
}

const agentColors: Record<string, string> = {
  'claude-code': '#D97706',
  'cline': '#00B87A',
  'codex': '#10B981',
  'deepseek': '#2563EB',
  'gemini-cli': '#6366F1',
  'cursor': '#3B82F6',
  'opencode': '#8B5CF6',
  'droid': '#EF4444',
  'qoder': '#EC4899',
  'codebuddy': '#14B8A6',
  'workbuddy': '#16C8A7',
  'copilot': '#6B7280',
  'kiro': '#F59E0B',
  'zcode': '#F8FAFC',
  'doubao': '#5A75FF',
}

const agentLabels: Record<string, string> = {
  'claude-code': 'C',
  'cline': 'Cl',
  'codex': 'X',
  'deepseek': 'DS',
  'gemini-cli': 'G',
  'cursor': 'Cu',
  'opencode': 'O',
  'droid': 'D',
  'qoder': 'Q',
  'codebuddy': 'B',
  'workbuddy': 'WB',
  'copilot': 'Co',
  'kiro': 'K',
  'zcode': 'Z',
  'doubao': '豆',
}

interface AgentIconProps {
  agentType: AgentType
  size?: number
}

export function AgentIcon({ agentType, size = 20 }: AgentIconProps) {
  const color = agentColors[agentType] || '#6B7280'
  const label = agentLabels[agentType] || '?'
  const iconSrc = agentIcons[agentType]

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
      {iconSrc ? (
        <img
          src={iconSrc}
          alt=""
          draggable={false}
          style={{ width: '100%', height: '100%', objectFit: 'contain', borderRadius: size * 0.24 }}
        />
      ) : (
        label
      )}
    </div>
  )
}
