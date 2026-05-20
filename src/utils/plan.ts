export function formatPlanMarkdown(content: string): string {
  return content
    .split('\n')
    .map((line) => {
      const trimmed = line.trim()
      if (!trimmed || trimmed.startsWith('#')) return line
      if (/^(context|plan|test plan|root cause|assumptions|requested permissions)$/i.test(trimmed)) {
        return `### ${trimmed}`
      }
      return line
    })
    .join('\n')
}

export function parsePlanPermission(permission: string): { tool: string; prompt?: string } {
  const match = permission.match(/^([^:：]+)[:：]\s*(.*)$/)
  if (!match) return { tool: permission }
  return { tool: match[1].trim(), prompt: match[2].trim() }
}
