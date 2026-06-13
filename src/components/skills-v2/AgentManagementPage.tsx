import { useEffect, useState } from 'react'
import { useSkillStoreV2 } from '../../stores/skillStoreV2'
import { skillApiV2 } from '../../services/skillApiV2'
import type { AgentSummary, AdoptPreview } from '../../services/skillApiV2'
import { AgentIconBadge } from './AgentIconBadge'
import { PreviewDialog } from './PreviewDialog'

export function AgentManagementPage() {
  const state = useSkillStoreV2()
  const [adopt, setAdopt] = useState<AdoptPreview | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    state.loadOverview()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const agents = state.agents

  return (
    <div className="sm2">
      <div className="sm2__header">
        <h2 className="sm2__title">Agent 管理</h2>
        <div className="sm2__tabs">
          <button className="sm2__btn" onClick={() => state.loadOverview()}>刷新</button>
        </div>
      </div>

      <div className="sm2__body">
        <div className="sm2__main" style={{ maxWidth: 320 }}>
          <div className="sm2__list">
            {agents.map((a) => (
              <AgentRow
                key={a.id}
                agent={a}
                selected={state.selectedAgentId === a.id}
                onClick={() => state.selectAgent(a.id)}
              />
            ))}
          </div>
        </div>
        <AgentDetailPanel
          agent={state.selectedAgentDetail}
          onAdopt={(agentId, unmanagedId) => openAdopt(agentId, unmanagedId, setAdopt, setBusy)}
          onRevoke={async (packId, agentId) => {
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
          }}
          busy={busy}
        />
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

async function openAdopt(
  agentId: string,
  unmanagedId: string,
  setAdopt: (p: AdoptPreview | null) => void,
  setBusy: (b: boolean) => void,
) {
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

function AgentRow({
  agent,
  selected,
  onClick,
}: {
  agent: AgentSummary
  selected: boolean
  onClick: () => void
}) {
  return (
    <div className={`sm2__row${selected ? ' sm2__row--selected' : ''}`} onClick={onClick}>
      <AgentIconBadge iconKey={agent.iconKey} title={agent.displayName} size={26} />
      <div className="sm2__row-main">
        <div className="sm2__row-title">{agent.displayName}</div>
        <div className="sm2__row-sub">
          {agent.installed ? '已安装' : '未检测到'} · 管理 {agent.managedSkillCount} · 未管理 {agent.unmanagedSkillCount}
        </div>
      </div>
    </div>
  )
}

function AgentDetailPanel({
  agent,
  onAdopt,
  onRevoke,
  busy,
}: {
  agent: ReturnType<typeof useSkillStoreV2.getState>['selectedAgentDetail']
  onAdopt: (agentId: string, unmanagedId: string) => void
  onRevoke: (packId: string, agentId: string) => void
  busy: boolean
}) {
  if (!agent) {
    return (
      <div className="sm2__detail">
        <div className="sm2__empty" style={{ padding: 20 }}>选择一个 Agent 查看详情</div>
      </div>
    )
  }
  return (
    <div className="sm2__detail" style={{ flex: 1 }}>
      <h3>{agent.displayName}</h3>
      <div className="sm2__detail-meta">
        <div>版本：{agent.version || '未知'}</div>
        <div>Skills 目录：{agent.skillsDir || '—'}</div>
        {agent.mcpConfigPath && <div>MCP 配置：{agent.mcpConfigPath}</div>}
      </div>

      {agent.health.length > 0 && (
        <div className="sm2__detail-section">
          <h4>路径健康</h4>
          {agent.health.map((h, i) => (
            <div key={i} className="sm2__change sm2__change--blocked">{h.message}</div>
          ))}
        </div>
      )}

      <div className="sm2__detail-section">
        <h4>已安装 Skills（{agent.skills.length}）</h4>
        {agent.skills.length === 0 ? (
          <div style={{ fontSize: 12, color: 'var(--text-secondary)' }}>无</div>
        ) : (
          agent.skills.map((s) => (
            <div key={s.id} className="sm2__target-row">
              <div>
                <strong>{s.targetPath.split('/').pop()}</strong>
                <div style={{ fontSize: 11, color: 'var(--text-secondary)' }}>
                  {s.actualMode} · {s.status}
                </div>
              </div>
            </div>
          ))
        )}
      </div>

      <div className="sm2__detail-section">
        <h4>已应用技能包</h4>
        {agent.appliedPacks.length === 0 ? (
          <div style={{ fontSize: 12, color: 'var(--text-secondary)' }}>无</div>
        ) : (
          agent.appliedPacks.map((p) => (
            <div key={p.packId} className="sm2__target-row">
              <span>{p.packName}</span>
              <button className="sm2__btn sm2__btn--danger" disabled={busy} onClick={() => onRevoke(p.packId, agent.id)}>
                撤销
              </button>
            </div>
          ))
        )}
      </div>

      <div className="sm2__detail-section">
        <h4>MCP 服务器</h4>
        {agent.mcpServers.length === 0 ? (
          <div style={{ fontSize: 12, color: 'var(--text-secondary)' }}>未配置</div>
        ) : (
          agent.mcpServers.map((m) => (
            <div key={m.name} className="sm2__target-row">
              <span>{m.name} <code style={{ fontSize: 11 }}>{m.command}</code></span>
              <span className={`sm2__tag sm2__tag--${m.valid ? 'ok' : 'conflict'}`}>{m.valid ? '有效' : '异常'}</span>
            </div>
          ))
        )}
      </div>

      <div className="sm2__detail-section">
        <h4>未管理 Skills</h4>
        <UnmanagedList agentId={agent.id} onAdopt={onAdopt} />
      </div>
    </div>
  )
}

function UnmanagedList({
  agentId,
  onAdopt,
}: {
  agentId: string
  onAdopt: (agentId: string, unmanagedId: string) => void
}) {
  const all = useSkillStoreV2((s) => s.unmanaged)
  const items = all.filter((u) => u.agentId === agentId)
  if (items.length === 0) {
    return <div style={{ fontSize: 12, color: 'var(--text-secondary)' }}>无未管理 Skill</div>
  }
  return (
    <>
      {items.map((u) => (
        <div key={u.id} className="sm2__target-row">
          <div>
            <strong>{u.inferredSkillId}</strong>
            <div style={{ fontSize: 11, color: 'var(--text-secondary)' }}>{u.path}</div>
            <div style={{ fontSize: 11, color: 'var(--text-secondary)' }}>{u.reason}</div>
          </div>
          <button className="sm2__btn sm2__btn--primary" onClick={() => onAdopt(agentId, u.id)}>
            接管
          </button>
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
      await skillApiV2.executeAdopt(
        preview.agentId,
        preview.unmanagedId,
        option,
        option === 'rename' ? renamedId : null,
      )
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
        <div>Hash：{preview.hash.slice(0, 12)}…</div>
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
