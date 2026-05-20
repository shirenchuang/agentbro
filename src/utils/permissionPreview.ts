export const WRITE_PERMISSION_PREVIEW_LINES = 8

export function parseToolInput(value: unknown): Record<string, unknown> {
  if (!value) return {}
  if (typeof value === 'object' && !Array.isArray(value)) return value as Record<string, unknown>
  if (typeof value !== 'string') return { raw: String(value) }
  try {
    const parsed = JSON.parse(value)
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed)
      ? parsed as Record<string, unknown>
      : { raw: value }
  } catch {
    return { raw: value }
  }
}

export function shortenPath(filePath: string, maxSegments = 3): string {
  const segments = filePath.split('/')
  if (segments.length <= maxSegments) return filePath
  return `.../${segments.slice(-maxSegments).join('/')}`
}

export function getStringField(input: Record<string, unknown>, keys: string[]): string {
  for (const key of keys) {
    const value = input[key]
    if (typeof value === 'string' && value.trim()) return value
  }
  return ''
}

export function getWritePermissionPreview(input: Record<string, unknown>, maxLines = WRITE_PERMISSION_PREVIEW_LINES) {
  const filePath = getStringField(input, ['file_path', 'filePath', 'path'])
  const content = getStringField(input, ['content'])
  const lines = content ? content.split('\n') : []
  const visibleLines = lines.slice(0, maxLines)

  return {
    filePath,
    shortPath: filePath ? shortenPath(filePath) : '',
    content,
    visibleContent: visibleLines.join('\n'),
    hiddenLineCount: Math.max(0, lines.length - visibleLines.length),
  }
}
