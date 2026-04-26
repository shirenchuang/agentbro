/* MCP Tool Badge — Friendly display for MCP tool names */
import { parseMcpTool } from '../../utils/mcp'
import './McpToolBadge.css'

interface McpToolBadgeProps {
  toolName: string
}

export function McpToolBadge({ toolName }: McpToolBadgeProps) {
  const parsed = parseMcpTool(toolName)

  if (!parsed.isMcp) {
    return <span className="mcp-badge__tool-only">{parsed.displayTool}</span>
  }

  return (
    <span className="mcp-badge">
      <span className="mcp-badge__icon" title="MCP Tool">⬡</span>
      <span className="mcp-badge__server">{parsed.displayServer}</span>
      <span className="mcp-badge__sep">—</span>
      <span className="mcp-badge__tool">{parsed.displayTool}</span>
    </span>
  )
}
