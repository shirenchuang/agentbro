import { useState } from 'react'
import claudeIcon from '../../assets/cli-icons/claude.png'
import codexIcon from '../../assets/cli-icons/codex.png'
import geminiIcon from '../../assets/cli-icons/gemini.png'
import cursorIcon from '../../assets/cli-icons/cursor.png'
import opencodeIcon from '../../assets/cli-icons/opencode.png'
import copilotIcon from '../../assets/cli-icons/copilot.png'
import qwenIcon from '../../assets/cli-icons/qwen.png'
import kimiIcon from '../../assets/cli-icons/kimi.png'
import hermesIcon from '../../assets/cli-icons/hermes.png'
import codebuddyIcon from '../../assets/cli-icons/codebuddy.png'
import qoderIcon from '../../assets/cli-icons/qoder.png'
import piIcon from '../../assets/cli-icons/pi.png'
import openclawIcon from '../../assets/openclaw.png'
import qclawIcon from '../../assets/qclaw.png'
import easyclawIcon from '../../assets/easyclaw.png'
import autoclawIcon from '../../assets/autoclaw.png'

const agentIcons: Record<string, string> = {
  'claude-code': claudeIcon,
  codex: codexIcon,
  gemini: geminiIcon,
  'gemini-cli': geminiIcon,
  cursor: cursorIcon,
  'cursor-cli': cursorIcon,
  opencode: opencodeIcon,
  openclaw: openclawIcon,
  qclaw: qclawIcon,
  easyclaw: easyclawIcon,
  autoclaw: autoclawIcon,
  copilot: copilotIcon,
  qwen: qwenIcon,
  kimi: kimiIcon,
  'kimi-code-cli': kimiIcon,
  hermes: hermesIcon,
  codebuddy: codebuddyIcon,
  qoder: qoderIcon,
  'qoder-cli': qoderIcon,
  pi: piIcon,
}

const agentColors: Record<string, string> = {
  agents: '#0F766E',
  'claude-code': '#D97706',
  codex: '#10B981',
  gemini: '#6366F1',
  cursor: '#3B82F6',
  opencode: '#8B5CF6',
  openclaw: '#30B0C7',
  qclaw: '#30B0C7',
  easyclaw: '#30B0C7',
  autoclaw: '#30B0C7',
  copilot: '#6B7280',
  qwen: '#9333EA',
  kimi: '#0EA5E9',
  deepseek: '#2563EB',
  windsurf: '#06B6D4',
  augment: '#F97316',
  kilocode: '#EC4899',
  aider: '#22C55E',
  amp: '#84CC16',
  kiro: '#F59E0B',
  hermes: '#14B8A6',
}

const agentLabels: Record<string, string> = {
  agents: 'Ag',
  'claude-code': 'C',
  codex: 'X',
  gemini: 'G',
  cursor: 'Cu',
  opencode: 'O',
  openclaw: 'OC',
  qclaw: 'QC',
  easyclaw: 'EC',
  autoclaw: 'AC',
  copilot: 'Co',
  qwen: 'Q',
  kimi: 'K',
  deepseek: 'DS',
  windsurf: 'W',
  augment: 'Au',
  kilocode: 'KC',
  aider: 'Ai',
  amp: 'Am',
  kiro: 'Ki',
  hermes: 'H',
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
  const iconSrc = agentIcons[iconKey]
  const [failed, setFailed] = useState(false)
  const color = agentColors[iconKey] || '#6B7280'
  const label = agentLabels[iconKey] || iconKey.slice(0, 2)

  return (
    <span
      className={`sm2__agent-badge${mode === 'copy' ? ' sm2__agent-badge--copy' : ''}`}
      title={title || iconKey}
      style={{ width: size, height: size, background: iconSrc && !failed ? '#fff' : `${color}33`, border: `1px solid ${color}66`, color }}
    >
      {iconSrc && !failed ? (
        <img
          src={iconSrc}
          alt=""
          draggable={false}
          onError={() => setFailed(true)}
          style={{ width: '100%', height: '100%', objectFit: 'contain' }}
        />
      ) : (
        label
      )}
    </span>
  )
}
