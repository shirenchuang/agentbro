import { isApplePlatform } from './platform'

export interface ShortcutKeyEvent {
  key: string
  metaKey: boolean
  ctrlKey: boolean
  altKey: boolean
  shiftKey: boolean
}

const MAC_COMMAND = '⌘'
const MAC_CONTROL = '⌃'
const MAC_OPTION = '⌥'
const MAC_SHIFT = '⇧'

const modifierAliases = {
  meta: new Set([MAC_COMMAND, 'cmd', 'command', 'meta', 'super', 'win', 'windows']),
  control: new Set([MAC_CONTROL, 'ctrl', 'control']),
  alt: new Set([MAC_OPTION, 'alt', 'option']),
  shift: new Set([MAC_SHIFT, 'shift']),
  commandOrControl: new Set(['commandorcontrol', 'cmdorctrl', 'ctrlorcmd']),
}

function shortcutParts(shortcut: string): string[] {
  return shortcut
    .split('+')
    .map((part) => part.trim())
    .filter(Boolean)
}

function normalizedToken(token: string): string {
  return token.replace(/\s+/g, '').toLowerCase()
}

function isModifierToken(token: string): boolean {
  const normalized = normalizedToken(token)
  return modifierAliases.meta.has(normalized)
    || modifierAliases.control.has(normalized)
    || modifierAliases.alt.has(normalized)
    || modifierAliases.shift.has(normalized)
    || modifierAliases.commandOrControl.has(normalized)
}

function displayKeyForEvent(key: string): string {
  if (key === 'Enter') return 'Enter'
  if (key === 'Backspace') return 'Backspace'
  if (key === 'Escape') return 'Escape'
  if (key === 'Tab') return 'Tab'
  if (key === ' ') return 'Space'
  if (key === 'ArrowUp') return '↑'
  if (key === 'ArrowDown') return '↓'
  if (key === 'ArrowLeft') return '←'
  if (key === 'ArrowRight') return '→'
  return key.length === 1 ? key.toUpperCase() : key
}

function normalizedPrimaryKey(key: string): string {
  const normalized = key.trim().toLowerCase()
  if (normalized === 'esc') return 'escape'
  if (normalized === 'space' || normalized === 'spacebar') return ' '
  if (normalized === '↑') return 'arrowup'
  if (normalized === '↓') return 'arrowdown'
  if (normalized === '←') return 'arrowleft'
  if (normalized === '→') return 'arrowright'
  return normalized
}

function displayMetaModifier(): string {
  return isApplePlatform() ? MAC_COMMAND : 'Win'
}

function displayControlModifier(): string {
  return isApplePlatform() ? MAC_CONTROL : 'Ctrl'
}

function displayAltModifier(): string {
  return isApplePlatform() ? MAC_OPTION : 'Alt'
}

function displayShiftModifier(): string {
  return isApplePlatform() ? MAC_SHIFT : 'Shift'
}

function displayCommandOrControlModifier(): string {
  return isApplePlatform() ? MAC_COMMAND : 'Ctrl'
}

export function formatShortcutKeyEvent(event: ShortcutKeyEvent): string {
  const parts: string[] = []
  if (event.metaKey) parts.push(displayMetaModifier())
  if (event.ctrlKey) parts.push(displayControlModifier())
  if (event.altKey) parts.push(displayAltModifier())
  if (event.shiftKey) parts.push(displayShiftModifier())
  if (!['Meta', 'Control', 'Alt', 'Shift'].includes(event.key)) {
    parts.push(displayKeyForEvent(event.key))
  }
  return parts.join('+')
}

export function shortcutHasPrimaryKey(shortcut: string): boolean {
  return shortcutParts(shortcut).some((part) => !isModifierToken(part))
}

export function isRecordableShortcutEvent(event: ShortcutKeyEvent): boolean {
  if (!shortcutHasPrimaryKey(formatShortcutKeyEvent(event))) return false
  const hasModifier = event.metaKey || event.ctrlKey || event.altKey
  const isSpecial = ['Escape', 'Enter', 'Backspace', 'Tab'].includes(event.key)
  const isFunctionKey = /^F([1-9]|1[0-2])$/.test(event.key)
  return hasModifier || isSpecial || isFunctionKey
}

export function shortcutDisplayParts(shortcut: string): string[] {
  return shortcutParts(shortcut).map((part) => {
    const normalized = normalizedToken(part)
    if (modifierAliases.meta.has(normalized)) return displayMetaModifier()
    if (modifierAliases.control.has(normalized)) return displayControlModifier()
    if (modifierAliases.alt.has(normalized)) return displayAltModifier()
    if (modifierAliases.shift.has(normalized)) return displayShiftModifier()
    if (modifierAliases.commandOrControl.has(normalized)) return displayCommandOrControlModifier()
    return part
  })
}

export function shortcutMatchesEvent(shortcut: string, event: ShortcutKeyEvent): boolean {
  const parts = shortcutParts(shortcut)
  if (parts.length === 0 || !shortcutHasPrimaryKey(shortcut)) return false

  const normalizedParts = parts.map(normalizedToken)
  const needsCommandOrControl = normalizedParts.some((part) => modifierAliases.commandOrControl.has(part))
  const needsMeta = normalizedParts.some((part) => modifierAliases.meta.has(part))
  const needsControl = normalizedParts.some((part) => modifierAliases.control.has(part))
  const needsAlt = normalizedParts.some((part) => modifierAliases.alt.has(part))
  const needsShift = normalizedParts.some((part) => modifierAliases.shift.has(part))
  const primaryKey = parts.find((part) => !isModifierToken(part))

  if (!primaryKey) return false
  if (normalizedPrimaryKey(event.key) !== normalizedPrimaryKey(primaryKey)) return false

  const commandOrControlUsesMeta = needsCommandOrControl && isApplePlatform()
  const commandOrControlUsesControl = needsCommandOrControl && !isApplePlatform()
  const expectsMeta = needsMeta || commandOrControlUsesMeta
  const expectsControl = needsControl || commandOrControlUsesControl

  if (event.metaKey !== expectsMeta) return false
  if (event.ctrlKey !== expectsControl) return false

  return event.altKey === needsAlt && event.shiftKey === needsShift
}
