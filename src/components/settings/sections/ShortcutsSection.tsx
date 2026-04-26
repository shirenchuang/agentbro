import { useState, useEffect, useCallback } from 'react'
import { useTranslation } from 'react-i18next'
import { useConfigStore } from '../../../stores/configStore'
import { SettingSection } from '../SettingSection'
import { SettingGroup } from '../SettingGroup'

function formatKeyEvent(e: KeyboardEvent): string {
  const parts: string[] = []
  if (e.metaKey) parts.push('⌘')
  if (e.ctrlKey) parts.push('⌃')
  if (e.altKey) parts.push('⌥')
  if (e.shiftKey) parts.push('⇧')

  const key = e.key
  if (!['Meta', 'Control', 'Alt', 'Shift'].includes(key)) {
    if (key === 'Enter') parts.push('Enter')
    else if (key === 'Backspace') parts.push('Backspace')
    else if (key === 'Escape') parts.push('Escape')
    else if (key === 'Tab') parts.push('Tab')
    else if (key === ' ') parts.push('Space')
    else if (key === 'ArrowUp') parts.push('↑')
    else if (key === 'ArrowDown') parts.push('↓')
    else if (key === 'ArrowLeft') parts.push('←')
    else if (key === 'ArrowRight') parts.push('→')
    else parts.push(key.length === 1 ? key.toUpperCase() : key)
  }

  return parts.join('+')
}

function ShortcutRow({ action, label, keys }: { action: string; label: string; keys: string }) {
  const { t } = useTranslation()
  const [recording, setRecording] = useState(false)
  const [conflict, setConflict] = useState<string | null>(null)
  const updateShortcut = useConfigStore((s) => s.updateShortcut)
  const allShortcuts = useConfigStore((s) => s.shortcuts)

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (!recording) return
      e.preventDefault()
      e.stopPropagation()

      if (e.key === 'Escape' && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) {
        setRecording(false)
        setConflict(null)
        return
      }

      // Only accept combos with at least one modifier, or special keys
      const hasModifier = e.metaKey || e.ctrlKey || e.altKey
      const isSpecial = ['Escape', 'Enter', 'Backspace', 'Tab'].includes(e.key)
      if (!hasModifier && !isSpecial && !['F1','F2','F3','F4','F5','F6','F7','F8','F9','F10','F11','F12'].includes(e.key)) {
        return
      }

      const formatted = formatKeyEvent(e)
      if (formatted) {
        const duplicate = allShortcuts.find((s) => s.keys === formatted && s.action !== action)
        if (duplicate) {
          setConflict(t('settings.alreadyUsedBy', { label: duplicate.label }))
          return
        }
        setConflict(null)
        updateShortcut(action, formatted)
        setRecording(false)
      }
    },
    [recording, action, updateShortcut, allShortcuts, t],
  )

  useEffect(() => {
    if (recording) {
      window.addEventListener('keydown', handleKeyDown, true)
      return () => window.removeEventListener('keydown', handleKeyDown, true)
    }
  }, [recording, handleKeyDown])

  return (
    <div className="shortcuts-row">
      <span className="shortcuts-row__action">{label}</span>
      {recording ? (
        <span className="shortcuts-row__recording">
          {conflict ? <span style={{ color: '#FF3B30' }}>{conflict}</span> : t('settings.pressKeys')}
        </span>
      ) : (
        <span className="shortcuts-row__keys">
          {keys.split('+').map((k, i) => (
            <kbd key={i}>{k}</kbd>
          ))}
        </span>
      )}
      <button className="shortcuts-row__edit" onClick={() => setRecording(!recording)}>
        {recording ? t('settings.cancel') : t('settings.edit')}
      </button>
    </div>
  )
}

export function ShortcutsSection() {
  const { t } = useTranslation()
  const shortcuts = useConfigStore((s) => s.shortcuts)

  return (
    <SettingSection title={t('settings.shortcuts')} description={t('settings.shortcutsDesc')}>
      <SettingGroup>
        <div className="shortcuts-table">
          {shortcuts.map((s) => (
            <ShortcutRow key={s.action} action={s.action} label={s.label} keys={s.keys} />
          ))}
        </div>
      </SettingGroup>
    </SettingSection>
  )
}
