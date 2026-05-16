import { useEffect, useMemo, useState } from 'react'
import { useAgentStore } from '../../stores/agentStore'
import { useSkillStore } from '../../stores/skillStore'
import { skillApi, type SkillCollection } from '../../services/skillApi'
import { detectedAgentOptions, displayAgentName } from '../../utils/agentPrograms'

function blankCollection(): SkillCollection {
  const now = new Date().toISOString()
  return {
    id: `collection-${Date.now()}`,
    name: '',
    description: '',
    skills: [],
    createdAt: now,
    updatedAt: now,
  }
}

export function CollectionsView() {
  const { skills, collections, loadAll, loadCollections } = useSkillStore()
  const { agents, loadAgents } = useAgentStore()
  const [editing, setEditing] = useState<SkillCollection | null>(null)
  const [message, setMessage] = useState('')
  const [importText, setImportText] = useState('')
  const [targetAgents, setTargetAgents] = useState<Set<string> | null>(null)
  const installTargets = useMemo(() => {
    const installed = detectedAgentOptions(agents)
    return installed.length > 0 ? installed : agents
  }, [agents])

  useEffect(() => {
    loadCollections()
    if (agents.length === 0) loadAgents()
  }, [agents.length, loadAgents, loadCollections])

  const selectedTargetAgents = useMemo(() => {
    if (targetAgents) return targetAgents
    return installTargets[0] ? new Set([installTargets[0].id]) : new Set<string>()
  }, [installTargets, targetAgents])

  const toggleTarget = (agentId: string) => {
    setTargetAgents(prev => {
      const next = new Set(prev ?? selectedTargetAgents)
      if (next.has(agentId)) next.delete(agentId)
      else next.add(agentId)
      return next
    })
  }

  const saveCollection = async (collection: SkillCollection) => {
    await skillApi.upsertCollection(collection)
    setEditing(null)
    await loadCollections()
  }

  const deleteCollection = async (id: string) => {
    if (!window.confirm('确认删除这个集合？集合里的 Skill 不会被卸载。')) return
    await skillApi.deleteCollection(id)
    await loadCollections()
  }

  const exportCollection = async (collection: SkillCollection) => {
    const json = await skillApi.exportCollection(collection.id)
    setImportText(json)
    setMessage(`已导出 ${collection.name}，内容已放入下方导入/导出框。`)
  }

  const importCollection = async () => {
    if (!importText.trim()) return
    try {
      await skillApi.importCollection(importText)
      setImportText('')
      setMessage('集合导入完成。')
      await loadCollections()
    } catch (error) {
      setMessage(String(error))
    }
  }

  const installCollection = async (collection: SkillCollection) => {
    if (selectedTargetAgents.size === 0) return
    setMessage('')
    try {
      await skillApi.batchInstallCollection(collection, Array.from(selectedTargetAgents))
      await loadAll()
      setMessage(`已把 ${collection.name} 安装到 ${selectedTargetAgents.size} 个 Agent。`)
    } catch (error) {
      setMessage(String(error))
    }
  }

  return (
    <div className="capability-page">
      <div className="capability-page-head">
        <h1>🗂️ 技能集合</h1>
        <p>复用 skills-manage 的 Collections 工作流：整理、导入导出，并批量安装到目标 Agent。</p>
      </div>

      <div className="capability-page-body collection-view">
        <div className="plugin-manager-stats">
          <div><strong>{collections.length}</strong><span>集合</span></div>
          <div><strong>{collections.reduce((total, item) => total + item.skills.length, 0)}</strong><span>集合条目</span></div>
          <div><strong>{skills.length}</strong><span>可选 Skills</span></div>
          <div><strong>{selectedTargetAgents.size}</strong><span>目标 Agent</span></div>
        </div>

        <div className="plugin-manager-toolbar">
          <div className="install-targets collection-targets">
            {installTargets.map(agent => (
              <button
                key={agent.id}
                type="button"
                className={`install-target-chip ${selectedTargetAgents.has(agent.id) ? 'install-target-chip--selected' : ''}`}
                onClick={() => toggleTarget(agent.id)}
              >
                {agent.displayName}
              </button>
            ))}
          </div>
          <button className="skills-btn skills-btn--primary" onClick={() => setEditing(blankCollection())}>
            + 新建集合
          </button>
        </div>

        {message && <div className="sync-status">{message}</div>}

        <div className="collection-list">
          {collections.map(collection => (
            <div key={collection.id} className="collection-card">
              <div className="collection-card__main">
                <div className="collection-card__name">{collection.name}</div>
                <div className="collection-card__desc">{collection.description || '暂无描述'}</div>
                <div className="collection-card__chips">
                  {collection.skills.slice(0, 12).map(skillId => (
                    <span key={skillId}>{skills.find(skill => skill.id === skillId)?.name ?? skillId}</span>
                  ))}
                  {collection.skills.length > 12 && <span>+{collection.skills.length - 12}</span>}
                </div>
              </div>
              <div className="collection-card__actions">
                <button className="skills-btn skills-btn--primary skills-btn--small" onClick={() => installCollection(collection)}>
                  批量安装
                </button>
                <button className="skills-btn skills-btn--small" onClick={() => setEditing(collection)}>编辑</button>
                <button className="skills-btn skills-btn--small" onClick={() => exportCollection(collection)}>导出</button>
                <button className="skills-btn skills-btn--danger skills-btn--small" onClick={() => deleteCollection(collection.id)}>删除</button>
              </div>
            </div>
          ))}
        </div>

        {collections.length === 0 && (
          <div className="skills-empty">
            <div className="skills-empty__icon">🗂️</div>
            <div className="skills-empty__text">还没有技能集合</div>
            <div className="skills-empty__hint">创建集合后可以像 skills-manage 一样批量安装和迁移。</div>
          </div>
        )}

        <section className="sync-section collection-import-box">
          <div className="sync-section__title">导入 / 导出 JSON</div>
          <textarea
            className="install-form-input collection-import-textarea"
            value={importText}
            onChange={event => setImportText(event.target.value)}
            placeholder="粘贴 collection JSON，或点击上方集合的导出按钮。"
          />
          <button className="skills-btn skills-btn--small" onClick={importCollection} disabled={!importText.trim()}>
            导入集合
          </button>
        </section>

        {editing && (
          <CollectionEditor
            collection={editing}
            skills={skills}
            onClose={() => setEditing(null)}
            onSave={saveCollection}
          />
        )}

        <div className="collection-agent-note">
          当前目标：{Array.from(selectedTargetAgents).map(agentId => displayAgentName(agentId, agents)).join('、') || '未选择'}
        </div>
      </div>
    </div>
  )
}

function CollectionEditor({
  collection,
  skills,
  onClose,
  onSave,
}: {
  collection: SkillCollection
  skills: { id: string; name: string; description: string }[]
  onClose: () => void
  onSave: (collection: SkillCollection) => Promise<void>
}) {
  const [draft, setDraft] = useState(collection)
  const [saving, setSaving] = useState(false)
  const selected = new Set(draft.skills)

  const toggleSkill = (skillId: string) => {
    const next = new Set(draft.skills)
    if (next.has(skillId)) next.delete(skillId)
    else next.add(skillId)
    setDraft({ ...draft, skills: Array.from(next), updatedAt: new Date().toISOString() })
  }

  const save = async () => {
    setSaving(true)
    try {
      await onSave({ ...draft, updatedAt: new Date().toISOString() })
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="skills-dialog-overlay" onClick={onClose}>
      <div className="skills-dialog" onClick={event => event.stopPropagation()}>
        <div className="skills-dialog__header">
          <div className="skills-dialog__title">编辑集合</div>
        </div>
        <div className="skills-dialog__body">
          <div className="install-form-row">
            <label className="install-form-label">名称</label>
            <input
              className="install-form-input"
              value={draft.name}
              onChange={event => setDraft({ ...draft, name: event.target.value })}
              placeholder="例如：内容生产、代码审查、发布流程"
            />
          </div>
          <div className="install-form-row">
            <label className="install-form-label">描述</label>
            <input
              className="install-form-input"
              value={draft.description}
              onChange={event => setDraft({ ...draft, description: event.target.value })}
              placeholder="这个集合解决什么场景"
            />
          </div>
          <div className="install-form-row">
            <label className="install-form-label">包含 Skills</label>
            <div className="install-targets collection-skill-picker">
              {skills.map(skill => (
                <button
                  key={skill.id}
                  type="button"
                  className={`install-target-chip ${selected.has(skill.id) ? 'install-target-chip--selected' : ''}`}
                  onClick={() => toggleSkill(skill.id)}
                  title={skill.description}
                >
                  {skill.name}
                </button>
              ))}
            </div>
          </div>
        </div>
        <div className="skills-dialog__footer">
          <button className="skills-btn" onClick={onClose}>取消</button>
          <button className="skills-btn skills-btn--primary" onClick={save} disabled={saving || !draft.name.trim()}>
            {saving ? '保存中...' : '保存'}
          </button>
        </div>
      </div>
    </div>
  )
}
