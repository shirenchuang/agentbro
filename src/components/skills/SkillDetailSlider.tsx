import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { motion, AnimatePresence } from 'framer-motion'
import { useSkillStore } from '../../stores/skillStore'
import { useAgentStore } from '../../stores/agentStore'
import { skillApi } from '../../services/skillApi'
import { FileTreeViewer } from './FileTreeViewer'
import { InlineConfirmAction } from './InlineConfirmAction'
import { FrontmatterCard } from './FrontmatterCard'
import { agentMatchesId, detectedAgentOptions, displayAgentName } from '../../utils/agentPrograms'
import { displayVersionValue } from '../../utils/versions'

type DetailTab = 'overview' | 'files' | 'locations'

export function SkillDetailSlider() {
  const { t } = useTranslation()
  const { skills, packs, selectedSkillId, detailOpen, closeDetail, fileTree, loadAll } = useSkillStore()
  const { agents, loadAgents } = useAgentStore()
  const [tab, setTab] = useState<DetailTab>('overview')
  const [viewingFile, setViewingFile] = useState(false)
  const [installing, setInstalling] = useState(false)
  const [descExpanded, setDescExpanded] = useState(false)
  const [notice, setNotice] = useState('')
  const [mcpValidation, setMcpValidation] = useState('')
  const [installModeByAgent, setInstallModeByAgent] = useState<Record<string, 'direct' | 'symlink'>>({})
  const [packToJoin, setPackToJoin] = useState('')

  const skill = skills.find(s => s.id === selectedSkillId)
  const skillVersion = displayVersionValue(skill?.frontmatter.version || skill?.frontmatter.versionName)
  const skillOwner = skill?.frontmatter.author || skill?.frontmatter.owner || skill?.frontmatter.maintainer
  const skillCategory = skill?.frontmatter.category || skill?.frontmatter.tags || skill?.skillType
  const installedAgents = skill ? skill.agents.map(a => a.agent) : []
  const targetAgents = useMemo(() => {
    const installed = detectedAgentOptions(agents)
    return installed.length > 0 ? installed : agents
  }, [agents])
  const uninstalledAgents = targetAgents.filter(agent => !installedAgents.some(id => agentMatchesId(id, agent.id)))
  const memberPacks = skill ? packs.filter(p => p.skills.includes(skill.id)) : []
  const availablePacks = skill ? packs.filter(p => !p.skills.includes(skill.id)) : []

  useEffect(() => {
    if (agents.length === 0) loadAgents()
  }, [agents.length, loadAgents])

  useEffect(() => {
    if (tab !== 'files') setViewingFile(false)
  }, [tab])

  const handleToggleAgent = async (agent: string, enabled: boolean) => {
    if (!skill) return
    setNotice('')
    try {
      await skillApi.toggle(skill.id, agent, enabled)
      await loadAll()
    } catch (e) {
      setNotice(e instanceof Error ? e.message : String(e))
    }
  }

  const handleInstallToAgent = async (agent: string) => {
    if (!skill?.filePath) return
    setInstalling(true)
    setNotice('')
    try {
      if (skill.skillType === 'mcp') {
        const command = skill.frontmatter.command
        if (!command) {
          setNotice('当前 MCP 缺少 command，无法安装到其他 Agent。')
          return
        }
        await skillApi.upsertMcpServer(agent, {
          name: skill.name,
          command,
          args: (skill.frontmatter.args || '').split(/\s+/).filter(Boolean),
          env: {},
        })
        await loadAll()
        setNotice(`MCP 已写入 ${displayAgentName(agent, agents)} 配置`)
        return
      }
      const mode = installModeByAgent[agent] ?? 'symlink'
      await skillApi.install(skill.filePath, [{ agent, installMode: mode }], mode)
      await loadAll()
      setNotice(`已安装到 ${displayAgentName(agent, agents)}`)
    } catch (e) {
      setNotice(e instanceof Error ? e.message : String(e))
    } finally {
      setInstalling(false)
    }
  }

  const handleUninstall = async () => {
    if (!skill) return
    if (skill.skillType === 'mcp') {
      for (const state of skill.agents) {
        await skillApi.removeMcpServer(state.agent, skill.name)
      }
      closeDetail()
      await loadAll()
      return
    }
    await skillApi.uninstall(skill.filePath)
    closeDetail()
    loadAll()
  }

  const handleUninstallPath = async (path: string, agent?: string) => {
    if (skill?.skillType === 'mcp' && agent) {
      await skillApi.removeMcpServer(agent, skill.name)
      await loadAll()
      return
    }
    await skillApi.uninstall(path)
    loadAll()
  }

  const handleOpenPath = async (path: string) => {
    try {
      await skillApi.openPath(path)
    } catch (e) {
      console.error(e)
    }
  }

  const handleAddToPack = async () => {
    if (!skill || !packToJoin) return
    const pack = packs.find(p => p.id === packToJoin)
    if (!pack) return
    await skillApi.updatePack({
      ...pack,
      skills: Array.from(new Set([...pack.skills, skill.id])),
    })
    setPackToJoin('')
    await loadAll()
  }

  const handleValidateMcp = async () => {
    if (!skill || skill.skillType !== 'mcp') return
    const agent = skill.agents[0]?.agent
    if (!agent) return
    try {
      const result = await skillApi.validateMcpServer(agent, skill.name)
      const warnings = result.warnings.length > 0 ? `；警告：${result.warnings.join('；')}` : ''
      setMcpValidation(`${result.valid ? '通过' : '失败'}：${result.message}${warnings}`)
    } catch (error) {
      setMcpValidation(String(error))
    }
  }

  const formatSize = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`
  }

  const formatDate = (ts: number) => ts ? new Date(ts * 1000).toLocaleDateString() : '-'

  return (
    <AnimatePresence>
      {detailOpen && skill && (
        <div className="skill-detail-overlay">
          <motion.div
            className="skill-detail-backdrop"
            initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}
            onClick={closeDetail}
          />
          <motion.div
            className={`skill-detail-panel ${tab === 'files' && viewingFile ? 'skill-detail-panel--files' : ''}`}
            initial={{ x: 520 }} animate={{ x: 0 }} exit={{ x: 520 }}
            transition={{ type: 'spring', damping: 30, stiffness: 300 }}
          >
            <button className="skill-detail-panel__close" onClick={closeDetail}>✕</button>

            <div className="skill-detail-panel__header">
              <div className="skill-detail-panel__title">{skill.name}</div>
              {skill.description && (
                <div className="skill-detail-panel__desc">
                  <span className={descExpanded ? '' : 'skill-detail-panel__desc--clamped'}>
                    {skill.description}
                  </span>
                  {skill.description.length > 80 && (
                    <button className="skill-detail-panel__desc-toggle" onClick={() => setDescExpanded(v => !v)}>
                      {descExpanded ? '收起' : '展开'}
                    </button>
                  )}
                </div>
              )}
              <div className="skill-detail-panel__meta">
                {skillCategory && <span>{skillCategory}</span>}
                {skillOwner && <span>{skillOwner}</span>}
                {skillVersion && <span>{skillVersion}</span>}
              </div>
            </div>

            <div className="skill-detail-tabs">
              {(['overview', 'files', 'locations'] as DetailTab[]).map(id => (
                <button
                  key={id}
                  className={`skill-detail-tab ${tab === id ? 'skill-detail-tab--active' : ''}`}
                  onClick={() => setTab(id)}
                >
                  {id === 'overview' ? '概览' : id === 'files' ? '文件' : '安装位置'}
                </button>
              ))}
            </div>

            <div className="skill-detail-tab-body">
              {tab === 'overview' && (
                <>
                  <div className="skill-detail-section">
                    <div className="skill-detail-section__title">{t('skills.basicInfo')}</div>
                    <div className="skill-detail-info-row">
                      <span className="skill-detail-info-row__label">{t('skills.type')}</span>
                      <span className="skill-detail-info-row__value">{skill.skillType.toUpperCase()}</span>
                    </div>
                    <div className="skill-detail-info-row">
                      <span className="skill-detail-info-row__label">{t('skills.size')}</span>
                      <span className="skill-detail-info-row__value">{formatSize(skill.fileSize)}</span>
                    </div>
                    <div className="skill-detail-info-row">
                      <span className="skill-detail-info-row__label">{t('skills.modified')}</span>
                      <span className="skill-detail-info-row__value">{formatDate(skill.modifiedAt)}</span>
                    </div>
                    {skill.originUrl && (
                      <div className="skill-detail-info-row">
                        <span className="skill-detail-info-row__label">{t('skills.source')}</span>
                        <span className="skill-detail-info-row__value" style={{ fontSize: 11 }}>{skill.originUrl}</span>
                      </div>
                    )}
                  </div>

                  {skill.frontmatter && Object.keys(skill.frontmatter).length > 0 && (
                    <div className="skill-detail-section">
                      <div className="skill-detail-section__title">{t('skills.frontmatter')}</div>
                      <FrontmatterCard data={skill.frontmatter} />
                    </div>
                  )}

                  {skill.skillType === 'mcp' && (
                    <div className="skill-detail-section">
                      <div className="skill-detail-section__title">MCP 可用性</div>
                      <div className="skill-detail-info-row">
                        <span className="skill-detail-info-row__label">配置校验</span>
                        <button className="skills-btn skills-btn--small" onClick={handleValidateMcp}>
                          校验
                        </button>
                      </div>
                      {mcpValidation && <div className="skill-detail-notice">{mcpValidation}</div>}
                    </div>
                  )}

                  {memberPacks.length > 0 && (
                    <div className="skill-detail-section">
                      <div className="skill-detail-section__title">{t('skills.packMembership')}</div>
                      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
                        {memberPacks.map(p => (
                          <span key={p.id} className="skills-chip skills-chip--active">{p.name}</span>
                        ))}
                      </div>
                    </div>
                  )}

                  {availablePacks.length > 0 && (
                    <div className="skill-detail-section skill-pack-join">
                      <div className="skill-detail-section__title">加入技能包</div>
                      <div className="skill-pack-join__row">
                        <select value={packToJoin} onChange={e => setPackToJoin(e.target.value)}>
                          <option value="">选择技能包...</option>
                          {availablePacks.map(pack => <option key={pack.id} value={pack.id}>{pack.name}</option>)}
                        </select>
                        <button className="skills-btn skills-btn--small" disabled={!packToJoin} onClick={handleAddToPack}>
                          加入
                        </button>
                      </div>
                    </div>
                  )}
                </>
              )}

              {tab === 'files' && (
                <div className="skill-detail-section">
                  {fileTree ? (
                    <FileTreeViewer tree={fileTree} onViewingFileChange={setViewingFile} />
                  ) : (
                    <div className="skill-detail-empty">{t('skills.noFiles')}</div>
                  )}
                </div>
              )}

              {tab === 'locations' && (
                <>
                  {notice && <div className="skill-detail-notice">{notice}</div>}
                  <div className="skill-detail-section skill-install-card">
                    <div className="skill-install-card__head">
                      <div>
                        <div className="skill-install-card__name">{skill.name}</div>
                        <div className="skill-install-card__sub">安装于 {formatDate(skill.modifiedAt)}</div>
                      </div>
                      {skill.agents.length > 0 && (
                        <button type="button" className="skill-install-card__remove" onClick={handleUninstall}>
                          一键卸载
                        </button>
                      )}
                    </div>
                    <div className="skill-install-card__list">
                      {skill.agents.map((a, index) => (
                        <LocationPathRow
                          key={`${a.agent}:${a.installPath}:${a.linkTarget ?? ''}`}
                          label={displayAgentName(a.agent, agents)}
                          path={a.installMode === 'symlink' && a.linkTarget ? a.linkTarget : a.installPath}
                          mode={a.installMode || 'direct'}
                          enabled={a.enabled}
                          primary={index === 0}
                          onToggle={() => handleToggleAgent(a.agent, !a.enabled)}
                          onOpen={handleOpenPath}
                          onUninstall={() => handleUninstallPath(a.installPath, a.agent)}
                        />
                      ))}
                    </div>
                    {skill.agents.length === 0 && <div className="skill-detail-empty">暂未安装到任何 Agent</div>}
                  </div>

                  {uninstalledAgents.length > 0 && skill.filePath && (
                    <div className="skill-detail-section skill-install-more">
                      <div className="skill-detail-section__title">其他可安装平台</div>
                      <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                        {uninstalledAgents.map(a => (
                          <div key={a.id} className="skill-detail-install-target">
                            <div className="skill-detail-install-target__main">
                              <strong>{a.displayName}</strong>
                              <button
                                type="button"
                                disabled={!a.skillsDir && !a.configDir}
                                onClick={() => {
                                  const path = a.skillsDir || a.configDir
                                  if (path) handleOpenPath(path)
                                }}
                              >
                                {a.skillsDir || a.configDir || '-'}
                              </button>
                            </div>
                            {skill.skillType !== 'mcp' && (
                              <select
                                value={installModeByAgent[a.id] ?? 'symlink'}
                                onChange={e => setInstallModeByAgent(s => ({ ...s, [a.id]: e.target.value as 'direct' | 'symlink' }))}
                              >
                                <option value="symlink">Symlink</option>
                                <option value="direct">复制</option>
                              </select>
                            )}
                            <button
                              className="skills-btn skills-btn--small"
                              disabled={installing}
                              onClick={() => handleInstallToAgent(a.id)}
                            >
                              {skill.skillType === 'mcp' ? '写入' : '安装'}
                            </button>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                </>
              )}
            </div>

            <div className="skill-detail-panel__footer">
              <InlineConfirmAction
                label={t('skills.uninstall')}
                confirmLabel={t('skills.confirmUninstall')}
                onConfirm={handleUninstall}
                icon={<span style={{ fontSize: 14 }}>🗑</span>}
              />
            </div>
          </motion.div>
        </div>
      )}
    </AnimatePresence>
  )
}

function LocationPathRow({
  label,
  path,
  mode,
  enabled,
  primary,
  onToggle,
  onUninstall,
  onOpen,
}: {
  label: string
  path: string
  mode: string
  enabled?: boolean
  primary?: boolean
  onToggle?: () => void
  onUninstall?: () => void
  onOpen: (path: string) => void
}) {
  const open = () => onOpen(path)
  return (
    <div
      className={`skill-location-row ${primary ? 'skill-location-row--primary' : ''}`}
      role="button"
      tabIndex={0}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || (event.key.toLowerCase() === 'o' && event.metaKey)) {
          event.preventDefault()
          open()
        }
      }}
      title="Enter 或 Cmd+O 打开文件夹"
    >
      <span className="skill-location-row__check">✓</span>
      <div className="skill-location-row__main">
        <div className="skill-location-row__title-line">
          <span className="skill-location-row__label">{label}</span>
          {primary && <em className="skill-location-row__primary-tag">主安装</em>}
          <span className={`skill-location-row__mode-tag skill-location-row__mode-tag--${mode}`}>{mode}</span>
        </div>
        <button type="button" className="skill-location-row__path" onClick={open}>
          {path}
        </button>
      </div>
      {onUninstall && (
        <button type="button" className="skill-location-row__remove" onClick={onUninstall}>
          卸载
        </button>
      )}
    </div>
  )
}
