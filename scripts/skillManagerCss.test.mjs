import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

describe('Skill Manager settings theme scope', () => {
  it('uses settings colors instead of island theme colors inside the settings window', () => {
    const skillManagerCss = readFileSync(resolve(process.cwd(), 'src/components/skills-v2/SkillManagerV2.css'), 'utf8')

    expect(skillManagerCss).toContain('.settings-content--skill-manager .sm2 {')
    expect(skillManagerCss).toContain('--text-primary: var(--settings-text-primary);')
    expect(skillManagerCss).toContain('--text-secondary: var(--settings-text-secondary);')
    expect(skillManagerCss).toContain('--card-bg: var(--settings-card-bg);')
  })
})
