import { describe, expect, it } from 'vitest'
import {
  formatShortcutKeyEvent,
  isRecordableShortcutEvent,
  shortcutMatchesEvent,
} from '../utils/keyboardShortcuts'
import { isApplePlatform } from '../utils/platform'

describe('keyboard shortcut helpers', () => {
  it('records the platform primary modifier in the same format the matcher accepts', () => {
    const event = { key: 'k', metaKey: isApplePlatform(), ctrlKey: !isApplePlatform(), altKey: false, shiftKey: true }
    const expected = isApplePlatform() ? '⌘+⇧+K' : 'Ctrl+Shift+K'

    expect(formatShortcutKeyEvent(event)).toBe(expected)
    expect(shortcutMatchesEvent(expected, event)).toBe(true)
  })

  it('matches Tauri-style CommandOrControl accelerators against the platform primary modifier', () => {
    expect(shortcutMatchesEvent('CommandOrControl+Shift+A', {
      key: 'a',
      metaKey: true,
      ctrlKey: false,
      altKey: false,
      shiftKey: true,
    })).toBe(isApplePlatform())
    expect(shortcutMatchesEvent('CommandOrControl+Shift+A', {
      key: 'a',
      metaKey: false,
      ctrlKey: true,
      altKey: false,
      shiftKey: true,
    })).toBe(!isApplePlatform())
  })

  it('does not save modifier-only shortcut recordings', () => {
    expect(isRecordableShortcutEvent({
      key: 'Meta',
      metaKey: true,
      ctrlKey: false,
      altKey: false,
      shiftKey: false,
    })).toBe(false)
  })
})
