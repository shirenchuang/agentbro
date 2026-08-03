import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import '@testing-library/jest-dom/vitest'
import i18n from '../i18n'
import { skillApiV2 } from '../services/skillApiV2'
import type { AgentSummary, SkillPackPickerData, SkillPackSummary } from '../services/skillApiV2'
import { SkillPackPicker } from '../components/tray/SkillPackPicker'
import { useConfigStore } from '../stores/configStore'
import {
  LOCAL_RUNTIME_ENVIRONMENT_ID,
  useRuntimeEnvironmentStore,
} from '../stores/runtimeEnvironmentStore'

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

const claudeCode: AgentSummary = {
  ...codex,
  id: 'claude-code',
  displayName: 'Claude Code',
  iconKey: 'claude',
  skillsDir: '/Users/me/.claude/skills',
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
    useConfigStore.setState({ remoteHostEntries: [] })
    useRuntimeEnvironmentStore.setState({
      selectedEnvironmentId: LOCAL_RUNTIME_ENVIRONMENT_ID,
    })
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

  it('marks the server when skill packs are managed remotely', async () => {
    useConfigStore.setState({
      remoteHostEntries: [{
        id: 'ubuntu',
        name: 'ubuntu',
        sshTarget: 'agent@ubuntu',
        port: 22,
        remoteSocketPath: '/tmp/agentbro.sock',
        autoConnect: false,
        connectionStatus: 'disconnected',
      }],
    })
    useRuntimeEnvironmentStore.setState({ selectedEnvironmentId: 'ubuntu' })

    render(<SkillPackPicker />)

    const environment = screen.getByRole('status', { name: '当前运行环境：ubuntu' })
    expect(environment).toHaveTextContent('远程')
    expect(environment).toHaveTextContent('ubuntu')
    expect(environment).toHaveAttribute(
      'title',
      '当前技能包来自 ubuntu；读写会按需通过 SSH 完成，不依赖实时事件通道。',
    )
    expect(await screen.findByRole('checkbox', { name: /前端开发/ })).toBeInTheDocument()
  })

  it('rehydrates the local target before loading a previously remote picker', async () => {
    useRuntimeEnvironmentStore.setState({ selectedEnvironmentId: 'ubuntu' })
    window.localStorage.setItem('agentbro-runtime-environment', JSON.stringify({
      state: { selectedEnvironmentId: LOCAL_RUNTIME_ENVIRONMENT_ID },
      version: 0,
    }))

    render(<SkillPackPicker />)

    expect(await screen.findByRole('checkbox', { name: /前端开发/ })).toBeInTheDocument()
    expect(useRuntimeEnvironmentStore.getState().selectedEnvironmentId).toBe(
      LOCAL_RUNTIME_ENVIRONMENT_ID,
    )
    expect(screen.queryByRole('status')).not.toBeInTheDocument()
    expect(skillApiV2.getSkillPackPickerData).toHaveBeenCalledTimes(1)
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

  it('hides the picker when the done button is clicked', async () => {
    render(<SkillPackPicker />)

    await screen.findByRole('checkbox', { name: /前端开发/ })
    fireEvent.click(screen.getByRole('button', { name: '完成' }))

    await waitFor(() => expect(hideWindow).toHaveBeenCalledTimes(1))
  })

  it('scrolls the agent tabs horizontally with the mouse wheel', async () => {
    vi.mocked(skillApiV2.getSkillPackPickerData).mockResolvedValueOnce({
      ...pickerData,
      agents: [codex, claudeCode],
      appliedByAgent: { codex: [], 'claude-code': [] },
    })

    render(<SkillPackPicker />)

    const tabs = await screen.findByRole('tablist')
    Object.defineProperties(tabs, {
      clientWidth: { configurable: true, value: 200 },
      scrollWidth: { configurable: true, value: 500 },
    })
    fireEvent.wheel(tabs, { deltaX: 0, deltaY: 80 })

    expect(tabs.scrollLeft).toBe(80)
  })

  it('switches agents on a regular pointer click without capturing it', async () => {
    vi.mocked(skillApiV2.getSkillPackPickerData).mockResolvedValueOnce({
      ...pickerData,
      agents: [codex, claudeCode],
      appliedByAgent: { codex: [], 'claude-code': [] },
    })

    render(<SkillPackPicker />)

    const tabs = await screen.findByRole('tablist')
    const claudeTab = screen.getByRole('tab', { name: /Claude Code/ })
    const setPointerCapture = vi.fn()
    Object.defineProperty(tabs, 'setPointerCapture', {
      configurable: true,
      value: setPointerCapture,
    })

    fireEvent.pointerDown(claudeTab, { pointerId: 1, pointerType: 'mouse', button: 0, clientX: 160 })
    fireEvent.pointerUp(claudeTab, { pointerId: 1, pointerType: 'mouse', clientX: 160 })
    fireEvent.click(claudeTab)

    expect(setPointerCapture).not.toHaveBeenCalled()
    expect(claudeTab).toHaveAttribute('aria-selected', 'true')
    expect(screen.getByRole('tab', { name: /Codex/ })).toHaveAttribute('aria-selected', 'false')
  })

  it('drags the agent tabs without selecting the tab under the pointer', async () => {
    vi.mocked(skillApiV2.getSkillPackPickerData).mockResolvedValueOnce({
      ...pickerData,
      agents: [codex, claudeCode],
      appliedByAgent: { codex: [], 'claude-code': [] },
    })

    render(<SkillPackPicker />)

    const tabs = await screen.findByRole('tablist')
    const claudeTab = screen.getByRole('tab', { name: /Claude Code/ })
    const setPointerCapture = vi.fn()
    Object.defineProperty(tabs, 'setPointerCapture', {
      configurable: true,
      value: setPointerCapture,
    })
    fireEvent.pointerDown(claudeTab, { pointerId: 1, pointerType: 'mouse', button: 0, clientX: 160 })
    fireEvent.pointerMove(tabs, { pointerId: 1, pointerType: 'mouse', clientX: 80 })
    fireEvent.pointerUp(tabs, { pointerId: 1, pointerType: 'mouse', clientX: 80 })
    fireEvent.click(claudeTab)

    expect(setPointerCapture).toHaveBeenCalledWith(1)
    expect(tabs.scrollLeft).toBe(80)
    expect(screen.getByRole('tab', { name: /Codex/ })).toHaveAttribute('aria-selected', 'true')
    expect(claudeTab).toHaveAttribute('aria-selected', 'false')
  })
})
