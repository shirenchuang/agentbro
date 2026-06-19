type BlockStyle = 'literal' | 'folded'

interface BlockScalar {
  key: string
  style: BlockStyle
  indent: number | null
  lines: string[]
}

export function stripSkillFrontmatter(content: string): string {
  const section = frontmatterSection(content)
  if (!section) return content
  return content.slice(section.bodyStart).trim()
}

export function extractSkillDescription(content: string): string {
  return parseSkillFrontmatter(content).description || ''
}

export function parseSkillFrontmatter(content: string): Record<string, string> {
  const section = frontmatterSection(content)
  if (!section) return {}
  const map: Record<string, string> = {}
  let block: BlockScalar | null = null

  for (const line of section.text.split(/\r?\n/)) {
    if (block) {
      const accepted = acceptBlockLine(block, line)
      if (accepted) continue
      finishBlock(map, block)
      block = null
    }

    const pair = splitKeyValue(line)
    if (!pair) continue
    const { key, value } = pair
    if (!key) continue
    const style = blockScalarStyle(value)
    if (style) {
      block = { key, style, indent: null, lines: [] }
      continue
    }
    const normalized = stripQuotes(value)
    if (normalized) map[key] = normalized
  }

  if (block) finishBlock(map, block)
  return map
}

function frontmatterSection(content: string): { text: string; bodyStart: number } | null {
  if (!content.startsWith('---')) return null
  const match = /^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/.exec(content)
  if (!match) return null
  return { text: match[1] ?? '', bodyStart: match[0].length }
}

function splitKeyValue(line: string): { key: string; value: string } | null {
  const trimmed = line.trim()
  const index = trimmed.indexOf(':')
  if (index === -1) return null
  return {
    key: trimmed.slice(0, index).trim(),
    value: trimmed.slice(index + 1).trim(),
  }
}

function blockScalarStyle(value: string): BlockStyle | null {
  if (value.startsWith('|')) return 'literal'
  if (value.startsWith('>')) return 'folded'
  return null
}

function stripQuotes(value: string): string {
  if (
    value.length >= 2
    && ((value.startsWith('"') && value.endsWith('"')) || (value.startsWith("'") && value.endsWith("'")))
  ) {
    return value.slice(1, -1)
  }
  return value
}

function acceptBlockLine(block: BlockScalar, line: string): boolean {
  if (!line.trim()) {
    if (block.indent !== null || block.lines.length > 0) block.lines.push('')
    return true
  }

  const indent = leadingWhitespace(line)
  if (block.indent === null) {
    if (indent === 0 && splitKeyValue(line)) return false
    block.indent = indent
  }

  if (indent < block.indent && splitKeyValue(line)) return false
  block.lines.push(line.slice(block.indent).trimEnd())
  return true
}

function finishBlock(map: Record<string, string>, block: BlockScalar) {
  const value = block.style === 'literal'
    ? block.lines.join('\n').trim()
    : foldLines(block.lines)
  if (value) map[block.key] = value
}

function leadingWhitespace(line: string): number {
  const match = /^\s*/.exec(line)
  return match?.[0].length ?? 0
}

function foldLines(lines: string[]): string {
  let out = ''
  for (const line of lines) {
    const trimmed = line.trim()
    if (!trimmed) {
      if (out && !out.endsWith('\n')) out += '\n'
      continue
    }
    if (out && !out.endsWith('\n')) out += ' '
    out += trimmed
  }
  return out.trim()
}
