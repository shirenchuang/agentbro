const NOTIFICATION_READER_MIN_HEIGHT = 128
const NOTIFICATION_READER_PREFERRED_HEIGHT = 360
const NOTIFICATION_READER_MAX_HEIGHT = 420
const NOTIFICATION_PANEL_CHROME_HEIGHT = 190
const NOTIFICATION_PANEL_MIN_HEIGHT = 300
const COMPACTING_PANEL_HEIGHT = 260

export interface NotificationContentMetrics {
  text?: string
  userMessage?: string
}

function weightedLength(value: string): number {
  return Array.from(value).reduce((total, char) => total + (char.charCodeAt(0) > 127 ? 1.65 : 1), 0)
}

function estimateMarkdownLineCount(text: string): number {
  const lines = text.split('\n')
  return lines.reduce((total, line) => {
    const trimmed = line.trim()
    if (!trimmed) return total + 0.35
    if (/^\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)+\|?$/.test(trimmed)) return total

    const tableLike = trimmed.includes('|')
    const charsPerLine = tableLike ? 72 : 92
    const markdownWeight = /^#{1,6}\s/.test(trimmed)
      ? 0.35
      : /^[-*+]\s+/.test(trimmed) || /^\d+\.\s+/.test(trimmed)
        ? 0.15
        : 0

    return total + Math.max(1, Math.ceil(weightedLength(trimmed) / charsPerLine)) + markdownWeight
  }, 0)
}

function estimateReadableHeight(content?: NotificationContentMetrics): number | null {
  const text = content?.text?.trim()
  if (!text) return null

  const statusHeight = 30
  const userMessageHeight = content?.userMessage ? 30 : 0
  const bodyLineCount = estimateMarkdownLineCount(text)
  const bodyHeight = Math.ceil(bodyLineCount * 19) + 24
  return Math.ceil(statusHeight + userMessageHeight + bodyHeight + 16)
}

export function getReadableNotificationHeight(
  completionCardHeight: number,
  maxPanelHeight: number,
  content?: NotificationContentMetrics,
): number {
  const availableHeight = Math.max(
    NOTIFICATION_READER_MIN_HEIGHT,
    (maxPanelHeight || 600) - NOTIFICATION_PANEL_CHROME_HEIGHT,
  )
  const desiredHeight = estimateReadableHeight(content)
    ?? Math.max(NOTIFICATION_READER_MIN_HEIGHT, Math.min(completionCardHeight, NOTIFICATION_READER_PREFERRED_HEIGHT))
  return Math.min(NOTIFICATION_READER_MAX_HEIGHT, Math.max(NOTIFICATION_READER_MIN_HEIGHT, desiredHeight), availableHeight)
}

export function getNotificationPanelHeight(
  completionCardHeight: number,
  maxPanelHeight: number,
  overlayType?: string,
  content?: NotificationContentMetrics,
): number {
  const panelMaxHeight = maxPanelHeight || 600
  if (overlayType === 'compacting') {
    return Math.min(COMPACTING_PANEL_HEIGHT, panelMaxHeight)
  }

  const readerHeight = getReadableNotificationHeight(completionCardHeight, panelMaxHeight, content)
  return Math.min(
    Math.max(readerHeight + NOTIFICATION_PANEL_CHROME_HEIGHT, NOTIFICATION_PANEL_MIN_HEIGHT),
    panelMaxHeight,
  )
}

function clampHeight(value: number, min: number, max: number): number {
  return Math.min(Math.max(Math.ceil(value), min), max)
}

function parseToolInput(value: unknown): Record<string, unknown> {
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

function estimatePlainTextHeight(value: string, charsPerLine = 86, lineHeight = 19): number {
  return Math.max(1, value.split('\n').reduce((total, line) => {
    const weighted = weightedLength(line.trim())
    return total + Math.max(1, Math.ceil(weighted / charsPerLine))
  }, 0)) * lineHeight
}

function estimatePermissionPanelHeight(data: Record<string, unknown>, maxPanelHeight: number): number {
  const toolName = String(data.toolName ?? '')
  const toolInput = parseToolInput(data.toolInput)
  const command = String(toolInput.command ?? toolInput.raw ?? '')
  const description = String(toolInput.description ?? '')
  const filePath = String(toolInput.file_path ?? toolInput.filePath ?? '')
  const content = String(toolInput.content ?? '')
  const oldString = String(toolInput.old_string ?? '')
  const newString = String(toolInput.new_string ?? '')
  const diff = data.diff as { lines?: unknown[] } | undefined

  const previewText = toolName === 'Bash'
    ? [command, description].filter(Boolean).join('\n')
    : toolName === 'Write'
      ? [filePath, content].filter(Boolean).join('\n')
      : toolName === 'Edit' || toolName === 'MultiEdit'
        ? [filePath, oldString, newString].filter(Boolean).join('\n')
        : JSON.stringify(toolInput)

  const previewHeight = clampHeight(44 + estimatePlainTextHeight(previewText || toolName, 92, 18), 72, 180)
  const diffLineCount = Array.isArray(diff?.lines) ? diff.lines.length : 0
  const diffHeight = diffLineCount > 0 ? clampHeight(34 + diffLineCount * 19, 120, 280) : 0
  return clampHeight(226 + previewHeight + diffHeight, 300, maxPanelHeight)
}

function estimatePlanPanelHeight(data: Record<string, unknown>, maxPanelHeight: number): number {
  const content = String(data.planContent ?? '')
  const permissions = Array.isArray(data.requestedPermissions) ? data.requestedPermissions.length : 0
  const contentHeight = clampHeight(46 + estimateMarkdownLineCount(content) * 19, 110, 330)
  const permissionsHeight = permissions > 0 ? clampHeight(28 + Math.ceil(permissions / 2) * 28, 42, 96) : 0
  return clampHeight(228 + contentHeight + permissionsHeight, 320, maxPanelHeight)
}

function estimateQuestionPanelHeight(data: Record<string, unknown>, maxPanelHeight: number): number {
  const questions = Array.isArray(data.questions) ? data.questions as Array<{ options?: unknown[]; question?: string }> : []
  if (questions.length > 1) {
    const optionCount = questions.reduce((total, question) => total + (Array.isArray(question.options) ? question.options.length : 0), 0)
    return clampHeight(230 + questions.length * 66 + optionCount * 18, 340, maxPanelHeight)
  }

  const options = Array.isArray(data.options) ? data.options.length : 0
  const questionHeight = estimatePlainTextHeight(String(data.question ?? ''), 90, 19)
  const confirmHeight = data.multiSelect ? 42 : 0
  return clampHeight(222 + questionHeight + options * 58 + confirmHeight, 300, maxPanelHeight)
}

export function getBlockingOverlayPanelHeight(
  overlayType: string | undefined,
  data: unknown,
  maxPanelHeight: number,
): number {
  const panelMaxHeight = maxPanelHeight || 600
  const payload = data && typeof data === 'object' && !Array.isArray(data)
    ? data as Record<string, unknown>
    : {}

  if (overlayType === 'permission') return estimatePermissionPanelHeight(payload, panelMaxHeight)
  if (overlayType === 'plan') return estimatePlanPanelHeight(payload, Math.max(panelMaxHeight, 600))
  if (overlayType === 'question') return estimateQuestionPanelHeight(payload, panelMaxHeight)
  return clampHeight(320, 260, panelMaxHeight)
}
