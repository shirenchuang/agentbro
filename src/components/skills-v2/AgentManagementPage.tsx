import { useEffect, useMemo, useState } from 'react'
import { useSkillStoreV2 } from '../../stores/skillStoreV2'
import { skillApiV2 } from '../../services/skillApiV2'
import { configureAgentHookEvents, getAllHookStatus, installAgentHook, uninstallAgentHook, type HookEventStatus, type HookStatus } from '../../services/tauriApi'
import type { AgentDetail, AdoptPreview, RemovePackFromAgentPreview, UnmanagedItemDto } from '../../services/skillApiV2'
import { AgentIconBadge } from './AgentIconBadge'
import { PreviewDialog } from './PreviewDialog'
import { SkillDetailSlider, type SkillDetailFallback } from './SkillDetailSlider'

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
  const [notice, setNotice] = useState<string | null>(null)

  useEffect(() => {
    state.loadOverview()
    // eslint-disable-next-line react-hooks/exhaustive-deps
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
    setBusy(true)
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
      setBusy(false)
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
          <button className="sm2__btn" onClick={() => state.loadOverview(true)} disabled={state.loading || busy}>
            刷新总览
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
            busy={busy}
            onAdopt={openAdopt}
            onRevoke={revoke}
            onApplyPack={applyPack}
            onScan={scanAgent}
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

function AgentDetailView({
  detail,
  tab,
  onTab,
  busy,
  onAdopt,
  onRevoke,
  onApplyPack,
  onScan,
  onOpenSkillDetail,
}: {
  detail: AgentDetail
  tab: DetailTab
  onTab: (t: DetailTab) => void
  busy: boolean
  onAdopt: (agentId: string, unmanagedId: string) => void
  onRevoke: (packId: string, agentId: string) => void
  onApplyPack: (packId: string, agentId: string) => void
  onScan: (agentId: string) => void
  onOpenSkillDetail: (skillId: string, fallback?: SkillDetailFallback | null) => void
}) {
  const unmanaged = useSkillStoreV2((s) => s.unmanaged).filter((u) => u.agentId === detail.id)
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
        <AgentIconBadge iconKey={detail.iconKey} size={52} />
        <div className="sm2__agent-hero-main">
          <div className="sm2__agent-hero-title">{detail.displayName}</div>
          <div className="sm2__agent-hero-sub">
            版本 {detail.version || '未知'}
            {detail.latestVersion && detail.latestVersion !== detail.version ? ` · 可更新到 ${detail.latestVersion}` : ''}
          </div>
          <PathLine label="Skills" value={detail.skillsDir} />
          <PathLine label="MCP" value={detail.mcpConfigPath} />
        </div>
        <div className="sm2__btn-row" style={{ margin: 0 }}>
          <button className="sm2__btn" disabled={busy} onClick={() => onScan(detail.id)}>
            重新扫描此 Agent
          </button>
        </div>
      </div>

      <div className="sm2__stat-grid">
        <Stat value={detail.skills.length} label="已管理 Skills" />
        <Stat value={unmanaged.length} label="未管理 Skills" tone={unmanaged.length > 0 ? 'warn' : 'ok'} />
        <Stat value={detail.appliedPacks.length} label="已应用技能包" />
        <Stat value={detail.mcpServers.length + detail.plugins.length} label="MCP / Plugins" />
      </div>

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
        {tab === 'skills' && <SkillsTab detail={detail} unmanaged={unmanaged} onAdopt={onAdopt} onOpenSkillDetail={onOpenSkillDetail} />}
        {tab === 'mcp' && <McpTab detail={detail} />}
        {tab === 'plugins' && <PluginsTab detail={detail} />}
        {tab === 'hooks' && <HooksTab agentId={detail.id} />}
        {tab === 'config' && <ConfigTab detail={detail} />}
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

function PathLine({ label, value }: { label: string; value: string | null }) {
  if (!value) return null
  return (
    <div className="sm2__pathline">
      <span>{label}</span>
      <code>{value}</code>
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
  onAdopt,
  onOpenSkillDetail,
}: {
  detail: AgentDetail
  unmanaged: UnmanagedItemDto[]
  onAdopt: (agentId: string, unmanagedId: string) => void
  onOpenSkillDetail: (skillId: string, fallback?: SkillDetailFallback | null) => void
}) {
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
    return unmanaged.filter((u) => [u.inferredSkillId, u.path, u.reason].filter(Boolean).join(' ').toLowerCase().includes(q))
  }, [q, unmanaged])
  const shownUnmanaged = filteredUnmanaged.slice(0, page * PAGE_SIZE)

  useEffect(() => setPage(1), [q, detail.id])

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

      <section className="sm2__panel">
        <div className="sm2__panel-head">
          <h3>未管理 Skills</h3>
          <span>{filteredUnmanaged.length}</span>
        </div>
        {filteredUnmanaged.length === 0 ? (
          <div className="sm2__empty sm2__empty--compact">
            {unmanaged.length === 0 ? '没有未管理 Skill。若刚手动安装过，请点击「重新扫描此 Agent」。' : '没有匹配的未管理 Skill'}
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
  const name = skill.targetPath.split('/').pop() || skill.id
  const claims = skill.claims.map((c) => c.packName || c.claimType).filter(Boolean)
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
        <span className={`sm2__tag sm2__tag--${skill.status}`}>{skill.status}</span>
      </div>
      <strong>{name}</strong>
      <span>{skill.actualMode} · {claims.length > 0 ? claims.join(' / ') : '独立安装'}</span>
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
  return (
    <div className="sm2__object-row sm2__object-row--path sm2__object-row--clickable" onClick={() => onOpenSkillDetail(skill.skillId)}>
      <div>
        <strong>{skill.targetPath.split('/').pop()}</strong>
        <span>{skill.actualMode} · {skill.status} · {skill.claims.map((c) => c.packName || c.claimType).join(' / ')}</span>
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
  if (mode === 'list') {
    return (
      <div className="sm2__agent-skill-list">
        {skills.map((u) => (
          <div
            key={u.id}
            className="sm2__object-row sm2__object-row--path sm2__object-row--clickable"
            onClick={() => openUnmanagedSkill(u, onOpenSkillDetail)}
          >
            <div>
              <strong>{u.inferredSkillId || u.path.split('/').pop()}</strong>
              <span>{u.reason}</span>
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
          onClick={() => openUnmanagedSkill(u, onOpenSkillDetail)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') openUnmanagedSkill(u, onOpenSkillDetail)
          }}
        >
          <div className="sm2__agent-skill-card-head">
            <div className="sm2__agent-skill-icon">{initials(u.inferredSkillId || u.path.split('/').pop() || 'SK')}</div>
            <span className="sm2__tag sm2__tag--unmanaged">未管理</span>
          </div>
          <strong>{u.inferredSkillId || u.path.split('/').pop()}</strong>
          <span>{u.reason}</span>
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
) {
  const name = item.inferredSkillId || item.path.split('/').pop() || item.id
  onOpenSkillDetail(name, {
    id: name,
    name,
    centerPath: item.path,
    description: `${item.reason} · ${item.path}`,
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
        {hook.installed ? (
          <button className="sm2__btn sm2__btn--danger" disabled={busy} onClick={uninstall}>卸载 Hook</button>
        ) : (
          <button className="sm2__btn sm2__btn--primary" disabled={busy} onClick={install}>安装 Hook</button>
        )}
      </section>
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

function ConfigTab({ detail }: { detail: AgentDetail }) {
  const rows: Array<{ label: string; value: string | null; openable?: boolean }> = [
    { label: 'Agent ID', value: detail.id },
    { label: '当前版本', value: detail.version },
    { label: '最新版本', value: detail.latestVersion },
    { label: 'Skills 目录', value: detail.skillsDir, openable: true },
    { label: 'MCP 配置', value: detail.mcpConfigPath, openable: true },
    { label: 'Plugin 目录', value: detail.pluginDir, openable: true },
    { label: '配置文件', value: detail.configPath, openable: true },
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

function AdoptDialog({
  preview,
  onClose,
  onDone,
}: {
  preview: AdoptPreview
  onClose: () => void
  onDone: () => void
}) {
  const [option, setOption] = useState(preview.options[0]?.value || 'import_keep')
  const [renamedId, setRenamedId] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const execute = async () => {
    setBusy(true)
    setError(null)
    try {
      await skillApiV2.executeAdopt(preview.agentId, preview.unmanagedId, option, option === 'rename' ? renamedId : null)
      onDone()
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <PreviewDialog
      title="接管未管理 Skill"
      confirmLabel="执行接管"
      destructive={preview.options.find((o) => o.value === option)?.destructive}
      busy={busy}
      onConfirm={execute}
      onCancel={onClose}
    >
      <div className="sm2__detail-meta">
        <div>Skill：{preview.inferredSkillId}</div>
        <div>路径：{preview.skillPath}</div>
        <div>中心库已有同名：{preview.centerHasSameId ? '是' : '否'}</div>
      </div>
      <div className="sm2__field" style={{ marginTop: 12 }}>
        <label>处理方式</label>
        <select value={option} onChange={(e) => setOption(e.target.value)}>
          {preview.options.map((o) => (
            <option key={o.value} value={o.value}>{o.label}</option>
          ))}
        </select>
      </div>
      {option === 'rename' && (
        <div className="sm2__field">
          <label>新的 Skill ID</label>
          <input value={renamedId} onChange={(e) => setRenamedId(e.target.value)} placeholder={`${preview.inferredSkillId}-import`} />
        </div>
      )}
      {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
    </PreviewDialog>
  )
}
