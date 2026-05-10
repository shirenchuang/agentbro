import { useCallback, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { motion, AnimatePresence } from 'framer-motion'
import { useSkillStore } from '../../stores/skillStore'
import { skillApi } from '../../services/skillApi'
import { FileTreeViewer } from './FileTreeViewer'

export function SkillDetailSlider() {
  const { t } = useTranslation()
  const {
    skills, selectedSkillId, detailOpen, closeDetail,
    fileTree, loadAll,
  } = useSkillStore()
  const [confirmUninstall, setConfirmUninstall] = useState(false)

  const skill = skills.find(s => s.id === selectedSkillId)

  const handleToggleAgent = useCallback(async (agent: string, enabled: boolean) => {
    if (!skill) return
    await skillApi.toggle(skill.id, agent, enabled)
    loadAll()
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
            animate={{ x: 0 }}
            exit={{ x: 420 }}
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
                <FileTreeViewer tree={fileTree} />
              </div>
            )}

            <div className="skill-detail-panel__footer">
              <button
                className="skills-btn skills-btn--danger"
                onClick={() => setConfirmUninstall(true)}
              >
                {t('skills.uninstall')}
              </button>
            </div>

            {confirmUninstall && (
              <div className="skills-dialog-overlay" onClick={() => setConfirmUninstall(false)}>
                <div className="skills-dialog" onClick={e => e.stopPropagation()}>
                  <div className="skills-dialog__header">
                    <div className="skills-dialog__title">{t('skills.confirmUninstall')}</div>
                  </div>
                  <div className="skills-dialog__body">
                    <p style={{ color: 'rgba(255,255,255,0.6)', fontSize: 13 }}>
                      {t('skills.confirmUninstallMsg', { name: skill.name })}
                    </p>
                  </div>
                  <div className="skills-dialog__footer">
                    <button className="skills-btn" onClick={() => setConfirmUninstall(false)}>
                      {t('skills.cancel')}
                    </button>
                    <button className="skills-btn skills-btn--danger" onClick={handleUninstall}>
                      {t('skills.uninstall')}
                    </button>
                  </div>
                </div>
              </div>
            )}
          </motion.div>
        </div>
      )}
    </AnimatePresence>
  )
}
