import { describe, expect, it } from 'vitest'
import {
  formatShortcutKeyEvent,
  isRecordableShortcutEvent,
  shortcutMatchesEvent,
} from '../utils/keyboardShortcuts'

describe('keyboard shortcut helpers', () => {
  it('records modifiers in the same symbol format the matcher accepts', () => {
    const event = { key: 'k', metaKey: true, ctrlKey: false, altKey: false, shiftKey: true }

    expect(formatShortcutKeyEvent(event)).toBe('⌘+⇧+K')
    expect(shortcutMatchesEvent('⌘+⇧+K', event)).toBe(true)
  })

  it('accepts Tauri-style CommandOrControl accelerators for window matching', () => {
    expect(shortcutMatchesEvent('CommandOrControl+Shift+A', {
      key: 'a',
      metaKey: true,
      ctrlKey: false,
      altKey: false,
      shiftKey: true,
    })).toBe(true)
    expect(shortcutMatchesEvent('CommandOrControl+Shift+A', {
      key: 'a',
      metaKey: false,
      ctrlKey: true,
      altKey: false,
      shiftKey: true,
    })).toBe(true)
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
