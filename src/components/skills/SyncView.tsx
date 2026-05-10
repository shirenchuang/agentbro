import { useState, useCallback } from 'react'
import { useTranslation } from 'react-i18next'
import { useSkillStore } from '../../stores/skillStore'
import { skillApi } from '../../services/skillApi'

const AGENTS = ['claude-code', 'codex', 'gemini-cli', 'cursor', 'hermes']

export function SyncView() {
  const { t } = useTranslation()
  const { syncConfig, loadAll } = useSkillStore()
  const [githubRepo, setGithubRepo] = useState(syncConfig?.githubRepo || '')
  const [githubToken, setGithubToken] = useState(syncConfig?.githubToken || '')
  const [syncing, setSyncing] = useState(false)
  const [syncMessage, setSyncMessage] = useState('')
  const [fromAgent, setFromAgent] = useState(AGENTS[0])
  const [toAgent, setToAgent] = useState(AGENTS[1])
  const [previewDetails, setPreviewDetails] = useState<string[]>([])

  const handleSaveConfig = useCallback(async () => {
    await skillApi.configureSyncConfig({
      method: 'github',
      githubRepo: githubRepo || null,
      githubToken: githubToken || null,
      lastSyncAt: syncConfig?.lastSyncAt || null,
      autoSync: false,
    })
    loadAll()
    setSyncMessage(t('skills.configSaved'))
  }, [githubRepo, githubToken, syncConfig, loadAll, t])

  const handlePush = useCallback(async () => {
    setSyncing(true)
    setSyncMessage('')
    try {
      const result = await skillApi.pushSync()
      setSyncMessage(result.message)
    } catch (e) {
      setSyncMessage(String(e))
    }
    setSyncing(false)
    loadAll()
  }, [loadAll])

  const handlePull = useCallback(async () => {
    setSyncing(true)
    setSyncMessage('')
    try {
      const result = await skillApi.pullSync()
      setSyncMessage(result.message)
    } catch (e) {
      setSyncMessage(String(e))
    }
    setSyncing(false)
    loadAll()
  }, [loadAll])

  const handleAgentPreview = useCallback(async () => {
    try {
      const preview = await skillApi.syncAgentPreview(fromAgent, toAgent)
      setPreviewDetails(preview.details)
    } catch (e) {
      setPreviewDetails([String(e)])
    }
  }, [fromAgent, toAgent])

  const handleAgentSync = useCallback(async () => {
    setSyncing(true)
    try {
      await skillApi.executeAgentSync(fromAgent, toAgent)
      setSyncMessage(t('skills.agentSyncDone'))
      setPreviewDetails([])
    } catch (e) {
      setSyncMessage(String(e))
    }
    setSyncing(false)
    loadAll()
  }, [fromAgent, toAgent, loadAll, t])

  const handleExport = useCallback(async () => {
    const path = prompt(t('skills.importPathPrompt'), '~/Desktop/agentbro-backup.zip')
    if (!path) return
    try {
      await skillApi.exportBackup(path)
      setSyncMessage(t('skills.exportDone', { path }))
    } catch (e) {
      setSyncMessage(String(e))
    }
  }, [t])

  const handleImport = useCallback(async () => {
    const path = prompt(t('skills.importPathPrompt'))
    if (!path) return
    try {
      await skillApi.importBackup(path)
      setSyncMessage(t('skills.importDone'))
      loadAll()
    } catch (e) {
      setSyncMessage(String(e))
    }
  }, [loadAll, t])

  return (
    <div>
      {/* GitHub Sync */}
      <div className="sync-section">
        <div className="sync-section__title">GitHub {t('skills.sync')}</div>
        <div className="sync-section__desc">{t('skills.githubSyncDesc')}</div>
        <div className="sync-input-row">
          <input
            className="sync-input"
            value={githubRepo}
            onChange={e => setGithubRepo(e.target.value)}
            placeholder="owner/repo"
          />
        </div>
        <div className="sync-input-row">
          <input
            className="sync-input"
            type="password"
            value={githubToken}
            onChange={e => setGithubToken(e.target.value)}
            placeholder="GitHub Token (PAT)"
          />
          <button className="skills-btn skills-btn--small" onClick={handleSaveConfig}>
            {t('skills.save')}
          </button>
        </div>
        <div className="sync-buttons">
          <button className="skills-btn" onClick={handlePush} disabled={syncing}>
            {t('skills.push')}
          </button>
          <button className="skills-btn" onClick={handlePull} disabled={syncing}>
            {t('skills.pull')}
          </button>
        </div>
        {syncConfig?.lastSyncAt && (
          <div className="sync-status">
            {t('skills.lastSync')}: {new Date(syncConfig.lastSyncAt).toLocaleString()}
          </div>
        )}
      </div>

      {/* Export / Import */}
      <div className="sync-section">
        <div className="sync-section__title">{t('skills.exportImport')}</div>
        <div className="sync-section__desc">{t('skills.exportImportDesc')}</div>
        <div className="sync-buttons">
          <button className="skills-btn" onClick={handleExport}>{t('skills.export')}</button>
          <button className="skills-btn" onClick={handleImport}>{t('skills.import')}</button>
        </div>
      </div>

      {/* Agent-to-Agent Sync */}
      <div className="sync-section">
        <div className="sync-section__title">{t('skills.agentSync')}</div>
        <div className="sync-section__desc">{t('skills.agentSyncDesc')}</div>
        <div className="sync-input-row">
          <select className="sync-agent-select" value={fromAgent} onChange={e => setFromAgent(e.target.value)}>
            {AGENTS.map(a => <option key={a} value={a}>{a}</option>)}
          </select>
          <span style={{ color: 'rgba(255,255,255,0.4)', padding: '0 4px' }}>→</span>
          <select className="sync-agent-select" value={toAgent} onChange={e => setToAgent(e.target.value)}>
            {AGENTS.map(a => <option key={a} value={a}>{a}</option>)}
          </select>
          <button className="skills-btn skills-btn--small" onClick={handleAgentPreview}>
            {t('skills.preview')}
          </button>
          <button className="skills-btn skills-btn--primary skills-btn--small" onClick={handleAgentSync} disabled={syncing}>
            {t('skills.syncNow')}
          </button>
        </div>
        {previewDetails.length > 0 && (
          <div style={{ marginTop: 8, fontSize: 12, color: 'rgba(255,255,255,0.5)' }}>
            {previewDetails.map((d, i) => <div key={i}>{d}</div>)}
          </div>
        )}
      </div>

      {/* Rescan */}
      <div className="sync-section">
        <div className="sync-section__title">{t('skills.scanStatus')}</div>
        <button className="skills-btn" onClick={loadAll}>{t('skills.rescan')}</button>
      </div>

      {syncMessage && (
        <div className="sync-status" style={{ marginTop: 8 }}>{syncMessage}</div>
      )}
    </div>
  )
}
