import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import '@testing-library/jest-dom/vitest'
import i18n from '../i18n'
import { skillApiV2 } from '../services/skillApiV2'
import type { AgentSummary, SkillPackPickerData, SkillPackSummary } from '../services/skillApiV2'
import { SkillPackPicker } from '../components/tray/SkillPackPicker'

const hideWindow = vi.fn().mockResolvedValue(undefined)

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ hide: hideWindow }),
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}))

const codex: AgentSummary = {
  id: 'codex',
  displayName: 'Codex',
  iconKey: 'codex',
  enabled: true,
  skillsDir: '/Users/me/.codex/skills',
  version: '1.0.0',
  latestVersion: null,
  installed: true,
  managedSkillCount: 0,
  unmanagedSkillCount: 0,
}

const packs: SkillPackSummary[] = [
  {
    id: 'frontend',
    name: '前端开发',
    description: '设计、实现与检查前端界面',
    tags: ['frontend'],
    memberCount: 3,
    appliedAgentCount: 0,
    healthy: true,
  },
  {
    id: 'review',
    name: '代码审查',
    description: '安全与质量检查',
    tags: ['review'],
    memberCount: 2,
    appliedAgentCount: 0,
    healthy: true,
  },
]

const pickerData: SkillPackPickerData = {
  agents: [codex],
  packs,
  appliedByAgent: { codex: [] },
  defaultDistributeMode: 'link',
}

describe('SkillPackPicker', () => {
  beforeEach(async () => {
    vi.clearAllMocks()
    await i18n.changeLanguage('zh')
    vi.spyOn(skillApiV2, 'getSkillPackPickerData').mockResolvedValue(pickerData)
    vi.spyOn(skillApiV2, 'previewApplyPack')
    vi.spyOn(skillApiV2, 'executeApplyPack').mockImplementation(async (packId, targetAgents, requestedMode) => ({
      skillIds: [packId],
      targetAgents,
      requestedMode,
      changes: [],
      blockers: [],
      blockerDecisions: [],
    }))
  })

  afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
  })

  it('keeps the picker open and the list stable while enabling multiple packs', async () => {
    render(<SkillPackPicker />)

    const frontend = await screen.findByRole('checkbox', { name: /前端开发/ })
    const review = screen.getByRole('checkbox', { name: /代码审查/ })
    fireEvent.click(review)
    expect(screen.getAllByRole('checkbox')[0]).toBe(frontend)
    expect(screen.getAllByRole('checkbox')[1]).toBe(review)
    fireEvent.click(frontend)

    expect(frontend).toHaveAttribute('aria-checked', 'true')
    expect(review).toHaveAttribute('aria-checked', 'true')
    expect(screen.getByText('面板会保持展开，可以连续勾选多个技能包。')).toBeInTheDocument()
    expect(hideWindow).not.toHaveBeenCalled()
    await waitFor(() => expect(skillApiV2.executeApplyPack).toHaveBeenCalledTimes(2))
    expect(skillApiV2.getSkillPackPickerData).toHaveBeenCalledTimes(1)
    expect(skillApiV2.previewApplyPack).not.toHaveBeenCalled()
  })

  it('places applied packs before unchecked packs', async () => {
    vi.mocked(skillApiV2.getSkillPackPickerData).mockResolvedValueOnce({
      ...pickerData,
      appliedByAgent: { codex: ['review'] },
    })

    render(<SkillPackPicker />)

    const items = await screen.findAllByRole('checkbox')
    expect(items[0]).toHaveTextContent('代码审查')
    expect(items[0]).toHaveAttribute('aria-checked', 'true')
    expect(items[1]).toHaveTextContent('前端开发')
  })
})
