import { useState } from 'react'
import { useSwitchStore } from '../../../../stores/switchStore'
import type { SwitchPrompt } from '../../../../services/switchApi'

interface Props {
  prompt: SwitchPrompt | null
  onClose: () => void
}

export function SwitchPromptEditor({ prompt, onClose }: Props) {
  const { activeAppType, createPrompt, updatePrompt } = useSwitchStore()
  const isNew = !prompt

  const [name, setName] = useState(prompt?.name ?? '')
  const [description, setDescription] = useState(prompt?.description ?? '')
  const [content, setContent] = useState(prompt?.content ?? '')
  const [saving, setSaving] = useState(false)

  const handleSave = async () => {
    if (!name.trim() || !content.trim()) return

    setSaving(true)
    const data: SwitchPrompt = {
      id: prompt?.id ?? crypto.randomUUID(),
      app_type: activeAppType,
      name: name.trim(),
      content: content,
      description: description || null,
      enabled: prompt?.enabled ?? true,
      created_at: prompt?.created_at ?? Math.floor(Date.now() / 1000),
      updated_at: Math.floor(Date.now() / 1000),
      sort_index: prompt?.sort_index ?? null,
    }

    try {
      if (isNew) {
        await createPrompt(data)
      } else {
        await updatePrompt(data)
      }
      onClose()
    } finally {
      setSaving(false)
    }
  }

  return (
    <div>
      <div className="switch-editor__header">
        <button type="button" className="switch-btn" onClick={onClose}>← 返回</button>
        <h3>{isNew ? '新建 Prompt' : `编辑「${prompt!.name}」`}</h3>
      </div>

      <div className="switch-editor__form">
        <label className="switch-field">
          <span>名称</span>
          <input type="text" value={name} onChange={(e) => setName(e.target.value)} placeholder="Prompt 名称" />
        </label>

        <label className="switch-field">
          <span>描述</span>
          <input type="text" value={description} onChange={(e) => setDescription(e.target.value)} placeholder="简要描述..." />
        </label>

        <div className="switch-field">
          <span>内容 (Markdown)</span>
          <textarea
            className="switch-textarea"
            rows={12}
            value={content}
            onChange={(e) => setContent(e.target.value)}
            placeholder="在此编写 Prompt 内容..."
          />
        </div>

        <div className="switch-editor__actions">
          <button type="button" className="switch-btn" onClick={onClose}>取消</button>
          <button
            type="button"
            className="switch-btn switch-btn--primary"
            disabled={!name.trim() || !content.trim() || saving}
            onClick={handleSave}
          >
            {saving ? '保存中...' : '保存'}
          </button>
        </div>
      </div>
    </div>
  )
}
