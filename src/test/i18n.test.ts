import { describe, it, expect } from 'vitest'
import en from '../i18n/locales/en.json'
import zh from '../i18n/locales/zh.json'
import ja from '../i18n/locales/ja.json'
import ko from '../i18n/locales/ko.json'
import tr from '../i18n/locales/tr.json'

function flatKeys(obj: Record<string, unknown>, prefix = ''): string[] {
  return Object.entries(obj).flatMap(([k, v]) => {
    const key = prefix ? `${prefix}.${k}` : k
    return typeof v === 'object' && v !== null
      ? flatKeys(v as Record<string, unknown>, key)
      : [key]
  })
}

const enKeys = flatKeys(en)

describe('i18n locale completeness', () => {
  it('zh has all keys from en', () => {
    const zhKeys = flatKeys(zh)
    const missing = enKeys.filter((k) => !zhKeys.includes(k))
    expect(missing).toEqual([])
  })

  it('ja has all keys from en', () => {
    const jaKeys = flatKeys(ja)
    const missing = enKeys.filter((k) => !jaKeys.includes(k))
    expect(missing).toEqual([])
  })

  it('ko has all keys from en', () => {
    const koKeys = flatKeys(ko)
    const missing = enKeys.filter((k) => !koKeys.includes(k))
    expect(missing).toEqual([])
  })

  it('en has expected top-level namespaces', () => {
    expect(Object.keys(en)).toEqual(expect.arrayContaining(['notch', 'settings', 'tray', 'trial']))
  })

  it('ja tray.quit is translated', () => {
    expect(ja.tray.quit).not.toBe(en.tray.quit)
  })

  it('ko tray.quit is translated', () => {
    expect(ko.tray.quit).not.toBe(en.tray.quit)
  })

  it('all five locales cover layered Agent Skill scope copy', () => {
    const keys = [
      'skillSource',
      'skillStatus',
      'agentSkills',
      'managedSkills',
      'unmanagedSkills',
      'inheritedManagedNoResults',
      'inheritedUnmanagedNoResults',
      'sharedAdoptAction',
      'sharedAdoptBusy',
      'sharedViewDetails',
      'builtinSkills',
    ]
    const locales = [en, zh, ja, ko, tr].map(
      (locale) => locale.skills.agentManagement as Record<string, unknown>,
    )

    for (const locale of locales) {
      for (const key of keys) {
        expect(locale[key], key).toEqual(expect.any(String))
        expect(String(locale[key]).trim(), key).not.toBe('')
      }
    }

    expect(locales.map((locale) => locale.builtinSkills)).toEqual([
      'Built-in read-only',
      '内置只读',
      '組み込み読み取り専用',
      '기본 제공 읽기 전용',
      'Yerleşik salt okunur',
    ])
  })
})
