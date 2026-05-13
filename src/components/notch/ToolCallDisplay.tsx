/* ToolCallDisplay — Green tool name with input preview */
import { useTranslation } from 'react-i18next'
import type { ToolStatus } from '../../types/agent'
import { getToolActivityLabel } from '../../utils/toolLabels'
import './ToolCallDisplay.css'

interface ToolCallDisplayProps {
  toolName: string
  toolInput?: string
  status: ToolStatus
}

function truncateInput(input: string | undefined, maxLen = 50): string {
  if (!input) return ''
  const clean = input.replace(/\n/g, ' ').trim()
  return clean.length > maxLen ? clean.slice(0, maxLen) + '\u2026' : clean
}

export function ToolCallDisplay({ toolName, toolInput, status }: ToolCallDisplayProps) {
  const { t } = useTranslation()
  const name = getToolActivityLabel(t, toolName)

  return (
    <div className="tool-call-display">
      <span className="tool-call-display__name">{name}</span>
      {toolInput && (
        <span className="tool-call-display__input">
          {truncateInput(toolInput)}
        </span>
      )}
      {status === 'running' && (
        <span className="tool-call-display__spinner" />
      )}
    </div>
  )
}
