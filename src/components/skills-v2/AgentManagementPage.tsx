import { useEffect, useState } from 'react'
import { useSkillStoreV2 } from '../../stores/skillStoreV2'
import { skillApiV2 } from '../../services/skillApiV2'
import type { AgentDetail, AdoptPreview } from '../../services/skillApiV2'
import { AgentIconBadge } from './AgentIconBadge'
import { PreviewDialog } from './PreviewDialog'

type DetailTab = 'overview' | 'skills' | 'mcp' | 'plugins'

export function AgentManagementPage() {
  const state = useSkillStoreV2()
  const agents = state.agents
  const detail = state.selectedAgentDetail
  const [tab, setTab] = useState<DetailTab>('overview')
  const [adopt, setAdopt] = useState<AdoptPreview | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    state.loadOverview()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    setTab('overview')
  }, [state.selectedAgentId])

  const openAdopt = async (agentId: string, unmanagedId: string) => {
    setBusy(true)
    try {
      const p = await skillApiV2.previewAdopt(agentId, unmanagedId)
      setAdopt(p)
    } catch (e) {
      alert(String(e))
    } finally {
      setBusy(false)
    }
  }

  const revoke = async (packId: string, agentId: string) => {
    if (!confirm(`从该 Agent 撤销技能包「${packId}」？`)) return
    setBusy(true)
    try {
      await skillApiV2.removePackFromAgent(packId, agentId)
      await state.loadAgentDetail(agentId)
      await state.loadOverview()
    } catch (e) {
      alert(String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="sm2 sm2--masterdetail">
      <div className="sm2__header">
        <h2 className="sm2__title">Agent 管理</h2>
        <div className="sm2__tabs">
          <button className="sm2__btn" onClick={() => state.loadOverview()}>刷新</button>
        </div>
      </div>

      <div className="sm2__body sm2__body--masterdetail">
        {/* Left agent rail */}
        <div className="sm2__rail settings-scroll">
          {agents.length === 0 ? (
            <div className="sm2__empty" style={{ padding: 12 }}>未检测到 Agent</div>
          ) : (
            agents.map((a) => (
              <div
                key={a.id}
                className={`sm2__rail-item${state.selectedAgentId === a.id ? ' sm2__rail-item--active' : ''}`}
                onClick={() => state.selectAgent(a.id)}
              >
                <AgentIconBadge iconKey={a.iconKey} title={a.displayName} size={28} />
                <div className="sm2__rail-item-main">
                  <div className="sm2__rail-item-title">{a.displayName}</div>
                  <div className="sm2__rail-item-sub">
                    {a.installed ? `管理 ${a.managedSkillCount}` : '未检测到'}
                    {a.unmanagedSkillCount > 0 && ` · 未管理 ${a.unmanagedSkillCount}`}
                  </div>
                </div>
                {a.installed && <span className="sm2__dot sm2__dot--ok" />}
              </div>
            ))
          )}
        </div>

        {/* Right detail */}
        <div className="sm2__detailpane settings-scroll">
          {!detail ? (
            <div className="sm2__empty" style={{ padding: 40 }}>← 选择左侧一个 Agent 查看详情</div>
          ) : (
            <AgentDetail
              detail={detail}
              tab={tab}
              onTab={setTab}
              busy={busy}
              onAdopt={openAdopt}
              onRevoke={revoke}
            />
          )}
        </div>
      </div>

      {adopt && (
        <AdoptDialog
          preview={adopt}
          onClose={() => setAdopt(null)}
          onDone={async () => {
            setAdopt(null)
            await state.loadAgentDetail(adopt.agentId)
            await state.loadOverview()
          }}
        />
      )}
    </div>
  )
}

function AgentDetail({
  detail,
  tab,
  onTab,
  busy,
  onAdopt,
  onRevoke,
}: {
  detail: AgentDetail
  tab: DetailTab
  onTab: (t: DetailTab) => void
  busy: boolean
  onAdopt: (agentId: string, unmanagedId: string) => void
  onRevoke: (packId: string, agentId: string) => void
}) {
  const tabs: Array<{ id: DetailTab; label: string }> = [
    { id: 'overview', label: '概览' },
    { id: 'skills', label: `Skills (${detail.skills.length})` },
    { id: 'mcp', label: `MCP (${detail.mcpServers.length})` },
    { id: 'plugins', label: `Plugins (${detail.plugins.length})` },
  ]
  return (
    <div className="sm2__agentdetail">
      {/* Summary header */}
      <div className="sm2__agentdetail-header">
        <AgentIconBadge iconKey={detail.iconKey} size={44} />
        <div className="sm2__agentdetail-titles">
          <h3>{detail.displayName}</h3>
          <div className="sm2__detail-meta">
            版本 {detail.version || '未知'}
            {detail.latestVersion && detail.latestVersion !== detail.version && (
              <span className="sm2__tag sm2__tag--copyDiverged"> → {detail.latestVersion}</span>
            )}
          </div>
        </div>
      </div>

      <div className="sm2__detail-meta" style={{ marginBottom: 12 }}>
        {detail.skillsDir && <div>Skills 目录：<code>{detail.skillsDir}</code></div>}
        {detail.mcpConfigPath && <div>MCP 配置：<code>{detail.mcpConfigPath}</code></div>}
      </div>

      {detail.health.length > 0 && (
        <div className="sm2__detail-section">
          {detail.health.map((h, i) => (
            <div key={i} className="sm2__change sm2__change--blocked">{h.message}</div>
          ))}
        </div>
      )}

      {/* Tabs */}
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
        {tab === 'overview' && <OverviewTab detail={detail} onRevoke={onRevoke} busy={busy} />}
        {tab === 'skills' && <SkillsTab detail={detail} onAdopt={onAdopt} />}
        {tab === 'mcp' && <McpTab detail={detail} />}
        {tab === 'plugins' && <PluginsTab detail={detail} />}
      </div>
    </div>
  )
}

function OverviewTab({
  detail,
  onRevoke,
  busy,
}: {
  detail: AgentDetail
  onRevoke: (packId: string, agentId: string) => void
  busy: boolean
}) {
  return (
    <>
      <div className="sm2__detail-section">
        <h4>已应用技能包（{detail.appliedPacks.length}）</h4>
        {detail.appliedPacks.length === 0 ? (
          <div className="sm2__empty" style={{ padding: 8 }}>无</div>
        ) : (
          detail.appliedPacks.map((p) => (
            <div key={p.packId} className="sm2__target-row">
              <span>{p.packName} · {p.memberCount} 成员</span>
              <button className="sm2__btn sm2__btn--danger" disabled={busy} onClick={() => onRevoke(p.packId, detail.id)}>
                撤销
              </button>
            </div>
          ))
        )}
      </div>
    </>
  )
}

function SkillsTab({
  detail,
  onAdopt,
}: {
  detail: AgentDetail
  onAdopt: (agentId: string, unmanagedId: string) => void
}) {
  const all = useSkillStoreV2((s) => s.unmanaged)
  const unmanaged = all.filter((u) => u.agentId === detail.id)
  return (
    <>
      <div className="sm2__detail-section">
        <h4>已管理 Skills（{detail.skills.length}）</h4>
        {detail.skills.length === 0 ? (
          <div className="sm2__empty" style={{ padding: 8 }}>无</div>
        ) : (
          detail.skills.map((s) => (
            <div key={s.id} className="sm2__target-row">
              <div>
                <strong>{s.targetPath.split('/').pop()}</strong>
                <div style={{ fontSize: 11, color: 'var(--text-secondary)' }}>
                  {s.actualMode} · {s.status} · {s.claims.length} claim(s)
                </div>
              </div>
            </div>
          ))
        )}
      </div>
      <div className="sm2__detail-section">
        <h4>未管理 Skills（{unmanaged.length}）</h4>
        {unmanaged.length === 0 ? (
          <div className="sm2__empty" style={{ padding: 8 }}>无未管理 Skill</div>
        ) : (
          unmanaged.map((u) => (
            <div key={u.id} className="sm2__target-row">
              <div>
                <strong>{u.inferredSkillId}</strong>
                <div style={{ fontSize: 11, color: 'var(--text-secondary)' }}>{u.path}</div>
                <div style={{ fontSize: 11, color: 'var(--text-secondary)' }}>{u.reason}</div>
              </div>
              <button className="sm2__btn sm2__btn--primary" onClick={() => onAdopt(detail.id, u.id)}>接管</button>
            </div>
          ))
        )}
      </div>
    </>
  )
}

function McpTab({ detail }: { detail: AgentDetail }) {
  if (detail.mcpServers.length === 0)
    return <div className="sm2__empty" style={{ padding: 12 }}>未配置 MCP 服务器</div>
  return (
    <>
      {detail.mcpServers.map((m) => (
        <div key={m.name} className="sm2__target-row">
          <div>
            <strong>{m.name}</strong>
            <div style={{ fontSize: 11, color: 'var(--text-secondary)' }}>
              <code>{m.command} {m.args.join(' ')}</code>
            </div>
          </div>
          <span className={`sm2__tag sm2__tag--${m.valid ? 'ok' : 'conflict'}`}>{m.valid ? '有效' : '异常'}</span>
        </div>
      ))}
    </>
  )
}

function PluginsTab({ detail }: { detail: AgentDetail }) {
  if (detail.plugins.length === 0)
    return <div className="sm2__empty" style={{ padding: 12 }}>未检测到插件</div>
  return (
    <>
      {detail.plugins.map((p) => (
        <div key={p.id} className="sm2__target-row">
          <div>
            <strong>{p.name}</strong>
            <div style={{ fontSize: 11, color: 'var(--text-secondary)' }}>
              {p.source} {p.version ? `· v${p.version}` : ''}
            </div>
          </div>
          <span className={`sm2__tag sm2__tag--${p.enabled ? 'ok' : 'unmanaged'}`}>{p.enabled ? '启用' : '禁用'}</span>
        </div>
      ))}
    </>
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
