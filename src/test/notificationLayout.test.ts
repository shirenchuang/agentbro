import { describe, expect, it } from 'vitest'
import { getBlockingOverlayPanelHeight, getNotificationPanelHeight, getReadableNotificationHeight } from '../utils/notificationLayout'

describe('notificationLayout', () => {
  it('uses the configured height as a fallback when content is unknown', () => {
    expect(getReadableNotificationHeight(200, 600)).toBe(200)
  })

  it('keeps short notifications compact', () => {
    expect(getReadableNotificationHeight(200, 600, {
      text: '明白。我会严格按你每一轮的指令执行。',
      userMessage: '请给出第一轮指令。',
    })).toBe(128)
  })

  it('respects the available panel height', () => {
    expect(getReadableNotificationHeight(420, 360)).toBe(170)
  })

  it('caps very tall notification preferences', () => {
    expect(getReadableNotificationHeight(520, 900, {
      text: Array.from({ length: 28 }, (_, index) => `- Item ${index + 1}: verify a detailed implementation note`).join('\n'),
    })).toBe(420)
  })

  it('sizes short notification panels from content instead of a fixed target', () => {
    expect(getNotificationPanelHeight(200, 600, 'response', {
      text: '明白。我会严格按你每一轮的指令执行。',
      userMessage: '请给出第一轮指令。',
    })).toBe(318)
  })

  it('keeps notification panels within the configured maximum height', () => {
    expect(getNotificationPanelHeight(420, 360, 'completion', {
      text: Array.from({ length: 20 }, (_, index) => `- Line ${index + 1}: enough content to require scrolling`).join('\n'),
    })).toBe(360)
  })

  it('uses a compact dedicated height for context compaction notices', () => {
    expect(getNotificationPanelHeight(200, 600, 'compacting')).toBe(260)
  })

  it('keeps simple permission prompts compact', () => {
    expect(getBlockingOverlayPanelHeight('permission', {
      toolName: 'Bash',
      toolInput: '{"command":"mkdir -p src/auth","description":"Create src/auth directory"}',
    }, 600)).toBe(306)
  })

  it('grows permission prompts when a diff is present', () => {
    expect(getBlockingOverlayPanelHeight('permission', {
      toolName: 'Edit',
      toolInput: '{"file_path":"src/auth/middleware.ts","old_string":"getToken()","new_string":"refreshToken()"}',
      diff: { lines: Array.from({ length: 12 }, () => ({})) },
    }, 600)).toBeGreaterThan(430)
  })

  it('sizes question prompts from the number of visible options', () => {
    expect(getBlockingOverlayPanelHeight('question', {
      question: '接下来优先采样哪个视图？',
      options: ['Overlay', 'Detail', 'Compact'],
    }, 600)).toBe(415)
  })
})
