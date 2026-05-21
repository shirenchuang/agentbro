const TITLE_METADATA_KEYS = new Set(['title'])

export function parseCodexTitleMetadata(text: string | undefined | null): string | null {
  const trimmed = (text || '').trim()
  if (!trimmed.startsWith('{') || !trimmed.endsWith('}')) return null

  try {
    const parsed = JSON.parse(trimmed) as unknown
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return null

    const record = parsed as Record<string, unknown>
    const keys = Object.keys(record)
    if (keys.length === 0 || !keys.every((key) => TITLE_METADATA_KEYS.has(key))) return null

    const title = record.title
    return typeof title === 'string' && title.trim() ? title.trim() : null
  } catch {
    return null
  }
}

export function isCodexTitleMetadata(text: string | undefined | null): boolean {
  return parseCodexTitleMetadata(text) !== null
}
