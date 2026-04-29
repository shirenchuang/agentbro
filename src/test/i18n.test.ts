import { describe, it, expect } from 'vitest'
import en from '../i18n/locales/en.json'
import zh from '../i18n/locales/zh.json'
import ja from '../i18n/locales/ja.json'
import ko from '../i18n/locales/ko.json'

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
})
