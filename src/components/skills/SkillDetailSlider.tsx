import { useCallback, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { motion, AnimatePresence } from 'framer-motion'
import { useSkillStore } from '../../stores/skillStore'
import { skillApi } from '../../services/skillApi'
import { FileTreeViewer } from './FileTreeViewer'
import { InlineConfirmAction } from './InlineConfirmAction'
import { FrontmatterCard } from './FrontmatterCard'

const ALL_AGENTS = ['claude-code', 'codex', 'gemini-cli', 'cursor', 'hermes']

export function SkillDetailSlider() {
  const { t } = useTranslation()
  const {
    skills, packs, selectedSkillId, detailOpen, closeDetail,
    fileTree, loadAll,
  } = useSkillStore()
  const [installing, setInstalling] = useState(false)
  const [viewingFile, setViewingFile] = useState(false)

  const skill = skills.find(s => s.id === selectedSkillId)
  const installedAgents = skill ? skill.agents.map(a => a.agent) : []
  const uninstalledAgents = ALL_AGENTS.filter(a => !installedAgents.includes(a))
  const memberPacks = skill ? packs.filter(p => p.skills.includes(skill.id)) : []

  const handleToggleAgent = useCallback(async (agent: string, enabled: boolean) => {
    if (!skill) return
    await skillApi.toggle(skill.id, agent, enabled)
    loadAll()
  }, [skill, loadAll])

  const handleInstallToAgent = useCallback(async (agent: string) => {
    if (!skill || !skill.filePath) return
    setInstalling(true)
    try {
      await skillApi.install(skill.filePath, [{ agent, installMode: 'direct' }], 'direct')
      loadAll()
    } catch (e) {
      console.error('Install to agent failed:', e)
    }
    setInstalling(false)
  }, [skill, loadAll])

  const handleUninstall = useCallback(async () => {
    if (!skill) return
    await skillApi.uninstall(skill.filePath)
    closeDetail()
    loadAll()
  }, [skill, closeDetail, loadAll])

  const formatSize = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`
  }

  const formatDate = (ts: number) => {
    if (!ts) return '-'
    return new Date(ts * 1000).toLocaleDateString()
  }

  return (
    <AnimatePresence>
      {detailOpen && skill && (
        <div className="skill-detail-overlay">
          <motion.div
            className="skill-detail-backdrop"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            onClick={closeDetail}
          />
          <motion.div
            className="skill-detail-panel"
            initial={{ x: 420 }}
            animate={{ x: 0, width: viewingFile ? 600 : 420 }}
            exit={{ x: 600 }}
            transition={{ type: 'spring', damping: 30, stiffness: 300 }}
          >
            <button className="skill-detail-panel__close" onClick={closeDetail}>✕</button>

            <div className="skill-detail-panel__header">
              <div className="skill-detail-panel__title">{skill.name}</div>
              {skill.description && (
                <div className="skill-detail-panel__desc">{skill.description}</div>
              )}
            </div>

            <div className="skill-detail-section">
              <div className="skill-detail-section__title">{t('skills.installedAgents')}</div>
              {skill.agents.map(a => (
                <div key={a.agent} className="skill-detail-agent-row">
                  <span className="skill-detail-agent-row__name">{a.agent}</span>
                  <button
                    className={`skill-card__toggle ${a.enabled ? 'skill-card__toggle--on' : ''}`}
                    onClick={() => handleToggleAgent(a.agent, !a.enabled)}
                  />
                </div>
              ))}
            </div>

            {uninstalledAgents.length > 0 && skill.filePath && (
              <div className="skill-detail-section">
                <div className="skill-detail-section__title">{t('skills.installToAgent')}</div>
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
                  {uninstalledAgents.map(a => (
                    <button
                      key={a}
                      className="skills-btn skills-btn--small"
                      disabled={installing}
                      onClick={() => handleInstallToAgent(a)}
                    >
                      + {a}
                    </button>
                  ))}
                </div>
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

            {skill.frontmatter && Object.keys(skill.frontmatter).length > 0 && (
              <div className="skill-detail-section">
                <div className="skill-detail-section__title">{t('skills.frontmatter')}</div>
                <FrontmatterCard data={skill.frontmatter} />
              </div>
            )}

            <div className="skill-detail-section">
              <div className="skill-detail-section__title">{t('skills.basicInfo')}</div>
              <div className="skill-detail-info-row">
                <span className="skill-detail-info-row__label">{t('skills.type')}</span>
                <span className="skill-detail-info-row__value">{skill.skillType.toUpperCase()}</span>
              </div>
              <div className="skill-detail-info-row">
                <span className="skill-detail-info-row__label">{t('skills.installMode')}</span>
                <span className="skill-detail-info-row__value">{skill.agents[0]?.installMode || '-'}</span>
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

            {fileTree && (
              <div className="skill-detail-section">
                <div className="skill-detail-section__title">{t('skills.fileViewer')}</div>
                <FileTreeViewer tree={fileTree} onViewingFileChange={setViewingFile} />
              </div>
            )}

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
