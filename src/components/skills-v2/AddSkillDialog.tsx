import { useEffect, useState } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import { skillApiV2 } from '../../services/skillApiV2'
import { skillApi } from '../../services/skillApi'
import type { AddCenterSkillPreview, AddCenterSkillDecision } from '../../services/skillApiV2'

type Tab = 'market' | 'local' | 'git'

export function AddSkillDialog({
  onClose,
  onDone,
}: {
  onClose: () => void
  onDone: () => void
}) {
  const [tab, setTab] = useState<Tab>('local')
  const [gitUrl, setGitUrl] = useState('')

  const installFromSource = (source?: string) => {
    if (source) setGitUrl(source)
    setTab('git')
  }

  return (
    <div className="sm2__overlay" onClick={onClose}>
      <div className="sm2__modal sm2__modal--wide" onClick={(e) => e.stopPropagation()}>
        <h3>添加到中心库</h3>
        <div className="sm2__addtabs">
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

        {tab === 'market' && <MarketTab onClose={onClose} onInstall={installFromSource} />}
        {tab === 'local' && <LocalTab onClose={onClose} onDone={onDone} />}
        {tab === 'git' && <GitTab initialUrl={gitUrl} onClose={onClose} onDone={onDone} />}
      </div>
    </div>
  )
}

function MarketTab({ onClose, onInstall }: { onClose: () => void; onInstall: (source?: string) => void }) {
  const [items, setItems] = useState<Array<{ id: string; name: string; description: string; author?: string; category?: string; source?: string }>>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    skillApi
      .listMarketplaceItems()
      .then((list) => setItems(list as never[]))
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false))
  }, [])

  const install = (item: { source?: string; name: string }) => {
    // Hand off to the Git tab pre-filled with the item's source — the git flow
    // clones and imports into the center library.
    if (!item.source) {
      alert('该市场条目没有可解析的来源地址,请手动用 Git 安装。')
      return
    }
    onInstall(item.source)
  }

  return (
    <div>
      <p className="sm2__addtab-sub">从市场查找并安装技能</p>
      {loading ? (
        <div className="sm2__empty" style={{ padding: 16 }}>加载市场…</div>
      ) : error ? (
        <div className="sm2__error" style={{ margin: 0 }}>{error}</div>
      ) : items.length === 0 ? (
        <div className="sm2__empty" style={{ padding: 24 }}>
          市场源为空。可在「设置」中添加市场源，或使用「本地安装 / Git 安装」。
        </div>
      ) : (
        <div className="sm2__scroll" style={{ maxHeight: 320 }}>
          {items.map((it) => (
            <div key={it.id} className="sm2__target-row" style={{ alignItems: 'center' }}>
              <div>
                <strong>{it.name}</strong>
                <div style={{ fontSize: 11, color: 'var(--text-secondary)' }}>{it.description}</div>
                {it.author && <span className="sm2__tag">{it.author}</span>}
              </div>
              <button className="sm2__btn sm2__btn--primary" onClick={() => install(it)}>安装</button>
            </div>
          ))}
        </div>
      )}
      <div className="sm2__btn-row" style={{ justifyContent: 'flex-end' }}>
        <button className="sm2__btn" onClick={onClose}>关闭</button>
      </div>
    </div>
  )
}

function LocalTab({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
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
      onClose()
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  if (preview) {
    return (
      <div>
        <p className="sm2__addtab-sub">确认导入预览</p>
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
        <div className="sm2__btn-row" style={{ justifyContent: 'flex-end' }}>
          <button className="sm2__btn" onClick={() => setPreview(null)} disabled={busy}>返回</button>
          <button className="sm2__btn sm2__btn--primary" onClick={execute} disabled={busy}>{busy ? '处理中…' : '执行导入'}</button>
        </div>
      </div>
    )
  }

  return (
    <div>
      <p className="sm2__addtab-sub">从本地文件夹或压缩包导入</p>
      <div className="sm2__field">
        <label>来源</label>
        <div style={{ display: 'flex', gap: 8 }}>
          <input value={sourcePath} onChange={(e) => setSourcePath(e.target.value)} placeholder="选择包含 SKILL.md 的目录" style={{ flex: 1 }} />
          <button className="sm2__btn" type="button" onClick={chooseFolder}>选目录</button>
          <button className="sm2__btn" type="button" onClick={chooseZip}>选压缩包</button>
        </div>
      </div>
      <label className="sm2__checkbox-row">
        <input type="checkbox" checked={multi} onChange={(e) => setMulti(e.target.checked)} />
        该目录包含多个 Skill（批量导入）
      </label>
      {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
      <p style={{ fontSize: 12, color: 'var(--text-secondary)', marginTop: 8 }}>
        每个 Skill 目录必须包含 SKILL.md。同名不同来源会被阻止并要求选择处理方式。
      </p>
      <div className="sm2__btn-row" style={{ justifyContent: 'flex-end' }}>
        <button className="sm2__btn" onClick={onClose} disabled={busy}>取消</button>
        <button className="sm2__btn sm2__btn--primary" onClick={runPreview} disabled={busy || !sourcePath}>{busy ? '处理中…' : '预览'}</button>
      </div>
    </div>
  )
}

function GitTab({ initialUrl, onClose, onDone }: { initialUrl?: string; onClose: () => void; onDone: () => void }) {
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
      // Use the legacy github repo importer (clones + detects skills), then
      // rescan the v2 center so imported skills appear in the library.
      const repo = await skillApi.previewGitHubRepoImport(url.trim())
      const selections = repo.skills.map((s) => ({ sourcePath: s.sourcePath, resolution: 'overwrite' as const }))
      if (selections.length === 0) {
        setError('该仓库未检测到任何 Skill（需含 SKILL.md）')
        return
      }
      await skillApi.importGitHubRepoSkills(url.trim(), selections)
      await skillApiV2.refresh()
      setStatus(`已导入 ${selections.length} 个 Skill`)
      onDone()
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div>
      <p className="sm2__addtab-sub">从 Git 仓库克隆并导入</p>
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
      <div className="sm2__btn-row" style={{ justifyContent: 'flex-end' }}>
        <button className="sm2__btn" onClick={onClose} disabled={busy}>取消</button>
        <button className="sm2__btn sm2__btn--primary" onClick={run} disabled={busy}>{busy ? '克隆中…' : '安装'}</button>
      </div>
    </div>
  )
}
