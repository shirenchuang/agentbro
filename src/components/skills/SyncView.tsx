import { useState, useCallback, useEffect, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { useSkillStore } from '../../stores/skillStore'
import { skillApi, type SyncResult } from '../../services/skillApi'
import { useAgentStore } from '../../stores/agentStore'
import { detectedAgentOptions } from '../../utils/agentPrograms'

export function SyncView() {
  const { t } = useTranslation()
  const { syncConfig, loadAll } = useSkillStore()
  const { agents, loadAgents } = useAgentStore()
  const syncAgents = useMemo(() => {
    const installed = detectedAgentOptions(agents)
    return installed.length > 0 ? installed : agents
  }, [agents])
  const [githubRepo, setGithubRepo] = useState(syncConfig?.githubRepo || '')
  const [githubToken, setGithubToken] = useState(syncConfig?.githubToken || '')
  const [syncing, setSyncing] = useState(false)
  const [syncMessage, setSyncMessage] = useState('')
  const [fromAgent, setFromAgent] = useState('')
  const [toAgent, setToAgent] = useState('')
  const [previewDetails, setPreviewDetails] = useState<string[]>([])
  const [conflicts, setConflicts] = useState<SyncResult['conflicts']>([])
  const defaultFromAgent = syncAgents[0]?.id || ''
  const defaultToAgent = syncAgents.find((agent) => agent.id !== defaultFromAgent)?.id || defaultFromAgent
  const selectedFromAgent = syncAgents.some(agent => agent.id === fromAgent) ? fromAgent : defaultFromAgent
  const selectedToAgent = syncAgents.some(agent => agent.id === toAgent) ? toAgent : defaultToAgent

  useEffect(() => {
    if (agents.length === 0) loadAgents()
  }, [agents.length, loadAgents])

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
      setConflicts(result.conflicts ?? [])
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
      setConflicts(result.conflicts ?? [])
    } catch (e) {
      setSyncMessage(String(e))
    }
    setSyncing(false)
    loadAll()
  }, [loadAll])

  const handleAgentPreview = useCallback(async () => {
    if (!selectedFromAgent || !selectedToAgent) return
    try {
      const preview = await skillApi.syncAgentPreview(selectedFromAgent, selectedToAgent)
      setPreviewDetails(preview.details)
    } catch (e) {
      setPreviewDetails([String(e)])
    }
  }, [selectedFromAgent, selectedToAgent])

  const handleAgentSync = useCallback(async () => {
    if (!selectedFromAgent || !selectedToAgent) return
    setSyncing(true)
    try {
      await skillApi.executeAgentSync(selectedFromAgent, selectedToAgent)
      setSyncMessage(t('skills.agentSyncDone'))
      setPreviewDetails([])
    } catch (e) {
      setSyncMessage(String(e))
    }
    setSyncing(false)
    loadAll()
  }, [selectedFromAgent, selectedToAgent, loadAll, t])

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

  const handleResolveConflicts = useCallback(async (action: 'keep_local' | 'use_remote' | 'keep_both') => {
    if (conflicts.length === 0) return
    await skillApi.resolveConflicts(conflicts.map(conflict => ({ skillId: conflict.skillId, action })))
    setConflicts([])
    setSyncMessage('冲突处理完成。')
    loadAll()
  }, [conflicts, loadAll])

  return (
    <div className="capability-page">
      <div className="capability-page-head">
        <h1>☁️ 同步中心</h1>
        <p>跨设备同步配置，也可以把一个 Agent 的 Skills 同步到另一个 Agent。</p>
      </div>

      <div className="capability-page-body sync-view">
        <div className="sync-overview">
          <div>
            <span>同步状态</span>
            <strong>{syncing ? '同步中...' : syncConfig?.lastSyncAt ? '已配置' : '未配置'}</strong>
          </div>
          <div>
            <span>同步方式</span>
            <strong>Git 仓库</strong>
          </div>
          <div>
            <span>上次同步</span>
            <strong>{syncConfig?.lastSyncAt ? new Date(syncConfig.lastSyncAt).toLocaleString() : '-'}</strong>
          </div>
        </div>

        <div className="sync-grid">
          <section className="sync-section">
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
          </section>

          <section className="sync-section">
            <div className="sync-section__title">{t('skills.exportImport')}</div>
            <div className="sync-section__desc">{t('skills.exportImportDesc')}</div>
            <div className="sync-buttons">
              <button className="skills-btn" onClick={handleExport}>{t('skills.export')}</button>
              <button className="skills-btn" onClick={handleImport}>{t('skills.import')}</button>
            </div>
          </section>

          <section className="sync-section sync-section--wide">
            <div className="sync-section__title">{t('skills.agentSync')}</div>
            <div className="sync-section__desc">{t('skills.agentSyncDesc')}</div>
            <div className="sync-input-row sync-agent-row">
              <select className="sync-agent-select" value={selectedFromAgent} onChange={e => setFromAgent(e.target.value)}>
                {syncAgents.map(a => <option key={a.id} value={a.id}>{a.displayName}</option>)}
              </select>
              <span className="sync-arrow">→</span>
              <select className="sync-agent-select" value={selectedToAgent} onChange={e => setToAgent(e.target.value)}>
                {syncAgents.map(a => <option key={a.id} value={a.id}>{a.displayName}</option>)}
              </select>
              <button className="skills-btn skills-btn--small" onClick={handleAgentPreview}>
                {t('skills.preview')}
              </button>
              <button className="skills-btn skills-btn--primary skills-btn--small" onClick={handleAgentSync} disabled={syncing}>
                {t('skills.syncNow')}
              </button>
            </div>
            {previewDetails.length > 0 && (
              <div className="sync-preview">
                {previewDetails.map((d, i) => <div key={i}>{d}</div>)}
              </div>
            )}
          </section>

          <section className="sync-section">
            <div className="sync-section__title">{t('skills.scanStatus')}</div>
            <div className="sync-section__desc">重新扫描本机 Agent、Skills、插件和 MCP 服务。</div>
            <button className="skills-btn" onClick={loadAll}>{t('skills.rescan')}</button>
          </section>
        </div>

        {syncMessage && (
          <div className="sync-status">{syncMessage}</div>
        )}

        {conflicts.length > 0 && (
          <div className="sync-conflicts">
            <div className="sync-conflicts__head">
              <strong>同步冲突</strong>
              <span>{conflicts.length} 项需要处理</span>
            </div>
            <div className="sync-conflicts__list">
              {conflicts.map(conflict => (
                <div key={conflict.skillId} className="sync-conflict-row">
                  <strong>{conflict.skillId}</strong>
                  <span>本地 {conflict.localModified || '-'} · 远端 {conflict.remoteModified || '-'}</span>
                </div>
              ))}
            </div>
            <div className="sync-conflicts__actions">
              <button className="skills-btn skills-btn--small" onClick={() => handleResolveConflicts('keep_local')}>保留本地</button>
              <button className="skills-btn skills-btn--small" onClick={() => handleResolveConflicts('use_remote')}>使用远端</button>
              <button className="skills-btn skills-btn--small" onClick={() => handleResolveConflicts('keep_both')}>保留两者</button>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
