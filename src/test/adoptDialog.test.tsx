import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import '@testing-library/jest-dom/vitest'
import { AdoptDialog } from '../components/skills-v2/AdoptDialog'
import i18n from '../i18n'
import { skillApiV2 } from '../services/skillApiV2'
import type { AdoptPreview, SkillPackDetail, SkillPackSummary } from '../services/skillApiV2'

const preview: AdoptPreview = {
  agentId: 'claude-code',
  unmanagedId: 'local-bird',
  skillPath: '/Users/me/.claude/skills/bird',
  inferredSkillId: 'bird',
  hash: 'hash-bird',
  centerHasSameId: false,
  canQuickAdopt: true,
  options: [
    { value: 'import_keep', label: 'Import to center, keep agent file as-is', destructive: false },
    { value: 'import_link', label: 'Import to center and replace agent file with link', destructive: true },
  ],
}

const existingPack: SkillPackSummary = {
  id: 'agent-tools',
  name: 'Agent Tools',
  description: 'Daily tools',
  tags: [],
  memberCount: 1,
  appliedAgentCount: 0,
  healthy: true,
}

const existingPackDetail: SkillPackDetail = {
  id: existingPack.id,
  name: existingPack.name,
  description: existingPack.description,
  tags: existingPack.tags,
  members: [
    { skillId: 'release-checklist', skillName: 'Release Checklist', required: true, sortOrder: 0, missing: false },
  ],
  appliedAgents: [],
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-01T00:00:00Z',
}

describe('single Skill adoption pack choice', () => {
  beforeEach(async () => {
    cleanup()
    vi.restoreAllMocks()
    await i18n.changeLanguage('zh')
  })

  afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
  })

  it('keeps skill-pack membership optional and can add the adopted Skill to an existing pack', async () => {
    const executeAdopt = vi.spyOn(skillApiV2, 'executeAdopt').mockResolvedValue('bird')
    vi.spyOn(skillApiV2, 'getPackDetail').mockResolvedValue(existingPackDetail)
    const upsertPack = vi.spyOn(skillApiV2, 'upsertPack').mockResolvedValue({
      ...existingPackDetail,
      members: [
        ...existingPackDetail.members,
        { skillId: 'bird', skillName: 'bird', required: true, sortOrder: 1, missing: false },
      ],
    })
    const onDone = vi.fn()

    render(
      <AdoptDialog
        preview={preview}
        packs={[
          { ...existingPack, id: 'default', name: '全量技能包' },
          existingPack,
        ]}
        onClose={() => {}}
        onDone={onDone}
      />,
    )

    const checkbox = screen.getByRole('checkbox', { name: /同时加入技能包/ })
    expect(checkbox).not.toBeChecked()
    expect(screen.queryByLabelText('技能包')).not.toBeInTheDocument()

    fireEvent.click(checkbox)
    expect(screen.getByLabelText('技能包')).toHaveValue('agent-tools')
    expect(screen.queryByRole('option', { name: /全量技能包/ })).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '确认接管' }))

    await waitFor(() => {
      expect(executeAdopt).toHaveBeenCalledWith('claude-code', 'local-bird', 'import_link', null)
      expect(upsertPack).toHaveBeenCalledWith({
        id: 'agent-tools',
        name: 'Agent Tools',
        description: 'Daily tools',
        tags: [],
        skillIds: ['release-checklist', 'bird'],
      })
      expect(onDone).toHaveBeenCalled()
    })
  })

  it('creates a new skill pack with the final adopted Skill ID', async () => {
    const renamedPreview: AdoptPreview = {
      ...preview,
      centerHasSameId: true,
      canQuickAdopt: false,
      options: [
        { value: 'rename', label: 'Import under a new id', destructive: false },
      ],
    }
    const executeAdopt = vi.spyOn(skillApiV2, 'executeAdopt').mockResolvedValue('bird-tools')
    const upsertPack = vi.spyOn(skillApiV2, 'upsertPack').mockResolvedValue({
      ...existingPackDetail,
      id: 'new-pack',
      name: '常用工具',
      members: [
        { skillId: 'bird-tools', skillName: 'bird-tools', required: true, sortOrder: 0, missing: false },
      ],
    })

    render(
      <AdoptDialog
        preview={renamedPreview}
        packs={[]}
        onClose={() => {}}
        onDone={() => {}}
      />,
    )

    fireEvent.change(screen.getByLabelText('新的 Skill ID'), { target: { value: 'bird-tools' } })
    fireEvent.click(screen.getByRole('checkbox', { name: /同时加入技能包/ }))
    expect(screen.getByRole('button', { name: '新建技能包' })).toHaveClass('active')
    fireEvent.change(screen.getByLabelText('技能包名称'), { target: { value: '常用工具' } })
    fireEvent.click(screen.getByRole('button', { name: '确认接管' }))

    await waitFor(() => {
      expect(executeAdopt).toHaveBeenCalledWith('claude-code', 'local-bird', 'rename', 'bird-tools')
      expect(upsertPack).toHaveBeenCalledWith({
        id: '',
        name: '常用工具',
        description: '',
        tags: [],
        skillIds: ['bird-tools'],
      })
    })
  })

  it('retries a failed pack update without adopting the Skill twice', async () => {
    const executeAdopt = vi.spyOn(skillApiV2, 'executeAdopt').mockResolvedValue('bird')
    vi.spyOn(skillApiV2, 'getPackDetail').mockResolvedValue(existingPackDetail)
    vi.spyOn(skillApiV2, 'upsertPack')
      .mockRejectedValueOnce(new Error('pack write failed'))
      .mockResolvedValue({
        ...existingPackDetail,
        members: [
          ...existingPackDetail.members,
          { skillId: 'bird', skillName: 'bird', required: true, sortOrder: 1, missing: false },
        ],
      })
    const onDone = vi.fn()

    render(
      <AdoptDialog
        preview={preview}
        packs={[existingPack]}
        onClose={() => {}}
        onDone={onDone}
      />,
    )

    fireEvent.click(screen.getByRole('checkbox', { name: /同时加入技能包/ }))
    fireEvent.click(screen.getByRole('button', { name: '确认接管' }))
    expect(await screen.findByText(/接管已完成，但加入技能包失败/)).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '重试加入技能包' }))

    await waitFor(() => {
      expect(executeAdopt).toHaveBeenCalledTimes(1)
      expect(onDone).toHaveBeenCalledTimes(1)
    })
  })
})
