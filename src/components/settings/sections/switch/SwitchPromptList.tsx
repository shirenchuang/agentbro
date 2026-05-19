import { useEffect, useState } from 'react'
import { useSwitchStore } from '../../../../stores/switchStore'
import { SwitchPromptEditor } from './SwitchPromptEditor'
import type { SwitchPrompt } from '../../../../services/switchApi'

export function SwitchPromptList() {
  const { prompts, activeAppType, error, loadPrompts, deletePrompt, togglePrompt, applyPrompts, clearError } = useSwitchStore()
  const [editingPrompt, setEditingPrompt] = useState<SwitchPrompt | null>(null)
  const [showEditor, setShowEditor] = useState(false)
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null)
  const [applying, setApplying] = useState(false)

  useEffect(() => {
    loadPrompts()
  }, [activeAppType])

  useEffect(() => {
    if (error) {
      const t = setTimeout(clearError, 5000)
      return () => clearTimeout(t)
    }
  }, [error])

  const handleNew = () => {
    setEditingPrompt(null)
    setShowEditor(true)
  }

  const handleEdit = (prompt: SwitchPrompt) => {
    setEditingPrompt(prompt)
    setShowEditor(true)
  }

  const handleEditorClose = () => {
    setShowEditor(false)
    setEditingPrompt(null)
  }

  const confirmDelete = () => {
    if (confirmDeleteId) {
      deletePrompt(confirmDeleteId)
      setConfirmDeleteId(null)
    }
  }

  const handleApply = async () => {
    setApplying(true)
    await applyPrompts()
    setApplying(false)
  }

  if (showEditor) {
    return <SwitchPromptEditor prompt={editingPrompt} onClose={handleEditorClose} />
  }

  return (
    <div>
      {error && <div className="switch-error">{error}</div>}

      <div className="switch-provider-list__header">
        <h3>Prompt 列表</h3>
        <div style={{ display: 'flex', gap: 6 }}>
          <button
            type="button"
            className="switch-btn switch-btn--small"
            disabled={applying}
            onClick={handleApply}
          >
            {applying ? '写入中...' : '写入配置'}
          </button>
          <button type="button" className="switch-btn switch-btn--primary" onClick={handleNew}>
            + 添加 Prompt
          </button>
        </div>
      </div>

      {prompts.length === 0 && (
        <div className="switch-empty">
          <span>当前未配置任何 Prompt</span>
          <button type="button" className="switch-btn switch-btn--primary" onClick={handleNew}>
            + 添加 Prompt
          </button>
        </div>
      )}

      <div className="switch-provider-cards">
        {prompts.map((p) => (
          <div key={p.id} className="switch-prompt-card">
            <div className="switch-prompt-card__header">
              <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                <label className="switch-prompt-toggle">
                  <input
                    type="checkbox"
                    checked={p.enabled}
                    onChange={() => togglePrompt(p.id)}
                  />
                </label>
                <div>
                  <strong style={{ fontSize: 14, color: 'var(--settings-text-primary)' }}>{p.name}</strong>
                  {p.description && <div style={{ fontSize: 12, color: 'var(--settings-text-secondary)', marginTop: 1 }}>{p.description}</div>}
                </div>
              </div>
              <div className="switch-provider-card__actions" style={{ opacity: 1 }}>
                <button type="button" className="switch-btn switch-btn--small" onClick={() => handleEdit(p)}>
                  编辑
                </button>
                <button type="button" className="switch-btn switch-btn--small switch-btn--danger" onClick={() => setConfirmDeleteId(p.id)}>
                  删除
                </button>
              </div>
            </div>
            <div className="switch-prompt-card__preview">
              {p.content.slice(0, 200)}{p.content.length > 200 ? '...' : ''}
            </div>
          </div>
        ))}
      </div>

      {confirmDeleteId && (
        <div className="switch-confirm-overlay">
          <div className="switch-confirm-dialog">
            <p>确定删除该 Prompt？此操作不可撤销。</p>
            <div className="switch-confirm-dialog__actions">
              <button type="button" className="switch-btn" onClick={() => setConfirmDeleteId(null)}>取消</button>
              <button type="button" className="switch-btn switch-btn--danger" onClick={confirmDelete}>删除</button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
