import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { ButtonHTMLAttributes, CSSProperties, ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { open } from '@tauri-apps/plugin-dialog'
import { useSkillStoreV2 } from '../../stores/skillStoreV2'
import { skillApiV2 } from '../../services/skillApiV2'
import { agentApi, type AgentOutputEvent, type AgentProgramInfo, type CustomAgentConfig } from '../../services/agentApi'
import { configureAgentHookEvents, getAllHookStatus, installAgentHook, uninstallAgentHook, type HookEventStatus, type HookStatus } from '../../services/tauriApi'
import type { AgentConfigDocument, AgentDetail, AdoptPreview, ConflictBlocker, DistributionBlockerDecision, DistributionPreview, InheritedSkillDetail, MoveDirectSkillToPackPreview, SkillPackSummary, SkillSummary, UnmanagedItemDto } from '../../services/skillApiV2'
import { useSessionStore } from '../../stores/sessionStore'
import {
  LOCAL_RUNTIME_ENVIRONMENT_ID,
  useRuntimeEnvironmentStore,
} from '../../stores/runtimeEnvironmentStore'
import { AgentIconBadge } from './AgentIconBadge'
import { AdoptDialog } from './AdoptDialog'
import { PreviewDialog } from './PreviewDialog'
import { McpManagementTab } from './McpManagementTab'
import { PluginManagementTab } from './PluginManagementTab'
import { SkillDetailSlider, type SkillDetailFallback } from './SkillDetailSlider'
import { distributionBlockerReason, isAdoptOptionUnavailableError, skillErrorMessage, skillModeLabel, skillSourceTypeLabel, skillStatusLabel, targetClaimLabel, unmanagedReasonLabel } from './skillLabels'
import { buildAgentUsageScores, readStoredAgentOrder, sortAgentSummaries } from '../../utils/agentOrdering'

type DetailTab = 'overview' | 'skills' | 'mcp' | 'plugins' | 'hooks' | 'config'
type AgentSkillViewMode = 'cards' | 'list'
type AgentSkillScope = 'managed' | 'unmanaged' | 'inherited' | 'builtin'
type AgentSkillPackFilter = 'all' | 'pack' | 'standalone'
type AgentLibraryScope = 'uninstalled' | 'installed' | 'all'
type InstallBlockerDecision = 'overwrite' | 'skip'
type PackBlockerDecision = DistributionBlockerDecision['action']
type PackApplyProgress = {
  packName: string
  agentName: string
  state: 'running' | 'done'
  percent: number
  detail: string
}
type PackApplyConflictDialogState = {
  packId: string
  packName: string
  agentId: string
  agentName: string
  mode: 'link' | 'copy'
  preview: DistributionPreview
}
type BatchAdoptPackSelection =
  | { kind: 'none' }
  | { kind: 'existing'; packId: string }
  | { kind: 'new'; name: string }
type BatchAdoptPackOption = {
  id: string
  name: string
  description: string
  memberCount: number
}
type MoveToPackOption = BatchAdoptPackOption & {
  applied: boolean
  isDefault: boolean
}

const PAGE_SIZE = 28
const SHARED_SKILLS_AGENT_ID = 'agents'
const NOTICE_DISMISS_MS = 3200
const PACK_PROGRESS_DONE_DISMISS_MS = 2400

export function AgentManagementPage() {
  const { t } = useTranslation()
  const state = useSkillStoreV2()
  const allSessions = useSessionStore((s) => s.sessionList)
  const activeSessionId = useSessionStore((s) => s.activeSessionId)
  const runtimeEnvironmentId = useRuntimeEnvironmentStore((s) => s.selectedEnvironmentId)
  const sessionList = useMemo(() => (
    runtimeEnvironmentId === LOCAL_RUNTIME_ENVIRONMENT_ID
      ? allSessions.filter((session) => !session.remoteHostId)
      : allSessions.filter((session) => session.remoteHostId === runtimeEnvironmentId)
  ), [allSessions, runtimeEnvironmentId])
  const agentUsageScores = useMemo(() => buildAgentUsageScores(sessionList, activeSessionId), [sessionList, activeSessionId])
  const agents = useMemo(() => sortAgentSummaries(
    state.agents.filter((agent) => agent.id !== SHARED_SKILLS_AGENT_ID),
    { manualOrder: readStoredAgentOrder(), usageScores: agentUsageScores },
  ), [agentUsageScores, state.agents])
  const detail = state.selectedAgentDetail?.id === SHARED_SKILLS_AGENT_ID ? null : state.selectedAgentDetail
  const [tab, setTab] = useState<DetailTab>('overview')
  const [adopt, setAdopt] = useState<AdoptPreview | null>(null)
  const [detailSkillId, setDetailSkillId] = useState<string | null>(null)
  const [detailFallback, setDetailFallback] = useState<SkillDetailFallback | null>(null)
  const [busy, setBusy] = useState(false)
  const [refreshingOverview, setRefreshingOverview] = useState(false)
  const [refreshingAll, setRefreshingAll] = useState(false)
  const [scanningAgentId, setScanningAgentId] = useState<string | null>(null)
  const [updatingAgentId, setUpdatingAgentId] = useState<string | null>(null)
  const [installingAgentId, setInstallingAgentId] = useState<string | null>(null)
  const [uninstallingAgentId, setUninstallingAgentId] = useState<string | null>(null)
  const [openingAgentId, setOpeningAgentId] = useState<string | null>(null)
  const [adoptingUnmanagedId, setAdoptingUnmanagedId] = useState<string | null>(null)
  const [programs, setPrograms] = useState<Record<string, AgentProgramInfo>>({})
  const [programLoading, setProgramLoading] = useState(false)
  const [agentOutput, setAgentOutput] = useState<string[]>([])
  const [notice, setNotice] = useState<string | null>(null)
  const [packApplyConflict, setPackApplyConflict] = useState<PackApplyConflictDialogState | null>(null)
  const [packApplyBusy, setPackApplyBusy] = useState(false)
  const [customDialogOpen, setCustomDialogOpen] = useState(false)
  const [uninstallAgentTarget, setUninstallAgentTarget] = useState<AgentDetail | null>(null)
  const [deleteAgentTarget, setDeleteAgentTarget] = useState<AgentDetail | null>(null)
  const [deletingAgentId, setDeletingAgentId] = useState<string | null>(null)
  const selectedAgentIdRef = useRef<string | null>(null)
  const packApplyResolverRef = useRef<((applied: boolean) => void) | null>(null)
  const actionBusy = busy || refreshingOverview || refreshingAll || scanningAgentId !== null || updatingAgentId !== null || installingAgentId !== null || uninstallingAgentId !== null || openingAgentId !== null || deletingAgentId !== null

  const loadPrograms = useCallback(async () => {
    setProgramLoading(true)
    try {
      const next = await agentApi.refresh()
      setPrograms(Object.fromEntries(next.map((agent) => [agent.id, agent])))
    } catch (e) {
      state.setError(skillErrorMessage(t, e))
    } finally {
      setProgramLoading(false)
    }
  }, [state, t])

  useEffect(() => {
    state.loadOverview()
    loadPrograms()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    if (state.customAgentDialogRequest <= 0) return
    setCustomDialogOpen(true)
    useSkillStoreV2.setState({ customAgentDialogRequest: 0 })
  }, [state.customAgentDialogRequest])

  useEffect(() => {
    selectedAgentIdRef.current = state.selectedAgentId
  }, [state.selectedAgentId])

  useEffect(() => {
    let unlisten: (() => void) | null = null
    let alive = true
    agentApi.onOutput((event) => {
      if (event.agentId !== selectedAgentIdRef.current) return
      setAgentOutput((prev) => [...prev, formatAgentOutput(event)].slice(-10))
    }).then((next) => {
      if (alive) unlisten = next
      else next()
    }).catch(() => {
      // Output streaming is best-effort; the awaited install/update command
      // still reports failure through the action itself.
    })
    return () => {
      alive = false
      unlisten?.()
    }
  }, [])

  useEffect(() => {
    if (agents.length === 0) {
      if (state.selectedAgentId === SHARED_SKILLS_AGENT_ID) state.selectAgent(null)
      return
    }
    if (state.selectedAgentId && agents.some((agent) => agent.id === state.selectedAgentId)) return
    const first = agents.find((a) => a.installed) || agents[0]
    if (first) state.selectAgent(first.id)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agents, state.selectedAgentId])

  useEffect(() => {
    setTab('overview')
    setNotice(null)
    setAgentOutput([])
  }, [state.selectedAgentId])

  useEffect(() => {
    if (!notice) return
    const timer = window.setTimeout(() => setNotice(null), NOTICE_DISMISS_MS)
    return () => window.clearTimeout(timer)
  }, [notice])

  const openAdopt = async (agentId: string, unmanagedId: string) => {
    setAdoptingUnmanagedId(unmanagedId)
    setBusy(true)
    try {
      const p = await skillApiV2.previewAdopt(agentId, unmanagedId)
      setAdopt(p)
    } catch (e) {
      state.setError(String(e))
    } finally {
      setAdoptingUnmanagedId(null)
      setBusy(false)
    }
  }

  const revoke = async (packId: string, agentId: string) => {
    setBusy(true)
    setNotice(null)
    state.setError(null)
    try {
      await skillApiV2.removePackFromAgent(packId, agentId)
      await state.loadAgentDetail(agentId, true)
      void state.loadOverview(true)
      state.setError(null)
      setNotice('技能包已取消应用')
    } catch (e) {
      state.setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  const resolvePackApplyConflict = (applied: boolean) => {
    packApplyResolverRef.current?.(applied)
    packApplyResolverRef.current = null
    setPackApplyConflict(null)
  }

  const openPackApplyConflict = (dialog: PackApplyConflictDialogState) =>
    new Promise<boolean>((resolve) => {
      packApplyResolverRef.current = resolve
      setPackApplyConflict(dialog)
    })

  const packApplyDialogInfo = (packId: string, agentId: string, preview: DistributionPreview, mode: 'link' | 'copy'): PackApplyConflictDialogState => {
    const detail = useSkillStoreV2.getState().selectedAgentDetail
    const pack = detail?.availablePacks.find((item) => item.id === packId) ?? detail?.appliedPacks.find((item) => item.packId === packId)
    return {
      packId,
      packName: pack && 'name' in pack ? pack.name : pack?.packName ?? packId,
      agentId,
      agentName: detail?.id === agentId ? detail.displayName : agentId,
      mode,
      preview,
    }
  }

  const applyPack = async (packId: string, agentId: string): Promise<boolean> => {
    setBusy(true)
    setNotice(null)
    state.setError(null)
    let keepBusyUntilReturn = false
    try {
      const mode = state.settings?.defaultDistributeMode || 'link'
      const preview = await skillApiV2.executeApplyPack(packId, [agentId], mode)
      if (preview.blockers.length > 0) {
        setBusy(false)
        keepBusyUntilReturn = true
        return openPackApplyConflict(packApplyDialogInfo(packId, agentId, preview, mode))
      }
      await state.loadAgentDetail(agentId, true)
      void state.loadOverview(true)
      state.setError(null)
      setNotice('技能包已应用')
      return true
    } catch (e) {
      state.setError(String(e))
      return false
    } finally {
      if (!keepBusyUntilReturn) setBusy(false)
    }
  }

  const executePackApplyConflict = async (decisions: DistributionBlockerDecision[]) => {
    if (!packApplyConflict) return
    setPackApplyBusy(true)
    try {
      const result = await skillApiV2.executeApplyPack(
        packApplyConflict.packId,
        [packApplyConflict.agentId],
        packApplyConflict.mode,
        decisions,
      )
      if (result.blockers.length > 0) {
        setPackApplyConflict((current) => current ? { ...current, preview: result } : current)
        return
      }
      await state.loadAgentDetail(packApplyConflict.agentId, true)
      void state.loadOverview(true)
      state.setError(null)
      setNotice('技能包已应用')
      resolvePackApplyConflict(true)
    } finally {
      setPackApplyBusy(false)
    }
  }

  const scanAgent = async (agentId: string) => {
    setScanningAgentId(agentId)
    setNotice(null)
    state.setError(null)
    try {
      const result = await skillApiV2.scanAgentInventory(agentId)
      const unmanaged = await skillApiV2.listUnmanaged()
      useSkillStoreV2.setState({ unmanaged })
      await state.loadAgentDetail(agentId, true)
      await state.loadOverview(true)
      const readOnlyCount = result.readOnly ?? 0
      const sharedReadOnlyCount = result.sharedReadOnly ?? 0
      setNotice(t(
        result.includedShared
          ? readOnlyCount > 0 || sharedReadOnlyCount > 0
            ? 'skills.agentManagement.scanCompleteSharedReadOnly'
            : 'skills.agentManagement.scanCompleteShared'
          : readOnlyCount > 0
          ? 'skills.agentManagement.scanCompleteReadOnly'
          : 'skills.agentManagement.scanComplete',
        {
          managed: result.managed,
          unmanaged: result.unmanaged,
          readOnly: readOnlyCount,
          sharedManaged: result.sharedManaged ?? 0,
          sharedUnmanaged: result.sharedUnmanaged ?? 0,
          sharedReadOnly: sharedReadOnlyCount,
        },
      ))
    } catch (e) {
      state.setError(String(e))
    } finally {
      setScanningAgentId(null)
    }
  }

  const updateAgent = async (agentId: string) => {
    setUpdatingAgentId(agentId)
    setNotice(null)
    state.setError(null)
    setAgentOutput([])
    try {
      await agentApi.update(agentId)
      await loadPrograms()
      await state.loadOverview(true)
      await state.loadAgentDetail(agentId, true)
      setNotice('Agent 更新完成')
    } catch (e) {
      state.setError(String(e))
    } finally {
      setUpdatingAgentId(null)
    }
  }

  const installAgent = async (agentId: string) => {
    setInstallingAgentId(agentId)
    setNotice(null)
    state.setError(null)
    setAgentOutput([])
    try {
      await agentApi.install(agentId)
      await loadPrograms()
      await state.loadOverview(true)
      await state.loadAgentDetail(agentId, true)
      setNotice('Agent 安装完成')
    } catch (e) {
      state.setError(String(e))
    } finally {
      setInstallingAgentId(null)
    }
  }

  const uninstallAgent = async (agent: AgentDetail) => {
    setUninstallingAgentId(agent.id)
    setNotice(null)
    state.setError(null)
    setAgentOutput([])
    const failures: string[] = []
    let removedSkills = 0
    try {
      const program = programs[agent.id]
      const programInstalled = program?.status === 'installed' || program?.status === 'updateAvailable'
      if (programInstalled && program.uninstallCommand) {
        try {
          await agentApi.uninstall(agent.id)
        } catch (error) {
          failures.push(`程序：${skillErrorMessage(t, error)}`)
        }
      }

      const targetIds = agent.skills.map((skill) => skill.id)
      if (targetIds.length > 0) {
        try {
          const result = await skillApiV2.deleteSkillTargetDistributions(targetIds)
          removedSkills += result.deleted
          failures.push(...result.failures.map((failure) => `Skill：${failure.error}`))
        } catch (error) {
          failures.push(`已管理 Skills：${skillErrorMessage(t, error)}`)
        }
      }

      const unmanagedItems = useSkillStoreV2.getState().unmanaged
        .filter((item) => item.agentId === agent.id && !isReadOnlyUnmanaged(item) && (item.itemType === 'agent_skill' || item.itemType === 'skill'))
      for (const item of unmanagedItems) {
        try {
          await skillApiV2.deleteUnmanagedAgentSkill(agent.id, item.id)
          removedSkills += 1
        } catch (error) {
          failures.push(`${item.inferredSkillId || item.path}：${skillErrorMessage(t, error)}`)
        }
      }

      if (program?.hooksInstalled) {
        try {
          await agentApi.uninstallHook(agent.id)
        } catch (error) {
          failures.push(`Hook：${skillErrorMessage(t, error)}`)
        }
      }

      const remainingUnmanaged = await skillApiV2.listUnmanaged()
      useSkillStoreV2.setState({ unmanaged: remainingUnmanaged })
      setUninstallAgentTarget(null)
      await Promise.all([loadPrograms(), state.loadOverview(true)])
      if (failures.length > 0) {
        if (useSkillStoreV2.getState().selectedAgentId === agent.id) {
          await state.loadAgentDetail(agent.id, true)
        }
        state.setError(`部分卸载失败：${failures.slice(0, 3).join('；')}`)
      } else {
        const programRemoved = !programInstalled || Boolean(program?.uninstallCommand)
        if (programRemoved) {
          const current = useSkillStoreV2.getState()
          const nextAgents = current.agents.map((item) => item.id === agent.id
            ? {
                ...item,
                enabled: false,
                installed: false,
                version: null,
                managedSkillCount: 0,
                unmanagedSkillCount: 0,
              }
            : item)
          useSkillStoreV2.setState({ agents: nextAgents })
          if (current.selectedAgentId === agent.id) {
            const nextAgent = nextAgents.find((item) => item.installed && item.id !== SHARED_SKILLS_AGENT_ID)
            await state.selectAgent(nextAgent?.id ?? null)
          }
        }
        const cleanup = removedSkills > 0 ? `，已清理 ${removedSkills} 个 Skills` : ''
        setNotice(`Agent「${agent.displayName}」已卸载${cleanup}`)
      }
    } catch (e) {
      state.setError(`卸载失败：${skillErrorMessage(t, e)}`)
    } finally {
      setUninstallingAgentId(null)
    }
  }

  const openAgentDownload = async (agentId: string) => {
    setOpeningAgentId(agentId)
    try {
      await agentApi.openDownload(agentId)
    } catch (e) {
      state.setError(String(e))
    } finally {
      setOpeningAgentId(null)
    }
  }

  const refreshAll = async () => {
    setRefreshingAll(true)
    setNotice(null)
    state.setError(null)
    try {
      await Promise.all([state.loadOverview(true), loadPrograms()])
      if (state.selectedAgentId && state.selectedAgentId !== SHARED_SKILLS_AGENT_ID) {
        await state.loadAgentDetail(state.selectedAgentId, true)
      }
    } finally {
      setRefreshingAll(false)
    }
  }

  const refreshOverview = async () => {
    setRefreshingOverview(true)
    setNotice(null)
    state.setError(null)
    try {
      await Promise.all([state.refresh(), loadPrograms()])
    } finally {
      setRefreshingOverview(false)
    }
  }

  const openSkillDetail = (skillId: string, fallback?: SkillDetailFallback | null) => {
    setDetailSkillId(skillId)
    setDetailFallback(fallback || null)
  }

  const addCustomAgent = async (config: CustomAgentConfig) => {
    setBusy(true)
    setNotice(null)
    state.setError(null)
    try {
      const added = await agentApi.addCustom(config)
      await Promise.all([state.loadOverview(true), loadPrograms()])
      if (useSkillStoreV2.getState().agents.some((agent) => agent.id === added.id)) {
        await state.selectAgent(added.id)
      }
      setCustomDialogOpen(false)
      setNotice(config.category === 'claude-compatible'
        ? 'Claude Code 实例已添加，Hook 已安装'
        : '自定义 Agent 已添加')
    } catch (e) {
      state.setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  const deleteCustomAgent = async (agent: AgentDetail) => {
    setDeletingAgentId(agent.id)
    setNotice(null)
    state.setError(null)
    try {
      await agentApi.removeCustom(agent.id)
      useSkillStoreV2.setState((current) => ({
        agents: current.agents.filter((item) => item.id !== agent.id),
      }))
      setDeleteAgentTarget(null)
      await state.selectAgent(null)
      await Promise.all([state.loadOverview(true), loadPrograms()])
      setNotice(`已删除自定义 Agent「${agent.displayName}」`)
    } catch (e) {
      state.setError(String(e))
    } finally {
      setDeletingAgentId(null)
    }
  }

  const selectedProgram = detail ? programs[detail.id] ?? null : null
  const selectedProgramInstalled = selectedProgram?.status === 'installed' || selectedProgram?.status === 'updateAvailable'
  const selectedUnmanagedItems = detail
    ? state.unmanaged.filter((item) => item.agentId === detail.id && (item.itemType === 'agent_skill' || item.itemType === 'skill'))
    : []
  const selectedHasResiduals = Boolean(detail && (
    detail.skills.length > 0
    || selectedUnmanagedItems.length > 0
    || detail.appliedPacks.length > 0
    || selectedProgram?.hooksInstalled
  ))
  const canUninstallSelectedAgent = Boolean(
    selectedProgram
    && !selectedProgram.isCustom
    && ((selectedProgramInstalled && selectedProgram?.uninstallCommand) || selectedHasResiduals),
  )
  const uninstallProgram = uninstallAgentTarget ? programs[uninstallAgentTarget.id] ?? null : null
  const uninstallProgramInstalled = uninstallProgram?.status === 'installed' || uninstallProgram?.status === 'updateAvailable'
  const uninstallUnmanagedItems = uninstallAgentTarget
    ? state.unmanaged.filter((item) => item.agentId === uninstallAgentTarget.id && (item.itemType === 'agent_skill' || item.itemType === 'skill'))
    : []

  return (
    <div className="sm2 sm2--agents">
      <div className="sm2__header sm2__header--stacked">
        <div>
          <h2 className="sm2__title">Agent 管理</h2>
          <p className="sm2__header-subtitle">查看每个 Agent 的 Skills、技能包、MCP、插件与 Hook 状态。</p>
        </div>
        <div className="sm2__tabs">
          {detail && canUninstallSelectedAgent && (
            <ActionButton
              className="sm2__btn sm2__btn--danger"
              disabled={state.loading || actionBusy}
              busy={uninstallingAgentId === detail.id}
              busyLabel="卸载中"
              onClick={() => setUninstallAgentTarget(detail)}
            >
              卸载 Agent
            </ActionButton>
          )}
          <ActionButton className="sm2__btn" onClick={refreshOverview} disabled={state.loading || actionBusy} busy={refreshingOverview} busyLabel="刷新中">
            刷新总览
          </ActionButton>
          <ActionButton className="sm2__btn" onClick={refreshAll} disabled={state.loading || actionBusy || programLoading} busy={refreshingAll || programLoading} busyLabel="获取中">
            获取版本
          </ActionButton>
        </div>
      </div>

      <AgentToastStack
        error={state.error}
        notice={notice}
        onDismissError={() => state.setError(null)}
        onDismissNotice={() => setNotice(null)}
      />

      <div className="sm2__main sm2__main--full settings-scroll">
        {!detail ? (
          <div className="sm2__empty">
            {state.agentDetailLoading ? '加载 Agent 详情…' : '选择一个已安装 Agent 查看详情'}
          </div>
        ) : (
          <AgentDetailView
            detail={detail}
            tab={tab}
            onTab={setTab}
            busy={actionBusy}
            scanning={scanningAgentId === detail.id}
            updating={updatingAgentId === detail.id}
            installing={installingAgentId === detail.id}
            opening={openingAgentId === detail.id}
            adoptingUnmanagedId={adoptingUnmanagedId}
            program={programs[detail.id] || null}
            agentInstalled={agents.find((agent) => agent.id === detail.id)?.installed ?? Boolean(detail.version || detail.skillsDir)}
            programLoading={programLoading}
            agentOutput={agentOutput}
            onAdopt={openAdopt}
            onRevoke={revoke}
            onApplyPack={applyPack}
            onScan={scanAgent}
            onUpdate={updateAgent}
            onInstall={installAgent}
            onOpenDownload={openAgentDownload}
            onOpenSkillDetail={openSkillDetail}
            deleting={deletingAgentId === detail.id}
            onRequestDelete={setDeleteAgentTarget}
          />
        )}
      </div>

      <SkillDetailSlider
        skillId={detailSkillId}
        open={!!detailSkillId}
        fallbackSkill={detailFallback}
        onClose={() => {
          setDetailSkillId(null)
          setDetailFallback(null)
        }}
      />

      {adopt && (
        <AdoptDialog
          preview={adopt}
          packs={state.packs}
          onClose={() => setAdopt(null)}
          onDone={async () => {
            const agentId = adopt.agentId
            const visibleDetailId = detail?.id
            setAdopt(null)
            const unmanaged = await skillApiV2.listUnmanaged()
            useSkillStoreV2.setState({ unmanaged })
            if (agentId !== SHARED_SKILLS_AGENT_ID) await state.loadAgentDetail(agentId, true)
            if (visibleDetailId && visibleDetailId !== agentId) await state.loadAgentDetail(visibleDetailId, true)
            await state.loadOverview(true)
          }}
        />
      )}
      {packApplyConflict && (
        <PackApplyConflictDialog
          dialog={packApplyConflict}
          busy={packApplyBusy}
          onCancel={() => resolvePackApplyConflict(false)}
          onExecute={executePackApplyConflict}
        />
      )}
      {customDialogOpen && (
        <CustomAgentDialog
          busy={busy}
          onClose={() => setCustomDialogOpen(false)}
          onSubmit={addCustomAgent}
        />
      )}
      {uninstallAgentTarget && (
        <PreviewDialog
          title={`卸载 Agent「${uninstallAgentTarget.displayName}」`}
          confirmLabel="确认卸载"
          busyLabel="卸载中"
          modalClassName="sm2__modal--agent-uninstall"
          destructive
          busy={uninstallingAgentId === uninstallAgentTarget.id}
          onCancel={() => setUninstallAgentTarget(null)}
          onConfirm={() => uninstallAgent(uninstallAgentTarget)}
        >
          <p className="sm2-agent-uninstall__intro">卸载会移除支持自动卸载的程序，并清理 AgentBro 检测到的本地能力残留。</p>
          <div className="sm2-agent-uninstall__summary" aria-label="卸载清理范围">
            {uninstallProgramInstalled && uninstallProgram?.uninstallCommand && (
              <div><strong>程序</strong><span>{uninstallProgram.kind === 'app' ? '移到废纸篓' : '执行卸载命令'}</span></div>
            )}
            {uninstallProgramInstalled && !uninstallProgram?.uninstallCommand && (
              <div><strong>程序</strong><span>不支持自动卸载，将保留</span></div>
            )}
            {uninstallAgentTarget.skills.length > 0 && (
              <div><strong>{uninstallAgentTarget.skills.length}</strong><span>已管理 Skills</span></div>
            )}
            {uninstallUnmanagedItems.length > 0 && (
              <div className="sm2-agent-uninstall__danger-count"><strong>{uninstallUnmanagedItems.length}</strong><span>未管理 Skills</span></div>
            )}
            {uninstallAgentTarget.appliedPacks.length > 0 && (
              <div><strong>{uninstallAgentTarget.appliedPacks.length}</strong><span>技能包关联</span></div>
            )}
            {uninstallProgram?.hooksInstalled && (
              <div><strong>Hook</strong><span>移除 AgentBro Hook</span></div>
            )}
            {!uninstallProgramInstalled && (
              <div><strong>程序</strong><span>未安装，仅清理残留</span></div>
            )}
          </div>
          {uninstallUnmanagedItems.length > 0 && (
            <div className="sm2-agent-uninstall__warning">未管理 Skills 会直接删除；若中心库没有副本，删除后无法从 AgentBro 恢复。</div>
          )}
          <div className="sm2-agent-uninstall__preserved">保留：中心技能库、Agent 配置、会话记录、MCP 与插件配置。</div>
          {uninstallProgramInstalled && uninstallProgram?.uninstallCommand && (
            <code className="sm2__command-preview">
              {uninstallProgram.kind === 'app'
                ? `移到废纸篓：${uninstallProgram.appPath}`
                : uninstallProgram.uninstallCommand}
            </code>
          )}
        </PreviewDialog>
      )}
      {deleteAgentTarget && (
        <PreviewDialog
          title={`删除 Agent「${deleteAgentTarget.displayName}」`}
          confirmLabel="确认删除"
          busyLabel="删除中"
          destructive
          busy={deletingAgentId === deleteAgentTarget.id}
          onCancel={() => setDeleteAgentTarget(null)}
          onConfirm={() => deleteCustomAgent(deleteAgentTarget)}
        >
          <p>会移除 AgentBro 注册并清理该实例的 AgentBro Hook，不会删除配置目录、会话记录或 Skills 文件。</p>
        </PreviewDialog>
      )}
    </div>
  )
}

function CustomAgentDialog({
  busy,
  onClose,
  onSubmit,
}: {
  busy: boolean
  onClose: () => void
  onSubmit: (config: CustomAgentConfig) => Promise<void>
}) {
  const { t } = useTranslation()
  const [displayName, setDisplayName] = useState('')
  const [configRoot, setConfigRoot] = useState('')
  const [skillsDir, setSkillsDir] = useState('')
  const [settingsFile, setSettingsFile] = useState('')
  const [mcpConfig, setMcpConfig] = useState('')
  const [pluginDir, setPluginDir] = useState('')
  const [error, setError] = useState<string | null>(null)

  const applyRoot = (root: string) => {
    setConfigRoot(root)
    const paths = deriveCustomAgentPaths(root)
    setSkillsDir(paths.skillsDir)
    setSettingsFile(paths.settingsFile)
    setMcpConfig(paths.mcpConfig)
    setPluginDir(paths.pluginDir)
  }

  const chooseRoot = async () => {
    try {
      const selected = await open({ directory: true, multiple: false })
      if (typeof selected === 'string') applyRoot(selected)
    } catch (nextError) {
      setError(String(nextError))
    }
  }

  const submit = async () => {
    const name = displayName.trim()
    const root = configRoot.trim()
    const skillPath = skillsDir.trim()
    if (!name) {
      setError('请填写显示名称')
      return
    }
    if (!root) {
      setError('请填写配置根目录')
      return
    }
    if (!skillPath) {
      setError('请填写 Skills 目录')
      return
    }
    setError(null)
    await onSubmit({
      id: null,
      displayName: name,
      category: 'claude-compatible',
      globalSkillsDir: skillPath,
      iconName: 'claude-code',
      configDir: root,
      settingsFile: settingsFile.trim() || null,
      mcpConfig: mcpConfig.trim() || null,
      pluginDir: pluginDir.trim() || null,
    })
  }

  return (
    <div className="skills-dialog-overlay sm2-custom-agent-overlay" onClick={onClose}>
      <div
        aria-labelledby="custom-agent-dialog-title"
        className="skills-dialog custom-agent-dialog"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
      >
        <div className="skills-dialog__header">
          <div>
            <div className="skills-dialog__title" id="custom-agent-dialog-title">{t('settings.addEngineBranch')}</div>
            <p className="custom-agent-dialog__subtitle">让企业封装版 Claude Code 使用自己的配置目录，同时复用 AgentBro 的 Hook、会话和 Skills 管理。</p>
          </div>
        </div>
        <div className="skills-dialog__body">
          <div className="custom-agent-compatibility" role="note">
            <AgentIconBadge iconKey="claude-code" size={28} />
            <div>
              <strong>沿用 Claude Code 能力</strong>
              <span>保存后自动写入该目录的 settings.json，并监听独立的 projects 与 Skills 目录。</span>
            </div>
            <em>自动安装 Hook</em>
          </div>
          <div className="install-form-row">
            <label className="install-form-label" htmlFor="custom-agent-name">显示名称</label>
            <input
              className="install-form-input"
              id="custom-agent-name"
              placeholder="例如研发团队 Claude Code"
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
            />
          </div>
          <div className="install-form-row">
            <label className="install-form-label" htmlFor="custom-agent-root">配置根目录</label>
            <div className="custom-agent-path-input">
              <input
                className="install-form-input"
                id="custom-agent-root"
                placeholder="例如 ~/.codefuse/engine/cc/"
                value={configRoot}
                onChange={(e) => applyRoot(e.target.value)}
              />
              <button className="skills-btn custom-agent-path-input__browse" type="button" disabled={busy} onClick={chooseRoot}>
                选择目录
              </button>
            </div>
            <div className="custom-agent-hint">请选择实际存在的 Claude 配置根目录；其下通常包含 settings.json、projects 和 skills。</div>
          </div>
          <details className="custom-agent-advanced">
            <summary>高级路径设置</summary>
            <div className="custom-agent-advanced__fields">
              <div className="install-form-row">
                <label className="install-form-label" htmlFor="custom-agent-skills">Skills 目录</label>
                <input
                  className="install-form-input"
                  id="custom-agent-skills"
                  value={skillsDir}
                  onChange={(e) => setSkillsDir(e.target.value)}
                />
              </div>
              <div className="install-form-row">
                <label className="install-form-label" htmlFor="custom-agent-settings">Settings 文件</label>
                <input
                  className="install-form-input"
                  id="custom-agent-settings"
                  value={settingsFile}
                  onChange={(e) => setSettingsFile(e.target.value)}
                />
              </div>
              <div className="install-form-row">
                <label className="install-form-label" htmlFor="custom-agent-mcp">MCP 配置</label>
                <input
                  className="install-form-input"
                  id="custom-agent-mcp"
                  value={mcpConfig}
                  onChange={(e) => setMcpConfig(e.target.value)}
                />
              </div>
              <div className="install-form-row">
                <label className="install-form-label" htmlFor="custom-agent-plugin">Plugin 目录</label>
                <input
                  className="install-form-input"
                  id="custom-agent-plugin"
                  value={pluginDir}
                  onChange={(e) => setPluginDir(e.target.value)}
                />
              </div>
            </div>
          </details>
          {error && <div className="custom-agent-error">{error}</div>}
        </div>
        <div className="skills-dialog__footer">
          <button className="skills-btn" disabled={busy} onClick={onClose} type="button">取消</button>
          <ActionButton className="skills-btn skills-btn--primary" busy={busy} busyLabel="保存中" onClick={submit}>
            保存 Agent
          </ActionButton>
        </div>
      </div>
    </div>
  )
}

function deriveCustomAgentPaths(root: string) {
  const normalized = root.trim().replace(/\/+$/, '')
  if (!normalized) {
    return { skillsDir: '', settingsFile: '', mcpConfig: '', pluginDir: '' }
  }
  const settingsFile = `${normalized}/settings.json`
  return {
    skillsDir: `${normalized}/skills`,
    settingsFile,
    mcpConfig: settingsFile,
    pluginDir: `${normalized}/plugins/cache`,
  }
}

function formatAgentOutput(event: AgentOutputEvent) {
  const prefix = event.stream === 'stderr' ? '!' : event.stream === 'stdout' ? '>' : '*'
  return `${prefix} ${event.line}`
}

interface ActionButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  busy?: boolean
  busyLabel?: ReactNode
}

function ActionButton({
  busy = false,
  busyLabel,
  className = 'sm2__btn',
  children,
  disabled,
  type = 'button',
  ...props
}: ActionButtonProps) {
  return (
    <button
      {...props}
      type={type}
      className={className}
      disabled={disabled || busy}
      aria-busy={busy || undefined}
      data-busy={busy ? 'true' : undefined}
    >
      {busy && <span className="sm2__spinner" aria-hidden="true" />}
      <span className="sm2__btn-label">{busy ? busyLabel ?? children : children}</span>
    </button>
  )
}

function AgentToastStack({
  error,
  notice,
  onDismissError,
  onDismissNotice,
  local = false,
}: {
  error?: string | null
  notice?: string | null
  onDismissError?: () => void
  onDismissNotice?: () => void
  local?: boolean
}) {
  if (!error && !notice) return null
  return (
    <div className={`sm2__agent-toast-stack${local ? ' sm2__agent-toast-stack--local' : ''}`} aria-live="polite">
      {error && (
        <div className="sm2__agent-toast sm2__agent-toast--error" role="alert">
          <span>{error}</span>
          {onDismissError && (
            <button type="button" className="sm2__agent-toast-close" onClick={onDismissError} aria-label="关闭错误提示">
              ×
            </button>
          )}
        </div>
      )}
      {notice && (
        <div className="sm2__agent-toast sm2__agent-toast--ok" role="status">
          <span>{notice}</span>
          {onDismissNotice && (
            <button type="button" className="sm2__agent-toast-close" onClick={onDismissNotice} aria-label="关闭提示">
              ×
            </button>
          )}
        </div>
      )}
    </div>
  )
}

function AgentDetailView({
  detail,
  tab,
  onTab,
  busy,
  scanning,
  updating,
  installing,
  opening,
  adoptingUnmanagedId,
  program,
  agentInstalled,
  programLoading,
  agentOutput,
  onAdopt,
  onRevoke,
  onApplyPack,
  onScan,
  onUpdate,
  onInstall,
  onOpenDownload,
  onOpenSkillDetail,
  deleting,
  onRequestDelete,
}: {
  detail: AgentDetail
  tab: DetailTab
  onTab: (t: DetailTab) => void
  busy: boolean
  scanning: boolean
  updating: boolean
  installing: boolean
  opening: boolean
  adoptingUnmanagedId: string | null
  program: AgentProgramInfo | null
  agentInstalled: boolean
  programLoading: boolean
  agentOutput: string[]
  onAdopt: (agentId: string, unmanagedId: string) => void
  onRevoke: (packId: string, agentId: string) => void
  onApplyPack: (packId: string, agentId: string) => boolean | Promise<boolean>
  onScan: (agentId: string) => void
  onUpdate: (agentId: string) => void
  onInstall: (agentId: string) => void
  onOpenDownload: (agentId: string) => void
  onOpenSkillDetail: (skillId: string, fallback?: SkillDetailFallback | null) => void
  deleting: boolean
  onRequestDelete: (agent: AgentDetail) => void
}) {
  const { t } = useTranslation()
  const showUnmanaged = useSkillStoreV2((s) => s.settings?.showUnmanaged ?? true)
  const observedSkills = useSkillStoreV2((s) => s.unmanaged).filter((u) => u.agentId === detail.id)
  const unmanaged = observedSkills.filter((item) => showUnmanaged && !isReadOnlyUnmanaged(item))
  const readOnlySkills = observedSkills.filter(isReadOnlyUnmanaged)
  const inheritedSkills = detail.inheritedSkills ?? []
  const inheritsSharedSkills = detail.inheritsSharedSkills ?? inheritedSkills.length > 0
  const installed = program ? program.status === 'installed' || program.status === 'updateAvailable' : agentInstalled
  const canInstall = Boolean(program?.installCommand)
  const canOpenDownload = Boolean(program?.downloadUrl)
  const canDeleteCustom = program?.isCustom === true
  const installedVersion = program?.installedVersion ?? detail.version
  const latestVersion = program?.latestVersion ?? detail.latestVersion
  const hasUpdate = installed && Boolean(latestVersion && latestVersion !== installedVersion)
  const versionLabel = installed ? installedVersion || '未知' : '未安装'
  const updateLabel = !installed
    ? canInstall ? '可一键安装' : canOpenDownload ? '可打开安装页' : '未提供安装方式'
    : latestVersion
      ? hasUpdate ? `可更新到 ${latestVersion}` : '已是最新版本'
      : programLoading ? '正在获取最新版本' : '未检测到最新版本'
  const primaryAction = !installed
    ? canInstall
      ? { label: installing ? '正在安装' : '安装此 Agent', disabled: busy || installing, onClick: () => onInstall(detail.id), busy: installing }
      : { label: opening ? '正在打开' : '打开安装页', disabled: busy || !canOpenDownload, onClick: () => onOpenDownload(detail.id), busy: opening }
    : { label: updating ? '正在更新' : hasUpdate ? '更新此 Agent' : '无需更新', disabled: busy || scanning || updating || !hasUpdate, onClick: () => onUpdate(detail.id), busy: updating }
  const tabs: Array<{ id: DetailTab; label: string }> = [
    { id: 'overview', label: '概览' },
    { id: 'skills', label: `Skills (${detail.skills.length + unmanaged.length + inheritedSkills.length + readOnlySkills.length})` },
    { id: 'mcp', label: `MCP (${detail.mcpServers.length})` },
    { id: 'plugins', label: `Plugins (${detail.plugins.length})` },
    { id: 'hooks', label: 'Hooks' },
    { id: 'config', label: t('skills.agentManagement.pathSettings.tab') },
  ]

  return (
    <div className="sm2__agent-workspace">
      <div className="sm2__agent-hero">
        <AgentIconBadge iconKey={detail.iconKey} size={38} />
        <div className="sm2__agent-hero-main">
          <div className="sm2__agent-hero-title">{detail.displayName}</div>
          <div className="sm2__agent-version-row">
            <span className="sm2__agent-version-pill">当前版本 {versionLabel}</span>
            <span className={`sm2__agent-update-state${hasUpdate ? ' sm2__agent-update-state--available' : ''}`}>
              {updateLabel}
            </span>
          </div>
        </div>
        <div className="sm2__agent-summary-strip" aria-label="Agent 摘要">
          <Stat value={detail.skills.length} label="已管理" />
          {showUnmanaged && <Stat value={unmanaged.length} label="未管理" tone={unmanaged.length > 0 ? 'warn' : 'ok'} />}
          {inheritsSharedSkills && <Stat value={inheritedSkills.length} label={t('skills.agentManagement.inheritedSkills')} />}
          {readOnlySkills.length > 0 && <Stat value={readOnlySkills.length} label={t('skills.agentManagement.builtinSkills')} />}
          <Stat value={detail.appliedPacks.length} label="技能包" />
          <Stat value={detail.mcpServers.length + detail.plugins.length} label="MCP/插件" />
        </div>
        <div className="sm2__btn-row" style={{ margin: 0 }}>
          <ActionButton className="sm2__btn sm2__btn--primary" disabled={primaryAction.disabled} onClick={primaryAction.onClick} busy={primaryAction.busy} busyLabel={primaryAction.label}>
            {primaryAction.label}
          </ActionButton>
          <ActionButton className="sm2__btn" disabled={busy || scanning || updating || installing || !installed} onClick={() => onScan(detail.id)} busy={scanning} busyLabel="正在扫描">
            {scanning ? '正在扫描' : '重新扫描此 Agent'}
          </ActionButton>
          {canDeleteCustom && (
            <ActionButton className="sm2__btn sm2__btn--danger" disabled={busy && !deleting} onClick={() => onRequestDelete(detail)} busy={deleting} busyLabel="删除中">
              删除此 Agent
            </ActionButton>
          )}
        </div>
      </div>

      {agentOutput.length > 0 && (
        <div className="sm2__agent-output" aria-label="安装更新输出">
          {agentOutput.map((line, index) => <code key={`${index}-${line}`}>{line}</code>)}
        </div>
      )}

      {detail.health.length > 0 && (
        <div className="sm2__notice sm2__notice--warn">
          {detail.health.map((h) => h.message).join('；')}
        </div>
      )}

      <div className="sm2__subtabs">
        {tabs.map((t) => (
          <button
            key={t.id}
            className={`sm2__subtab${tab === t.id ? ' sm2__subtab--active' : ''}`}
            onClick={() => onTab(t.id)}
          >
            {t.label}
          </button>
        ))}
      </div>

      <div className="sm2__subtab-body">
        {tab === 'overview' && (
          <OverviewTab
            detail={detail}
            busy={busy}
            onOpenSection={onTab}
            onRevoke={onRevoke}
            onApplyPack={onApplyPack}
          />
        )}
        {tab === 'skills' && (
          <SkillsTab
            detail={detail}
            unmanaged={unmanaged}
            inheritedSkills={inheritedSkills}
            readOnlySkills={readOnlySkills}
            showUnmanaged={showUnmanaged}
            busy={busy}
            scanning={scanning}
            adoptingUnmanagedId={adoptingUnmanagedId}
            onAdopt={onAdopt}
            onRevoke={onRevoke}
            onApplyPack={onApplyPack}
            onScan={onScan}
            onOpenSkillDetail={onOpenSkillDetail}
          />
        )}
        {tab === 'mcp' && <McpTab detail={detail} />}
        {tab === 'plugins' && <PluginManagementTab detail={detail} />}
        {tab === 'hooks' && <HooksTab detail={detail} program={program} />}
        {tab === 'config' && <ConfigTab detail={detail} program={program} />}
      </div>
    </div>
  )
}

function Stat({ value, label, tone }: { value: number; label: string; tone?: 'ok' | 'warn' }) {
  return (
    <div className={`sm2__stat${tone ? ` sm2__stat--${tone}` : ''}`}>
      <strong>{value}</strong>
      <span>{label}</span>
    </div>
  )
}

function OverviewTab({
  detail,
  busy,
  onOpenSection,
  onRevoke,
  onApplyPack,
}: {
  detail: AgentDetail
  busy: boolean
  onOpenSection: (tab: DetailTab) => void
  onRevoke: (packId: string, agentId: string) => void
  onApplyPack: (packId: string, agentId: string) => boolean | Promise<boolean>
}) {
  const showUnmanaged = useSkillStoreV2((s) => s.settings?.showUnmanaged ?? true)
  const observedSkills = useSkillStoreV2((s) => s.unmanaged)
    .filter((item) => item.agentId === detail.id)
  const unmanagedCount = observedSkills
    .filter((item) => showUnmanaged && !isReadOnlyUnmanaged(item)).length
  const readOnlyCount = observedSkills.filter(isReadOnlyUnmanaged).length
  const inheritedCount = detail.inheritedSkills?.length ?? 0
  const appliedIds = new Set(detail.appliedPacks.map((p) => p.packId))
  const available = detail.availablePacks.filter((p) => !appliedIds.has(p.id))
  const validMcpCount = detail.mcpServers.filter((server) => server.valid).length
  const enabledPluginCount = detail.plugins.filter((plugin) => plugin.enabled).length
  const invalidMcpCount = detail.mcpServers.length - validMcpCount
  const configuredPaths = [detail.skillsDir, detail.configPath, detail.mcpConfigPath, detail.pluginDir]
    .filter(Boolean).length
  const healthErrors = detail.health.filter((issue) => ['error', 'critical'].includes(issue.severity.toLowerCase())).length
  const attentionCount = detail.health.length + unmanagedCount + invalidMcpCount
  const statusTone = healthErrors > 0 ? 'danger' : attentionCount > 0 ? 'attention' : 'ready'
  const statusTitle = healthErrors > 0
    ? `${healthErrors} 项配置异常`
    : attentionCount > 0
      ? `${attentionCount} 项需要关注`
      : '运行状态良好'
  const statusDescription = healthErrors > 0
    ? '关键配置存在异常，建议先修复后再同步能力。'
    : attentionCount > 0
      ? 'Agent 可以继续使用，完成下方事项后会更稳定。'
      : 'Skills、扩展与配置均未发现待处理问题。'
  const nextActions: Array<{ label: string; meta: string; tab: DetailTab; tone?: 'warn' }> = []

  if (unmanagedCount > 0) {
    nextActions.push({ label: `接管 ${unmanagedCount} 个未管理 Skill`, meta: '统一纳入中心库管理', tab: 'skills', tone: 'warn' })
  }
  if (invalidMcpCount > 0) {
    nextActions.push({ label: `修复 ${invalidMcpCount} 个 MCP 配置`, meta: '存在缺失或无效命令', tab: 'mcp', tone: 'warn' })
  }
  if (detail.health.length > 0) {
    nextActions.push({ label: `查看 ${detail.health.length} 项健康提示`, meta: '检查路径与配置详情', tab: 'config', tone: 'warn' })
  }
  if (available.length > 0) {
    nextActions.push({ label: `${available.length} 个技能包可应用`, meta: '按场景继续扩展能力', tab: 'skills' })
  }

  const capabilities: Array<{
    id: string
    label: string
    value: number
    unit: string
    detail: string
    tab: DetailTab
    tone: 'blue' | 'green' | 'amber' | 'violet'
    progress: number
  }> = [
    {
      id: 'skills',
      label: 'Skills',
      value: detail.skills.length + inheritedCount + readOnlyCount,
      unit: '可用',
      detail: unmanagedCount > 0
        ? `${unmanagedCount} 个待接管`
        : inheritedCount > 0
          ? `${inheritedCount} 个来自 .agents 共享继承`
          : readOnlyCount > 0 ? `${readOnlyCount} 个 Agent 内置只读` : '全部已纳入管理',
      tab: 'skills',
      tone: unmanagedCount > 0 ? 'amber' : 'blue',
      progress: detail.skills.length + unmanagedCount === 0 ? 100 : detail.skills.length / (detail.skills.length + unmanagedCount) * 100,
    },
    {
      id: 'packs',
      label: '技能包',
      value: detail.appliedPacks.length,
      unit: '已应用',
      detail: available.length > 0
        ? `${available.length} 个可继续应用`
        : detail.appliedPacks.length > 0 ? '当前技能包已全部应用' : '尚未应用技能包',
      tab: 'skills',
      tone: 'violet',
      progress: detail.appliedPacks.length + available.length === 0 ? 0 : detail.appliedPacks.length / (detail.appliedPacks.length + available.length) * 100,
    },
    {
      id: 'mcp',
      label: 'MCP 服务',
      value: validMcpCount,
      unit: detail.mcpServers.length > 0 ? `/ ${detail.mcpServers.length} 可用` : '未配置',
      detail: detail.mcpServers.length === 0 ? '尚未连接服务' : invalidMcpCount > 0 ? `${invalidMcpCount} 个配置异常` : '服务连接正常',
      tab: 'mcp',
      tone: invalidMcpCount > 0 ? 'amber' : detail.mcpServers.length > 0 ? 'green' : 'blue',
      progress: detail.mcpServers.length === 0 ? 0 : validMcpCount / detail.mcpServers.length * 100,
    },
    {
      id: 'plugins',
      label: 'Plugins',
      value: enabledPluginCount,
      unit: detail.plugins.length > 0 ? `/ ${detail.plugins.length} 启用` : '未安装',
      detail: detail.plugins.length === 0 ? '暂无插件扩展' : enabledPluginCount === detail.plugins.length ? '插件均已启用' : `${detail.plugins.length - enabledPluginCount} 个未启用`,
      tab: 'plugins',
      tone: 'green',
      progress: detail.plugins.length === 0 ? 0 : enabledPluginCount / detail.plugins.length * 100,
    },
  ]

  return (
    <div className="sm2__agent-overview">
      <section className={`sm2__agent-overview-status sm2__agent-overview-status--${statusTone}`}>
        <div className="sm2__agent-overview-status-copy">
          <div className="sm2__agent-overview-kicker">
            <span className="sm2__agent-overview-pulse" />
            Agent 状态
          </div>
          <h3>{statusTitle}</h3>
          <p>{statusDescription}</p>
        </div>
        <div className="sm2__agent-overview-readiness" aria-label={`配置完整度 ${configuredPaths} / 4`}>
          <div>
            <span>配置完整度</span>
            <strong>{configuredPaths}<small>/4</small></strong>
          </div>
          <div className="sm2__agent-overview-readiness-track" aria-hidden="true">
            <span style={{ width: `${configuredPaths / 4 * 100}%` }} />
          </div>
          <button type="button" onClick={() => onOpenSection('config')}>查看配置 <span aria-hidden="true">→</span></button>
        </div>
      </section>

      <section className="sm2__agent-overview-section" aria-labelledby="agent-capability-title">
        <div className="sm2__agent-overview-section-head">
          <div>
            <h3 id="agent-capability-title">能力快照</h3>
            <p>点击卡片查看和管理对应能力</p>
          </div>
          <span>{detail.skills.length + readOnlyCount + detail.mcpServers.length + detail.plugins.length} 项能力已连接</span>
        </div>
        <div className="sm2__agent-capability-grid">
          {capabilities.map((capability) => (
            <button
              key={capability.id}
              type="button"
              className={`sm2__agent-capability-card sm2__agent-capability-card--${capability.tone}`}
              onClick={() => onOpenSection(capability.tab)}
            >
              <span className="sm2__agent-capability-label">
                <i aria-hidden="true">{capability.label.slice(0, 1)}</i>
                {capability.label}
                <b aria-hidden="true">↗</b>
              </span>
              <span className="sm2__agent-capability-value">
                <strong>{capability.value}</strong>
                <small>{capability.unit}</small>
              </span>
              <span className="sm2__agent-capability-detail">{capability.detail}</span>
              <span className="sm2__agent-capability-track" aria-hidden="true">
                <span style={{ width: `${Math.max(0, Math.min(100, capability.progress))}%` }} />
              </span>
            </button>
          ))}
        </div>
      </section>

      <div className="sm2__agent-overview-lower">
        <section className="sm2__agent-overview-panel">
          <div className="sm2__agent-overview-panel-head">
            <div>
              <h3>技能包</h3>
              <p>快速调整当前 Agent 生效的技能组合</p>
            </div>
            <button type="button" onClick={() => onOpenSection('skills')}>管理全部</button>
          </div>
          <div className="sm2__agent-overview-pack-groups">
            <div className="sm2__agent-overview-pack-group">
              <div className="sm2__agent-overview-pack-group-head">
                <span>已生效</span>
                <b>{detail.appliedPacks.length}</b>
              </div>
              {detail.appliedPacks.length > 0 ? (
                <div className="sm2__agent-overview-pack-list">
                  {detail.appliedPacks.map((pack) => (
                    <div key={pack.packId} className="sm2__agent-overview-pack-row sm2__agent-overview-pack-row--active">
                      <span aria-hidden="true">✓</span>
                      <div>
                        <strong>{pack.packName}</strong>
                        <small>{pack.memberCount} 个 Skill</small>
                      </div>
                      <button
                        type="button"
                        aria-label={`取消应用 ${pack.packName}`}
                        disabled={busy}
                        onClick={() => onRevoke(pack.packId, detail.id)}
                      >
                        取消
                      </button>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="sm2__agent-overview-pack-empty">暂未应用技能包</div>
              )}
            </div>

            <div className="sm2__agent-overview-pack-group">
              <div className="sm2__agent-overview-pack-group-head">
                <span>可应用</span>
                <b>{available.length}</b>
              </div>
              {available.length > 0 ? (
                <div className="sm2__agent-overview-pack-list">
                  {available.map((pack) => (
                    <div key={pack.id} className="sm2__agent-overview-pack-row">
                      <span aria-hidden="true">＋</span>
                      <div>
                        <strong>{pack.name}</strong>
                        <small>{pack.memberCount} 个 Skill · {pack.appliedAgentCount} 个 Agent 已用</small>
                      </div>
                      <button
                        type="button"
                        aria-label={`应用 ${pack.name}`}
                        disabled={busy || pack.memberCount === 0}
                        onClick={() => onApplyPack(pack.id, detail.id)}
                      >
                        应用
                      </button>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="sm2__agent-overview-pack-empty">没有更多可应用技能包</div>
              )}
            </div>
          </div>
        </section>

        <section className="sm2__agent-overview-panel sm2__agent-overview-panel--next">
          <div className="sm2__agent-overview-panel-head">
            <div>
              <h3>下一步</h3>
              <p>{nextActions.length > 0 ? '根据当前状态整理的建议' : '当前没有必须处理的事项'}</p>
            </div>
            {nextActions.length > 0 && <span>{nextActions.length}</span>}
          </div>
          {nextActions.length > 0 ? (
            <div className="sm2__agent-overview-actions">
              {nextActions.slice(0, 4).map((action) => (
                <button key={`${action.tab}-${action.label}`} type="button" onClick={() => onOpenSection(action.tab)}>
                  <i className={action.tone === 'warn' ? 'is-warn' : ''} aria-hidden="true" />
                  <div>
                    <strong>{action.label}</strong>
                    <small>{action.meta}</small>
                  </div>
                  <b aria-hidden="true">→</b>
                </button>
              ))}
            </div>
          ) : (
            <div className="sm2__agent-overview-all-clear">
              <span aria-hidden="true">✓</span>
              <div>
                <strong>已准备就绪</strong>
                <small>可以开始在任务中使用这个 Agent。</small>
              </div>
            </div>
          )}
        </section>
      </div>
    </div>
  )
}

function SkillsTab({
  detail,
  unmanaged,
  inheritedSkills,
  readOnlySkills,
  showUnmanaged,
  busy,
  scanning,
  adoptingUnmanagedId,
  onAdopt,
  onRevoke,
  onApplyPack,
  onScan,
  onOpenSkillDetail,
}: {
  detail: AgentDetail
  unmanaged: UnmanagedItemDto[]
  inheritedSkills: InheritedSkillDetail[]
  readOnlySkills: UnmanagedItemDto[]
  showUnmanaged: boolean
  busy: boolean
  scanning: boolean
  adoptingUnmanagedId: string | null
  onAdopt: (agentId: string, unmanagedId: string) => void
  onRevoke: (packId: string, agentId: string) => void
  onApplyPack: (packId: string, agentId: string) => boolean | Promise<boolean>
  onScan: (agentId: string) => void
  onOpenSkillDetail: (skillId: string, fallback?: SkillDetailFallback | null) => void
}) {
  const { t } = useTranslation()
  const state = useSkillStoreV2()
  const [query, setQuery] = useState('')
  const [page, setPage] = useState(1)
  const [viewMode, setViewMode] = useState<AgentSkillViewMode>('cards')
  const [scope, setScope] = useState<AgentSkillScope>('managed')
  const [packFilter, setPackFilter] = useState<AgentSkillPackFilter>('all')
  const [selectedManagedIds, setSelectedManagedIds] = useState<Set<string>>(() => new Set())
  const [selectedUnmanagedIds, setSelectedUnmanagedIds] = useState<Set<string>>(() => new Set())
  const [managedSelectionMode, setManagedSelectionMode] = useState(false)
  const [unmanagedSelectionMode, setUnmanagedSelectionMode] = useState(false)
  const [installDialogOpen, setInstallDialogOpen] = useState(false)
  const [deletingIds, setDeletingIds] = useState<Set<string>>(() => new Set())
  const [deletingUnmanagedIds, setDeletingUnmanagedIds] = useState<Set<string>>(() => new Set())
  const [adoptingIds, setAdoptingIds] = useState<Set<string>>(() => new Set())
  const [batchDeleteTargets, setBatchDeleteTargets] = useState<AgentDetail['skills'] | null>(null)
  const [batchDeleteUnmanagedTargets, setBatchDeleteUnmanagedTargets] = useState<UnmanagedItemDto[] | null>(null)
  const [batchAdoptItems, setBatchAdoptItems] = useState<UnmanagedItemDto[] | null>(null)
  const [quickTakeoverItems, setQuickTakeoverItems] = useState<UnmanagedItemDto[] | null>(null)
  const [moveToPackTarget, setMoveToPackTarget] = useState<AgentDetail['skills'][number] | null>(null)
  const [deleteUnmanagedTarget, setDeleteUnmanagedTarget] = useState<UnmanagedItemDto | null>(null)
  const [deleteUnmanagedError, setDeleteUnmanagedError] = useState<string | null>(null)
  const [confirmingPackApply, setConfirmingPackApply] = useState(false)
  const [packApplyProgress, setPackApplyProgress] = useState<PackApplyProgress | null>(null)
  const [localNotice, setLocalNotice] = useState<string | null>(null)
  const inheritsSharedSkills = detail.inheritsSharedSkills ?? inheritedSkills.length > 0
  const q = query.trim().toLowerCase()
  const searchedManaged = useMemo(() => {
    if (!q) return detail.skills
    return detail.skills.filter((s) =>
      [
        s.targetPath,
        s.status,
        s.actualMode,
        ...s.claims.map((claim) => claim.packName || claim.claimType),
      ].filter(Boolean).join(' ').toLowerCase().includes(q),
    )
  }, [q, detail.skills])
  const packFilterCounts = useMemo(() => {
    const pack = searchedManaged.filter(hasSkillPackClaim).length
    return { all: searchedManaged.length, pack, standalone: searchedManaged.length - pack }
  }, [searchedManaged])
  const filteredManaged = useMemo(() => {
    if (packFilter === 'all') return searchedManaged
    return searchedManaged.filter((skill) => packFilter === 'pack' ? hasSkillPackClaim(skill) : !hasSkillPackClaim(skill))
  }, [packFilter, searchedManaged])
  const filteredUnmanaged = useMemo(() => {
    if (!q) return unmanaged
    return unmanaged.filter((u) =>
      [u.inferredSkillId, u.path, u.reason, unmanagedReasonLabel(t, u.reason), unmanagedSourceLabel(t, u)]
        .filter(Boolean)
        .join(' ')
        .toLowerCase()
        .includes(q),
    )
  }, [q, t, unmanaged])
  const filteredInherited = useMemo(() => {
    if (!q) return inheritedSkills
    return inheritedSkills.filter((item) =>
      [item.skillId, item.path, item.resolvedPath]
        .filter(Boolean)
        .join(' ')
        .toLowerCase()
        .includes(q),
    )
  }, [inheritedSkills, q])
  const filteredReadOnly = useMemo(() => {
    if (!q) return readOnlySkills
    return readOnlySkills.filter((item) =>
      [item.inferredSkillId, item.path, item.reason, unmanagedReasonLabel(t, item.reason)]
        .filter(Boolean)
        .join(' ')
        .toLowerCase()
        .includes(q),
    )
  }, [q, readOnlySkills, t])
  const [pageResetKey, setPageResetKey] = useState(`${q}|${detail.id}|${scope}`)
  const currentResetKey = `${q}|${detail.id}|${scope}`
  if (pageResetKey !== currentResetKey) {
    setPageResetKey(currentResetKey)
    if (page !== 1) setPage(1)
  }
  const shownUnmanaged = filteredUnmanaged.slice(0, page * PAGE_SIZE)
  const shownInherited = filteredInherited.slice(0, page * PAGE_SIZE)
  const shownReadOnly = filteredReadOnly.slice(0, page * PAGE_SIZE)
  const canManageUnmanaged = filteredUnmanaged
  const quickTakeoverCandidates = useMemo(
    () => unmanaged.filter((item) => item.reason === 'same_name_as_center_skill'),
    [unmanaged],
  )
  const sharedUnmanagedCount = filteredUnmanaged.filter(isSharedAgentsUnmanaged).length
  const managedDeleting = deletingIds.size > 0
  const unmanagedDeleting = deletingUnmanagedIds.size > 0
  const unmanagedAdopting = adoptingIds.size > 0
  const actionBusy = busy || managedDeleting || unmanagedDeleting || unmanagedAdopting
  const moveToPackOptions = useMemo(
    () => packOptionsForMove(detail, state.packs),
    [detail, state.packs],
  )

  useEffect(() => {
    setManagedSelectionMode(false)
    setUnmanagedSelectionMode(false)
    setSelectedManagedIds(new Set())
    setSelectedUnmanagedIds(new Set())
    setBatchDeleteTargets(null)
    setBatchDeleteUnmanagedTargets(null)
    setQuickTakeoverItems(null)
    setMoveToPackTarget(null)
    setDeleteUnmanagedTarget(null)
    setDeleteUnmanagedError(null)
  }, [detail.id, packFilter, scope])

  useEffect(() => {
    if (scope === 'inherited' && !inheritsSharedSkills) setScope('managed')
  }, [detail.id, inheritsSharedSkills, scope])

  useEffect(() => {
    if (!localNotice) return
    const timer = window.setTimeout(() => setLocalNotice(null), NOTICE_DISMISS_MS)
    return () => window.clearTimeout(timer)
  }, [localNotice])

  useEffect(() => {
    if (packApplyProgress?.state !== 'done') return
    const timer = window.setTimeout(() => setPackApplyProgress(null), PACK_PROGRESS_DONE_DISMISS_MS)
    return () => window.clearTimeout(timer)
  }, [packApplyProgress])

  const refreshAgentSkills = async () => {
    const nextUnmanaged = await skillApiV2.listUnmanaged()
    useSkillStoreV2.setState({ unmanaged: nextUnmanaged })
    await state.loadAgentDetail(detail.id, true)
    await state.loadOverview(true)
  }

  const syncAdoptedSkillsToPack = async (selection: BatchAdoptPackSelection, skillIds: string[]) => {
    const uniqueSkillIds = uniqueValues(skillIds)
    if (selection.kind === 'none' || uniqueSkillIds.length === 0) return null
    if (selection.kind === 'new') {
      const saved = await skillApiV2.upsertPack({
        id: '',
        name: selection.name.trim(),
        description: '',
        tags: [],
        skillIds: uniqueSkillIds,
      })
      return { packName: saved.name, added: saved.members.length }
    }

    const existing = await skillApiV2.getPackDetail(selection.packId)
    const existingIds = existing.members.map((member) => member.skillId)
    const merged = uniqueValues([...existingIds, ...uniqueSkillIds])
    if (merged.length === existingIds.length) {
      return { packName: existing.name, added: 0 }
    }
    const saved = await skillApiV2.upsertPack({
      id: existing.id,
      name: existing.name,
      description: existing.description,
      tags: existing.tags,
      skillIds: merged,
    })
    return { packName: saved.name, added: merged.length - existingIds.length }
  }

  const deleteManaged = async (targets: AgentDetail['skills']) => {
    if (targets.length === 0) return
    const ids = targets.map((target) => target.id)
    setDeletingIds(new Set(ids))
    setLocalNotice(`正在删除 ${targets.length} 个 Skill 分发...`)
    state.setError(null)
    try {
      const targetNames = new Map(targets.map((target) => [target.id, pathBasename(target.targetPath) || target.skillId]))
      const result = await skillApiV2.deleteSkillTargetDistributions(ids)
      const failed = result.failures.map((failure) => `${targetNames.get(failure.targetId) || failure.targetId}: ${failure.error}`)
      await refreshAgentSkills()
      setSelectedManagedIds(new Set())
      setManagedSelectionMode(false)
      setLocalNotice(`已删除 ${result.deleted} 个 Skill 分发${failed.length ? `，${failed.length} 个失败` : ''}`)
      if (failed.length === 0) state.setError(null)
      if (failed.length > 0) state.setError(failed.slice(0, 3).join('\n'))
    } catch (e) {
      state.setError(String(e))
    } finally {
      setDeletingIds(new Set())
    }
  }

  const adoptUnmanaged = async (items: UnmanagedItemDto[], packSelection: BatchAdoptPackSelection = { kind: 'none' }) => {
    if (items.length === 0) return
    const ids = items.map((item) => item.id)
    setAdoptingIds(new Set(ids))
    setLocalNotice(null)
    state.setError(null)
    try {
      let ok = 0
      let packResult: { packName: string; added: number } | null = null
      const adoptedSkillIds: string[] = []
      const requiresIndividualReview: string[] = []
      const failed: string[] = []
      const itemsById = new Map(items.map((item) => [item.id, item]))
      const result = await skillApiV2.executeAdoptBatch(items.map((item) => ({
        agentId: adoptOwnerAgentId(detail.id, item),
        unmanagedId: item.id,
        option: defaultAgentDetailAdoptMode(item),
        renamedId: null,
      })))
      for (const itemResult of result.items) {
        const item = itemsById.get(itemResult.unmanagedId)
        const name = item?.inferredSkillId || (item ? pathBasename(item.path) : '') || itemResult.unmanagedId
        if (itemResult.skillId !== null) {
          adoptedSkillIds.push(itemResult.skillId)
          ok += 1
        } else if (isAdoptOptionUnavailableError(itemResult.error)) {
          requiresIndividualReview.push(name)
        } else {
          failed.push(`${name}: ${skillErrorMessage(t, itemResult.error)}`)
        }
      }
      if (result.finalizationError) {
        failed.push(`批量刷新失败: ${skillErrorMessage(t, result.finalizationError)}`)
      }
      try {
        packResult = await syncAdoptedSkillsToPack(packSelection, adoptedSkillIds)
      } catch (e) {
        failed.push(`技能包同步失败: ${skillErrorMessage(t, e)}`)
      }
      await refreshAgentSkills()
      setSelectedUnmanagedIds(new Set())
      setUnmanagedSelectionMode(false)
      const packNotice = packResult
        ? t('skills.batchAdoptPack.packSynced', { count: packResult.added, pack: packResult.packName })
        : ''
      setLocalNotice(t('skills.batchAdoptPack.complete', {
        adopted: ok,
        review: requiresIndividualReview.length,
        failed: failed.length,
        packNotice,
      }))
      const messages: string[] = []
      if (requiresIndividualReview.length > 0) {
        messages.push(t('skills.batchAdoptPack.individualReviewRequired', {
          count: requiresIndividualReview.length,
          names: requiresIndividualReview.join(t('skills.batchAdoptPack.nameSeparator')),
        }))
      }
      if (failed.length > 0) messages.push(failed.slice(0, 3).join('\n'))
      state.setError(messages.length > 0 ? messages.join('\n') : null)
    } catch (e) {
      state.setError(skillErrorMessage(t, e))
    } finally {
      setAdoptingIds(new Set())
    }
  }

  const takeoverExistingCenterSkills = async (items: UnmanagedItemDto[]) => {
    if (items.length === 0) return
    const ids = items.map((item) => item.id)
    const names = new Map(items.map((item) => [
      item.id,
      item.inferredSkillId || pathBasename(item.path) || item.id,
    ]))
    setAdoptingIds(new Set(ids))
    setLocalNotice(t('skills.agentManagement.quickTakeover.running', { count: items.length }))
    state.setError(null)
    try {
      const result = await skillApiV2.takeoverCenterSkills(detail.id, ids)
      const adopted = result.items.filter((item) => item.skillId !== null).length
      const failures = result.items
        .filter((item) => item.skillId === null)
        .map((item) => `${names.get(item.unmanagedId) || item.unmanagedId}: ${skillErrorMessage(t, item.error)}`)
      if (result.finalizationError) {
        failures.push(t('skills.agentManagement.quickTakeover.refreshFailed', {
          error: skillErrorMessage(t, result.finalizationError),
        }))
      }
      try {
        await refreshAgentSkills()
      } catch (error) {
        failures.push(t('skills.agentManagement.quickTakeover.refreshFailed', {
          error: skillErrorMessage(t, error),
        }))
      }
      setSelectedUnmanagedIds(new Set())
      setLocalNotice(t('skills.agentManagement.quickTakeover.complete', {
        adopted,
        failed: failures.length,
      }))
      state.setError(failures.length > 0 ? failures.slice(0, 3).join('\n') : null)
    } catch (error) {
      state.setError(skillErrorMessage(t, error))
      setLocalNotice(null)
    } finally {
      setAdoptingIds(new Set())
    }
  }

  const deleteUnmanaged = async (items: UnmanagedItemDto[]) => {
    if (items.length === 0) return false
    const itemNames = new Map(items.map((item) => [item.id, item.inferredSkillId || pathBasename(item.path) || item.id]))
    const itemIds = new Set(items.map((item) => item.id))
    setDeletingUnmanagedIds(itemIds)
    setDeleteUnmanagedError(null)
    setLocalNotice(items.length === 1
      ? t('skills.agentManagement.unmanagedDelete.deleting', { name: itemNames.get(items[0].id) })
      : `正在删除 ${items.length} 个未管理 Skill...`)
    state.setError(null)

    if (items.length === 1) {
      const item = items[0]
      try {
        await skillApiV2.deleteUnmanagedAgentSkill(adoptOwnerAgentId(detail.id, item), item.id)
        await refreshAgentSkills()
        setSelectedUnmanagedIds((current) => {
          const next = new Set(current)
          next.delete(item.id)
          return next
        })
        setLocalNotice(t('skills.agentManagement.unmanagedDelete.deleted', { name: itemNames.get(item.id) }))
        state.setError(null)
        return true
      } catch (e) {
        setDeleteUnmanagedError(skillErrorMessage(t, e))
        return false
      } finally {
        setDeletingUnmanagedIds(new Set())
      }
    }

    const deletedIds = new Set<string>()
    const failed: string[] = []
    const itemsByOwner = new Map<string, UnmanagedItemDto[]>()
    items.forEach((item) => {
      const owner = adoptOwnerAgentId(detail.id, item)
      itemsByOwner.set(owner, [...(itemsByOwner.get(owner) || []), item])
    })
    for (const [owner, ownerItems] of itemsByOwner) {
      try {
        const result = await skillApiV2.deleteUnmanagedAgentSkills(owner, ownerItems.map((item) => item.id))
        const failedIds = new Set(result.failures.map((failure) => failure.unmanagedId))
        ownerItems.forEach((item) => {
          if (!failedIds.has(item.id)) deletedIds.add(item.id)
        })
        failed.push(...result.failures.map((failure) => `${itemNames.get(failure.unmanagedId) || failure.unmanagedId}: ${failure.error}`))
      } catch (e) {
        failed.push(`${owner}: ${skillErrorMessage(t, e)}`)
      }
    }
    try {
      await refreshAgentSkills()
    } catch (e) {
      failed.push(`刷新列表: ${skillErrorMessage(t, e)}`)
    }
    setSelectedUnmanagedIds((current) => {
      const next = new Set(current)
      deletedIds.forEach((id) => next.delete(id))
      return next
    })
    if (failed.length === 0) setUnmanagedSelectionMode(false)
    setLocalNotice(`已删除 ${deletedIds.size} 个未管理 Skill${failed.length ? `，${failed.length} 个失败` : ''}`)
    if (failed.length === 0) state.setError(null)
    if (failed.length > 0) state.setError(failed.slice(0, 3).join('\n'))
    setDeletingUnmanagedIds(new Set())
    return deletedIds.size === items.length
  }

  const toggleManaged = (targetId: string) => {
    setSelectedManagedIds((current) => toggleSetValue(current, targetId))
  }

  const toggleUnmanaged = (unmanagedId: string) => {
    setSelectedUnmanagedIds((current) => toggleSetValue(current, unmanagedId))
  }

  const selectedManaged = filteredManaged.filter((item) => selectedManagedIds.has(item.id))
  const selectedUnmanaged = filteredUnmanaged.filter((item) => selectedUnmanagedIds.has(item.id))
  const confirmBatchDelete = async () => {
    if (!batchDeleteTargets || batchDeleteTargets.length === 0) return
    await deleteManaged(batchDeleteTargets)
    setBatchDeleteTargets(null)
  }
  const applyPackFromRail = async (packId: string, agentId: string) => {
    const pack = detail.availablePacks.find((item) => item.id === packId)
    if (!pack) return false
    setConfirmingPackApply(true)
    setPackApplyProgress({
      packName: pack.name,
      agentName: detail.displayName,
      state: 'running',
      percent: 18,
      detail: pack.id === 'default' ? '准备应用中心库全部 Skills' : `准备应用 ${pack.memberCount} 个 Skill`,
    })
    state.setError(null)
    try {
      const applied = await Promise.resolve(onApplyPack(pack.id, agentId))
      if (!applied) {
        setPackApplyProgress(null)
        return false
      }
      setPackApplyProgress({
        packName: pack.name,
        agentName: detail.displayName,
        state: 'done',
        percent: 100,
        detail: '已完成',
      })
      return true
    } finally {
      setConfirmingPackApply(false)
    }
  }
  const finishInstallFromLibrary = async () => {
    await refreshAgentSkills()
    setInstallDialogOpen(false)
    setScope('managed')
    setLocalNotice('Skill 已安装到当前 Agent')
  }

  return (
    <div className="sm2__skills-tab">
      <div className="sm2__view-toggle sm2__agent-skill-scope-tabs" aria-label="Skill 管理状态">
        <button
          className={scope === 'managed' ? 'active' : ''}
          aria-selected={scope === 'managed'}
          onClick={() => setScope('managed')}
        >
          已管理 {filteredManaged.length}
        </button>
        {showUnmanaged && (
          <button
            className={scope === 'unmanaged' ? 'active' : ''}
            aria-selected={scope === 'unmanaged'}
            onClick={() => setScope('unmanaged')}
          >
            未管理 {filteredUnmanaged.length}
          </button>
        )}
        {inheritsSharedSkills && (
          <button
            className={scope === 'inherited' ? 'active' : ''}
            aria-selected={scope === 'inherited'}
            onClick={() => setScope('inherited')}
          >
            {t('skills.agentManagement.inheritedSkills')} {filteredInherited.length}
          </button>
        )}
        {readOnlySkills.length > 0 && (
          <button
            className={scope === 'builtin' ? 'active' : ''}
            aria-selected={scope === 'builtin'}
            onClick={() => setScope('builtin')}
          >
            {t('skills.agentManagement.builtinSkills')} {filteredReadOnly.length}
          </button>
        )}
      </div>
      {scope === 'managed' && (
        <AgentPackToggleRail
          detail={detail}
          busy={actionBusy || confirmingPackApply}
          onApplyPack={applyPackFromRail}
          onRevoke={onRevoke}
        />
      )}
      <div className="sm2__toolbar sm2__toolbar--inset sm2__toolbar--split">
        <input className="sm2__search" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="搜索 Skill 名称 / 路径 / 来源 / 原因" />
        <div className="sm2__agent-skill-actions">
          {scope === 'managed' && (
            <label className="sm2__agent-skill-pack-filter">
              <span>{t('skills.agentManagement.packFilter.label')}</span>
              <select
                className="sm2__select"
                value={packFilter}
                onChange={(event) => setPackFilter(event.target.value as AgentSkillPackFilter)}
              >
                <option value="all">{t('skills.agentManagement.packFilter.all')} ({packFilterCounts.all})</option>
                <option value="pack">{t('skills.agentManagement.packFilter.inPack')} ({packFilterCounts.pack})</option>
                <option value="standalone">{t('skills.agentManagement.packFilter.standalone')} ({packFilterCounts.standalone})</option>
              </select>
            </label>
          )}
          {scope === 'managed' ? (
            <>
              <button className="sm2__btn sm2__btn--primary" disabled={actionBusy || state.skills.length === 0} onClick={() => setInstallDialogOpen(true)}>
                新增SKILL
              </button>
              {managedSelectionMode ? (
              <>
                <span>已选择 {selectedManagedIds.size} 个</span>
                <button className="sm2__btn" disabled={filteredManaged.length === 0 || actionBusy} onClick={() => setSelectedManagedIds(new Set(filteredManaged.map((item) => item.id)))}>
                  选择当前
                </button>
                <button className="sm2__btn" disabled={selectedManagedIds.size === 0 || actionBusy} onClick={() => setSelectedManagedIds(new Set())}>
                  清空
                </button>
                <ActionButton className="sm2__btn sm2__btn--danger" disabled={selectedManaged.length === 0 || actionBusy} busy={managedDeleting} busyLabel="删除中" onClick={() => setBatchDeleteTargets(selectedManaged)}>
                  批量删除 {selectedManaged.length} 个
                </ActionButton>
                <button className="sm2__btn sm2__btn--ghost" disabled={actionBusy} onClick={() => {
                  setSelectedManagedIds(new Set())
                  setManagedSelectionMode(false)
                }}>
                  取消多选
                </button>
              </>
              ) : (
                <button className="sm2__btn" disabled={filteredManaged.length === 0 || actionBusy} onClick={() => setManagedSelectionMode(true)}>
                  批量选择
                </button>
              )}
            </>
          ) : scope === 'unmanaged' ? (
            unmanagedSelectionMode ? (
              <>
                <span>已选择 {selectedUnmanagedIds.size} 个</span>
                <button className="sm2__btn" disabled={canManageUnmanaged.length === 0 || actionBusy} onClick={() => setSelectedUnmanagedIds(new Set(canManageUnmanaged.map((item) => item.id)))}>
                  选择当前可接管
                </button>
                <button className="sm2__btn" disabled={selectedUnmanagedIds.size === 0 || actionBusy} onClick={() => setSelectedUnmanagedIds(new Set())}>
                  清空
                </button>
                <ActionButton className="sm2__btn sm2__btn--primary" disabled={selectedUnmanaged.length === 0 || actionBusy} busy={unmanagedAdopting} busyLabel="接管中" onClick={() => setBatchAdoptItems(selectedUnmanaged)}>
                  接管到中心库
                </ActionButton>
                <ActionButton className="sm2__btn sm2__btn--danger" disabled={selectedUnmanaged.length === 0 || actionBusy} busy={unmanagedDeleting} busyLabel="删除中" onClick={() => setBatchDeleteUnmanagedTargets(selectedUnmanaged)}>
                  批量删除 {selectedUnmanaged.length} 个
                </ActionButton>
                <button className="sm2__btn sm2__btn--ghost" disabled={actionBusy} onClick={() => {
                  setSelectedUnmanagedIds(new Set())
                  setUnmanagedSelectionMode(false)
                }}>
                  取消多选
                </button>
              </>
            ) : (
              <>
                <ActionButton
                  className="sm2__btn sm2__btn--primary"
                  disabled={quickTakeoverCandidates.length === 0 || actionBusy}
                  busy={unmanagedAdopting}
                  busyLabel={t('skills.agentManagement.quickTakeover.busy')}
                  onClick={() => setQuickTakeoverItems(quickTakeoverCandidates)}
                >
                  {t('skills.agentManagement.quickTakeover.action', { count: quickTakeoverCandidates.length })}
                </ActionButton>
                <button className="sm2__btn" disabled={canManageUnmanaged.length === 0 || actionBusy} onClick={() => setUnmanagedSelectionMode(true)}>
                  批量管理
                </button>
              </>
            )
          ) : null}
          <div className="sm2__view-toggle sm2__view-toggle--soft">
            <button className={viewMode === 'cards' ? 'active' : ''} onClick={() => setViewMode('cards')}>卡片</button>
            <button className={viewMode === 'list' ? 'active' : ''} onClick={() => setViewMode('list')}>列表</button>
          </div>
        </div>
      </div>
      <AgentToastStack
        notice={localNotice}
        onDismissNotice={() => setLocalNotice(null)}
        local
      />

      {scope === 'managed' && (
        <ManagedSkillCollection
          skills={filteredManaged}
          mode={viewMode}
          selectable={managedSelectionMode}
          selectedIds={selectedManagedIds}
          deletingIds={deletingIds}
          busy={actionBusy}
          emptyMessage={q || packFilter !== 'all' ? t('skills.agentManagement.packFilter.noResults') : undefined}
          onToggle={toggleManaged}
          onDelete={(skill) => deleteManaged([skill])}
          onMoveToPack={moveToPackOptions.length > 0 ? setMoveToPackTarget : undefined}
          onOpenSkillDetail={onOpenSkillDetail}
        />
      )}

      {scope === 'unmanaged' && showUnmanaged && (
        <>
          {sharedUnmanagedCount > 0 && (
            <div className="sm2__notice sm2__notice--info">
              {t('skills.agentManagement.sharedDirectoryNotice', { count: sharedUnmanagedCount })}
            </div>
          )}
          {filteredUnmanaged.length === 0 ? (
            <div className="sm2__empty sm2__empty--compact sm2__unmanaged-empty">
              <span>
                {unmanaged.length === 0 ? '没有未管理 Skill。若刚手动安装过，可以重新扫描。' : '没有匹配的未管理 Skill'}
              </span>
              {unmanaged.length === 0 && (
                <ActionButton className="sm2__btn" disabled={busy} onClick={() => onScan(detail.id)} busy={scanning} busyLabel="正在扫描">
                  {scanning ? '正在扫描' : '重新扫描此 Agent'}
                </ActionButton>
              )}
            </div>
          ) : (
            <>
              <UnmanagedSkillCollection
                skills={shownUnmanaged}
                mode={viewMode}
                agentId={detail.id}
                busy={actionBusy}
                selectable={unmanagedSelectionMode}
                selectedIds={selectedUnmanagedIds}
                adoptingIds={adoptingIds}
                adoptingUnmanagedId={adoptingUnmanagedId}
                deletingIds={deletingUnmanagedIds}
                onToggle={toggleUnmanaged}
                onAdopt={onAdopt}
                onDelete={(item) => {
                  setDeleteUnmanagedError(null)
                  setDeleteUnmanagedTarget(item)
                }}
                onOpenSkillDetail={onOpenSkillDetail}
              />
              {shownUnmanaged.length < filteredUnmanaged.length && (
                <button className="sm2__btn sm2__btn--ghost sm2__load-more" onClick={() => setPage((p) => p + 1)}>
                  继续显示 {Math.min(PAGE_SIZE, filteredUnmanaged.length - shownUnmanaged.length)} 个
                </button>
              )}
            </>
          )}
        </>
      )}

      {scope === 'builtin' && (
        <>
          <div className="sm2__notice sm2__notice--info">
            {t('skills.agentManagement.builtinSkillsNotice', {
              count: readOnlySkills.length,
              agent: detail.displayName,
            })}
          </div>
          {filteredReadOnly.length === 0 ? (
            <div className="sm2__empty sm2__empty--compact">
              {t('skills.agentManagement.builtinSkillsNoResults')}
            </div>
          ) : (
            <>
              <UnmanagedSkillCollection
                skills={shownReadOnly}
                mode={viewMode}
                agentId={detail.id}
                busy={actionBusy}
                selectable={false}
                selectedIds={new Set<string>()}
                adoptingIds={adoptingIds}
                adoptingUnmanagedId={adoptingUnmanagedId}
                deletingIds={deletingUnmanagedIds}
                onToggle={toggleUnmanaged}
                onAdopt={onAdopt}
                onDelete={() => undefined}
                onOpenSkillDetail={onOpenSkillDetail}
              />
              {shownReadOnly.length < filteredReadOnly.length && (
                <button className="sm2__btn sm2__btn--ghost sm2__load-more" onClick={() => setPage((p) => p + 1)}>
                  继续显示 {Math.min(PAGE_SIZE, filteredReadOnly.length - shownReadOnly.length)} 个
                </button>
              )}
            </>
          )}
        </>
      )}

      {scope === 'inherited' && inheritsSharedSkills && (
        <>
          <div className="sm2__notice sm2__notice--info">
            {t('skills.agentManagement.inheritedSkillsNotice', { agent: detail.displayName })}
          </div>
          {filteredInherited.length === 0 ? (
            <div className="sm2__empty sm2__empty--compact">
              {t('skills.agentManagement.inheritedSkillsNoResults')}
            </div>
          ) : (
            <>
              <InheritedSkillCollection
                skills={shownInherited}
                mode={viewMode}
                onOpenSkillDetail={onOpenSkillDetail}
              />
              {shownInherited.length < filteredInherited.length && (
                <button className="sm2__btn sm2__btn--ghost sm2__load-more" onClick={() => setPage((p) => p + 1)}>
                  继续显示 {Math.min(PAGE_SIZE, filteredInherited.length - shownInherited.length)} 个
                </button>
              )}
            </>
          )}
        </>
      )}

      {batchDeleteTargets && (
        <PreviewDialog
          title="确认批量删除 Skill？"
          confirmLabel="确认删除"
          busyLabel="删除中"
          destructive
          busy={managedDeleting}
          disabled={batchDeleteTargets.length === 0}
          onCancel={() => setBatchDeleteTargets(null)}
          onConfirm={confirmBatchDelete}
        >
          <p>{batchDeleteTargets.length}个SKILL 将从当前Agent直接删除，您后续仍旧可以从中心库安装</p>
        </PreviewDialog>
      )}
      {batchAdoptItems && (
        <BatchAdoptPackDialog
          items={batchAdoptItems}
          agentName={detail.displayName}
          packOptions={packOptionsForBatchAdopt(detail, state.packs)}
          busy={unmanagedAdopting}
          onCancel={() => setBatchAdoptItems(null)}
          onConfirm={async (selection) => {
            const items = batchAdoptItems
            setBatchAdoptItems(null)
            await adoptUnmanaged(items, selection)
          }}
        />
      )}
      {quickTakeoverItems && (
        <PreviewDialog
          title={t('skills.agentManagement.quickTakeover.title', { count: quickTakeoverItems.length })}
          confirmLabel={t('skills.agentManagement.quickTakeover.confirm')}
          busyLabel={t('skills.agentManagement.quickTakeover.busy')}
          busy={unmanagedAdopting}
          disabled={quickTakeoverItems.length === 0}
          modalClassName="sm2__modal--adopt"
          onCancel={() => setQuickTakeoverItems(null)}
          onConfirm={async () => {
            await takeoverExistingCenterSkills(quickTakeoverItems)
            setQuickTakeoverItems(null)
          }}
        >
          <p>{t('skills.agentManagement.quickTakeover.description', {
            count: quickTakeoverItems.length,
            agent: detail.displayName,
          })}</p>
          <p>{t('skills.agentManagement.quickTakeover.preserved', {
            count: Math.max(0, unmanaged.length - quickTakeoverItems.length),
          })}</p>
        </PreviewDialog>
      )}
      {batchDeleteUnmanagedTargets && (
        <PreviewDialog
          title="确认批量删除未管理 Skill？"
          confirmLabel="确认删除"
          busyLabel="删除中"
          destructive
          busy={unmanagedDeleting}
          disabled={batchDeleteUnmanagedTargets.length === 0}
          onCancel={() => setBatchDeleteUnmanagedTargets(null)}
          onConfirm={async () => {
            const items = batchDeleteUnmanagedTargets
            await deleteUnmanaged(items)
            setBatchDeleteUnmanagedTargets(null)
          }}
        >
          <p><strong>{batchDeleteUnmanagedTargets.length}</strong> 个未管理 Skill 将从当前 Agent 直接删除。</p>
          <p>这些 Skill 不会写入中心库，删除后无法从 AgentBro 恢复。</p>
        </PreviewDialog>
      )}
      {moveToPackTarget && (
        <MoveDirectSkillToPackDialog
          target={moveToPackTarget}
          packs={moveToPackOptions}
          onCancel={() => setMoveToPackTarget(null)}
          onDone={async (result) => {
            await refreshAgentSkills()
            setMoveToPackTarget(null)
            setLocalNotice(t('skills.moveToPack.success', {
              skill: result.skillName,
              pack: result.packName,
            }))
          }}
        />
      )}
      {deleteUnmanagedTarget && (
        <PreviewDialog
          title={t('skills.agentManagement.unmanagedDelete.title', {
            name: deleteUnmanagedTarget.inferredSkillId || pathBasename(deleteUnmanagedTarget.path) || deleteUnmanagedTarget.id,
          })}
          confirmLabel={t('skills.agentManagement.unmanagedDelete.confirm')}
          busyLabel={t('skills.agentManagement.unmanagedDelete.busy')}
          modalClassName="sm2__modal--unmanaged-delete"
          destructive
          busy={deletingUnmanagedIds.has(deleteUnmanagedTarget.id)}
          onCancel={() => {
            setDeleteUnmanagedError(null)
            setDeleteUnmanagedTarget(null)
          }}
          onConfirm={async () => {
            const ok = await deleteUnmanaged([deleteUnmanagedTarget])
            if (ok) {
              setDeleteUnmanagedError(null)
              setDeleteUnmanagedTarget(null)
            }
          }}
        >
          <p>{t('skills.agentManagement.unmanagedDelete.description')}</p>
          <code>{deleteUnmanagedTarget.path}</code>
          {deleteUnmanagedError && <div className="sm2__error sm2__modal-inline-error">{deleteUnmanagedError}</div>}
        </PreviewDialog>
      )}
      {installDialogOpen && (
        <AgentSkillInstallDialog
          agent={detail}
          skills={state.skills}
          defaultMode={state.settings?.defaultDistributeMode || 'link'}
          onClose={() => setInstallDialogOpen(false)}
          onDone={finishInstallFromLibrary}
        />
      )}
      {packApplyProgress && <PackApplyProgressToast progress={packApplyProgress} />}
    </div>
  )
}

function BatchAdoptPackDialog({
  items,
  agentName,
  packOptions,
  busy,
  onCancel,
  onConfirm,
}: {
  items: UnmanagedItemDto[]
  agentName: string
  packOptions: BatchAdoptPackOption[]
  busy: boolean
  onCancel: () => void
  onConfirm: (selection: BatchAdoptPackSelection) => Promise<void> | void
}) {
  const { t } = useTranslation()
  const [mode, setMode] = useState<BatchAdoptPackSelection['kind']>('none')
  const [packId, setPackId] = useState(packOptions[0]?.id ?? '')
  const [newPackName, setNewPackName] = useState('')
  const [error, setError] = useState<string | null>(null)
  const visibleItems = items.slice(0, 6)
  const remaining = Math.max(0, items.length - visibleItems.length)
  const syncEnabled = mode !== 'none'
  const disabled = mode === 'existing'
    ? !packId
    : mode === 'new'
      ? !newPackName.trim()
      : false

  const setSyncEnabled = (enabled: boolean) => {
    if (!enabled) {
      setMode('none')
      return
    }
    if (packOptions.length > 0) {
      setMode('existing')
      if (!packId) setPackId(packOptions[0]?.id ?? '')
      return
    }
    setMode('new')
  }

  const execute = async () => {
    const selection: BatchAdoptPackSelection = mode === 'existing'
      ? { kind: 'existing', packId }
      : mode === 'new'
        ? { kind: 'new', name: newPackName.trim() }
        : { kind: 'none' }
    setError(null)
    try {
      await Promise.resolve(onConfirm(selection))
    } catch (e) {
      setError(String(e))
    }
  }

  return (
    <PreviewDialog
      title={t('skills.batchAdoptPack.title', { count: items.length })}
      confirmLabel={t('skills.batchAdoptPack.confirm')}
      cancelLabel={t('skills.cancel')}
      busy={busy}
      disabled={disabled}
      modalClassName="sm2__modal--adopt sm2__modal--batch-adopt-pack"
      onCancel={onCancel}
      onConfirm={execute}
    >
      <div className="sm2-adopt sm2-batch-adopt-pack">
        <div className="sm2-adopt__summary">
          <div>
            <span>{t('skills.batchAdoptPack.agent')}</span>
            <strong>{agentName}</strong>
          </div>
          <div>
            <span>{t('skills.batchAdoptPack.skillCount')}</span>
            <strong>{items.length}</strong>
          </div>
          <div className="sm2-adopt__summary-path">
            <span>{t('skills.batchAdoptPack.summary')}</span>
            <code>{visibleItems.map((item) => item.inferredSkillId || pathBasename(item.path) || item.id).join(', ')}{remaining > 0 ? ` +${remaining}` : ''}</code>
          </div>
        </div>

        <div className="sm2-adopt__impact sm2-adopt__impact--warn">
          <strong>{t('skills.batchAdoptPack.strategyTitle')}</strong>
          <span>{t('skills.batchAdoptPack.strategyHint')}</span>
        </div>

        <section className="sm2-adopt__section">
          <div className="sm2-adopt__section-head">
            <h4>{t('skills.batchAdoptPack.syncQuestion')}</h4>
            <span>{t('skills.batchAdoptPack.syncHint')}</span>
          </div>
          <label className="sm2-batch-adopt-pack__toggle">
            <input
              type="checkbox"
              checked={syncEnabled}
              onChange={(event) => setSyncEnabled(event.currentTarget.checked)}
            />
            <span aria-hidden="true" />
            <div>
              <strong>{t('skills.batchAdoptPack.syncToggle')}</strong>
              <small>{syncEnabled ? t('skills.batchAdoptPack.syncEnabledHelp') : t('skills.batchAdoptPack.syncDisabledHelp')}</small>
            </div>
          </label>
        </section>

        {syncEnabled && (
          <div className="sm2-adopt__rename sm2-batch-adopt-pack__target">
            <div className="sm2__view-toggle sm2__view-toggle--soft" aria-label={t('skills.batchAdoptPack.targetMode')}>
              <button
                type="button"
                className={mode === 'existing' ? 'active' : ''}
                disabled={packOptions.length === 0}
                onClick={() => {
                  setMode('existing')
                  if (!packId) setPackId(packOptions[0]?.id ?? '')
                }}
              >
                {t('skills.batchAdoptPack.existingTitle')}
              </button>
              <button
                type="button"
                className={mode === 'new' ? 'active' : ''}
                onClick={() => setMode('new')}
              >
                {t('skills.batchAdoptPack.newTitle')}
              </button>
            </div>
            {mode === 'existing' && (
              <>
                <label htmlFor="sm2-batch-adopt-pack-existing">{t('skills.batchAdoptPack.targetPack')}</label>
                <select
                  id="sm2-batch-adopt-pack-existing"
                  value={packId}
                  onChange={(event) => setPackId(event.target.value)}
                >
                  {packOptions.map((pack) => (
                    <option key={pack.id} value={pack.id}>
                      {pack.name} ({pack.memberCount})
                    </option>
                  ))}
                </select>
                <span>{packOptions.length > 0 ? t('skills.batchAdoptPack.existingImpact') : t('skills.batchAdoptPack.noExistingPacks')}</span>
              </>
            )}
            {mode === 'new' && (
              <>
                <label htmlFor="sm2-batch-adopt-pack-new">{t('skills.batchAdoptPack.newPackName')}</label>
                <input
                  id="sm2-batch-adopt-pack-new"
                  value={newPackName}
                  onChange={(event) => setNewPackName(event.target.value)}
                  placeholder={t('skills.batchAdoptPack.newPackPlaceholder')}
                />
                <span>{t('skills.batchAdoptPack.newImpact')}</span>
              </>
            )}
          </div>
        )}

        {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
      </div>
    </PreviewDialog>
  )
}

function MoveDirectSkillToPackDialog({
  target,
  packs,
  onCancel,
  onDone,
}: {
  target: AgentDetail['skills'][number]
  packs: MoveToPackOption[]
  onCancel: () => void
  onDone: (result: MoveDirectSkillToPackPreview) => Promise<void> | void
}) {
  const { t } = useTranslation()
  const [packId, setPackId] = useState(() => {
    const existingPackClaim = target.claims.find((claim) => (
      claim.claimType === 'pack'
      && claim.packId !== null
      && packs.some((pack) => pack.id === claim.packId)
    ))
    return existingPackClaim?.packId ?? packs[0]?.id ?? ''
  })
  const [preview, setPreview] = useState<MoveDirectSkillToPackPreview | null>(null)
  const [loading, setLoading] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const name = pathBasename(target.targetPath) || target.skillId

  useEffect(() => {
    if (!packId) return
    let active = true
    setLoading(true)
    setPreview(null)
    setError(null)
    skillApiV2.previewMoveDirectSkillToPack(target.id, packId)
      .then((next) => {
        if (active) setPreview(next)
      })
      .catch((nextError) => {
        if (active) setError(skillErrorMessage(t, nextError))
      })
      .finally(() => {
        if (active) setLoading(false)
      })
    return () => {
      active = false
    }
  }, [packId, t, target.id])

  const selectedPack = packs.find((pack) => pack.id === packId)

  const execute = async () => {
    if (!preview) return
    setBusy(true)
    setError(null)
    try {
      const result = await skillApiV2.moveDirectSkillToPack(target.id, packId, [])
      await onDone(result)
    } catch (nextError) {
      setError(skillErrorMessage(t, nextError))
    } finally {
      setBusy(false)
    }
  }

  return (
    <PreviewDialog
      title={t('skills.moveToPack.title')}
      confirmLabel={t('skills.moveToPack.confirm')}
      busyLabel={t('skills.moveToPack.moving')}
      cancelLabel={t('skills.cancel')}
      busy={busy}
      disabled={loading || !preview}
      modalClassName="sm2__modal--move-to-pack sm2__modal--light-surface"
      onCancel={onCancel}
      onConfirm={execute}
    >
      <div className="sm2-move-to-pack">
        <div className="sm2-move-to-pack__subject">
          <div className="sm2__agent-skill-icon">{initials(name)}</div>
          <div>
            <span>{t('skills.moveToPack.skill')}</span>
            <strong>{name}</strong>
            <code>{target.targetPath}</code>
          </div>
          <span className="sm2__tag sm2__tag--ready">{t('skills.claim.direct')}</span>
        </div>

        <label className="sm2-move-to-pack__select" htmlFor="sm2-move-to-pack-select">
          <span>{t('skills.moveToPack.targetPack')}</span>
          <select
            id="sm2-move-to-pack-select"
            value={packId}
            disabled={busy}
            onChange={(event) => setPackId(event.target.value)}
          >
            {packs.map((pack) => (
              <option key={pack.id} value={pack.id}>
                {pack.name} · {t('skills.moveToPack.memberCount', { count: pack.memberCount })} · {pack.applied ? t('skills.moveToPack.applied') : t('skills.moveToPack.notApplied')}
              </option>
            ))}
          </select>
          {selectedPack?.description && <small>{selectedPack.description}</small>}
          {selectedPack?.isDefault && <small>{t('skills.moveToPack.defaultPackHint')}</small>}
        </label>

        {loading && <div className="sm2-move-to-pack__loading">{t('skills.moveToPack.previewing')}</div>}

        {preview && (
          <section className="sm2-move-to-pack__impact" aria-label={t('skills.moveToPack.impact')}>
            <h4>{t('skills.moveToPack.impact')}</h4>
            <div className="sm2-move-to-pack__impact-row">
              <span>1</span>
              <div>
                <strong>{preview.willAddToPack ? t('skills.moveToPack.addMembershipTitle') : t('skills.moveToPack.keepMembershipTitle')}</strong>
                <small>{preview.willAddToPack
                  ? t('skills.moveToPack.addMembership', { skill: preview.skillName, pack: preview.packName })
                  : t('skills.moveToPack.keepMembership', { skill: preview.skillName, pack: preview.packName })}</small>
              </div>
            </div>
            <div className="sm2-move-to-pack__impact-row">
              <span>2</span>
              <div>
                <strong>{t('skills.moveToPack.keepOtherMembersTitle')}</strong>
                <small>{t('skills.moveToPack.keepOtherMembers', {
                  pack: preview.packName,
                  count: preview.otherMemberCount,
                })}</small>
              </div>
            </div>
            <div className="sm2-move-to-pack__impact-row sm2-move-to-pack__impact-row--final">
              <span>3</span>
              <div>
                <strong>{t('skills.moveToPack.removeDirectTitle')}</strong>
                <small>{t('skills.moveToPack.removeDirect', { skill: preview.skillName, agent: preview.displayName })}</small>
              </div>
            </div>
          </section>
        )}

        {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
      </div>
    </PreviewDialog>
  )
}

function PackApplyProgressToast({ progress }: { progress: PackApplyProgress }) {
  return (
    <div
      className={`sm2__pack-apply-progress sm2__pack-apply-progress--floating${progress.state === 'done' ? ' sm2__pack-apply-progress--done' : ''}`}
      role="status"
      aria-label="技能包应用进度"
      aria-live="polite"
    >
      <div className="sm2__pack-apply-progress-main">
        <span>{progress.state === 'done' ? '已应用' : '正在应用'}</span>
        <strong>{progress.packName}</strong>
      </div>
      <div className="sm2__pack-apply-progress-sub">
        <span>{progress.agentName}</span>
        <span>{progress.detail}</span>
      </div>
      <div
        className="sm2__pack-apply-progress-bar"
        role="progressbar"
        aria-label="技能包应用进度条"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={progress.percent}
        style={{ '--sm2-pack-apply-progress': `${progress.percent}%` } as CSSProperties}
      >
        <span />
      </div>
    </div>
  )
}

function PackApplyConflictDialog({
  dialog,
  busy,
  onCancel,
  onExecute,
}: {
  dialog: PackApplyConflictDialogState
  busy: boolean
  onCancel: () => void
  onExecute: (decisions: DistributionBlockerDecision[]) => Promise<void>
}) {
  const { t } = useTranslation()
  const [blockerDecisions, setBlockerDecisions] = useState<Record<string, PackBlockerDecision>>({})
  const [error, setError] = useState<string | null>(null)
  const unresolvedBlockers = dialog.preview.blockers.filter((blocker) => !blockerDecisions[installBlockerKey(blocker)]).length

  const execute = async () => {
    const payload = dialog.preview.blockers
      .map((blocker) => {
        const action = blockerDecisions[installBlockerKey(blocker)]
        return action ? { skillId: blocker.skillId, agentId: blocker.agentId, action } : null
      })
      .filter((item): item is DistributionBlockerDecision => Boolean(item))
    try {
      await onExecute(payload)
    } catch (e) {
      setError(String(e))
    }
  }

  return (
    <PreviewDialog
      title={`应用「${dialog.packName}」`}
      confirmLabel={dialog.preview.blockers.length > 0 ? '按选择执行' : '执行应用'}
      cancelLabel="取消"
      busy={busy}
      disabled={unresolvedBlockers > 0}
      modalClassName="sm2__modal--agent-install sm2__modal--light-surface"
      onCancel={onCancel}
      onConfirm={execute}
    >
      <div className="sm2-agent-install">
        <div className="sm2-agent-install__target">
          <span>目标：</span>
          <strong>{dialog.agentName}</strong>
          <em>{dialog.preview.skillIds.length} 个 Skill · {skillModeLabel(t, dialog.mode)}</em>
        </div>
        <div className="sm2-agent-install__preview-list">
          {dialog.preview.changes.map((change) => (
            <div key={`${change.skillId}-${change.agentId}`} className="sm2-agent-install__preview-row">
              <span className="sm2__tag sm2__tag--ok">
                {change.action === 'create' ? '新增' : change.action === 'reinstall' ? '重装' : change.action === 'convert' ? '转换' : change.action}
              </span>
              <div>
                <strong>{change.skillId}</strong>
                <code>{change.targetPath}</code>
              </div>
            </div>
          ))}
          {dialog.preview.blockers.map((blocker) => {
            const key = installBlockerKey(blocker)
            const decision = blockerDecisions[key]
            const managedCopyBlocker = isManagedCopyBlocker(blocker)
            return (
              <div key={key} className="sm2-agent-install__preview-row sm2-agent-install__preview-row--blocked">
                <span className="sm2__tag sm2__tag--conflict">阻止</span>
                <div>
                  <strong>{blocker.skillId}</strong>
                  <span>{distributionBlockerReason(t, blocker)}</span>
                  {blocker.existingPath && (
                    <div className="sm2-distribute__path-row" style={{ marginTop: 8 }}>
                      <code>{blocker.existingPath}</code>
                      <button type="button" className="sm2__btn sm2__btn--ghost" onClick={() => skillApiV2.openPath(blocker.existingPath!)}>
                        打开
                      </button>
                    </div>
                  )}
                  <div className="sm2-agent-install__decision-row">
                    {blocker.existingPath && (
                      <button
                        type="button"
                        className={`sm2__btn${decision === 'overwrite' ? ' sm2__btn--active' : ''}`}
                        onClick={() => setBlockerDecisions((current) => ({ ...current, [key]: 'overwrite' }))}
                      >
                        {managedCopyBlocker ? '以中心库为准' : '覆盖安装'}
                      </button>
                    )}
                    {managedCopyBlocker && (
                      <button
                        type="button"
                        className={`sm2__btn${decision === 'agent_over_center' ? ' sm2__btn--active' : ''}`}
                        onClick={() => setBlockerDecisions((current) => ({ ...current, [key]: 'agent_over_center' }))}
                      >
                        以 Agent 为准
                      </button>
                    )}
                    <button
                      type="button"
                      className={`sm2__btn${decision === 'skip' ? ' sm2__btn--active' : ''}`}
                      onClick={() => setBlockerDecisions((current) => ({ ...current, [key]: 'skip' }))}
                    >
                      忽略此目标
                    </button>
                  </div>
                </div>
              </div>
            )
          })}
        </div>
        {dialog.preview.blockers.length > 0 && (
          <p className="sm2-distribute__blocked-note">
            请选择每个阻止项的处理方式。覆盖安装会用中心库版本替换当前 Agent 中同名未管理 Skill。
          </p>
        )}
        {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
      </div>
    </PreviewDialog>
  )
}

function AgentPackToggleRail({
  detail,
  busy,
  onApplyPack,
  onRevoke,
}: {
  detail: AgentDetail
  busy: boolean
  onApplyPack: (packId: string, agentId: string) => boolean | Promise<boolean>
  onRevoke: (packId: string, agentId: string) => void
}) {
  const appliedById = new Map(detail.appliedPacks.map((pack) => [pack.packId, pack]))
  const availableRows = detail.availablePacks.map((pack) => ({
    id: pack.id,
    name: pack.name,
    memberCount: pack.memberCount,
    applied: appliedById.has(pack.id),
    isDefault: pack.id === 'default',
  }))
  const appliedOnlyRows = detail.appliedPacks
    .filter((pack) => !availableRows.some((item) => item.id === pack.packId))
    .map((pack) => ({
      id: pack.packId,
      name: pack.packName,
      memberCount: pack.memberCount,
      applied: true,
      isDefault: pack.packId === 'default',
    }))
  const rows = [...availableRows, ...appliedOnlyRows].sort((left, right) => {
    if (left.isDefault !== right.isDefault) return left.isDefault ? -1 : 1
    if (left.applied !== right.applied) return left.applied ? -1 : 1
    return 0
  })
  const appliedCount = rows.filter((pack) => pack.applied).length

  if (rows.length === 0) return null

  return (
    <section className="sm2__agent-pack-rail" aria-label="技能包应用">
      <div className="sm2__agent-pack-rail-head">
        <div className="sm2__agent-pack-rail-title">
          <span className="sm2__agent-pack-rail-mark" aria-hidden="true">
            <i />
            <i />
            <i />
            <i />
          </span>
          <div>
            <strong>技能包</strong>
            <span>选择一组能力快速应用，再次点击即可取消</span>
          </div>
        </div>
        <div className="sm2__agent-pack-rail-summary" aria-label={`已应用 ${appliedCount} 个，共 ${rows.length} 个技能包`}>
          <strong>{appliedCount}</strong>
          <span>/ {rows.length} 已应用</span>
        </div>
      </div>
      <div className="sm2__agent-pack-toggles">
        {rows.map((pack) => (
          <button
            key={pack.id}
            type="button"
            className={`sm2__agent-pack-toggle${pack.applied ? ' sm2__agent-pack-toggle--applied' : ''}${pack.isDefault ? ' sm2__agent-pack-toggle--default' : ''}`}
            disabled={busy || (!pack.applied && pack.memberCount === 0)}
            aria-pressed={pack.applied}
            aria-label={`${pack.applied ? '取消应用' : '应用'} ${pack.name}`}
            title={pack.name}
            onClick={() => pack.applied ? onRevoke(pack.id, detail.id) : onApplyPack(pack.id, detail.id)}
          >
            <span className="sm2__agent-pack-toggle-mark" aria-hidden="true">{pack.applied ? '✓' : '+'}</span>
            <span className="sm2__agent-pack-toggle-main">
              <strong>{pack.name}</strong>
              <span>{pack.isDefault ? `中心库全量 · ${pack.memberCount} 个 Skill` : `${pack.memberCount} 个 Skill`}</span>
            </span>
            <span className="sm2__agent-pack-toggle-state" aria-hidden="true">
              {pack.applied ? '已应用' : '应用'}
              <i>{pack.applied ? '×' : '→'}</i>
            </span>
          </button>
        ))}
      </div>
    </section>
  )
}

function AgentSkillInstallDialog({
  agent,
  skills,
  defaultMode,
  onClose,
  onDone,
}: {
  agent: AgentDetail
  skills: SkillSummary[]
  defaultMode: 'link' | 'copy'
  onClose: () => void
  onDone: () => void | Promise<void>
}) {
  const { t } = useTranslation()
  const [scope, setScope] = useState<AgentLibraryScope>('uninstalled')
  const [query, setQuery] = useState('')
  const [source, setSource] = useState('')
  const [mode, setMode] = useState<'link' | 'copy'>(defaultMode)
  const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set())
  const [preview, setPreview] = useState<DistributionPreview | null>(null)
  const [blockerDecisions, setBlockerDecisions] = useState<Record<string, InstallBlockerDecision>>({})
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const sources = useMemo(
    () => Array.from(new Set(skills.map((skill) => skill.sourceType).filter(Boolean))).sort(),
    [skills],
  )
  const counts = useMemo(() => {
    let installed = 0
    for (const skill of skills) {
      if (skillInstalledOnAgent(skill, agent.id)) installed += 1
    }
    return { installed, uninstalled: skills.length - installed, all: skills.length }
  }, [agent.id, skills])
  const filteredSkills = useMemo(() => {
    const q = query.trim().toLowerCase()
    return skills.filter((skill) => {
      const installed = skillInstalledOnAgent(skill, agent.id)
      if (scope === 'installed' && !installed) return false
      if (scope === 'uninstalled' && installed) return false
      if (source && skill.sourceType !== source) return false
      if (!q) return true
      const haystack = [
        skill.id,
        skill.name,
        skill.description,
        skill.sourceType,
        skillSourceTypeLabel(t, skill.sourceType),
        skill.sourceUri,
        skill.skillType,
      ].filter(Boolean).join(' ').toLowerCase()
      return haystack.includes(q)
    })
  }, [agent.id, query, scope, skills, source, t])
  const selectedSkills = useMemo(
    () => skills.filter((skill) => selectedIds.has(skill.id)),
    [selectedIds, skills],
  )
  const unresolvedBlockers = preview?.blockers.filter((blocker) => !blockerDecisions[installBlockerKey(blocker)]).length ?? 0

  useEffect(() => {
    setMode(defaultMode)
  }, [defaultMode])

  useEffect(() => {
    setSelectedIds((current) => {
      const known = new Set(skills.map((skill) => skill.id))
      const next = new Set(Array.from(current).filter((id) => known.has(id)))
      return next.size === current.size ? current : next
    })
  }, [skills])

  const toggleSkill = (skillId: string) => {
    setSelectedIds((current) => toggleSetValue(current, skillId))
  }

  const selectVisible = () => {
    setSelectedIds((current) => {
      const next = new Set(current)
      for (const skill of filteredSkills) next.add(skill.id)
      return next
    })
  }

  const runPreview = async () => {
    if (selectedSkills.length === 0) return
    setBusy(true)
    setError(null)
    try {
      const nextPreview = await skillApiV2.previewDistribute(selectedSkills.map((skill) => skill.id), [agent.id], mode)
      setPreview(nextPreview)
      setBlockerDecisions({})
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  const execute = async () => {
    if (!preview) return
    setBusy(true)
    setError(null)
    try {
      const blockerDecisionsPayload = preview.blockers
        .map((blocker) => {
          const action = blockerDecisions[installBlockerKey(blocker)]
          return action ? { skillId: blocker.skillId, agentId: blocker.agentId, action } : null
        })
        .filter((item): item is { skillId: string; agentId: string; action: InstallBlockerDecision } => Boolean(item))
      await skillApiV2.executeDistribute({ ...preview, blockerDecisions: blockerDecisionsPayload })
      await Promise.resolve(onDone())
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  if (preview) {
    return (
      <PreviewDialog
        title="确认安装 Skill"
        confirmLabel={preview.blockers.length > 0 ? '按选择执行' : '执行分发'}
        cancelLabel="返回选择"
        busy={busy}
        disabled={unresolvedBlockers > 0}
        modalClassName="sm2__modal--agent-install sm2__modal--light-surface"
        onCancel={() => setPreview(null)}
        onConfirm={execute}
      >
        <div className="sm2-agent-install">
          <div className="sm2-agent-install__target">
            <span>目标：</span>
            <strong>{agent.displayName}</strong>
            <em>{selectedSkills.length} 个 Skill · {skillModeLabel(t, mode)}</em>
          </div>
          <div className="sm2-agent-install__preview-list">
            {preview.changes.map((change) => (
              <div key={`${change.skillId}-${change.agentId}`} className="sm2-agent-install__preview-row">
                <span className="sm2__tag sm2__tag--ok">
                  {change.action === 'create' ? '新增' : change.action === 'reinstall' ? '重装' : change.action === 'convert' ? '转换' : change.action}
                </span>
                <div>
                  <strong>{selectedSkills.find((skill) => skill.id === change.skillId)?.name ?? change.skillId}</strong>
                  <code>{change.targetPath}</code>
                </div>
              </div>
            ))}
            {preview.blockers.map((blocker) => {
              const key = installBlockerKey(blocker)
              const decision = blockerDecisions[key]
              return (
                <div key={key} className="sm2-agent-install__preview-row sm2-agent-install__preview-row--blocked">
                  <span className="sm2__tag sm2__tag--conflict">阻止</span>
                  <div>
                    <strong>{selectedSkills.find((skill) => skill.id === blocker.skillId)?.name ?? blocker.skillId}</strong>
                    <span>{distributionBlockerReason(t, blocker)}</span>
                    {blocker.existingPath && <code>{blocker.existingPath}</code>}
                    <div className="sm2-agent-install__decision-row">
                      {blocker.existingPath && (
                        <button
                          type="button"
                          className={`sm2__btn${decision === 'overwrite' ? ' sm2__btn--active' : ''}`}
                          onClick={() => setBlockerDecisions((current) => ({ ...current, [key]: 'overwrite' }))}
                        >
                          覆盖安装
                        </button>
                      )}
                      <button
                        type="button"
                        className={`sm2__btn${decision === 'skip' ? ' sm2__btn--active' : ''}`}
                        onClick={() => setBlockerDecisions((current) => ({ ...current, [key]: 'skip' }))}
                      >
                        忽略此目标
                      </button>
                    </div>
                  </div>
                </div>
              )
            })}
          </div>
          {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
        </div>
      </PreviewDialog>
    )
  }

  return (
    <PreviewDialog
      title="从技能库添加"
      confirmLabel={`添加 ${selectedIds.size} 个 Skill`}
      busy={busy}
      disabled={selectedIds.size === 0}
      modalClassName="sm2__modal--agent-install sm2__modal--light-surface"
      onCancel={onClose}
      onConfirm={runPreview}
    >
      <div className="sm2-agent-install">
        <div className="sm2-agent-install__target">
          <span>目标：</span>
          <AgentIconBadge iconKey={agent.iconKey} title={agent.displayName} size={24} />
          <strong>{agent.displayName}</strong>
          <em>{skillModeLabel(t, mode)}</em>
        </div>
        <div className="sm2-agent-install__search">
          <span className="sm2__filter-icon">⌕</span>
          <input
            className="sm2__search sm2__search--quiet"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="搜索技能库..."
          />
        </div>
        <div className="sm2-agent-install__filters">
          <span>状态</span>
          <div className="sm2__view-toggle sm2__view-toggle--soft">
            <button className={scope === 'uninstalled' ? 'active' : ''} onClick={() => setScope('uninstalled')}>未安装 {counts.uninstalled}</button>
            <button className={scope === 'installed' ? 'active' : ''} onClick={() => setScope('installed')}>已安装 {counts.installed}</button>
            <button className={scope === 'all' ? 'active' : ''} onClick={() => setScope('all')}>全部 {counts.all}</button>
          </div>
        </div>
        <div className="sm2-agent-install__filters">
          <span>标签</span>
          <div className="sm2-agent-install__chips">
            <button className={`sm2__source-chip${source === '' ? ' sm2__source-chip--active' : ''}`} onClick={() => setSource('')}>
              全部标签
            </button>
            {sources.map((item) => (
              <button
                key={item}
                className={`sm2__source-chip${source === item ? ' sm2__source-chip--active' : ''}`}
                onClick={() => setSource(item)}
              >
                {skillSourceTypeLabel(t, item)}
              </button>
            ))}
          </div>
        </div>
        <div className="sm2-agent-install__filters">
          <span>安装方式</span>
          <div className="sm2__view-toggle sm2__view-toggle--soft">
            <button className={mode === 'link' ? 'active' : ''} onClick={() => setMode('link')}>{skillModeLabel(t, 'link')}</button>
            <button className={mode === 'copy' ? 'active' : ''} onClick={() => setMode('copy')}>{skillModeLabel(t, 'copy')}</button>
          </div>
        </div>
        <div className="sm2-agent-install__bulk">
          <span>已选择 {selectedIds.size} 个</span>
          <button className="sm2__btn sm2__btn--ghost" disabled={filteredSkills.length === 0} onClick={selectVisible}>
            选择当前
          </button>
          <button className="sm2__btn sm2__btn--ghost" disabled={selectedIds.size === 0} onClick={() => setSelectedIds(new Set())}>
            清空
          </button>
        </div>
        <div className="sm2-agent-install__list">
          {filteredSkills.length === 0 ? (
            <div className="sm2__empty sm2__empty--compact">没有匹配的 Skill</div>
          ) : (
            filteredSkills.map((skill) => {
              const installedRef = skill.installedAgents.find((ref) => ref.agentId === agent.id)
              const checked = selectedIds.has(skill.id)
              return (
                <label key={skill.id} className={`sm2-agent-install__row${checked ? ' sm2-agent-install__row--selected' : ''}${installedRef ? ' sm2-agent-install__row--installed' : ''}`}>
                  <input
                    type="checkbox"
                    aria-label={`选择 ${skill.name}`}
                    checked={checked}
                    onChange={() => toggleSkill(skill.id)}
                  />
                  <span className="sm2-agent-install__check" aria-hidden="true">{checked ? '✓' : ''}</span>
                  <div className="sm2-agent-install__row-main">
                    <strong>{skill.name}</strong>
                    <span>{skill.description || skill.id}</span>
                  </div>
                  <span className="sm2-agent-install__source">{skillSourceTypeLabel(t, skill.sourceType)}</span>
                  <span className={`sm2__tag sm2__tag--${installedRef ? 'ok' : 'unmanaged'}`}>
                    {installedRef ? '已添加' : '未安装'}
                  </span>
                </label>
              )
            })
          )}
        </div>
        {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
      </div>
    </PreviewDialog>
  )
}

function ManagedSkillCollection({
  skills,
  mode,
  selectable,
  selectedIds,
  deletingIds,
  busy,
  emptyMessage,
  onToggle,
  onDelete,
  onMoveToPack,
  onOpenSkillDetail,
}: {
  skills: AgentDetail['skills']
  mode: AgentSkillViewMode
  selectable: boolean
  selectedIds: Set<string>
  deletingIds: Set<string>
  busy: boolean
  emptyMessage?: string
  onToggle: (targetId: string) => void
  onDelete: (skill: AgentDetail['skills'][number]) => void
  onMoveToPack?: (skill: AgentDetail['skills'][number]) => void
  onOpenSkillDetail: (skillId: string) => void
}) {
  if (skills.length === 0) {
    return <div className="sm2__empty sm2__empty--compact">{emptyMessage || '暂无已管理 Skill'}</div>
  }

  if (mode === 'list') {
    return (
      <div className="sm2__agent-skill-list">
        {skills.map((s) => (
          <ManagedSkillListRow
            key={s.id}
            skill={s}
            selectable={selectable}
            selected={selectable && selectedIds.has(s.id)}
            deleting={deletingIds.has(s.id)}
            busy={busy}
            onToggle={onToggle}
            onDelete={onDelete}
            onMoveToPack={onMoveToPack}
            onOpenSkillDetail={onOpenSkillDetail}
          />
        ))}
      </div>
    )
  }

  return (
    <div className="sm2__agent-skill-grid">
      {skills.map((s) => (
        <ManagedSkillCard
          key={s.id}
          skill={s}
          selectable={selectable}
          selected={selectable && selectedIds.has(s.id)}
          deleting={deletingIds.has(s.id)}
          busy={busy}
          onToggle={onToggle}
          onDelete={onDelete}
          onMoveToPack={onMoveToPack}
          onOpenSkillDetail={onOpenSkillDetail}
        />
      ))}
    </div>
  )
}

function ManagedSkillCard({
  skill,
  selectable,
  selected,
  deleting,
  busy,
  onToggle,
  onDelete,
  onMoveToPack,
  onOpenSkillDetail,
}: {
  skill: AgentDetail['skills'][number]
  selectable: boolean
  selected: boolean
  deleting: boolean
  busy: boolean
  onToggle: (targetId: string) => void
  onDelete: (skill: AgentDetail['skills'][number]) => void
  onMoveToPack?: (skill: AgentDetail['skills'][number]) => void
  onOpenSkillDetail: (skillId: string) => void
}) {
  const { t } = useTranslation()
  const name = pathBasename(skill.targetPath) || skill.id
  const claims = skill.claims.map((c) => targetClaimLabel(t, c)).filter(Boolean)
  const directlyDistributed = skill.claims.some((claim) => claim.claimType === 'direct')
  return (
    <article
      className={`sm2__agent-skill-card sm2__agent-skill-card--clickable${selected ? ' sm2__agent-skill-card--selected' : ''}${deleting ? ' sm2__agent-skill-card--deleting' : ''}`}
      role="button"
      tabIndex={0}
      aria-busy={deleting || undefined}
      onClick={() => onOpenSkillDetail(skill.skillId)}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') onOpenSkillDetail(skill.skillId)
      }}
    >
      <div className={`sm2__agent-skill-card-head${selectable ? ' sm2__agent-skill-card-head--selectable' : ''}`}>
        {selectable && (
          <input
            type="checkbox"
            className="sm2__agent-skill-select"
            aria-label={`选择 ${name}`}
            checked={selected}
            disabled={busy}
            onClick={(e) => e.stopPropagation()}
            onChange={() => onToggle(skill.id)}
          />
        )}
        <div className="sm2__agent-skill-icon">{initials(name)}</div>
        <div className="sm2__agent-skill-card-titleline">
          <strong>{name}</strong>
          <span>{claims.length > 0 ? claims.join(' / ') : targetClaimLabel(t, null)}</span>
        </div>
        <span className={`sm2__tag sm2__tag--${skill.status}`}>{skillStatusLabel(t, skill.status)}</span>
      </div>
      <div className="sm2__agent-skill-meta">
        <span className="sm2__source-pill">{skillModeLabel(t, skill.actualMode)}</span>
        {skill.claims.length > 0
          ? skill.claims.map((claim) => (
            <span key={claim.id} className={`sm2__source-pill sm2__source-pill--claim-${claim.claimType}`}>
              {targetClaimLabel(t, claim)}
            </span>
          ))
          : <span className="sm2__source-pill sm2__source-pill--claim-direct">{targetClaimLabel(t, null)}</span>}
      </div>
      <code>{skill.targetPath}</code>
      <div className="sm2__agent-skill-card-actions">
        {directlyDistributed && onMoveToPack && (
          <button className="sm2__btn sm2__btn--pack" aria-label={`${t('skills.moveToPack.action')} ${name}`} disabled={busy} onClick={(event) => {
            event.stopPropagation()
            onMoveToPack(skill)
          }}>
            {t('skills.moveToPack.action')}
          </button>
        )}
        <ActionButton className="sm2__btn sm2__btn--danger" disabled={busy && !deleting} busy={deleting} busyLabel="删除中" aria-label={deleting ? `删除中 ${name}` : `删除 ${name}`} onClick={(e) => {
          e.stopPropagation()
          onDelete(skill)
        }}>
          删除
        </ActionButton>
      </div>
    </article>
  )
}

function ManagedSkillListRow({
  skill,
  selectable,
  selected,
  deleting,
  busy,
  onToggle,
  onDelete,
  onMoveToPack,
  onOpenSkillDetail,
}: {
  skill: AgentDetail['skills'][number]
  selectable: boolean
  selected: boolean
  deleting: boolean
  busy: boolean
  onToggle: (targetId: string) => void
  onDelete: (skill: AgentDetail['skills'][number]) => void
  onMoveToPack?: (skill: AgentDetail['skills'][number]) => void
  onOpenSkillDetail: (skillId: string) => void
}) {
  const { t } = useTranslation()
  const claims = skill.claims.map((c) => targetClaimLabel(t, c)).filter(Boolean)
  const name = pathBasename(skill.targetPath) || skill.id
  const directlyDistributed = skill.claims.some((claim) => claim.claimType === 'direct')
  return (
    <div className={`sm2__object-row sm2__object-row--path sm2__object-row--clickable${selected ? ' sm2__object-row--selected' : ''}${deleting ? ' sm2__object-row--deleting' : ''}`} aria-busy={deleting || undefined} onClick={() => onOpenSkillDetail(skill.skillId)}>
      {selectable && (
        <input
          type="checkbox"
          className="sm2__agent-skill-select"
          aria-label={`选择 ${name}`}
          checked={selected}
          disabled={busy}
          onClick={(e) => e.stopPropagation()}
          onChange={() => onToggle(skill.id)}
        />
      )}
      <div>
        <strong>{name}</strong>
        <span>{skillModeLabel(t, skill.actualMode)} · {skillStatusLabel(t, skill.status)} · {claims.join(' / ')}</span>
        <code>{skill.targetPath}</code>
      </div>
      <div className="sm2__object-row-actions">
        {directlyDistributed && onMoveToPack && (
          <button className="sm2__btn sm2__btn--pack" aria-label={`${t('skills.moveToPack.action')} ${name}`} disabled={busy} onClick={(event) => {
            event.stopPropagation()
            onMoveToPack(skill)
          }}>
            {t('skills.moveToPack.action')}
          </button>
        )}
        <ActionButton className="sm2__btn sm2__btn--danger" disabled={busy && !deleting} busy={deleting} busyLabel="删除中" aria-label={deleting ? `删除中 ${name}` : `删除 ${name}`} onClick={(e) => {
          e.stopPropagation()
          onDelete(skill)
        }}>
          删除
        </ActionButton>
      </div>
    </div>
  )
}

function InheritedSkillCollection({
  skills,
  mode,
  onOpenSkillDetail,
}: {
  skills: InheritedSkillDetail[]
  mode: AgentSkillViewMode
  onOpenSkillDetail: (skillId: string, fallback?: SkillDetailFallback | null) => void
}) {
  const { t } = useTranslation()
  if (mode === 'list') {
    return (
      <div className="sm2__agent-skill-list">
        {skills.map((skill) => (
          <div
            key={skill.id}
            className="sm2__object-row sm2__object-row--path sm2__object-row--clickable"
            onClick={() => openInheritedSkill(skill, onOpenSkillDetail)}
          >
            <div>
              <strong>{skill.skillId}</strong>
              <span>{t('skills.agentManagement.inheritedSkillsSource')}</span>
              <code title={skill.resolvedPath ?? undefined}>{skill.path}</code>
            </div>
            <span className="sm2__tag sm2__tag--shared">
              {t('skills.agentManagement.inheritedSkills')}
            </span>
          </div>
        ))}
      </div>
    )
  }

  return (
    <div className="sm2__agent-skill-grid">
      {skills.map((skill) => (
        <article
          key={skill.id}
          className="sm2__agent-skill-card sm2__agent-skill-card--clickable"
          role="button"
          tabIndex={0}
          onClick={() => openInheritedSkill(skill, onOpenSkillDetail)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' || event.key === ' ') {
              openInheritedSkill(skill, onOpenSkillDetail)
            }
          }}
        >
          <div className="sm2__agent-skill-card-head">
            <div className="sm2__agent-skill-icon">{initials(skill.skillId || 'SK')}</div>
            <div className="sm2__agent-skill-card-titleline">
              <strong>{skill.skillId}</strong>
              <span>{t('skills.agentManagement.inheritedSkillsSource')}</span>
            </div>
            <span className="sm2__tag sm2__tag--shared">
              {t('skills.agentManagement.inheritedSkills')}
            </span>
          </div>
          <code title={skill.resolvedPath ?? undefined}>{skill.path}</code>
        </article>
      ))}
    </div>
  )
}

function UnmanagedSkillCollection({
  skills,
  mode,
  agentId,
  busy,
  selectable,
  selectedIds,
  adoptingIds,
  adoptingUnmanagedId,
  deletingIds,
  onToggle,
  onAdopt,
  onDelete,
  onOpenSkillDetail,
}: {
  skills: UnmanagedItemDto[]
  mode: AgentSkillViewMode
  agentId: string
  busy: boolean
  selectable: boolean
  selectedIds: Set<string>
  adoptingIds: Set<string>
  adoptingUnmanagedId: string | null
  deletingIds: Set<string>
  onToggle: (unmanagedId: string) => void
  onAdopt: (agentId: string, unmanagedId: string) => void
  onDelete: (item: UnmanagedItemDto) => void
  onOpenSkillDetail: (skillId: string, fallback?: SkillDetailFallback | null) => void
}) {
  const { t } = useTranslation()
  if (mode === 'list') {
    return (
      <div className="sm2__agent-skill-list">
        {skills.map((u) => {
          const name = u.inferredSkillId || pathBasename(u.path) || u.id
          const readOnly = isReadOnlyUnmanaged(u)
          const adopting = adoptingUnmanagedId === u.id || adoptingIds.has(u.id)
          const deleting = deletingIds.has(u.id)
          return (
            <div
              key={u.id}
              className={`sm2__object-row sm2__object-row--path sm2__object-row--clickable${selectable && selectedIds.has(u.id) ? ' sm2__object-row--selected' : ''}${adopting ? ' sm2__object-row--adopting' : ''}${deleting ? ' sm2__object-row--deleting' : ''}`}
              aria-busy={adopting || deleting || undefined}
              onClick={() => openUnmanagedSkill(u, onOpenSkillDetail)}
            >
              {selectable && !readOnly && (
                <input
                  type="checkbox"
                  className="sm2__agent-skill-select"
                  aria-label={`选择 ${name}`}
                  checked={selectedIds.has(u.id)}
                  disabled={busy}
                  onClick={(e) => e.stopPropagation()}
                  onChange={() => onToggle(u.id)}
                />
              )}
              <div>
                <strong>{name}</strong>
                <span>{unmanagedReasonLabel(t, u.reason)}{unmanagedSourceLabel(t, u) ? ` · ${unmanagedSourceLabel(t, u)}` : ''}</span>
                <code>{u.path}</code>
              </div>
              <div className="sm2__object-row-actions">
                {readOnly ? (
                  <span className="sm2__tag sm2__tag--shared">{unmanagedReasonLabel(t, u.reason)}</span>
                ) : (
                  <>
                    <ActionButton className="sm2__btn sm2__btn--primary" disabled={busy && !adopting} busy={adopting} busyLabel="准备接管" onClick={(e) => {
                      e.stopPropagation()
                      onAdopt(adoptOwnerAgentId(agentId, u), u.id)
                    }}>
                      接管
                    </ActionButton>
                    <ActionButton className="sm2__btn sm2__btn--danger" disabled={busy && !deleting} busy={deleting} busyLabel="删除中" aria-label={deleting ? `删除中 ${name}` : `删除 ${name}`} onClick={(e) => {
                      e.stopPropagation()
                      onDelete(u)
                    }}>
                      删除
                    </ActionButton>
                  </>
                )}
              </div>
            </div>
          )
        })}
      </div>
    )
  }

  return (
    <div className="sm2__agent-skill-grid">
      {skills.map((u) => {
        const name = u.inferredSkillId || pathBasename(u.path) || u.id
        const readOnly = isReadOnlyUnmanaged(u)
        const adopting = adoptingUnmanagedId === u.id || adoptingIds.has(u.id)
        const deleting = deletingIds.has(u.id)
        return (
          <article
            key={u.id}
            className={`sm2__agent-skill-card sm2__agent-skill-card--unmanaged sm2__agent-skill-card--clickable${selectable && selectedIds.has(u.id) ? ' sm2__agent-skill-card--selected' : ''}${adopting ? ' sm2__agent-skill-card--adopting' : ''}${deleting ? ' sm2__agent-skill-card--deleting' : ''}`}
            aria-busy={adopting || deleting || undefined}
            role="button"
            tabIndex={0}
            onClick={() => openUnmanagedSkill(u, onOpenSkillDetail)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') openUnmanagedSkill(u, onOpenSkillDetail)
            }}
          >
            <div className={`sm2__agent-skill-card-head${selectable ? ' sm2__agent-skill-card-head--selectable' : ''}`}>
              {selectable && !readOnly && (
                <input
                  type="checkbox"
                  className="sm2__agent-skill-select"
                  aria-label={`选择 ${name}`}
                  checked={selectedIds.has(u.id)}
                  disabled={busy}
                  onClick={(e) => e.stopPropagation()}
                  onChange={() => onToggle(u.id)}
                />
              )}
              <div className="sm2__agent-skill-icon">{initials(name || 'SK')}</div>
              <div className="sm2__agent-skill-card-titleline">
                <strong>{name}</strong>
                <span>{unmanagedReasonLabel(t, u.reason)}</span>
              </div>
              <div className="sm2__tag-row">
                {unmanagedSourceLabel(t, u) && <span className="sm2__tag sm2__tag--shared">{unmanagedSourceLabel(t, u)}</span>}
                <span className={`sm2__tag sm2__tag--${readOnly ? 'shared' : 'unmanaged'}`}>
                  {readOnly ? unmanagedReasonLabel(t, u.reason) : '未管理'}
                </span>
              </div>
            </div>
            <code>{u.path}</code>
            {!readOnly && (
              <div className="sm2__agent-skill-card-actions">
                <ActionButton className="sm2__btn sm2__btn--primary" disabled={busy && !adopting} busy={adopting} busyLabel="准备接管" onClick={(e) => {
                  e.stopPropagation()
                  onAdopt(adoptOwnerAgentId(agentId, u), u.id)
                }}>
                  接管
                </ActionButton>
                <ActionButton className="sm2__btn sm2__btn--danger" disabled={busy && !deleting} busy={deleting} busyLabel="删除中" aria-label={deleting ? `删除中 ${name}` : `删除 ${name}`} onClick={(e) => {
                  e.stopPropagation()
                  onDelete(u)
                }}>
                  删除
                </ActionButton>
              </div>
            )}
          </article>
        )
      })}
    </div>
  )
}

function toggleSetValue(current: Set<string>, value: string) {
  const next = new Set(current)
  if (next.has(value)) next.delete(value)
  else next.add(value)
  return next
}

function hasSkillPackClaim(skill: AgentDetail['skills'][number]) {
  return skill.claims.some((claim) => claim.claimType === 'pack')
}

function uniqueValues(values: string[]) {
  return Array.from(new Set(values.filter(Boolean)))
}

function packOptionsForBatchAdopt(detail: AgentDetail, packs: BatchAdoptPackOption[]) {
  const byId = new Map<string, BatchAdoptPackOption>()
  const add = (pack: BatchAdoptPackOption) => {
    if (!pack.id || pack.id === 'default' || byId.has(pack.id)) return
    byId.set(pack.id, pack)
  }
  for (const pack of packs) add(pack)
  for (const pack of detail.availablePacks) add(pack)
  for (const pack of detail.appliedPacks) {
    add({
      id: pack.packId,
      name: pack.packName,
      description: '',
      memberCount: pack.memberCount,
    })
  }
  return Array.from(byId.values()).sort((a, b) => a.name.localeCompare(b.name))
}

function packOptionsForMove(detail: AgentDetail, packs: SkillPackSummary[]) {
  const appliedIds = new Set(detail.appliedPacks.map((pack) => pack.packId))
  const byId = new Map<string, MoveToPackOption>()
  const add = (pack: BatchAdoptPackOption) => {
    if (!pack.id || byId.has(pack.id)) return
    byId.set(pack.id, {
      ...pack,
      applied: appliedIds.has(pack.id),
      isDefault: pack.id === 'default',
    })
  }
  for (const pack of packs) add(pack)
  for (const pack of detail.availablePacks) add(pack)
  for (const pack of detail.appliedPacks) {
    add({
      id: pack.packId,
      name: pack.packName,
      description: '',
      memberCount: pack.memberCount,
    })
  }
  return Array.from(byId.values()).sort((left, right) => {
    if (left.isDefault !== right.isDefault) return left.isDefault ? 1 : -1
    if (left.applied !== right.applied) return left.applied ? -1 : 1
    return left.name.localeCompare(right.name)
  })
}

function skillInstalledOnAgent(skill: SkillSummary, agentId: string) {
  return skill.installedAgents.some((agent) => agent.agentId === agentId)
}

function installBlockerKey(blocker: ConflictBlocker) {
  return `${blocker.skillId}\u0000${blocker.agentId}`
}

function isManagedCopyBlocker(blocker: ConflictBlocker) {
  return blocker.reason.startsWith('Managed copy ')
}

function defaultAgentDetailAdoptMode(item: UnmanagedItemDto) {
  if (item.agentId === SHARED_SKILLS_AGENT_ID) return 'import_cleanup'
  return isSharedAgentsSkillsPath(item.path) ? 'import_link' : 'import_keep'
}

function adoptOwnerAgentId(detailAgentId: string, item: UnmanagedItemDto) {
  return item.agentId || detailAgentId
}

function isSharedAgentsUnmanaged(item: UnmanagedItemDto) {
  return item.agentId === SHARED_SKILLS_AGENT_ID || isSharedAgentsSkillsPath(item.path)
}

function isReadOnlyUnmanaged(item: UnmanagedItemDto) {
  return item.readOnly === true || item.reason === 'agent_builtin_read_only'
}

function unmanagedSourceLabel(t: Parameters<typeof unmanagedReasonLabel>[0], item: UnmanagedItemDto) {
  return isSharedAgentsUnmanaged(item) ? unmanagedReasonLabel(t, 'shared_agents_directory') : ''
}

function isSharedAgentsSkillsPath(path: string) {
  const parts = path.split(/[\\/]+/)
  return parts.some((part, index) => part === '.agents' && parts[index + 1] === 'skills')
}

function openUnmanagedSkill(
  item: UnmanagedItemDto,
  onOpenSkillDetail: (skillId: string, fallback?: SkillDetailFallback | null) => void,
) {
  const name = item.inferredSkillId || pathBasename(item.path) || item.id
  onOpenSkillDetail(name, {
    id: name,
    name,
    centerPath: item.path,
    currentHash: item.hash,
    sourceType: 'unmanaged_agent',
    sourceUri: item.path,
  })
}

function openInheritedSkill(
  item: InheritedSkillDetail,
  onOpenSkillDetail: (skillId: string, fallback?: SkillDetailFallback | null) => void,
) {
  onOpenSkillDetail(item.skillId, {
    id: item.skillId,
    name: item.skillId,
    centerPath: item.path,
    sourceType: 'unmanaged_agent',
    sourceUri: item.resolvedPath || item.path,
  })
}

function pathBasename(path: string) {
  return path.split(/[\\/]+/).filter(Boolean).pop() || ''
}

function initials(value: string) {
  return value
    .split(/[-_\s]+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0])
    .join('')
    .toUpperCase()
    .slice(0, 2) || 'SK'
}

function McpTab({ detail }: { detail: AgentDetail }) {
  return <McpManagementTab detail={detail} />
}

function HooksTab({ detail, program }: { detail: AgentDetail; program: AgentProgramInfo | null }) {
  const agentId = detail.id
  const [hook, setHook] = useState<HookStatus | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const load = async () => {
    setLoading(true)
    try {
      const all = await getAllHookStatus()
      const found = all.find((h) => hookMatchesAgent(h, detail, program))
      setHook(found || null)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agentId])

  const install = async () => {
    setBusy(true)
    setError(null)
    try {
      await installAgentHook(hook?.toolId || agentId)
      await load()
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  const toggleEvent = async (eventName: string, enabled: boolean) => {
    if (!hook?.events) return
    setBusy(true)
    setError(null)
    try {
      const next = hook.events
        .filter((event) => (event.name === eventName ? enabled : event.enabled))
        .map((event) => event.name)
      await configureAgentHookEvents(hook.toolId || agentId, next)
      await load()
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  const uninstall = async () => {
    if (!confirm(`移除 ${agentId} 的 Hook？`)) return
    setBusy(true)
    setError(null)
    try {
      await uninstallAgentHook(hook?.toolId || agentId)
      await load()
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  if (loading) return <div className="sm2__empty sm2__empty--compact">加载 Hook 状态…</div>
  if (!hook) return <div className="sm2__empty sm2__empty--compact">该 Agent 暂不支持 Hook 接入</div>

  const statusLabel = hook.installStatus === 'installed'
    ? '已安装'
    : hook.installStatus === 'needs_reinstall'
      ? '需重新安装'
      : hook.installStatus === 'settings_corrupted'
        ? '配置异常'
        : '未安装'

  const grouped = groupHookEvents(hook.events || [])

  return (
    <div className="sm2__hooks-workspace">
      <section className="sm2__panel sm2__hook-summary">
        <div className="sm2__hook-summary-main">
          <strong>Hook 接入</strong>
          <span>{statusLabel}</span>
          {hook.configPath && <code>{hook.configPath}</code>}
        </div>
        <div className="sm2__btn-row sm2__hook-actions">
          {hook.configPath && (
            <button className="sm2__btn sm2__btn--ghost" disabled={busy} onClick={() => skillApiV2.openPath(hook.configPath!)}>
              打开配置
            </button>
          )}
          {hook.configDir && (
            <button className="sm2__btn sm2__btn--ghost" disabled={busy} onClick={() => skillApiV2.openPath(hook.configDir!)}>
              打开目录
            </button>
          )}
          {hook.installed ? (
            <ActionButton className="sm2__btn sm2__btn--danger" disabled={busy} busy={busy} busyLabel="正在卸载" onClick={uninstall}>卸载 Hook</ActionButton>
          ) : (
            <ActionButton className="sm2__btn sm2__btn--primary" disabled={busy} busy={busy} busyLabel="正在安装" onClick={install}>安装 Hook</ActionButton>
          )}
        </div>
      </section>
      {(hook.bridgeCommand || hook.bridgePath) && (
        <section className="sm2__panel sm2__hook-bridge">
          <div className="sm2__panel-head">
            <h3>桥接命令</h3>
            {hook.bridgePath && (
              <button className="sm2__btn sm2__btn--ghost" disabled={busy} onClick={() => skillApiV2.openPath(hook.bridgePath!)}>
                打开脚本
              </button>
            )}
          </div>
          {hook.bridgeCommand && <code>{hook.bridgeCommand}</code>}
          {hook.bridgePath && <span>脚本路径：{hook.bridgePath}</span>}
        </section>
      )}
      {hook.supportsEventSelection && grouped.length > 0 ? (
        grouped.map((group) => (
          <section key={group.category} className="sm2__panel sm2__hook-group">
            <div className="sm2__panel-head sm2__hook-group-head">
              <div>
                <h3>{group.title}</h3>
                <p>{group.subtitle}</p>
              </div>
              <span>{group.events.filter((event) => event.enabled).length}/{group.events.length} 已启用</span>
            </div>
            <div className="sm2__hook-event-list">
              {group.events.map((event) => (
                <HookEventRow
                  key={event.name}
                  event={event}
                  installed={hook.installed}
                  configPath={hook.configPath}
                  busy={busy}
                  onToggle={toggleEvent}
                />
              ))}
            </div>
          </section>
        ))
      ) : (
        <section className="sm2__panel">
          <div className="sm2__empty sm2__empty--compact">该 Agent 没有暴露可查看的 Hook 事件配置</div>
        </section>
      )}
      {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
    </div>
  )
}

function hookMatchesAgent(hook: HookStatus, detail: AgentDetail, program: AgentProgramInfo | null) {
  const agentId = detail.id
  if (hook.adapterId === agentId || hook.toolId === agentId || hook.name === agentId) return true
  const displayName = detail.displayName.toLowerCase()
  if (hook.isCustom && hook.displayName.toLowerCase() === displayName) return true
  const hookValues = [
    hook.displayName,
    hook.name,
    hook.toolId,
    hook.adapterId,
    hook.profileId,
    hook.configPath,
    hook.configDir,
    hook.bridgeCommand,
  ].map((value) => String(value || '').toLowerCase())
  const targetValues = [
    agentId,
    detail.displayName,
    detail.configPath,
    detail.mcpConfigPath,
    detail.pluginDir,
    detail.skillsDir,
    program?.configDir,
    program?.skillsDir,
  ].map((value) => String(value || '').toLowerCase()).filter(Boolean)
  return hookValues.some((hookValue) =>
    targetValues.some((target) => hookValue === target || hookValue.includes(target)),
  )
}

function groupHookEvents(events: HookEventStatus[]) {
  const order = ['approvals', 'notifications', 'lifecycle', 'activity']
  return order
    .map((category) => {
      const groupEvents = events.filter((event) => event.category === category)
      return {
        category,
        title: groupEvents[0]?.categoryTitle || category,
        subtitle: groupEvents[0]?.categorySubtitle || '',
        events: groupEvents,
      }
    })
    .filter((group) => group.events.length > 0)
}

function HookEventRow({
  event,
  installed,
  configPath,
  busy,
  onToggle,
}: {
  event: HookEventStatus
  installed: boolean
  configPath?: string
  busy: boolean
  onToggle: (eventName: string, enabled: boolean) => void
}) {
  return (
    <div className="sm2__hook-event-row">
      <label className="sm2__hook-event-toggle">
        <input
          type="checkbox"
          checked={event.enabled}
          disabled={busy}
          onChange={(e) => onToggle(event.name, e.target.checked)}
        />
        <span />
      </label>
      <div className="sm2__hook-event-main">
        <div className="sm2__hook-event-title">
          <strong>{event.name}</strong>
          <span className={`sm2__tag sm2__tag--${event.enabled ? 'ok' : 'unmanaged'}`}>
            {event.enabled ? '启用' : '停用'}
          </span>
        </div>
        <div className="sm2__hook-event-hooks">
          <span>{installed && event.enabled ? 'AgentBro Bridge' : '未生效'}</span>
          {typeof event.timeout === 'number' && <span>timeout {event.timeout}s</span>}
        </div>
        {configPath && <code>{configPath}</code>}
      </div>
    </div>
  )
}

type ConfigResourceKind = 'application' | 'directory' | 'executable' | 'file' | 'link'
type ConfigResource = {
  id: string
  label: string
  value: string
  kind: ConfigResourceKind
  editable: boolean
}

function ConfigTab({ detail, program }: { detail: AgentDetail; program: AgentProgramInfo | null }) {
  const { t } = useTranslation()
  const currentVersion = program?.installedVersion ?? detail.version
  const latestVersion = program?.latestVersion ?? detail.latestVersion
  const executablePath = program?.appPath ?? program?.binaryPath ?? null
  const executableIsApp = Boolean(program?.appPath)
  const skillsDir = detail.skillsDir ?? program?.skillsDir ?? null
  const [editorTarget, setEditorTarget] = useState<ConfigResource | null>(null)
  const [actionError, setActionError] = useState<string | null>(null)
  const [actionNotice, setActionNotice] = useState<string | null>(null)
  const resources = useMemo(() => {
    const candidates: Array<Omit<ConfigResource, 'value' | 'editable'> & { value: string | null; editable?: boolean }> = [
      {
        id: 'executable',
        label: executableIsApp
          ? t('skills.agentManagement.pathSettings.resources.application')
          : t('skills.agentManagement.pathSettings.resources.executable'),
        value: executablePath,
        kind: executableIsApp ? 'application' : 'executable',
      },
      {
        id: 'config-directory',
        label: t('skills.agentManagement.pathSettings.resources.configDirectory'),
        value: program?.configDir ?? null,
        kind: 'directory',
      },
      {
        id: 'config-file',
        label: t('skills.agentManagement.pathSettings.resources.configFile'),
        value: detail.configPath,
        kind: looksLikeWebUrl(detail.configPath) ? 'link' : 'file',
        editable: isEditableConfigFile(detail.configPath),
      },
      {
        id: 'skills-directory',
        label: t('skills.agentManagement.pathSettings.resources.skillsDirectory'),
        value: skillsDir,
        kind: 'directory',
      },
      {
        id: 'mcp-config',
        label: t('skills.agentManagement.pathSettings.resources.mcpConfig'),
        value: detail.mcpConfigPath,
        kind: looksLikeWebUrl(detail.mcpConfigPath) ? 'link' : 'file',
        editable: isEditableConfigFile(detail.mcpConfigPath),
      },
      {
        id: 'plugin-directory',
        label: t('skills.agentManagement.pathSettings.resources.pluginDirectory'),
        value: detail.pluginDir,
        kind: 'directory',
      },
      {
        id: 'agent-directory',
        label: t('skills.agentManagement.pathSettings.resources.agentDirectory'),
        value: detail.agentDir ?? null,
        kind: 'directory',
      },
    ]
    return candidates
      .filter((resource): resource is Omit<ConfigResource, 'editable'> & { editable?: boolean } => Boolean(resource.value))
      .map((resource) => ({ ...resource, editable: resource.editable ?? false }))
  }, [
    detail.agentDir,
    detail.configPath,
    detail.mcpConfigPath,
    detail.pluginDir,
    executableIsApp,
    executablePath,
    program?.configDir,
    skillsDir,
    t,
  ])

  const runResourceAction = async (action: () => Promise<void>, notice: string) => {
    setActionError(null)
    setActionNotice(null)
    try {
      await action()
      setActionNotice(notice)
    } catch (error) {
      setActionError(skillErrorMessage(t, error))
    }
  }

  const openResource = (resource: ConfigResource) => runResourceAction(
    () => resource.kind === 'directory' || resource.kind === 'link'
      ? skillApiV2.openPath(resource.value)
      : skillApiV2.revealPath(resource.value),
    resource.kind === 'directory'
      ? t('skills.agentManagement.pathSettings.notices.directoryOpened', { name: resource.label })
      : t('skills.agentManagement.pathSettings.notices.resourceLocated', { name: resource.label }),
  )

  return (
    <>
      <section className="sm2__panel sm2__config-panel">
        <div className="sm2__config-panel-head">
          <div>
            <h3>{t('skills.agentManagement.pathSettings.title')}</h3>
            <p>{t('skills.agentManagement.pathSettings.description')}</p>
          </div>
        </div>
        <div className="sm2__config-facts">
          <div>
            <span>{t('skills.agentManagement.pathSettings.agentId')}</span>
            <strong>{detail.id}</strong>
          </div>
          <div>
            <span>{t('skills.agentManagement.pathSettings.currentVersion')}</span>
            <strong>{currentVersion || t('skills.agentManagement.pathSettings.notDetected')}</strong>
          </div>
          <div>
            <span>{t('skills.agentManagement.pathSettings.latestVersion')}</span>
            <strong>{latestVersion || t('skills.agentManagement.pathSettings.notDetected')}</strong>
          </div>
        </div>
        <div className="sm2__config-resource-heading">
          <strong>{t('skills.agentManagement.pathSettings.localResources')}</strong>
          <span>{t('skills.agentManagement.pathSettings.resourceCount', { count: resources.length })}</span>
        </div>
        <div className="sm2__config-resource-list">
          {resources.length === 0 && (
            <div className="sm2__config-empty">
              <strong>{t('skills.agentManagement.pathSettings.emptyTitle')}</strong>
              <span>{t('skills.agentManagement.pathSettings.emptyDescription')}</span>
            </div>
          )}
          {resources.map((resource) => (
            <div key={resource.id} className="sm2__config-resource">
              <span className="sm2__config-resource-type">{configResourceBadge(resource)}</span>
              <div className="sm2__config-resource-main">
                <strong>{resource.label}</strong>
                <code title={resource.value}>{resource.value}</code>
              </div>
              <div className="sm2__config-resource-actions">
                {resource.editable && (
                  <button
                    type="button"
                    className="sm2__btn sm2__btn--primary sm2__config-resource-primary"
                    onClick={() => {
                      setActionError(null)
                      setEditorTarget(resource)
                    }}
                  >
                    {t('skills.agentManagement.pathSettings.actions.edit')}
                  </button>
                )}
                <button
                  type="button"
                  className="sm2__btn sm2__btn--ghost"
                  onClick={() => void openResource(resource)}
                >
                  {resource.kind === 'directory'
                    ? t('skills.agentManagement.pathSettings.actions.openDirectory')
                    : resource.kind === 'link'
                      ? t('skills.agentManagement.pathSettings.actions.openLink')
                      : t('skills.agentManagement.pathSettings.actions.reveal')}
                </button>
              </div>
            </div>
          ))}
        </div>
        {(actionError || actionNotice) && (
          <div
            className={`sm2__config-action-status${actionError ? ' sm2__config-action-status--error' : ''}`}
            role={actionError ? 'alert' : 'status'}
          >
            {actionError || actionNotice}
          </div>
        )}
      </section>
      {editorTarget && (
        <ConfigFileEditorDialog
          agentId={detail.id}
          resource={editorTarget}
          onClose={() => setEditorTarget(null)}
          onSaved={() => {
            setActionError(null)
            setActionNotice(t('skills.agentManagement.pathSettings.editor.saved'))
          }}
        />
      )}
    </>
  )
}

function ConfigFileEditorDialog({
  agentId,
  resource,
  onClose,
  onSaved,
}: {
  agentId: string
  resource: ConfigResource
  onClose: () => void
  onSaved: () => void
}) {
  const { t } = useTranslation()
  const [configDocument, setConfigDocument] = useState<AgentConfigDocument | null>(null)
  const [draft, setDraft] = useState('')
  const [baseline, setBaseline] = useState('')
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [saved, setSaved] = useState(false)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const lineNumbersRef = useRef<HTMLDivElement>(null)
  const isJson = /\.json$/i.test(resource.value)
  const dirty = configDocument !== null && draft !== baseline
  const jsonValid = useMemo(() => {
    if (!isJson) return true
    try {
      JSON.parse(draft)
      return true
    } catch {
      return false
    }
  }, [draft, isJson])

  const loadDocument = useCallback(async () => {
    setLoading(true)
    setError(null)
    setSaved(false)
    try {
      const next = await skillApiV2.readAgentConfigFile(agentId, resource.value)
      const prepared = prepareConfigEditorContent(next.content, resource.value)
      setConfigDocument(next)
      setDraft(prepared)
      setBaseline(prepared)
    } catch (nextError) {
      setConfigDocument(null)
      setError(skillErrorMessage(t, nextError))
    } finally {
      setLoading(false)
    }
  }, [agentId, resource.value, t])

  useEffect(() => {
    void loadDocument()
  }, [loadDocument])

  const requestClose = () => {
    if (saving) return
    if (dirty && !window.confirm(t('skills.agentManagement.pathSettings.editor.discardConfirm'))) return
    onClose()
  }

  const save = async () => {
    if (!configDocument || !dirty || saving || !jsonValid) return
    setSaving(true)
    setError(null)
    setSaved(false)
    try {
      const next = await skillApiV2.writeAgentConfigFile(
        agentId,
        configDocument.path,
        draft,
        configDocument.revision,
      )
      const prepared = prepareConfigEditorContent(next.content, resource.value)
      setConfigDocument(next)
      setDraft(prepared)
      setBaseline(prepared)
      setSaved(true)
      onSaved()
    } catch (nextError) {
      setError(skillErrorMessage(t, nextError))
    } finally {
      setSaving(false)
    }
  }

  const format = resource.value.split('.').pop()?.toUpperCase() || 'TEXT'
  const lineCount = Math.max(1, draft.split(/\r?\n/).length)

  const formatJson = () => {
    if (!isJson || !jsonValid) return
    setDraft(formatJsonContent(draft))
    setSaved(false)
    textareaRef.current?.focus()
  }

  return (
    <PreviewDialog
      title={t('skills.agentManagement.pathSettings.editor.title', { name: resource.label })}
      modalClassName="sm2__modal--config-editor"
      busy={saving}
      onCancel={requestClose}
      onConfirm={() => void save()}
      actions={(
        <>
          <div className="sm2__config-editor-action-status" aria-live="polite">
            {saved && t('skills.agentManagement.pathSettings.editor.saved')}
          </div>
          <button type="button" className="sm2__btn" disabled={saving} onClick={requestClose}>
            {t('skills.agentManagement.pathSettings.editor.close')}
          </button>
          <button
            type="button"
            className="sm2__btn sm2__btn--primary"
            disabled={!dirty || saving || loading || !configDocument || !jsonValid}
            aria-busy={saving || undefined}
            onClick={() => void save()}
          >
            {saving && <span className="sm2__spinner" aria-hidden="true" />}
            <span>{saving
              ? t('skills.agentManagement.pathSettings.editor.saving')
              : t('skills.agentManagement.pathSettings.editor.save')}</span>
          </button>
        </>
      )}
    >
      <div className="sm2__config-editor-toolbar">
        <div className="sm2__config-editor-path">
          <span>{format}</span>
          <code title={resource.value}>{resource.value}</code>
        </div>
        {isJson && (
          <button
            type="button"
            className="sm2__btn sm2__btn--ghost sm2__config-editor-format"
            disabled={!jsonValid || loading}
            onClick={formatJson}
          >
            <span aria-hidden="true">{'{ }'}</span>
            {t('skills.agentManagement.pathSettings.editor.formatJson')}
          </button>
        )}
      </div>
      {loading && (
        <div className="sm2__config-editor-state">
          <span className="sm2__spinner" aria-hidden="true" />
          {t('skills.agentManagement.pathSettings.editor.loading')}
        </div>
      )}
      {!loading && error && !configDocument && (
        <div className="sm2__config-editor-state sm2__config-editor-state--error">
          <span>{error}</span>
          <button type="button" className="sm2__btn" onClick={() => void loadDocument()}>
            {t('skills.agentManagement.pathSettings.editor.retry')}
          </button>
        </div>
      )}
      {!loading && configDocument && (
        <>
          <div className="sm2__config-editor-surface">
            <div ref={lineNumbersRef} className="sm2__config-editor-line-numbers" aria-hidden="true">
              {Array.from({ length: lineCount }, (_, index) => (
                <span key={index}>{index + 1}</span>
              ))}
            </div>
            <textarea
              ref={textareaRef}
              className="sm2__config-editor-textarea"
              value={draft}
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              autoFocus
              aria-invalid={isJson && !jsonValid}
              aria-label={t('skills.agentManagement.pathSettings.editor.textareaLabel', { name: resource.label })}
              onChange={(event) => {
                setDraft(event.target.value)
                setSaved(false)
              }}
              onScroll={(event) => {
                if (lineNumbersRef.current) {
                  lineNumbersRef.current.scrollTop = event.currentTarget.scrollTop
                }
              }}
              onKeyDown={(event) => {
                if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 's') {
                  event.preventDefault()
                  void save()
                  return
                }
                if (event.key === 'Tab' && !event.shiftKey) {
                  event.preventDefault()
                  const target = event.currentTarget
                  const next = `${draft.slice(0, target.selectionStart)}  ${draft.slice(target.selectionEnd)}`
                  const cursor = target.selectionStart + 2
                  setDraft(next)
                  setSaved(false)
                  requestAnimationFrame(() => {
                    textareaRef.current?.setSelectionRange(cursor, cursor)
                  })
                }
              }}
            />
          </div>
          <div className="sm2__config-editor-statusbar">
            <span
              className={`sm2__config-editor-syntax${isJson && !jsonValid ? ' sm2__config-editor-syntax--invalid' : ''}`}
              role="status"
            >
              <i aria-hidden="true" />
              {isJson
                ? t(`skills.agentManagement.pathSettings.editor.${jsonValid ? 'validJson' : 'invalidJson'}`)
                : t('skills.agentManagement.pathSettings.editor.tomlValidation')}
            </span>
            <span>{t('skills.agentManagement.pathSettings.editor.lines', { count: lineCount })}</span>
            <span>{dirty
              ? t('skills.agentManagement.pathSettings.editor.unsaved')
              : t('skills.agentManagement.pathSettings.editor.savedState')}</span>
            <span className="sm2__config-editor-shortcuts">
              <kbd>Tab</kbd>
              <kbd>⌘S</kbd>
            </span>
          </div>
        </>
      )}
      {error && configDocument && (
        <div className="sm2__config-editor-inline-error" role="alert">{error}</div>
      )}
      <p className="sm2__config-editor-backup-note">
        {t('skills.agentManagement.pathSettings.editor.backupNote')}
      </p>
    </PreviewDialog>
  )
}

function looksLikeWebUrl(value: string | null | undefined) {
  return Boolean(value && /^https?:\/\//i.test(value))
}

function isEditableConfigFile(value: string | null | undefined) {
  return Boolean(value && !looksLikeWebUrl(value) && /\.(json|toml)$/i.test(value))
}

function configResourceBadge(resource: ConfigResource) {
  if (resource.kind === 'directory') return 'DIR'
  if (resource.kind === 'application') return 'APP'
  if (resource.kind === 'executable') return 'BIN'
  if (resource.kind === 'link') return 'URL'
  return resource.value.split('.').pop()?.toUpperCase() || 'FILE'
}

function prepareConfigEditorContent(content: string, path: string) {
  if (!/\.json$/i.test(path)) return content
  try {
    return formatJsonContent(content)
  } catch {
    return content
  }
}

function formatJsonContent(content: string) {
  JSON.parse(content)
  const source = content.trim()
  let formatted = ''
  let depth = 0
  let inString = false
  let escaped = false

  for (let index = 0; index < source.length; index += 1) {
    const character = source[index]

    if (inString) {
      formatted += character
      if (escaped) {
        escaped = false
      } else if (character === '\\') {
        escaped = true
      } else if (character === '"') {
        inString = false
      }
      continue
    }

    if (character === '"') {
      inString = true
      formatted += character
      continue
    }
    if (/\s/.test(character)) continue

    if (character === '{' || character === '[') {
      formatted += character
      let nextIndex = index + 1
      while (nextIndex < source.length && /\s/.test(source[nextIndex])) nextIndex += 1
      const closesImmediately = (character === '{' && source[nextIndex] === '}')
        || (character === '[' && source[nextIndex] === ']')
      if (!closesImmediately) {
        depth += 1
        formatted += `\n${'  '.repeat(depth)}`
      }
      continue
    }

    if (character === '}' || character === ']') {
      let previousIndex = index - 1
      while (previousIndex >= 0 && /\s/.test(source[previousIndex])) previousIndex -= 1
      const closesEmptyValue = (character === '}' && source[previousIndex] === '{')
        || (character === ']' && source[previousIndex] === '[')
      if (!closesEmptyValue) {
        depth = Math.max(0, depth - 1)
        formatted += `\n${'  '.repeat(depth)}`
      }
      formatted += character
      continue
    }

    if (character === ',') {
      formatted += `,\n${'  '.repeat(depth)}`
      continue
    }

    formatted += character === ':' ? ': ' : character
  }

  return `${formatted}\n`
}
