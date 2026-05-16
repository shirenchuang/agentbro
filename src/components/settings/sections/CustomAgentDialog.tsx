import { useEffect, useMemo, useState } from 'react'
import { agentApi, type AgentProgramInfo } from '../../../services/agentApi'
import { useAgentStore } from '../../../stores/agentStore'

const KNOWN_AGENTS = [
  { id: 'claude-code', label: 'Claude Code', defaultDir: '~/.claude/commands' },
  { id: 'codex', label: 'OpenAI Codex', defaultDir: '~/.codex/commands' },
  { id: 'gemini', label: 'Gemini CLI', defaultDir: '~/.gemini/commands' },
  { id: 'opencode', label: 'OpenCode', defaultDir: '~/.config/opencode/commands' },
  { id: 'cursor', label: 'Cursor', defaultDir: '~/.cursor/commands' },
  { id: 'cursor-cli', label: 'Cursor CLI', defaultDir: '~/.cursor/commands' },
  { id: 'copilot', label: 'GitHub Copilot', defaultDir: '~/.config/gh/commands' },
  { id: 'hermes', label: 'Hermes', defaultDir: '~/.hermes/commands' },
  { id: 'kiro', label: 'Kiro', defaultDir: '~/.kiro/commands' },
]

interface CustomAgentDialogProps {
  agent?: AgentProgramInfo | null
  onClose: () => void
  onSaved: () => void
}

export function CustomAgentDialog({ agent, onClose, onSaved }: CustomAgentDialogProps) {
  const editing = Boolean(agent?.isCustom)
  const { agents } = useAgentStore()

  // Filter out already-added custom agents (same base id)
  const existingCustomIds = useMemo(() => agents.filter(a => a.isCustom).map(a => a.id.replace(/-custom-\d+$/, '')), [agents])
  const availableAgents = useMemo(
    () => KNOWN_AGENTS.filter(a => !existingCustomIds.includes(a.id) || (editing && agent?.id === a.id)),
    [agent?.id, editing, existingCustomIds],
  )

  const [selectedId, setSelectedId] = useState(KNOWN_AGENTS[0].id)
  const [displayName, setDisplayName] = useState('')
  const [skillsDir, setSkillsDir] = useState('')
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')

  useEffect(() => {
    if (editing && agent) {
      setSelectedId(agent.id)
      setDisplayName(agent.displayName)
      setSkillsDir(agent.skillsDir ?? agent.configDir ?? '')
    } else {
      const first = availableAgents[0] ?? KNOWN_AGENTS[0]
      setSelectedId(first.id)
      setDisplayName('')
      setSkillsDir(first.defaultDir)
    }
    setError('')
  }, [agent, availableAgents, editing])

  const handleAgentChange = (id: string) => {
    setSelectedId(id)
    const found = KNOWN_AGENTS.find(a => a.id === id)
    if (found && !skillsDir) setSkillsDir(found.defaultDir)
  }

  const handleSubmit = async () => {
    const name = displayName.trim() || KNOWN_AGENTS.find(a => a.id === selectedId)?.label || selectedId
    const path = skillsDir.trim()
    if (!path) {
      setError('请填写 Skills 目录路径。')
      return
    }
    setSaving(true)
    setError('')
    try {
      if (editing && agent) {
        await agentApi.updateCustom(agent.id, { displayName: name, category: 'custom', globalSkillsDir: path })
      } else {
        // Don't pass the base agent id directly — let backend generate a unique id from displayName
        await agentApi.addCustom({ id: null, displayName: name, category: 'custom', globalSkillsDir: path })
      }
      onSaved()
      onClose()
    } catch (err) {
      setError(String(err))
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="skills-dialog-overlay" onClick={onClose}>
      <div className="skills-dialog custom-agent-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="skills-dialog__header">
          <div className="skills-dialog__title">{editing ? '编辑自定义 Agent' : '添加自定义 Agent'}</div>
        </div>

        <div className="skills-dialog__body">
          <div className="install-form-row">
            <label className="install-form-label">Agent 类型</label>
            <select
              className="install-form-input"
              value={selectedId}
              onChange={(e) => handleAgentChange(e.target.value)}
              disabled={editing}
            >
              {KNOWN_AGENTS.map(a => (
                <option key={a.id} value={a.id}>{a.label}</option>
              ))}
            </select>
            <div className="custom-agent-hint">自定义 Agent 使用相同引擎，但指向不同的 Skills 目录（如 AntCC 是 Claude Code 的自定义路径版本）。</div>
          </div>

          <div className="install-form-row">
            <label className="install-form-label">显示名称</label>
            <input
              className="install-form-input"
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              placeholder={KNOWN_AGENTS.find(a => a.id === selectedId)?.label}
            />
          </div>

          <div className="install-form-row">
            <label className="install-form-label">Skills 目录</label>
            <input
              className="install-form-input"
              value={skillsDir}
              onChange={(e) => setSkillsDir(e.target.value)}
              placeholder="~/.my-agent/commands"
            />
            <div className="custom-agent-hint">该目录会参与扫描、安装、卸载和 Agent 间同步。</div>
          </div>

          {error && <div className="custom-agent-error">{error}</div>}
        </div>

        <div className="skills-dialog__footer">
          <button type="button" className="skills-btn" onClick={onClose}>取消</button>
          <button type="button" className="skills-btn skills-btn--primary" onClick={handleSubmit} disabled={saving}>
            {saving ? '保存中...' : '保存'}
          </button>
        </div>
      </div>
    </div>
  )
}
