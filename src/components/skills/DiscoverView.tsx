import { useEffect, useMemo, useState } from 'react'
import { useAgentStore } from '../../stores/agentStore'
import { useSkillStore } from '../../stores/skillStore'
import { skillApi, type DiscoveredSkill } from '../../services/skillApi'
import { detectedAgentOptions } from '../../utils/agentPrograms'

const ROOTS_KEY = 'agentbro.skillDiscover.roots.v1'

function readRoots() {
  if (typeof window === 'undefined') return ['~/code', '~/projects', '~/workspace']
  const saved = window.localStorage.getItem(ROOTS_KEY)
  if (!saved) return ['~/code', '~/projects', '~/workspace']
  try {
    const roots = JSON.parse(saved) as string[]
    return roots.length > 0 ? roots : ['~/code']
  } catch {
    return ['~/code']
  }
}

export function DiscoverView() {
  const { agents, loadAgents } = useAgentStore()
  const { loadAll } = useSkillStore()
  const [rootsText, setRootsText] = useState(() => readRoots().join('\n'))
  const [skills, setSkills] = useState<DiscoveredSkill[]>([])
  const [scanning, setScanning] = useState(false)
  const [installingId, setInstallingId] = useState('')
  const [targetAgent, setTargetAgent] = useState('')
  const [message, setMessage] = useState('')
  const targetAgents = useMemo(() => {
    const installed = detectedAgentOptions(agents)
    return installed.length > 0 ? installed : agents
  }, [agents])

  useEffect(() => {
    if (agents.length === 0) loadAgents()
  }, [agents.length, loadAgents])

  useEffect(() => {
    if (!targetAgent && targetAgents.length > 0) setTargetAgent(targetAgents[0].id)
  }, [targetAgent, targetAgents])

  const scan = async () => {
    const roots = rootsText.split('\n').map(root => root.trim()).filter(Boolean)
    window.localStorage?.setItem(ROOTS_KEY, JSON.stringify(roots))
    setScanning(true)
    setMessage('')
    try {
      const result = await skillApi.discoverProjectSkills(roots)
      setSkills(result)
      setMessage(`发现 ${result.length} 个项目 Skill。`)
    } catch (error) {
      setMessage(String(error))
    } finally {
      setScanning(false)
    }
  }

  const install = async (skill: DiscoveredSkill) => {
    if (!targetAgent) return
    setInstallingId(skill.id)
    setMessage('')
    try {
      await skillApi.install(skill.dirPath, [{ agent: targetAgent, installMode: 'symlink' }], 'symlink')
      await loadAll()
      setMessage(`已导入 ${skill.name}。`)
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
          <div className="discover-config__actions">
            <select value={targetAgent} onChange={event => setTargetAgent(event.target.value)}>
              {targetAgents.map(agent => <option key={agent.id} value={agent.id}>{agent.displayName}</option>)}
            </select>
            <button type="button" className="skills-btn skills-btn--primary" onClick={scan} disabled={scanning}>
              {scanning ? '扫描中...' : '开始扫描'}
            </button>
          </div>
        </section>

        {message && <div className="sync-status">{message}</div>}

        <div className="discover-list">
          {skills.map(skill => (
            <div key={skill.id} className="discover-card">
              <div>
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
