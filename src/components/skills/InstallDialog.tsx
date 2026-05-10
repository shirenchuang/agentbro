import { useState, useCallback } from 'react'
import { useTranslation } from 'react-i18next'
import { skillApi } from '../../services/skillApi'
import type { TargetConfig } from '../../services/skillApi'
import { useSkillStore } from '../../stores/skillStore'

const AGENTS = ['claude-code', 'codex', 'gemini-cli', 'cursor', 'hermes']
type SourceType = 'url' | 'github' | 'local' | 'skillsh'

interface InstallDialogProps {
  onClose: () => void
}

export function InstallDialog({ onClose }: InstallDialogProps) {
  const { t } = useTranslation()
  const { loadAll } = useSkillStore()
  const [sourceType, setSourceType] = useState<SourceType>('url')
  const [url, setUrl] = useState('')
  const [githubRepo, setGithubRepo] = useState('')
  const [githubPath, setGithubPath] = useState('')
  const [localPath, setLocalPath] = useState('')
  const [selectedAgents, setSelectedAgents] = useState<Set<string>>(new Set(['claude-code']))
  const [installMode, setInstallMode] = useState<'direct' | 'symlink'>('direct')
  const [installing, setInstalling] = useState(false)

  const toggleAgent = useCallback((a: string) => {
    setSelectedAgents(prev => {
      const next = new Set(prev)
      if (next.has(a)) next.delete(a); else next.add(a)
      return next
    })
  }, [])

  const getSource = (): string => {
    switch (sourceType) {
      case 'url': return url
      case 'github': return githubPath ? `${githubRepo}/${githubPath}` : githubRepo
      case 'local': return localPath
      case 'skillsh': return url
    }
  }

  const handleInstall = useCallback(async () => {
    const source = getSource()
    if (!source || selectedAgents.size === 0) return

    setInstalling(true)
    try {
      const targets: TargetConfig[] = Array.from(selectedAgents).map(a => ({
        agent: a,
        installMode,
      }))
      await skillApi.install(source, targets, installMode)
      loadAll()
      onClose()
    } catch (e) {
      console.error('Install failed:', e)
    }
    setInstalling(false)
  }, [sourceType, url, githubRepo, githubPath, localPath, selectedAgents, installMode, loadAll, onClose])

  return (
    <div className="skills-dialog-overlay" onClick={onClose}>
      <div className="skills-dialog" onClick={e => e.stopPropagation()}>
        <div className="skills-dialog__header">
          <div className="skills-dialog__title">{t('skills.installSkill')}</div>
        </div>
        <div className="skills-dialog__body">
          <div className="install-source-tabs">
            {(['url', 'github', 'local', 'skillsh'] as const).map(s => (
              <button
                key={s}
                className={`skills-chip ${sourceType === s ? 'skills-chip--active' : ''}`}
                onClick={() => setSourceType(s)}
              >
                {s === 'url' ? 'URL' : s === 'github' ? 'GitHub' : s === 'local' ? t('skills.localPath') : 'skills.sh'}
              </button>
            ))}
          </div>

          {sourceType === 'url' && (
            <div className="install-form-row">
              <label className="install-form-label">URL</label>
              <input
                className="install-form-input"
                value={url}
                onChange={e => setUrl(e.target.value)}
                placeholder="https://..."
              />
            </div>
          )}

          {sourceType === 'github' && (
            <>
              <div className="install-form-row">
                <label className="install-form-label">{t('skills.githubRepo')}</label>
                <input
                  className="install-form-input"
                  value={githubRepo}
                  onChange={e => setGithubRepo(e.target.value)}
                  placeholder="owner/repo"
                />
              </div>
              <div className="install-form-row">
                <label className="install-form-label">{t('skills.subPath')}</label>
                <input
                  className="install-form-input"
                  value={githubPath}
                  onChange={e => setGithubPath(e.target.value)}
                  placeholder="skills/my-skill"
                />
              </div>
            </>
          )}

          {sourceType === 'local' && (
            <div className="install-form-row">
              <label className="install-form-label">{t('skills.localPath')}</label>
              <input
                className="install-form-input"
                value={localPath}
                onChange={e => setLocalPath(e.target.value)}
                placeholder="/path/to/skill"
              />
            </div>
          )}

          {sourceType === 'skillsh' && (
            <div className="install-form-row">
              <label className="install-form-label">skills.sh URL</label>
              <input
                className="install-form-input"
                value={url}
                onChange={e => setUrl(e.target.value)}
                placeholder="https://skills.sh/..."
              />
            </div>
          )}

          <div className="install-form-row">
            <label className="install-form-label">{t('skills.targetAgents')}</label>
            <div className="install-targets">
              {AGENTS.map(a => (
                <div
                  key={a}
                  className={`install-target-chip ${selectedAgents.has(a) ? 'install-target-chip--selected' : ''}`}
                  onClick={() => toggleAgent(a)}
                >
                  {a}
                </div>
              ))}
            </div>
          </div>

          <div className="install-form-row">
            <label className="install-form-label">{t('skills.installMode')}</label>
            <div className="install-mode-row">
              <div
                className={`install-mode-option ${installMode === 'direct' ? 'install-mode-option--selected' : ''}`}
                onClick={() => setInstallMode('direct')}
              >
                {t('skills.directCopy')}
              </div>
              <div
                className={`install-mode-option ${installMode === 'symlink' ? 'install-mode-option--selected' : ''}`}
                onClick={() => setInstallMode('symlink')}
              >
                Symlink
              </div>
            </div>
          </div>
        </div>
        <div className="skills-dialog__footer">
          <button className="skills-btn" onClick={onClose}>{t('skills.cancel')}</button>
          <button
            className="skills-btn skills-btn--primary"
            onClick={handleInstall}
            disabled={installing}
          >
            {installing ? t('skills.installing') : t('skills.install')}
          </button>
        </div>
      </div>
    </div>
  )
}
