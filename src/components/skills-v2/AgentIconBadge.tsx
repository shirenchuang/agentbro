const agentColors: Record<string, string> = {
  'claude-code': '#D97706',
  'codex': '#10B981',
  'gemini': '#6366F1',
  'cursor': '#3B82F6',
  'opencode': '#8B5CF6',
  'copilot': '#6B7280',
  'qwen': '#9333EA',
  'kimi': '#0EA5E9',
  'deepseek': '#2563EB',
  'windsurf': '#06B6D4',
  'augment': '#F97316',
  'kilocode': '#EC4899',
  'aider': '#22C55E',
  'amp': '#84CC16',
  'kiro': '#F59E0B',
  'hermes': '#14B8A6',
}

const agentLabels: Record<string, string> = {
  'claude-code': 'C',
  'codex': 'X',
  'gemini': 'G',
  'cursor': 'Cu',
  'opencode': 'O',
  'copilot': 'Co',
  'qwen': 'Q',
  'kimi': 'K',
  'deepseek': 'DS',
  'windsurf': 'W',
  'augment': 'Au',
  'kilocode': 'KC',
  'aider': 'Ai',
  'amp': 'Am',
  'kiro': 'Ki',
  'hermes': 'H',
}

export function AgentIconBadge({
  iconKey,
  mode,
  title,
  size = 22,
}: {
  iconKey: string
  mode?: 'link' | 'copy' | string
  title?: string
  size?: number
}) {
  const color = agentColors[iconKey] || '#6B7280'
  const label = agentLabels[iconKey] || iconKey.slice(0, 2)
  return (
    <span
      className={`sm2__agent-badge${mode === 'copy' ? ' sm2__agent-badge--copy' : ''}`}
      title={title || iconKey}
      style={{ width: size, height: size, background: `${color}33`, border: `1px solid ${color}66`, color }}
    >
      {label}
    </span>
  )
}
