import { useCallback, useMemo, useState } from 'react'
import { skillApi } from '../../services/skillApi'
import { skillApiV2 } from '../../services/skillApiV2'
import type { GitHubRepoPreview } from '../../services/skillApi'
import { OFFICIAL_PUBLISHERS, RECOMMENDED_SKILLS, TAG_LABELS, ALL_TAGS } from '../../data/officialSources'
import type { OfficialPublisher, OfficialRepo, RecommendedSkill, SkillTag } from '../../data/officialSources'

type View = 'publishers' | 'recommended'

export function OfficialSourcesPanel({ onDone }: { onDone: (skillId?: string) => void }) {
  const [view, setView] = useState<View>('recommended')
  const [query, setQuery] = useState('')

  return (
    <div className="sm2__official">
      <div className="sm2__official-toolbar">
        <div className="sm2__view-toggle sm2__view-toggle--soft">
          <button className={view === 'recommended' ? 'active' : ''} onClick={() => setView('recommended')}>精选推荐</button>
          <button className={view === 'publishers' ? 'active' : ''} onClick={() => setView('publishers')}>官方发布者</button>
        </div>
        <div className="sm2__search-wrapper" style={{ maxWidth: 320 }}>
          <span className="sm2__search-icon">⌕</span>
          <input
            className="sm2__search sm2__search--with-icon"
            placeholder={view === 'recommended' ? '搜索推荐 Skill…' : '搜索发布者…'}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
      </div>

      {view === 'recommended' ? (
        <RecommendedView query={query} onDone={onDone} />
      ) : (
        <PublishersView query={query} onDone={onDone} />
      )}
    </div>
  )
}

function RecommendedView({ query, onDone }: { query: string; onDone: (skillId?: string) => void }) {
  const [tagFilter, setTagFilter] = useState<SkillTag | ''>('')
  const [installing, setInstalling] = useState<string | null>(null)
  const [status, setStatus] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    return RECOMMENDED_SKILLS.filter((s) => {
      if (tagFilter && !s.tags.includes(tagFilter)) return false
      if (!q) return true
      return [s.name, s.description, s.publisher].join(' ').toLowerCase().includes(q)
    })
  }, [query, tagFilter])

  const install = async (skill: RecommendedSkill) => {
    setInstalling(skill.name)
    setError(null)
    setStatus(null)
    try {
      const repoUrl = `https://github.com/${skill.repoFullName}`
      const preview = await skillApi.previewGitHubRepoImport(repoUrl)
      const target = preview.skills.find((s) => s.skillName === skill.name || s.skillId === skill.name)
      if (!target) {
        setError(`在仓库 ${skill.repoFullName} 中未找到 Skill「${skill.name}」`)
        return
      }
      await skillApi.importGitHubRepoSkills(repoUrl, [{ sourcePath: target.sourcePath, resolution: 'overwrite' }])
      await skillApiV2.refresh()
      setStatus(`已安装「${skill.name}」到中心库`)
      onDone(skill.name)
    } catch (e) {
      setError(String(e))
    } finally {
      setInstalling(null)
    }
  }

  return (
    <div>
      <div className="sm2__official-tags">
        <button
          className={`sm2__source-chip${tagFilter === '' ? ' sm2__source-chip--active' : ''}`}
          onClick={() => setTagFilter('')}
        >
          全部
        </button>
        {ALL_TAGS.map((tag) => (
          <button
            key={tag}
            className={`sm2__source-chip${tagFilter === tag ? ' sm2__source-chip--active' : ''}`}
            onClick={() => setTagFilter(tag)}
          >
            {TAG_LABELS[tag].zh}
          </button>
        ))}
      </div>

      {status && <div className="sm2__notice sm2__notice--ok">{status}</div>}
      {error && <div className="sm2__error" style={{ margin: '10px 0' }}>{error}</div>}

      {filtered.length === 0 ? (
        <div className="sm2__empty">没有匹配的推荐 Skill。</div>
      ) : (
        <div className="sm2__install-grid">
          {filtered.map((skill) => (
            <div key={skill.name} className="sm2__install-card">
              <div className="sm2__install-card-accent" />
              <div className="sm2__install-card-head">
                <div className="sm2__market-skill-icon">{skill.publisher.slice(0, 2)}</div>
                <div className="sm2__install-card-body">
                  <div className="sm2__install-card-title">{skill.name}</div>
                  <div className="sm2__install-card-sub">{skill.publisher} · {skill.repoFullName}</div>
                </div>
                <button
                  className="sm2__icon-btn sm2__icon-btn--add"
                  title="安装到中心库"
                  disabled={installing === skill.name}
                  onClick={() => install(skill)}
                >
                  {installing === skill.name ? '…' : '+'}
                </button>
              </div>
              <div className="sm2__install-card-desc">{skill.description}</div>
              <div className="sm2__install-card-meta">
                {skill.tags.map((tag) => (
                  <span key={tag} className="sm2__tag">{TAG_LABELS[tag].zh}</span>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

function PublishersView({ query, onDone }: { query: string; onDone: (skillId?: string) => void }) {
  const [selectedPublisher, setSelectedPublisher] = useState<OfficialPublisher | null>(null)
  const [selectedRepo, setSelectedRepo] = useState<OfficialRepo | null>(null)
  const [repoPreview, setRepoPreview] = useState<GitHubRepoPreview | null>(null)
  const [loading, setLoading] = useState(false)
  const [installing, setInstalling] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [status, setStatus] = useState<string | null>(null)

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return OFFICIAL_PUBLISHERS
    return OFFICIAL_PUBLISHERS.filter((p) =>
      [p.name, p.slug, ...p.repos.map((r) => r.fullName)].join(' ').toLowerCase().includes(q)
    )
  }, [query])

  const openRepo = useCallback(async (publisher: OfficialPublisher, repo: OfficialRepo) => {
    setSelectedPublisher(publisher)
    setSelectedRepo(repo)
    setRepoPreview(null)
    setError(null)
    setStatus(null)
    setLoading(true)
    try {
      const preview = await skillApi.previewGitHubRepoImport(repo.url)
      setRepoPreview(preview)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [])

  const installSkill = async (sourcePath: string, skillName: string) => {
    if (!selectedRepo) return
    setInstalling(sourcePath)
    setError(null)
    setStatus(null)
    try {
      await skillApi.importGitHubRepoSkills(selectedRepo.url, [{ sourcePath, resolution: 'overwrite' }])
      await skillApiV2.refresh()
      setStatus(`已安装「${skillName}」到中心库`)
      onDone(skillName)
    } catch (e) {
      setError(String(e))
    } finally {
      setInstalling(null)
    }
  }

  const installAll = async () => {
    if (!repoPreview || !selectedRepo) return
    setInstalling('__all__')
    setError(null)
    setStatus(null)
    try {
      const selections = repoPreview.skills.map((s) => ({ sourcePath: s.sourcePath, resolution: 'overwrite' as const }))
      await skillApi.importGitHubRepoSkills(selectedRepo.url, selections)
      await skillApiV2.refresh()
      setStatus(`已安装 ${selections.length} 个 Skill 到中心库`)
      onDone()
    } catch (e) {
      setError(String(e))
    } finally {
      setInstalling(null)
    }
  }

  if (selectedRepo && selectedPublisher) {
    return (
      <div className="sm2__official-repo">
        <button className="sm2__btn sm2__btn--ghost" onClick={() => { setSelectedRepo(null); setSelectedPublisher(null); setRepoPreview(null) }}>
          ← 返回发布者列表
        </button>
        <div className="sm2__official-repo-hero">
          <div>
            <h3>{selectedPublisher.name} / {selectedRepo.fullName.split('/')[1]}</h3>
            {selectedRepo.description && <p>{selectedRepo.description}</p>}
          </div>
          {repoPreview && repoPreview.skills.length > 0 && (
            <button
              className="sm2__btn sm2__btn--primary"
              disabled={installing === '__all__'}
              onClick={installAll}
            >
              {installing === '__all__' ? '安装中…' : `全部安装 (${repoPreview.skills.length})`}
            </button>
          )}
        </div>

        {status && <div className="sm2__notice sm2__notice--ok">{status}</div>}
        {error && <div className="sm2__error" style={{ margin: '10px 0' }}>{error}</div>}

        {loading ? (
          <div className="sm2__empty">正在扫描仓库中的 Skills…</div>
        ) : repoPreview && repoPreview.skills.length === 0 ? (
          <div className="sm2__empty">该仓库未检测到包含 SKILL.md 的 Skill。</div>
        ) : repoPreview ? (
          <div className="sm2__market-list">
            {repoPreview.skills.map((skill) => (
              <div key={skill.sourcePath} className="sm2__market-item">
                <div className="sm2__market-skill-icon">{skill.skillName.slice(0, 2).toUpperCase()}</div>
                <div className="sm2__market-item-main">
                  <div className="sm2__market-item-title">
                    <strong>{skill.skillName}</strong>
                  </div>
                  <div className="sm2__market-item-meta">
                    {skill.description && <span style={{ color: '#64748b', fontSize: 12 }}>{skill.description}</span>}
                  </div>
                </div>
                <button
                  className="sm2__icon-btn sm2__icon-btn--add"
                  title="安装到中心库"
                  disabled={installing === skill.sourcePath}
                  onClick={() => installSkill(skill.sourcePath, skill.skillName)}
                >
                  {installing === skill.sourcePath ? '…' : '+'}
                </button>
              </div>
            ))}
          </div>
        ) : null}
      </div>
    )
  }

  return (
    <div>
      <p className="sm2__market-note">
        {filtered.length} 个官方发布者，点击仓库查看并安装其中的 Skills。
      </p>
      {filtered.length === 0 ? (
        <div className="sm2__empty">没有匹配的发布者。</div>
      ) : (
        <div className="sm2__official-list">
          {filtered.map((publisher) => (
            <div key={publisher.slug} className="sm2__official-publisher">
              <div className="sm2__official-publisher-head">
                <div className="sm2__market-skill-icon">{publisher.name.slice(0, 2)}</div>
                <div>
                  <strong>{publisher.name}</strong>
                  <span>{publisher.totalSkills} skills · {publisher.repos.length} 仓库</span>
                </div>
              </div>
              <div className="sm2__official-repos">
                {publisher.repos.map((repo) => (
                  <button
                    key={repo.fullName}
                    className="sm2__official-repo-chip"
                    onClick={() => openRepo(publisher, repo)}
                  >
                    <span className="sm2__official-repo-name">{repo.fullName}</span>
                    <span className="sm2__official-repo-count">{repo.skillCount}</span>
                  </button>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
