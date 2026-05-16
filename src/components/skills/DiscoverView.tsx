import { useEffect, useMemo, useState } from 'react'
import { useAgentStore } from '../../stores/agentStore'
import { useSkillStore } from '../../stores/skillStore'
import { skillApi, type DiscoveredSkill, type ScanRoot } from '../../services/skillApi'
import { detectedAgentOptions } from '../../utils/agentPrograms'

export function DiscoverView() {
  const { agents, loadAgents } = useAgentStore()
  const { scanRoots, loadAll, loadObsidianVaults, loadScanRoots } = useSkillStore()
  const [rootsText, setRootsText] = useState('')
  const [skills, setSkills] = useState<DiscoveredSkill[]>([])
  const [scanning, setScanning] = useState(false)
  const [installingId, setInstallingId] = useState('')
  const [selectedSkillIds, setSelectedSkillIds] = useState<Set<string>>(new Set())
  const [targetAgent, setTargetAgent] = useState('')
  const [installMode, setInstallMode] = useState<'direct' | 'symlink'>('symlink')
  const [message, setMessage] = useState('')
  const targetAgents = useMemo(() => {
    const installed = detectedAgentOptions(agents)
    return installed.length > 0 ? installed : agents
  }, [agents])

  useEffect(() => {
    if (agents.length === 0) loadAgents()
  }, [agents.length, loadAgents])

  useEffect(() => {
    loadScanRoots()
  }, [loadScanRoots])

  useEffect(() => {
    let cancelled = false
    skillApi.getDiscoveredSkills()
      .then((cached) => {
        if (!cancelled && cached.length > 0) {
          setSkills(cached)
          setMessage(`已载入上次扫描的 ${cached.length} 个项目 Skill。`)
        }
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [])

  useEffect(() => {
    if (rootsText || scanRoots.length === 0) return
    setRootsText(scanRoots.map(root => `${root.enabled ? '' : '# '}${root.path}`).join('\n'))
  }, [rootsText, scanRoots])

  useEffect(() => {
    if (!targetAgent && targetAgents.length > 0) setTargetAgent(targetAgents[0].id)
  }, [targetAgent, targetAgents])

  const scan = async () => {
    const roots: ScanRoot[] = rootsText
      .split('\n')
      .map((raw) => {
        const trimmed = raw.trim()
        const enabled = !trimmed.startsWith('#')
        const path = trimmed.replace(/^#\s*/, '').trim()
        return { path, enabled, label: path.split('/').filter(Boolean).pop() || path }
      })
      .filter(root => root.path)
    setScanning(true)
    setMessage('')
    try {
      await skillApi.setScanRoots(roots)
      await loadScanRoots()
      const result = await skillApi.discoverProjectSkills(roots.filter(root => root.enabled).map(root => root.path))
      setSkills(result)
      setSelectedSkillIds(new Set())
      await loadObsidianVaults()
      setMessage(`发现 ${result.length} 个项目 Skill。`)
    } catch (error) {
      setMessage(String(error))
    } finally {
      setScanning(false)
    }
  }

  const stopScan = async () => {
    await skillApi.stopProjectScan()
    setScanning(false)
    setMessage('已停止展示扫描状态，保留当前结果。')
  }

  const clearResults = async () => {
    await skillApi.clearDiscoveredSkills()
    setSkills([])
    setSelectedSkillIds(new Set())
    setMessage('已清空发现结果缓存。')
  }

  const install = async (skill: DiscoveredSkill) => {
    if (!targetAgent) return
    setInstallingId(skill.id)
    setMessage('')
    try {
      await skillApi.install(skill.dirPath, [{ agent: targetAgent, installMode }], installMode)
      await loadAll()
      setMessage(`已导入 ${skill.name}。`)
    } catch (error) {
      setMessage(String(error))
    } finally {
      setInstallingId('')
    }
  }

  const toggleSelected = (skillId: string) => {
    setSelectedSkillIds(prev => {
      const next = new Set(prev)
      if (next.has(skillId)) next.delete(skillId)
      else next.add(skillId)
      return next
    })
  }

  const toggleAll = () => {
    setSelectedSkillIds(prev => {
      if (prev.size === skills.length) return new Set()
      return new Set(skills.map(skill => skill.id))
    })
  }

  const batchImport = async (agentId: string) => {
    const selected = skills.filter(skill => selectedSkillIds.has(skill.id))
    if (selected.length === 0) return
    setInstallingId('__batch__')
    setMessage('')
    try {
      const targetAgentsForImport = agentId === 'central' ? ['central'] : [agentId]
      const imported = await skillApi.batchImportDiscoveredSkills(selected, targetAgentsForImport, installMode)
      await loadAll()
      setSelectedSkillIds(new Set())
      setMessage(`已批量导入 ${imported.length || selected.length} 个 Skill。`)
    } catch (error) {
      setMessage(String(error))
    } finally {
      setInstallingId('')
    }
  }

  return (
    <div className="capability-page">
      <div className="capability-page-head">
        <h1>🔎 项目发现</h1>
        <p>扫描项目里的 .skills、.agents/skills、.claude/skills 目录，并导入到目标 Agent。</p>
      </div>

      <div className="capability-page-body discover-view">
        <section className="discover-config">
          <label>
            <span>扫描根目录</span>
            <textarea value={rootsText} onChange={event => setRootsText(event.target.value)} rows={4} />
          </label>
          <div className="discover-root-list">
            {scanRoots.map(root => (
              <button
                key={root.path}
                type="button"
                className={`skills-chip ${root.enabled ? 'skills-chip--active' : ''}`}
                onClick={async () => {
                  await skillApi.setScanRootEnabled(root.path, !root.enabled)
                  await loadScanRoots()
                  setRootsText('')
                }}
              >
                {root.enabled ? '启用' : '停用'} · {root.label}
              </button>
            ))}
          </div>
          <div className="discover-config__actions">
            <select value={targetAgent} onChange={event => setTargetAgent(event.target.value)}>
              {targetAgents.map(agent => <option key={agent.id} value={agent.id}>{agent.displayName}</option>)}
            </select>
            <select value={installMode} onChange={event => setInstallMode(event.target.value as 'direct' | 'symlink')}>
              <option value="symlink">Symlink</option>
              <option value="direct">Direct copy</option>
            </select>
            <button type="button" className="skills-btn skills-btn--primary" onClick={scan} disabled={scanning}>
              {scanning ? '扫描中...' : '开始扫描'}
            </button>
            {scanning && (
              <button type="button" className="skills-btn" onClick={stopScan}>
                停止
              </button>
            )}
            {skills.length > 0 && (
              <button type="button" className="skills-btn" onClick={clearResults}>
                清空结果
              </button>
            )}
          </div>
        </section>

        {message && <div className="sync-status">{message}</div>}

        {skills.length > 0 && (
          <div className="discover-config__actions">
            <button type="button" className="skills-btn skills-btn--small" onClick={toggleAll}>
              {selectedSkillIds.size === skills.length ? '取消全选' : '全选'}
            </button>
            <button
              type="button"
              className="skills-btn skills-btn--small skills-btn--primary"
              disabled={selectedSkillIds.size === 0 || installingId === '__batch__'}
              onClick={() => batchImport('central')}
            >
              导入选中到中央库
            </button>
            <button
              type="button"
              className="skills-btn skills-btn--small"
              disabled={selectedSkillIds.size === 0 || !targetAgent || installingId === '__batch__'}
              onClick={() => batchImport(targetAgent)}
            >
              导入选中到目标 Agent
            </button>
            <span className="skills-chip skills-chip--active">已选 {selectedSkillIds.size}</span>
          </div>
        )}

        <div className="discover-list">
          {skills.map(skill => (
            <div key={skill.id} className="discover-card">
              <div>
                <label className="discover-select">
                  <input
                    type="checkbox"
                    checked={selectedSkillIds.has(skill.id)}
                    onChange={() => toggleSelected(skill.id)}
                  />
                  <span>选择</span>
                </label>
                <div className="discover-card__title">{skill.name}</div>
                <div className="discover-card__desc">{skill.description || '暂无描述'}</div>
                <div className="discover-card__meta">
                  <span>{skill.projectName}</span>
                  <span>{skill.sourceKind}</span>
                  <code>{skill.dirPath}</code>
                </div>
              </div>
              <button
                type="button"
                className="skills-btn skills-btn--small"
                disabled={!targetAgent || installingId === skill.id}
                onClick={() => install(skill)}
              >
                {installingId === skill.id ? '导入中...' : '导入'}
              </button>
            </div>
          ))}
        </div>

        {!scanning && skills.length === 0 && (
          <div className="skills-empty">
            <div className="skills-empty__icon">🔎</div>
            <div className="skills-empty__text">还没有发现项目 Skill</div>
            <div className="skills-empty__hint">输入要扫描的目录后开始扫描。</div>
          </div>
        )}
      </div>
    </div>
  )
}
