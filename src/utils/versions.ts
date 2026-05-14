export function displayVersionValue(version: string | null | undefined) {
  if (!version) return ''
  const trimmed = version.trim()
  if (!trimmed) return ''
  return trimmed.toLowerCase().startsWith('v') ? trimmed : `v${trimmed}`
}
