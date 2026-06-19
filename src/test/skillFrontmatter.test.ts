import { describe, expect, it } from 'vitest'
import { extractSkillDescription, stripSkillFrontmatter } from '../components/skills-v2/frontmatter'

describe('skill frontmatter', () => {
  it('extracts literal block descriptions with a leading blank line', () => {
    const content = [
      '---',
      'name: post-creator',
      'description: |',
      '',
      '  图文内容创作工作流。',
      '  适用于小红书、公众号图文等平台。',
      'version: 1',
      '---',
      '# Post Creator',
    ].join('\n')

    expect(extractSkillDescription(content)).toBe('图文内容创作工作流。\n适用于小红书、公众号图文等平台。')
    expect(stripSkillFrontmatter(content)).toBe('# Post Creator')
  })

  it('extracts folded block descriptions', () => {
    const content = [
      '---',
      'description: >-',
      '  first line',
      '  second line',
      '---',
      '# Body',
    ].join('\n')

    expect(extractSkillDescription(content)).toBe('first line second line')
  })
})
