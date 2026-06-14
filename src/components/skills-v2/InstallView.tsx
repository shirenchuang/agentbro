import { useEffect, useState } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import { skillApiV2 } from '../../services/skillApiV2'
import { skillApi } from '../../services/skillApi'
import type { AddCenterSkillPreview, AddCenterSkillDecision } from '../../services/skillApiV2'

type Tab = 'market' | 'local' | 'git'

interface MarketItem {
  id: string
  name: string
  description: string
  author?: string
  category?: string
  source?: string
  sourceType?: string
  accent?: string
}

export function InstallView({ onBack, onDone }: { onBack: () => void; onDone: () => void }) {
  const [tab, setTab] = useState<Tab>('market')
  const [gitUrl, setGitUrl] = useState('')

  const installFromSource = (source?: string) => {
    if (source) setGitUrl(source)
    setTab('git')
  }

  return (
    <div className="sm2__install">
      <div className="sm2__install-header">
        <button className="sm2__btn sm2__btn--ghost" onClick={onBack}>← 返回 Skill 库</button>
        <h2 className="sm2__title">添加到中心库</h2>
      </div>

      <div className="sm2__install-tabs">
        <button className={`sm2__addtab${tab === 'market' ? ' sm2__addtab--active' : ''}`} onClick={() => setTab('market')}>
          浏览市场
        </button>
        <button className={`sm2__addtab${tab === 'local' ? ' sm2__addtab--active' : ''}`} onClick={() => setTab('local')}>
          本地安装
        </button>
        <button className={`sm2__addtab${tab === 'git' ? ' sm2__addtab--active' : ''}`} onClick={() => setTab('git')}>
          Git 安装
        </button>
      </div>

      <div className="sm2__install-body settings-scroll">
        {tab === 'market' && <MarketPanel onInstall={installFromSource} />}
        {tab === 'local' && <LocalPanel onDone={onDone} />}
        {tab === 'git' && <GitPanel initialUrl={gitUrl} onDone={onDone} />}
      </div>
    </div>
  )
}

// ── Marketplace ──────────────────────────────────────────────────

function MarketPanel({ onInstall }: { onInstall: (source?: string) => void }) {
  const [items, setItems] = useState<MarketItem[]>([])
  const [query, setQuery] = useState('')
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    skillApi
      .listMarketplaceItems()
      .then((list) => setItems(list as MarketItem[]))
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false))
  }, [])

  const filtered = items.filter((it) => {
    const q = query.trim().toLowerCase()
    if (!q) return true
    return [it.name, it.description, it.author, it.category].filter(Boolean).join(' ').toLowerCase().includes(q)
  })

  return (
    <div className="sm2__install-market">
      <div className="sm2__install-searchrow">
        <input
          className="sm2__search"
          placeholder="搜索技能名称、描述或标签"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </div>
      {loading ? (
        <div className="sm2__empty">加载市场…</div>
      ) : error ? (
        <div className="sm2__error" style={{ margin: 0 }}>{error}</div>
      ) : filtered.length === 0 ? (
        <div className="sm2__empty">
          {items.length === 0
            ? '市场源为空。可在「设置」中添加市场源,或使用本地 / Git 安装。'
            : '没有匹配的技能。'}
        </div>
      ) : (
        <div className="sm2__install-grid">
          {filtered.map((it) => (
            <div key={it.id} className="sm2__install-card">
              <div className="sm2__install-card-accent" style={{ background: it.accent || '#34C759' }} />
              <div className="sm2__install-card-body">
                <div className="sm2__install-card-title">{it.name}</div>
                <div className="sm2__install-card-sub">{it.author ? `@${it.author}` : it.sourceType || 'market'}</div>
                <div className="sm2__install-card-desc">{it.description}</div>
              </div>
              <div className="sm2__install-card-foot">
                {it.category && <span className="sm2__tag">{it.category}</span>}
                <button className="sm2__btn sm2__btn--primary" onClick={() => onInstall(it.source)}>
                  安装
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

// ── Local ────────────────────────────────────────────────────────

function LocalPanel({ onDone }: { onDone: () => void }) {
  const [sourcePath, setSourcePath] = useState('')
  const [multi, setMulti] = useState(false)
  const [preview, setPreview] = useState<AddCenterSkillPreview | null>(null)
  const [renames, setRenames] = useState<Record<string, string>>({})
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const chooseFolder = async () => {
    const dir = await open({ directory: true, multiple: false })
    if (typeof dir === 'string') setSourcePath(dir)
  }
  const chooseZip = async () => {
    const f = await open({ filters: [{ name: '压缩包', extensions: ['zip', 'tar', 'gz'] }], multiple: false })
    if (typeof f === 'string') setSourcePath(f)
  }

  const runPreview = async () => {
    if (!sourcePath) {
      setError('请先选择来源')
      return
    }
    setBusy(true)
    setError(null)
    try {
      const p = await skillApiV2.previewAddCenterSkill({ sourcePath, sourceType: 'local_folder', multi })
      setPreview(p)
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
      const decisions: AddCenterSkillDecision[] = preview.blockers.map((b) => {
        const renamed = renames[b.skillId]?.trim()
        return renamed
          ? { skillId: b.skillId, proposedSkillId: renamed, resolution: 'create' }
          : { skillId: b.skillId, resolution: 'skip' }
      })
      const r = await skillApiV2.executeAddCenterSkill({ sourcePath, sourceType: 'local_folder', multi }, decisions)
      alert(`导入完成：新增 ${r.skillIds.length}，更新 ${r.updated.length}，跳过 ${r.skipped.length}`)
      onDone()
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  if (preview) {
    return (
      <div className="sm2__install-form">
        <h3 className="sm2__install-h">确认导入预览</h3>
        {preview.candidates.map((c) => (
          <div key={c.skillId} className="sm2__change">
            <strong>{c.name}</strong> → <code>{c.proposedSkillId}</code>{' '}
            <span className={`sm2__tag sm2__tag--${c.action === 'update' ? 'ok' : 'unmanaged'}`}>
              {c.action === 'update' ? '更新' : '新增'}
            </span>
          </div>
        ))}
        {preview.blockers.map((b) => (
          <div key={b.skillId} className="sm2__change sm2__change--blocked">
            <strong>{b.skillId}</strong>：{b.reason}
            <div className="sm2__field" style={{ marginTop: 6 }}>
              <label>重命名为（留空则跳过）</label>
              <input value={renames[b.skillId] || ''} onChange={(e) => setRenames({ ...renames, [b.skillId]: e.target.value })} placeholder={`${b.skillId}-rename`} />
            </div>
          </div>
        ))}
        {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
        <div className="sm2__btn-row">
          <button className="sm2__btn" onClick={() => setPreview(null)} disabled={busy}>返回</button>
          <button className="sm2__btn sm2__btn--primary" onClick={execute} disabled={busy}>{busy ? '处理中…' : '执行导入'}</button>
        </div>
      </div>
    )
  }

  return (
    <div className="sm2__install-form">
      <h3 className="sm2__install-h">从本地导入</h3>
      <p className="sm2__install-sub">支持文件夹、压缩包,以及批量导入一个含多个 Skill 的目录。</p>

      <div className="sm2__install-options">
        <button className="sm2__install-option" onClick={chooseFolder}>
          <span className="sm2__install-option-icon">📁</span>
          <span className="sm2__install-option-label">选择文件夹</span>
        </button>
        <button className="sm2__install-option" onClick={chooseZip}>
          <span className="sm2__install-option-icon">🗜️</span>
          <span className="sm2__install-option-label">选择压缩包 (.zip)</span>
        </button>
      </div>

      <div className="sm2__field">
        <label>来源路径</label>
        <input value={sourcePath} onChange={(e) => setSourcePath(e.target.value)} placeholder="选择或粘贴包含 SKILL.md 的目录 / .zip" />
      </div>
      <label className="sm2__checkbox-row">
        <input type="checkbox" checked={multi} onChange={(e) => setMulti(e.target.checked)} />
        批量导入（该目录包含多个 Skill）
      </label>

      {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
      <p style={{ fontSize: 12, color: 'var(--text-secondary)' }}>
        每个 Skill 目录必须包含 SKILL.md。同名不同来源会被阻止并要求选择处理方式。
      </p>
      <div className="sm2__btn-row">
        <button className="sm2__btn sm2__btn--primary" onClick={runPreview} disabled={busy || !sourcePath}>
          {busy ? '处理中…' : '预览导入'}
        </button>
      </div>
    </div>
  )
}

// ── Git ──────────────────────────────────────────────────────────

function GitPanel({ initialUrl, onDone }: { initialUrl?: string; onDone: () => void }) {
  const [url, setUrl] = useState(initialUrl || '')
  const [branch, setBranch] = useState('')
  const [token, setToken] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [status, setStatus] = useState<string | null>(null)

  const run = async () => {
    if (!url.trim()) {
      setError('请输入 Git 仓库 URL')
      return
    }
    setBusy(true)
    setError(null)
    setStatus(null)
    try {
      const repo = await skillApi.previewGitHubRepoImport(url.trim())
      const selections = repo.skills.map((s) => ({ sourcePath: s.sourcePath, resolution: 'overwrite' as const }))
      if (selections.length === 0) {
        setError('该仓库未检测到任何 Skill（需含 SKILL.md）')
        return
      }
      await skillApi.importGitHubRepoSkills(url.trim(), selections)
      await skillApiV2.refresh()
      setStatus(`已导入 ${selections.length} 个 Skill 到中心库`)
      onDone()
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="sm2__install-form">
      <h3 className="sm2__install-h">从 Git 仓库克隆并导入</h3>
      <p className="sm2__install-sub">输入 GitHub / Git 仓库地址,自动检测并导入其中的 Skill。</p>
      <div className="sm2__field">
        <label>仓库 URL</label>
        <input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://github.com/username/repo" />
      </div>
      <div className="sm2__field">
        <label>分支（可选）</label>
        <input value={branch} onChange={(e) => setBranch(e.target.value)} placeholder="main" />
      </div>
      <div className="sm2__field">
        <label>访问令牌（私有仓库可选）</label>
        <input value={token} onChange={(e) => setToken(e.target.value)} type="password" placeholder="ghp_..." />
      </div>
      {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
      {status && <div className="sm2__change" style={{ background: 'rgba(52,199,89,0.12)' }}>{status}</div>}
      <div className="sm2__btn-row">
        <button className="sm2__btn sm2__btn--primary" onClick={run} disabled={busy}>
          {busy ? '克隆中…' : '安装'}
        </button>
      </div>
    </div>
  )
}
