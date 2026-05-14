import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { skillApi } from '../../services/skillApi'
import type { TargetConfig } from '../../services/skillApi'
import { useAgentStore } from '../../stores/agentStore'
import { useSkillStore } from '../../stores/skillStore'
import { detectedAgentOptions, isAgentProgramInstalled } from '../../utils/agentPrograms'

type SourceType = 'url' | 'github' | 'local' | 'skillsh'

interface InstallDialogProps {
  onClose: () => void
  initialSourceType?: SourceType
  initialUrl?: string
  initialGithubRepo?: string
  initialGithubPath?: string
  initialLocalPath?: string
}

export function InstallDialog({
  onClose,
  initialSourceType = 'url',
  initialUrl = '',
  initialGithubRepo = '',
  initialGithubPath = '',
  initialLocalPath = '',
}: InstallDialogProps) {
  const { t } = useTranslation()
  const { loadAll } = useSkillStore()
  const { agents, loadAgents } = useAgentStore()
  const [sourceType, setSourceType] = useState<SourceType>(initialSourceType)
  const [url, setUrl] = useState(initialUrl)
  const [githubRepo, setGithubRepo] = useState(initialGithubRepo)
  const [githubPath, setGithubPath] = useState(initialGithubPath)
  const [localPath, setLocalPath] = useState(initialLocalPath)
  const [selectedAgents, setSelectedAgents] = useState<Set<string>>(new Set())
  const [installMode, setInstallMode] = useState<'direct' | 'symlink'>('symlink')
  const [installing, setInstalling] = useState(false)
  const targetAgents = useMemo(() => {
    const installed = detectedAgentOptions(agents)
    return installed.length > 0 ? installed : agents
  }, [agents])

  useEffect(() => {
    if (agents.length === 0) loadAgents()
  }, [agents.length, loadAgents])

  useEffect(() => {
    if (selectedAgents.size > 0 || targetAgents.length === 0) return
    const preferred = targetAgents.find(isAgentProgramInstalled) ?? targetAgents[0]
    setSelectedAgents(new Set([preferred.id]))
  }, [selectedAgents.size, targetAgents])

  const toggleAgent = (a: string) => {
    setSelectedAgents(prev => {
      const next = new Set(prev)
      if (next.has(a)) next.delete(a); else next.add(a)
      return next
    })
  }

  const getSource = (): string => {
    switch (sourceType) {
      case 'url': return url
      case 'github': return githubPath ? `github:${githubRepo}/${githubPath}` : `github:${githubRepo}`
      case 'local': return localPath
      case 'skillsh': return url
    }
  }

  const handleInstall = async () => {
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
  }

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
              {targetAgents.map(agent => (
                <div
                  key={agent.id}
                  className={`install-target-chip ${selectedAgents.has(agent.id) ? 'install-target-chip--selected' : ''}`}
                  onClick={() => toggleAgent(agent.id)}
                >
                  {agent.displayName}
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
