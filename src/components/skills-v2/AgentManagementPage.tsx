import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useSkillStoreV2 } from '../../stores/skillStoreV2'
import { skillApiV2 } from '../../services/skillApiV2'
import { agentApi, type AgentOutputEvent, type AgentProgramInfo } from '../../services/agentApi'
import { configureAgentHookEvents, getAllHookStatus, installAgentHook, uninstallAgentHook, type HookEventStatus, type HookStatus } from '../../services/tauriApi'
import type { AgentDetail, AdoptPreview, RemovePackFromAgentPreview, UnmanagedItemDto } from '../../services/skillApiV2'
import { AgentIconBadge } from './AgentIconBadge'
import { AdoptDialog } from './AdoptDialog'
import { PreviewDialog } from './PreviewDialog'
import { SkillDetailSlider, type SkillDetailFallback } from './SkillDetailSlider'
import { skillModeLabel, skillStatusLabel, targetClaimLabel, unmanagedReasonLabel } from './skillLabels'

type DetailTab = 'overview' | 'skills' | 'mcp' | 'plugins' | 'hooks' | 'config'
type AgentSkillViewMode = 'cards' | 'list'

const PAGE_SIZE = 28

export function AgentManagementPage() {
  const state = useSkillStoreV2()
  const agents = state.agents
  const detail = state.selectedAgentDetail
  const [tab, setTab] = useState<DetailTab>('overview')
  const [adopt, setAdopt] = useState<AdoptPreview | null>(null)
  const [revokePreview, setRevokePreview] = useState<RemovePackFromAgentPreview | null>(null)
  const [detailSkillId, setDetailSkillId] = useState<string | null>(null)
  const [detailFallback, setDetailFallback] = useState<SkillDetailFallback | null>(null)
  const [busy, setBusy] = useState(false)
  const [scanningAgentId, setScanningAgentId] = useState<string | null>(null)
  const [updatingAgentId, setUpdatingAgentId] = useState<string | null>(null)
  const [installingAgentId, setInstallingAgentId] = useState<string | null>(null)
  const [programs, setPrograms] = useState<Record<string, AgentProgramInfo>>({})
  const [programLoading, setProgramLoading] = useState(false)
  const [agentOutput, setAgentOutput] = useState<string[]>([])
  const [notice, setNotice] = useState<string | null>(null)
  const selectedAgentIdRef = useRef<string | null>(null)
  const actionBusy = busy || scanningAgentId !== null || updatingAgentId !== null || installingAgentId !== null

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
    if (state.selectedAgentId || agents.length === 0) return
    const first = agents.find((a) => a.installed) || agents[0]
    if (first) state.selectAgent(first.id)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agents, state.selectedAgentId])

  useEffect(() => {
    setTab('overview')
    setNotice(null)
    setAgentOutput([])
  }, [state.selectedAgentId])

  const openAdopt = async (agentId: string, unmanagedId: string) => {
    setBusy(true)
    try {
      const p = await skillApiV2.previewAdopt(agentId, unmanagedId)
      setAdopt(p)
    } catch (e) {
      state.setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  const revoke = async (packId: string, agentId: string) => {
    setBusy(true)
    try {
      const p = await skillApiV2.previewRemovePackFromAgent(packId, agentId)
      setRevokePreview(p)
    } catch (e) {
      state.setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  const applyPack = async (packId: string, agentId: string) => {
    setBusy(true)
    setNotice(null)
    try {
      const mode = state.settings?.defaultDistributeMode || 'link'
      const preview = await skillApiV2.previewApplyPack(packId, [agentId], mode)
      if (preview.blockers.length > 0) {
        state.setError(preview.blockers.map((b) => `${b.skillId}: ${b.reason}`).join('\n'))
        return
      }
      await skillApiV2.executeApplyPack(packId, [agentId], mode)
      await state.loadAgentDetail(agentId, true)
      await state.loadOverview(true)
      setNotice('技能包已应用')
    } catch (e) {
      state.setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  const scanAgent = async (agentId: string) => {
    setScanningAgentId(agentId)
    setNotice(null)
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
    try {
      await agentApi.openDownload(agentId)
    } catch (e) {
      state.setError(String(e))
    }
  }

  const refreshAll = async () => {
    await Promise.all([state.loadOverview(true), loadPrograms()])
    if (state.selectedAgentId) await state.loadAgentDetail(state.selectedAgentId, true)
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
          <button className="sm2__btn" onClick={() => state.loadOverview(true)} disabled={state.loading || actionBusy}>
            刷新总览
          </button>
          <button className="sm2__btn" onClick={refreshAll} disabled={state.loading || actionBusy || programLoading}>
            {programLoading && <span className="sm2__spinner" />}
            获取版本
          </button>
        </div>
      </div>

      {state.error && <div className="sm2__error">{state.error}</div>}
      {notice && <div className="sm2__notice sm2__notice--ok">{notice}</div>}

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
      {revokePreview && (
        <RevokePackDialog
          preview={revokePreview}
          busy={busy}
          onClose={() => setRevokePreview(null)}
          onExecute={async () => {
            setBusy(true)
            try {
              await skillApiV2.removePackFromAgent(revokePreview.packId, revokePreview.agentId)
              const agentId = revokePreview.agentId
              setRevokePreview(null)
              await state.loadAgentDetail(agentId, true)
              await state.loadOverview(true)
            } catch (e) {
              state.setError(String(e))
            } finally {
              setBusy(false)
            }
          }}
        />
      )}
    </div>
  )
}

function formatAgentOutput(event: AgentOutputEvent) {
  const prefix = event.stream === 'stderr' ? '!' : event.stream === 'stdout' ? '>' : '*'
  return `${prefix} ${event.line}`
}

function AgentDetailView({
  detail,
  tab,
  onTab,
  busy,
  scanning,
  updating,
  installing,
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
  program: AgentProgramInfo | null
  programLoading: boolean
  agentOutput: string[]
  onAdopt: (agentId: string, unmanagedId: string) => void
  onRevoke: (packId: string, agentId: string) => void
  onApplyPack: (packId: string, agentId: string) => void
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
      : { label: '打开安装页', disabled: busy || !canOpenDownload, onClick: () => onOpenDownload(detail.id), busy: false }
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
          <button className="sm2__btn sm2__btn--primary" disabled={primaryAction.disabled} onClick={primaryAction.onClick}>
            {primaryAction.busy && <span className="sm2__spinner" />}
            {primaryAction.label}
          </button>
          <button className="sm2__btn" disabled={busy || scanning || updating || installing || !installed} onClick={() => onScan(detail.id)}>
            {scanning && <span className="sm2__spinner" />}
            {scanning ? '正在扫描' : '重新扫描此 Agent'}
          </button>
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
            onAdopt={onAdopt}
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
  onApplyPack: (packId: string, agentId: string) => void
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
  onAdopt,
  onScan,
  onOpenSkillDetail,
}: {
  detail: AgentDetail
  unmanaged: UnmanagedItemDto[]
  showUnmanaged: boolean
  busy: boolean
  scanning: boolean
  onAdopt: (agentId: string, unmanagedId: string) => void
  onScan: (agentId: string) => void
  onOpenSkillDetail: (skillId: string, fallback?: SkillDetailFallback | null) => void
}) {
  const { t } = useTranslation()
  const [query, setQuery] = useState('')
  const [page, setPage] = useState(1)
  const [viewMode, setViewMode] = useState<AgentSkillViewMode>('cards')
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

  return (
    <div className="sm2__skills-tab">
      <div className="sm2__toolbar sm2__toolbar--inset sm2__toolbar--split">
        <input className="sm2__search" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="搜索 Skill 名称 / 路径 / 来源 / 原因" />
        <div className="sm2__view-toggle sm2__view-toggle--soft">
          <button className={viewMode === 'cards' ? 'active' : ''} onClick={() => setViewMode('cards')}>卡片</button>
          <button className={viewMode === 'list' ? 'active' : ''} onClick={() => setViewMode('list')}>列表</button>
        </div>
      </div>
      <section className="sm2__panel">
        <div className="sm2__panel-head">
          <h3>已管理 Skills</h3>
          <span>{filteredManaged.length}</span>
        </div>
        <ManagedSkillCollection skills={filteredManaged} mode={viewMode} onOpenSkillDetail={onOpenSkillDetail} />
      </section>

      {showUnmanaged && (
        <section className="sm2__panel">
          <div className="sm2__panel-head">
            <h3>未管理 Skills</h3>
            <span>{filteredUnmanaged.length}</span>
          </div>
          {filteredUnmanaged.length === 0 ? (
            <div className="sm2__empty sm2__empty--compact sm2__unmanaged-empty">
              <span>
                {unmanaged.length === 0 ? '没有未管理 Skill。若刚手动安装过，可以重新扫描。' : '没有匹配的未管理 Skill'}
              </span>
              {unmanaged.length === 0 && (
                <button className="sm2__btn" disabled={busy} onClick={() => onScan(detail.id)}>
                  {scanning && <span className="sm2__spinner" />}
                  {scanning ? '正在扫描' : '重新扫描此 Agent'}
                </button>
              )}
            </div>
          ) : (
            <>
              <UnmanagedSkillCollection
                skills={shownUnmanaged}
                mode={viewMode}
                agentId={detail.id}
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
        </section>
      )}
    </div>
  )
}

function ManagedSkillCollection({
  skills,
  mode,
  onOpenSkillDetail,
}: {
  skills: AgentDetail['skills']
  mode: AgentSkillViewMode
  onOpenSkillDetail: (skillId: string) => void
}) {
  if (skills.length === 0) {
    return <div className="sm2__empty sm2__empty--compact">暂无已管理 Skill</div>
  }

  if (mode === 'list') {
    return (
      <div className="sm2__agent-skill-list">
        {skills.map((s) => (
          <ManagedSkillListRow key={s.id} skill={s} onOpenSkillDetail={onOpenSkillDetail} />
        ))}
      </div>
    )
  }

  return (
    <div className="sm2__agent-skill-grid">
      {skills.map((s) => (
        <ManagedSkillCard key={s.id} skill={s} onOpenSkillDetail={onOpenSkillDetail} />
      ))}
    </div>
  )
}

function ManagedSkillCard({
  skill,
  onOpenSkillDetail,
}: {
  skill: AgentDetail['skills'][number]
  onOpenSkillDetail: (skillId: string) => void
}) {
  const { t } = useTranslation()
  const name = skill.targetPath.split('/').pop() || skill.id
  const claims = skill.claims.map((c) => targetClaimLabel(t, c)).filter(Boolean)
  return (
    <article
      className="sm2__agent-skill-card sm2__agent-skill-card--clickable"
      role="button"
      tabIndex={0}
      onClick={() => onOpenSkillDetail(skill.skillId)}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') onOpenSkillDetail(skill.skillId)
      }}
    >
      <div className="sm2__agent-skill-card-head">
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
    </article>
  )
}

function ManagedSkillListRow({
  skill,
  onOpenSkillDetail,
}: {
  skill: AgentDetail['skills'][number]
  onOpenSkillDetail: (skillId: string) => void
}) {
  const { t } = useTranslation()
  const claims = skill.claims.map((c) => targetClaimLabel(t, c)).filter(Boolean)
  return (
    <div className="sm2__object-row sm2__object-row--path sm2__object-row--clickable" onClick={() => onOpenSkillDetail(skill.skillId)}>
      <div>
        <strong>{skill.targetPath.split('/').pop()}</strong>
        <span>{skillModeLabel(t, skill.actualMode)} · {skillStatusLabel(t, skill.status)} · {claims.join(' / ')}</span>
        <code>{skill.targetPath}</code>
      </div>
    </div>
  )
}

function UnmanagedSkillCollection({
  skills,
  mode,
  agentId,
  onAdopt,
  onOpenSkillDetail,
}: {
  skills: UnmanagedItemDto[]
  mode: AgentSkillViewMode
  agentId: string
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
            className="sm2__object-row sm2__object-row--path sm2__object-row--clickable"
            onClick={() => openUnmanagedSkill(u, onOpenSkillDetail, unmanagedReasonLabel(t, u.reason))}
          >
            <div>
              <strong>{u.inferredSkillId || u.path.split('/').pop()}</strong>
              <span>{unmanagedReasonLabel(t, u.reason)}</span>
              <code>{u.path}</code>
            </div>
            <button className="sm2__btn sm2__btn--primary" onClick={(e) => {
              e.stopPropagation()
              onAdopt(agentId, u.id)
            }}>
              接管
            </button>
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
          className="sm2__agent-skill-card sm2__agent-skill-card--unmanaged sm2__agent-skill-card--clickable"
          role="button"
          tabIndex={0}
            onClick={() => openUnmanagedSkill(u, onOpenSkillDetail, unmanagedReasonLabel(t, u.reason))}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') openUnmanagedSkill(u, onOpenSkillDetail, unmanagedReasonLabel(t, u.reason))
          }}
        >
          <div className="sm2__agent-skill-card-head">
            <div className="sm2__agent-skill-icon">{initials(u.inferredSkillId || u.path.split('/').pop() || 'SK')}</div>
            <div className="sm2__agent-skill-card-titleline">
              <strong>{u.inferredSkillId || u.path.split('/').pop()}</strong>
              <span>{unmanagedReasonLabel(t, u.reason)}</span>
            </div>
            <span className="sm2__tag sm2__tag--unmanaged">未管理</span>
          </div>
          <code>{u.path}</code>
          <button className="sm2__btn sm2__btn--primary" onClick={(e) => {
            e.stopPropagation()
            onAdopt(agentId, u.id)
          }}>
            接管
          </button>
        </article>
      ))}
    </div>
  )
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
            <button className="sm2__btn sm2__btn--danger" disabled={busy} onClick={uninstall}>卸载 Hook</button>
          ) : (
            <button className="sm2__btn sm2__btn--primary" disabled={busy} onClick={install}>安装 Hook</button>
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

function RevokePackDialog({
  preview,
  busy,
  onClose,
  onExecute,
}: {
  preview: RemovePackFromAgentPreview
  busy: boolean
  onClose: () => void
  onExecute: () => void
}) {
  return (
    <PreviewDialog
      title={`从 ${preview.displayName} 撤销「${preview.packName}」`}
      confirmLabel="撤销技能包"
      destructive
      busy={busy}
      onConfirm={onExecute}
      onCancel={onClose}
    >
      <div className="sm2__detail-meta">
        将移除 {preview.willRemoveTargets} 个仅由该技能包安装的目标，保留 {preview.willPreserveTargets} 个仍有其他归属的目标。
      </div>
      {preview.affectedTargets.length > 0 && (
        <div className="sm2__scroll" style={{ marginTop: 8 }}>
          {preview.affectedTargets.map((t) => (
            <div key={t.targetId} className="sm2__target-row">
              <span>{t.mode} · {t.claimCount} claim(s)</span>
              <code>{t.targetPath}</code>
            </div>
          ))}
        </div>
      )}
    </PreviewDialog>
  )
}
