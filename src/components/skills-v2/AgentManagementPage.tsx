import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { ButtonHTMLAttributes, CSSProperties, ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { useSkillStoreV2 } from '../../stores/skillStoreV2'
import { skillApiV2 } from '../../services/skillApiV2'
import { agentApi, type AgentOutputEvent, type AgentProgramInfo } from '../../services/agentApi'
import { configureAgentHookEvents, getAllHookStatus, installAgentHook, uninstallAgentHook, type HookEventStatus, type HookStatus } from '../../services/tauriApi'
import type { AgentDetail, AdoptPreview, ConflictBlocker, DistributionBlockerDecision, DistributionPreview, SkillSummary, UnmanagedItemDto } from '../../services/skillApiV2'
import { AgentIconBadge } from './AgentIconBadge'
import { AdoptDialog } from './AdoptDialog'
import { PreviewDialog } from './PreviewDialog'
import { SkillDetailSlider, type SkillDetailFallback } from './SkillDetailSlider'
import { skillModeLabel, skillSourceTypeLabel, skillStatusLabel, targetClaimLabel, unmanagedReasonLabel } from './skillLabels'

type DetailTab = 'overview' | 'skills' | 'mcp' | 'plugins' | 'hooks' | 'config'
type AgentSkillViewMode = 'cards' | 'list'
type AgentSkillScope = 'managed' | 'unmanaged'
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

const PAGE_SIZE = 28
const SHARED_SKILLS_AGENT_ID = 'agents'
const NOTICE_DISMISS_MS = 3200
const PACK_PROGRESS_DONE_DISMISS_MS = 2400

export function AgentManagementPage() {
  const state = useSkillStoreV2()
  const agents = useMemo(() => state.agents.filter((agent) => agent.id !== SHARED_SKILLS_AGENT_ID), [state.agents])
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
  const [openingAgentId, setOpeningAgentId] = useState<string | null>(null)
  const [adoptingUnmanagedId, setAdoptingUnmanagedId] = useState<string | null>(null)
  const [programs, setPrograms] = useState<Record<string, AgentProgramInfo>>({})
  const [programLoading, setProgramLoading] = useState(false)
  const [agentOutput, setAgentOutput] = useState<string[]>([])
  const [notice, setNotice] = useState<string | null>(null)
  const [packApplyConflict, setPackApplyConflict] = useState<PackApplyConflictDialogState | null>(null)
  const [packApplyBusy, setPackApplyBusy] = useState(false)
  const selectedAgentIdRef = useRef<string | null>(null)
  const packApplyResolverRef = useRef<((applied: boolean) => void) | null>(null)
  const actionBusy = busy || refreshingOverview || refreshingAll || scanningAgentId !== null || updatingAgentId !== null || installingAgentId !== null || openingAgentId !== null

  const loadPrograms = useCallback(async () => {
    setProgramLoading(true)
    try {
      const next = await agentApi.refresh()
      setPrograms(Object.fromEntries(next.map((agent) => [agent.id, agent])))
    } catch (e) {
      state.setError(String(e))
    } finally {
      setProgramLoading(false)
    }
  }, [state])

  useEffect(() => {
    state.loadOverview()
    loadPrograms()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

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
      setNotice(`扫描完成：已管理 ${result.managed}，未管理 ${result.unmanaged}`)
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
      await state.loadOverview(true)
    } finally {
      setRefreshingOverview(false)
    }
  }

  const openSkillDetail = (skillId: string, fallback?: SkillDetailFallback | null) => {
    setDetailSkillId(skillId)
    setDetailFallback(fallback || null)
  }

  return (
    <div className="sm2 sm2--agents">
      <div className="sm2__header sm2__header--stacked">
        <div>
          <h2 className="sm2__title">Agent 管理</h2>
          <p className="sm2__header-subtitle">查看每个 Agent 的 Skills、技能包、MCP、插件与 Hook 状态。</p>
        </div>
        <div className="sm2__tabs">
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
          onClose={() => setAdopt(null)}
          onDone={async () => {
            const agentId = adopt.agentId
            setAdopt(null)
            const unmanaged = await skillApiV2.listUnmanaged()
            useSkillStoreV2.setState({ unmanaged })
            await state.loadAgentDetail(agentId, true)
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
    </div>
  )
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
}) {
  const showUnmanaged = useSkillStoreV2((s) => s.settings?.showUnmanaged ?? true)
  const unmanaged = useSkillStoreV2((s) => s.unmanaged).filter((u) => showUnmanaged && u.agentId === detail.id)
  const installed = program ? program.status === 'installed' || program.status === 'updateAvailable' : true
  const canInstall = Boolean(program?.installCommand)
  const canOpenDownload = Boolean(program?.downloadUrl)
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
    { id: 'skills', label: `Skills (${detail.skills.length + unmanaged.length})` },
    { id: 'mcp', label: `MCP (${detail.mcpServers.length})` },
    { id: 'plugins', label: `Plugins (${detail.plugins.length})` },
    { id: 'hooks', label: 'Hooks' },
    { id: 'config', label: '路径与设置' },
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
        {tab === 'overview' && <OverviewTab detail={detail} busy={busy} onRevoke={onRevoke} onApplyPack={onApplyPack} />}
        {tab === 'skills' && (
          <SkillsTab
            detail={detail}
            unmanaged={unmanaged}
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
        {tab === 'plugins' && <PluginsTab detail={detail} />}
        {tab === 'hooks' && <HooksTab agentId={detail.id} />}
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
  onRevoke,
  onApplyPack,
}: {
  detail: AgentDetail
  busy: boolean
  onRevoke: (packId: string, agentId: string) => void
  onApplyPack: (packId: string, agentId: string) => boolean | Promise<boolean>
}) {
  const appliedIds = new Set(detail.appliedPacks.map((p) => p.packId))
  const available = detail.availablePacks.filter((p) => !appliedIds.has(p.id))
  return (
    <div className="sm2__two-col">
      <section className="sm2__panel">
        <div className="sm2__panel-head">
          <h3>已应用技能包</h3>
          <span>{detail.appliedPacks.length}</span>
        </div>
        {detail.appliedPacks.length === 0 ? (
          <div className="sm2__empty sm2__empty--compact">暂未应用技能包</div>
        ) : (
          detail.appliedPacks.map((p) => (
            <div key={p.packId} className="sm2__object-row">
              <div>
                <strong>{p.packName}</strong>
                <span>{p.memberCount} 个 Skill</span>
              </div>
              <button className="sm2__btn sm2__btn--danger" disabled={busy} onClick={() => onRevoke(p.packId, detail.id)}>
                撤销
              </button>
            </div>
          ))
        )}
      </section>
      <section className="sm2__panel">
        <div className="sm2__panel-head">
          <h3>可应用技能包</h3>
          <span>{available.length}</span>
        </div>
        {available.length === 0 ? (
          <div className="sm2__empty sm2__empty--compact">没有更多可应用技能包</div>
        ) : (
          available.map((p) => (
            <div key={p.id} className="sm2__object-row">
              <div>
                <strong>{p.name}</strong>
                <span>{p.memberCount} 个 Skill · {p.appliedAgentCount} 个 Agent 已用</span>
              </div>
              <button className="sm2__btn sm2__btn--primary" disabled={busy || p.memberCount === 0} onClick={() => onApplyPack(p.id, detail.id)}>
                应用
              </button>
            </div>
          ))
        )}
      </section>
    </div>
  )
}

function SkillsTab({
  detail,
  unmanaged,
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
  const [selectedManagedIds, setSelectedManagedIds] = useState<Set<string>>(() => new Set())
  const [selectedUnmanagedIds, setSelectedUnmanagedIds] = useState<Set<string>>(() => new Set())
  const [managedSelectionMode, setManagedSelectionMode] = useState(false)
  const [unmanagedSelectionMode, setUnmanagedSelectionMode] = useState(false)
  const [installDialogOpen, setInstallDialogOpen] = useState(false)
  const [deletingIds, setDeletingIds] = useState<Set<string>>(() => new Set())
  const [adoptingIds, setAdoptingIds] = useState<Set<string>>(() => new Set())
  const [batchDeleteTargets, setBatchDeleteTargets] = useState<AgentDetail['skills'] | null>(null)
  const [batchAdoptItems, setBatchAdoptItems] = useState<UnmanagedItemDto[] | null>(null)
  const [confirmingPackApply, setConfirmingPackApply] = useState(false)
  const [packApplyProgress, setPackApplyProgress] = useState<PackApplyProgress | null>(null)
  const [localNotice, setLocalNotice] = useState<string | null>(null)
  const q = query.trim().toLowerCase()
  const filteredManaged = useMemo(() => {
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
  const filteredUnmanaged = useMemo(() => {
    if (!q) return unmanaged
    return unmanaged.filter((u) =>
      [u.inferredSkillId, u.path, u.reason, unmanagedReasonLabel(t, u.reason)]
        .filter(Boolean)
        .join(' ')
        .toLowerCase()
        .includes(q),
    )
  }, [q, t, unmanaged])
  const [pageResetKey, setPageResetKey] = useState(`${q}|${detail.id}`)
  const currentResetKey = `${q}|${detail.id}`
  if (pageResetKey !== currentResetKey) {
    setPageResetKey(currentResetKey)
    if (page !== 1) setPage(1)
  }
  const shownUnmanaged = filteredUnmanaged.slice(0, page * PAGE_SIZE)
  const canManageUnmanaged = filteredUnmanaged
  const managedDeleting = deletingIds.size > 0
  const unmanagedAdopting = adoptingIds.size > 0
  const actionBusy = busy || managedDeleting || unmanagedAdopting

  useEffect(() => {
    setManagedSelectionMode(false)
    setUnmanagedSelectionMode(false)
    setSelectedManagedIds(new Set())
    setSelectedUnmanagedIds(new Set())
    setBatchDeleteTargets(null)
  }, [detail.id, scope])

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
      const targetNames = new Map(targets.map((target) => [target.id, target.targetPath.split('/').pop() || target.skillId]))
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
      const failed: string[] = []
      for (const item of items) {
        try {
          const skillId = await skillApiV2.executeAdopt(detail.id, item.id, defaultAgentDetailAdoptMode(item), null)
          adoptedSkillIds.push(skillId)
          ok += 1
        } catch (e) {
          failed.push(`${item.inferredSkillId || item.path.split('/').pop() || item.id}: ${String(e)}`)
        }
      }
      try {
        packResult = await syncAdoptedSkillsToPack(packSelection, adoptedSkillIds)
      } catch (e) {
        failed.push(`技能包同步失败: ${String(e)}`)
      }
      await refreshAgentSkills()
      setSelectedUnmanagedIds(new Set())
      setUnmanagedSelectionMode(false)
      const packNotice = packResult
        ? `，已同步 ${packResult.added} 个到「${packResult.packName}」`
        : ''
      setLocalNotice(`已接管 ${ok} 个 Skill${packNotice}${failed.length ? `，${failed.length} 个失败` : ''}`)
      if (failed.length === 0) state.setError(null)
      if (failed.length > 0) state.setError(failed.slice(0, 3).join('\n'))
    } catch (e) {
      state.setError(String(e))
    } finally {
      setAdoptingIds(new Set())
    }
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
          ) : (
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
                <button className="sm2__btn sm2__btn--ghost" disabled={actionBusy} onClick={() => {
                  setSelectedUnmanagedIds(new Set())
                  setUnmanagedSelectionMode(false)
                }}>
                  取消多选
                </button>
              </>
            ) : (
              <button className="sm2__btn sm2__btn--primary" disabled={canManageUnmanaged.length === 0 || actionBusy} onClick={() => setUnmanagedSelectionMode(true)}>
                批量管理
              </button>
            )
          )}
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
          onToggle={toggleManaged}
          onDelete={(skill) => deleteManaged([skill])}
          onOpenSkillDetail={onOpenSkillDetail}
        />
      )}

      {scope === 'unmanaged' && showUnmanaged && (
        <>
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
                onToggle={toggleUnmanaged}
                onAdopt={onAdopt}
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
  const disabled = mode === 'existing'
    ? !packId
    : mode === 'new'
      ? !newPackName.trim()
      : false

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
            <code>{visibleItems.map((item) => item.inferredSkillId || item.path.split('/').pop() || item.id).join(', ')}{remaining > 0 ? ` +${remaining}` : ''}</code>
          </div>
        </div>

        <section className="sm2-adopt__section">
          <div className="sm2-adopt__section-head">
            <h4>{t('skills.batchAdoptPack.syncTitle')}</h4>
            <span>{t('skills.batchAdoptPack.syncHint')}</span>
          </div>
          <div className="sm2-adopt__options" role="radiogroup" aria-label={t('skills.batchAdoptPack.syncTitle')}>
            <button
              type="button"
              className={`sm2-adopt__option${mode === 'none' ? ' sm2-adopt__option--active' : ''}`}
              role="radio"
              aria-checked={mode === 'none'}
              onClick={() => setMode('none')}
            >
              <span className="sm2-adopt__radio" />
              <span className="sm2-adopt__option-main">
                <strong>{t('skills.batchAdoptPack.skipTitle')}</strong>
                <span>{t('skills.batchAdoptPack.skipDescription')}</span>
              </span>
              <em>{t('skills.batchAdoptPack.skipBadge')}</em>
            </button>
            <button
              type="button"
              className={`sm2-adopt__option${mode === 'existing' ? ' sm2-adopt__option--active' : ''}`}
              role="radio"
              aria-checked={mode === 'existing'}
              disabled={packOptions.length === 0}
              onClick={() => {
                setMode('existing')
                if (!packId) setPackId(packOptions[0]?.id ?? '')
              }}
            >
              <span className="sm2-adopt__radio" />
              <span className="sm2-adopt__option-main">
                <strong>{t('skills.batchAdoptPack.existingTitle')}</strong>
                <span>{packOptions.length > 0 ? t('skills.batchAdoptPack.existingDescription') : t('skills.batchAdoptPack.noExistingPacks')}</span>
              </span>
              <em>{t('skills.batchAdoptPack.existingBadge')}</em>
            </button>
            <button
              type="button"
              className={`sm2-adopt__option${mode === 'new' ? ' sm2-adopt__option--active' : ''}`}
              role="radio"
              aria-checked={mode === 'new'}
              onClick={() => setMode('new')}
            >
              <span className="sm2-adopt__radio" />
              <span className="sm2-adopt__option-main">
                <strong>{t('skills.batchAdoptPack.newTitle')}</strong>
                <span>{t('skills.batchAdoptPack.newDescription')}</span>
              </span>
              <em>{t('skills.batchAdoptPack.newBadge')}</em>
            </button>
          </div>
        </section>

        {mode === 'existing' && (
          <div className="sm2-adopt__rename">
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
            <span>{t('skills.batchAdoptPack.existingImpact')}</span>
          </div>
        )}

        {mode === 'new' && (
          <div className="sm2-adopt__rename">
            <label htmlFor="sm2-batch-adopt-pack-new">{t('skills.batchAdoptPack.newPackName')}</label>
            <input
              id="sm2-batch-adopt-pack-new"
              value={newPackName}
              onChange={(event) => setNewPackName(event.target.value)}
              placeholder={t('skills.batchAdoptPack.newPackPlaceholder')}
            />
            <span>{t('skills.batchAdoptPack.newImpact')}</span>
          </div>
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
                  <span>{blocker.reason}</span>
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
  const rows = [...availableRows, ...appliedOnlyRows]

  if (rows.length === 0) return null

  return (
    <section className="sm2__agent-pack-rail" aria-label="技能包应用">
      <div className="sm2__agent-pack-rail-head">
        <strong>技能包</strong>
        <span>点击应用到当前 Agent，再点取消应用。</span>
      </div>
      <div className="sm2__agent-pack-toggles">
        {rows.map((pack) => (
          <button
            key={pack.id}
            className={`sm2__agent-pack-toggle${pack.applied ? ' sm2__agent-pack-toggle--applied' : ''}${pack.isDefault ? ' sm2__agent-pack-toggle--default' : ''}`}
            disabled={busy || (!pack.applied && pack.memberCount === 0)}
            aria-pressed={pack.applied}
            aria-label={`${pack.applied ? '取消应用' : '应用'} ${pack.name}`}
            onClick={() => pack.applied ? onRevoke(pack.id, detail.id) : onApplyPack(pack.id, detail.id)}
          >
            <span className="sm2__agent-pack-toggle-main">
              <strong>{pack.name}</strong>
              <span>{pack.isDefault ? '中心库全量' : `${pack.memberCount} 个 Skill`}</span>
            </span>
            <span className="sm2__agent-pack-toggle-state">{pack.applied ? '已应用' : '应用'}</span>
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
                    <span>{blocker.reason}</span>
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
  onToggle,
  onDelete,
  onOpenSkillDetail,
}: {
  skills: AgentDetail['skills']
  mode: AgentSkillViewMode
  selectable: boolean
  selectedIds: Set<string>
  deletingIds: Set<string>
  busy: boolean
  onToggle: (targetId: string) => void
  onDelete: (skill: AgentDetail['skills'][number]) => void
  onOpenSkillDetail: (skillId: string) => void
}) {
  if (skills.length === 0) {
    return <div className="sm2__empty sm2__empty--compact">暂无已管理 Skill</div>
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
  onOpenSkillDetail,
}: {
  skill: AgentDetail['skills'][number]
  selectable: boolean
  selected: boolean
  deleting: boolean
  busy: boolean
  onToggle: (targetId: string) => void
  onDelete: (skill: AgentDetail['skills'][number]) => void
  onOpenSkillDetail: (skillId: string) => void
}) {
  const { t } = useTranslation()
  const name = skill.targetPath.split('/').pop() || skill.id
  const claims = skill.claims.map((c) => targetClaimLabel(t, c)).filter(Boolean)
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
        <span className="sm2__source-pill">{targetClaimLabel(t, skill.claims[0])}</span>
      </div>
      <code>{skill.targetPath}</code>
      <ActionButton className="sm2__btn sm2__btn--danger" disabled={busy && !deleting} busy={deleting} busyLabel="删除中" aria-label={deleting ? `删除中 ${name}` : `删除 ${name}`} onClick={(e) => {
        e.stopPropagation()
        onDelete(skill)
      }}>
        删除
      </ActionButton>
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
  onOpenSkillDetail,
}: {
  skill: AgentDetail['skills'][number]
  selectable: boolean
  selected: boolean
  deleting: boolean
  busy: boolean
  onToggle: (targetId: string) => void
  onDelete: (skill: AgentDetail['skills'][number]) => void
  onOpenSkillDetail: (skillId: string) => void
}) {
  const { t } = useTranslation()
  const claims = skill.claims.map((c) => targetClaimLabel(t, c)).filter(Boolean)
  const name = skill.targetPath.split('/').pop() || skill.id
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
      <ActionButton className="sm2__btn sm2__btn--danger" disabled={busy && !deleting} busy={deleting} busyLabel="删除中" aria-label={deleting ? `删除中 ${name}` : `删除 ${name}`} onClick={(e) => {
        e.stopPropagation()
        onDelete(skill)
      }}>
        删除
      </ActionButton>
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
  onToggle,
  onAdopt,
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
  onToggle: (unmanagedId: string) => void
  onAdopt: (agentId: string, unmanagedId: string) => void
  onOpenSkillDetail: (skillId: string, fallback?: SkillDetailFallback | null) => void
}) {
  const { t } = useTranslation()
  if (mode === 'list') {
    return (
      <div className="sm2__agent-skill-list">
        {skills.map((u) => (
          <div
            key={u.id}
            className={`sm2__object-row sm2__object-row--path sm2__object-row--clickable${selectable && selectedIds.has(u.id) ? ' sm2__object-row--selected' : ''}${adoptingIds.has(u.id) ? ' sm2__object-row--adopting' : ''}`}
            onClick={() => openUnmanagedSkill(u, onOpenSkillDetail, unmanagedReasonLabel(t, u.reason))}
          >
            {selectable && (
              <input
                type="checkbox"
                className="sm2__agent-skill-select"
                aria-label={`选择 ${u.inferredSkillId || u.path.split('/').pop() || u.id}`}
                checked={selectedIds.has(u.id)}
                disabled={busy}
                onClick={(e) => e.stopPropagation()}
                onChange={() => onToggle(u.id)}
              />
            )}
            <div>
              <strong>{u.inferredSkillId || u.path.split('/').pop()}</strong>
              <span>{unmanagedReasonLabel(t, u.reason)}</span>
              <code>{u.path}</code>
            </div>
            <ActionButton className="sm2__btn sm2__btn--primary" disabled={busy} busy={adoptingUnmanagedId === u.id || adoptingIds.has(u.id)} busyLabel="准备接管" onClick={(e) => {
              e.stopPropagation()
              onAdopt(agentId, u.id)
            }}>
              接管
            </ActionButton>
          </div>
        ))}
      </div>
    )
  }

  return (
    <div className="sm2__agent-skill-grid">
      {skills.map((u) => (
        <article
          key={u.id}
          className={`sm2__agent-skill-card sm2__agent-skill-card--unmanaged sm2__agent-skill-card--clickable${selectable && selectedIds.has(u.id) ? ' sm2__agent-skill-card--selected' : ''}${adoptingIds.has(u.id) ? ' sm2__agent-skill-card--adopting' : ''}`}
          role="button"
          tabIndex={0}
            onClick={() => openUnmanagedSkill(u, onOpenSkillDetail, unmanagedReasonLabel(t, u.reason))}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') openUnmanagedSkill(u, onOpenSkillDetail, unmanagedReasonLabel(t, u.reason))
          }}
        >
          <div className={`sm2__agent-skill-card-head${selectable ? ' sm2__agent-skill-card-head--selectable' : ''}`}>
            {selectable && (
              <input
                type="checkbox"
                className="sm2__agent-skill-select"
                aria-label={`选择 ${u.inferredSkillId || u.path.split('/').pop() || u.id}`}
                checked={selectedIds.has(u.id)}
                disabled={busy}
                onClick={(e) => e.stopPropagation()}
                onChange={() => onToggle(u.id)}
              />
            )}
            <div className="sm2__agent-skill-icon">{initials(u.inferredSkillId || u.path.split('/').pop() || 'SK')}</div>
            <div className="sm2__agent-skill-card-titleline">
              <strong>{u.inferredSkillId || u.path.split('/').pop()}</strong>
              <span>{unmanagedReasonLabel(t, u.reason)}</span>
            </div>
            <span className="sm2__tag sm2__tag--unmanaged">未管理</span>
          </div>
          <code>{u.path}</code>
          <ActionButton className="sm2__btn sm2__btn--primary" disabled={busy} busy={adoptingUnmanagedId === u.id || adoptingIds.has(u.id)} busyLabel="准备接管" onClick={(e) => {
            e.stopPropagation()
            onAdopt(agentId, u.id)
          }}>
            接管
          </ActionButton>
        </article>
      ))}
    </div>
  )
}

function toggleSetValue(current: Set<string>, value: string) {
  const next = new Set(current)
  if (next.has(value)) next.delete(value)
  else next.add(value)
  return next
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
  return item.agentId === 'agents' || item.path.includes('/.agents/skills/') ? 'import_link' : 'import_keep'
}

function openUnmanagedSkill(
  item: UnmanagedItemDto,
  onOpenSkillDetail: (skillId: string, fallback?: SkillDetailFallback | null) => void,
  reasonLabel?: string,
) {
  const name = item.inferredSkillId || item.path.split('/').pop() || item.id
  onOpenSkillDetail(name, {
    id: name,
    name,
    centerPath: item.path,
    description: `${reasonLabel || item.reason} · ${item.path}`,
    sourceType: 'unmanaged_agent',
    sourceUri: item.path,
  })
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
  if (detail.mcpServers.length === 0)
    return <div className="sm2__empty sm2__empty--compact">未配置 MCP 服务器</div>
  return (
    <section className="sm2__panel">
      {detail.mcpServers.map((m) => (
        <div key={m.name} className="sm2__object-row sm2__object-row--path">
          <div>
            <strong>{m.name}</strong>
            <span>{m.valid ? '有效' : m.message || '异常'}</span>
            <code>{m.command} {m.args.join(' ')}</code>
          </div>
          <span className={`sm2__tag sm2__tag--${m.valid ? 'ok' : 'conflict'}`}>{m.valid ? '有效' : '异常'}</span>
        </div>
      ))}
    </section>
  )
}

function PluginsTab({ detail }: { detail: AgentDetail }) {
  if (detail.plugins.length === 0)
    return <div className="sm2__empty sm2__empty--compact">未检测到插件</div>
  return (
    <section className="sm2__panel">
      {detail.plugins.map((p) => (
        <div key={p.id} className="sm2__object-row">
          <div>
            <strong>{p.name}</strong>
            <span>{p.source || 'local'} {p.version ? `· v${p.version}` : ''}</span>
          </div>
          <span className={`sm2__tag sm2__tag--${p.enabled ? 'ok' : 'unmanaged'}`}>{p.enabled ? '启用' : '禁用'}</span>
        </div>
      ))}
    </section>
  )
}

function HooksTab({ agentId }: { agentId: string }) {
  const [hook, setHook] = useState<HookStatus | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const load = async () => {
    setLoading(true)
    try {
      const all = await getAllHookStatus()
      const found = all.find((h) => h.adapterId === agentId || h.toolId === agentId)
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
      await installAgentHook(agentId)
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
      await configureAgentHookEvents(agentId, next)
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
      await uninstallAgentHook(agentId)
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

function ConfigTab({ detail, program }: { detail: AgentDetail; program: AgentProgramInfo | null }) {
  const currentVersion = program?.installedVersion ?? detail.version
  const latestVersion = program?.latestVersion ?? detail.latestVersion
  const executablePath = program?.binaryPath ?? program?.appPath ?? null
  const skillsDir = detail.skillsDir ?? program?.skillsDir ?? null
  const rows: Array<{ label: string; value: string | null; openable?: boolean }> = [
    { label: 'Agent ID', value: detail.id },
    { label: '当前版本', value: currentVersion },
    { label: '最新版本', value: latestVersion },
    { label: program?.kind === 'app' ? '应用路径' : '可执行文件', value: executablePath, openable: true },
    { label: '配置目录', value: program?.configDir ?? null, openable: true },
    { label: '配置文件', value: detail.configPath, openable: true },
    { label: 'Skills 目录', value: skillsDir, openable: true },
    { label: 'MCP 配置', value: detail.mcpConfigPath, openable: true },
    { label: 'Plugin 目录', value: detail.pluginDir, openable: true },
  ]
  return (
    <section className="sm2__panel">
      {rows.map((r) => (
        <div key={r.label} className="sm2__object-row sm2__object-row--path">
          <div>
            <strong>{r.label}</strong>
            <code>{r.value || '未检测到'}</code>
          </div>
          {r.openable && r.value && (
            <button className="sm2__btn sm2__btn--ghost" onClick={() => skillApiV2.openPath(r.value!)}>
              打开
            </button>
          )}
        </div>
      ))}
    </section>
  )
}
