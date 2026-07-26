import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useConfigStore } from '../../../stores/configStore'
import {
  addRemoteHost,
  checkRemoteHooks,
  installRemoteAgentHooks,
  isTauri,
  listRemoteInstallableAgents,
  listSshConfigHosts,
  probeRemoteHost,
  removeRemoteHost,
  uninstallRemoteAgentHooks,
} from '../../../services/tauriApi'
import type {
  ConnectionStatus,
  RemoteHost,
  RemoteProbeReport,
  SshConfigHost,
} from '../../../services/tauriApi'
import { SettingSection } from '../SettingSection'
import { SettingGroup } from '../SettingGroup'
import { GlassButton, GlassInput } from '../../shared'
import { useRemoteServerStore } from '../../../stores/remoteServerStore'
import { useRuntimeEnvironmentView } from '../../../hooks/useRuntimeEnvironment'

export function RemoteServersSection() {
  const { t } = useTranslation()
  const { hosts, statuses } = useRuntimeEnvironmentView()
  const connectedCount = hosts.filter((host) => statuses[host.id]?.state === 'connected').length

  return (
    <SettingSection
      className="setting-section--compact remote-servers-section"
      title={t('settings.remoteServers.title', { defaultValue: 'Remote Servers' })}
      description={t('settings.remoteServers.description', {
        defaultValue: 'Manage the shared SSH connections used by Agent management, diagnostics, and the Dynamic Island.',
      })}
    >
      <div className="remote-servers-foundation">
        <span className="remote-servers-foundation__terminal" aria-hidden="true">&gt;_</span>
        <span className="remote-servers-foundation__copy">
          <strong>{t('settings.remoteServers.foundationTitle', { defaultValue: 'One server directory for AgentBro' })}</strong>
          <span>{t('settings.remoteServers.foundationDescription', {
            defaultValue: 'Configure each server once, then select it wherever you work.',
          })}</span>
        </span>
        <span className="remote-servers-foundation__stats">
          <span>
            <strong>{hosts.length}</strong>
            <small>{t('settings.remoteHosts')}</small>
          </span>
          <span className={connectedCount > 0 ? 'remote-servers-foundation__stat--online' : ''}>
            <strong>{connectedCount}</strong>
            <small>{t('skills.runtimeEnvironment.connected')}</small>
          </span>
        </span>
      </div>
      <RemoteServersPanel />
    </SettingSection>
  )
}

type RemoteActionKind = 'connect' | 'disconnect' | 'installHooks' | 'uninstallHooks' | 'remove' | 'import' | 'probe'

// ── Shared remote server management ──
function RemoteServersPanel() {
  const { t } = useTranslation()
  const config = useConfigStore()
  const remoteHosts = useRemoteServerStore((state) => state.remoteHosts)
  const remoteStatuses = useRemoteServerStore((state) => state.remoteStatuses)
  const refreshRemoteHosts = useRemoteServerStore((state) => state.refreshServers)
  const connectServer = useRemoteServerStore((state) => state.connectServer)
  const disconnectServer = useRemoteServerStore((state) => state.disconnectServer)
  const [newHostName, setNewHostName] = useState('')
  const [newHostAddr, setNewHostAddr] = useState('')
  const [remoteBusyAction, setRemoteBusyAction] = useState<{ id: string; action: RemoteActionKind } | null>(null)
  const [sshConfigHosts, setSshConfigHosts] = useState<SshConfigHost[]>([])
  const [sshConfigRefreshing, setSshConfigRefreshing] = useState(false)
  const [remoteNotices, setRemoteNotices] = useState<Record<string, { type: 'success' | 'error'; message: string }>>({})
  const [installableAgents, setInstallableAgents] = useState<string[]>([])
  const [remoteHookStatuses, setRemoteHookStatuses] = useState<Record<string, string[]>>({})
  const [remoteProbeReports, setRemoteProbeReports] = useState<Record<string, RemoteProbeReport>>({})
  const [hooksPanelHost, setHooksPanelHost] = useState<string | null>(null)
  const [hookBusy, setHookBusy] = useState<{ hostId: string; agentId: string } | null>(null)
  useEffect(() => {
    refreshRemoteHosts().catch((err) => console.error('Failed to load remote hosts:', err))
  }, [refreshRemoteHosts])

  const refreshSshConfigHosts = useCallback(async () => {
    if (!isTauri()) return
    setSshConfigHosts(await listSshConfigHosts())
  }, [])

  useEffect(() => {
    refreshSshConfigHosts().catch((err) => console.error('Failed to load SSH config hosts:', err))
  }, [refreshSshConfigHosts])

  const refreshRemoteData = useCallback(async () => {
    await Promise.all([
      refreshRemoteHosts(),
      refreshSshConfigHosts(),
    ])
  }, [refreshRemoteHosts, refreshSshConfigHosts])

  useEffect(() => {
    if (!isTauri()) return
    const hasConnectingHost = Object.values(remoteStatuses).some((status) => status.state === 'connecting')
    if (!hasConnectingHost) return

    const timer = window.setInterval(() => {
      refreshRemoteHosts().catch((err) => console.error('Failed to poll remote host status:', err))
    }, 1000)
    return () => window.clearInterval(timer)
  }, [refreshRemoteHosts, remoteStatuses])

  useEffect(() => {
    if (!isTauri()) return
    listRemoteInstallableAgents().then(setInstallableAgents).catch(() => {})
  }, [])

  const refreshHookStatus = useCallback(async (hostId: string) => {
    if (!isTauri()) return
    try {
      const agents = await checkRemoteHooks(hostId)
      setRemoteHookStatuses((prev) => ({ ...prev, [hostId]: agents }))
    } catch {
      setRemoteHookStatuses((prev) => ({ ...prev, [hostId]: [] }))
    }
  }, [])

  // Auto-check hook status when a host transitions to connected
  const prevStatuses = useRef<Record<string, ConnectionStatus>>({})
  useEffect(() => {
    for (const [id, status] of Object.entries(remoteStatuses)) {
      const prev = prevStatuses.current[id]
      if (status.state === 'connected' && prev?.state !== 'connected') {
        refreshHookStatus(id)
      }
    }
    prevStatuses.current = remoteStatuses
  }, [remoteStatuses, refreshHookStatus])

  async function refreshSshConfigImportList() {
    setSshConfigRefreshing(true)
    try {
      await refreshRemoteData()
    } catch (err) {
      console.error('Failed to refresh SSH config hosts:', err)
    } finally {
      setSshConfigRefreshing(false)
    }
  }

  function parseRemoteTarget(raw: string): { sshTarget: string; port: number | null } {
    const trimmed = raw.trim()
    const portMatch = trimmed.match(/^(.+):(\d+)$/)
    if (!portMatch) return { sshTarget: trimmed, port: null }
    return { sshTarget: portMatch[1], port: Number(portMatch[2]) }
  }

  async function addHost() {
    if (!newHostName.trim() || !newHostAddr.trim()) return
    if (!isTauri()) {
      config.addSSHHost({ id: `ssh-${Date.now()}`, name: newHostName.trim(), host: newHostAddr.trim(), enabled: true })
      setNewHostName('')
      setNewHostAddr('')
      return
    }

    const { sshTarget, port } = parseRemoteTarget(newHostAddr)
    await addRemoteHost({
      id: `remote-${Date.now()}`,
      name: newHostName.trim(),
      sshTarget,
      port,
      identityFile: null,
      authSocket: null,
      remoteSocketPath: '/tmp/agentbro-remote.sock',
      autoConnect: false,
    })
    setNewHostName('')
    setNewHostAddr('')
    await refreshRemoteData()
  }

  async function importSshConfigHost(host: SshConfigHost) {
    const hostname = host.hostname || host.name
    const sshTarget = host.user ? `${host.user}@${hostname}` : hostname
    await addRemoteHost({
      id: `remote-${Date.now()}-${host.name}`,
      name: host.name,
      sshTarget,
      port: host.port,
      identityFile: host.identityFile,
      authSocket: null,
      remoteSocketPath: '/tmp/agentbro-remote.sock',
      autoConnect: false,
    })
    await refreshRemoteData()
  }

  async function runRemoteAction(id: string, actionName: RemoteActionKind, action: () => Promise<unknown>) {
    setRemoteBusyAction({ id, action: actionName })
    setRemoteNotices((prev) => { const next = { ...prev }; delete next[id]; return next })
    try {
      const result = await action()
      await refreshRemoteData()
      if (actionName === 'installHooks' || actionName === 'uninstallHooks') {
        const msg = actionName === 'installHooks'
          ? t('settings.remoteInstallSuccess', { defaultValue: 'Hooks 安装成功' })
          : t('settings.remoteUninstallSuccess', { defaultValue: 'Hooks 卸载成功' })
        const detail = typeof result === 'string' && result !== 'ok' ? `: ${result}` : ''
        setRemoteNotices((prev) => ({ ...prev, [id]: { type: 'success', message: msg + detail } }))
      }
    } catch (err) {
      console.error('Remote host action failed:', err)
      await refreshRemoteData().catch(() => {})
      if (actionName === 'installHooks' || actionName === 'uninstallHooks') {
        const msg = actionName === 'installHooks'
          ? t('settings.remoteInstallFailed', { defaultValue: 'Hooks 安装失败' })
          : t('settings.remoteUninstallFailed', { defaultValue: 'Hooks 卸载失败' })
        const detail = err instanceof Error ? `: ${err.message}` : ''
        setRemoteNotices((prev) => ({ ...prev, [id]: { type: 'error', message: msg + detail } }))
      }
    } finally {
      setRemoteBusyAction(null)
    }
  }

  async function runRemoteProbe(hostId: string) {
    setRemoteBusyAction({ id: hostId, action: 'probe' })
    setRemoteNotices((prev) => { const next = { ...prev }; delete next[hostId]; return next })
    try {
      const report = await probeRemoteHost(hostId)
      setRemoteProbeReports((prev) => ({ ...prev, [hostId]: report }))
      setRemoteNotices((prev) => ({
        ...prev,
        [hostId]: {
          type: report.ok ? 'success' : 'error',
          message: report.summary,
        },
      }))
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setRemoteNotices((prev) => ({
        ...prev,
        [hostId]: {
          type: 'error',
          message: message || t('settings.remoteProbeFailed', { defaultValue: '诊断失败' }),
        },
      }))
    } finally {
      setRemoteBusyAction(null)
    }
  }

  const displayedRemoteHosts = isTauri() ? remoteHosts : config.sshHosts.map((host) => ({
    id: host.id,
    name: host.name,
    sshTarget: host.host,
    port: null,
    identityFile: null,
    authSocket: null,
    remoteSocketPath: '/tmp/agentbro-remote.sock',
    autoConnect: false,
  } satisfies RemoteHost))

  function statusText(status: ConnectionStatus | undefined): string {
    if (!status) return t('settings.disconnected', { defaultValue: 'Disconnected' })
    if (status.state === 'failed') return t('settings.failed', { defaultValue: 'Failed' })
    return t(`settings.${status.state}`, { defaultValue: status.state })
  }

  function statusTone(status: ConnectionStatus | undefined): string {
    if (!status) return 'disconnected'
    return status.state
  }

  const existingRemoteNames = new Set(remoteHosts.map((remote) => remote.name))
  const seenSshConfigNames = new Set<string>()
  const importableSshConfigHosts = sshConfigHosts.filter((host) => {
    if (existingRemoteNames.has(host.name) || seenSshConfigNames.has(host.name)) return false
    seenSshConfigNames.add(host.name)
    return true
  })

  return (
    <>
      <div className="remote-servers-infra">
        <div className="remote-servers-infra__item remote-servers-infra__item--port">
          <span className="remote-servers-infra__icon" aria-hidden="true">⇄</span>
          <span className="remote-servers-infra__copy">
            <strong>{t('settings.listeningPort')}</strong>
            <span>{t('settings.listeningPortDesc')}</span>
          </span>
          <GlassInput
            className="remote-servers-infra__port"
            type="number"
            value={config.tcpPort}
            onChange={(e) => config.updateConfig('tcpPort', Number((e.target as HTMLInputElement).value))}
          />
        </div>
        <div className="remote-servers-infra__item">
          <span className="remote-servers-infra__icon remote-servers-infra__icon--requirement" aria-hidden="true">✓</span>
          <span className="remote-servers-infra__copy">
            <strong>{t('settings.sshPrerequisites')}</strong>
            <span>{t('settings.sshPrerequisitesText')}</span>
          </span>
        </div>
      </div>

      <SettingGroup label={t('settings.remoteHosts')}>
        {displayedRemoteHosts.length === 0 && (
          <div className="ssh-empty-state">
            <div className="ssh-empty-state__title">{t('settings.noRemoteHosts')}</div>
            <div className="ssh-empty-state__text">
              {t('settings.remoteHostsEmptyHint', { defaultValue: '手动添加主机，或从本机 SSH 配置中导入。' })}
            </div>
          </div>
        )}
        {displayedRemoteHosts.map((host) => {
          const status = remoteStatuses[host.id]
          const tone = statusTone(status)
          const isConnecting = status?.state === 'connecting'
          const actionForHost = remoteBusyAction?.id === host.id ? remoteBusyAction.action : null
          const busy = actionForHost !== null || isConnecting
          const isConnected = status?.state === 'connected'
          return (
            <div key={host.id} className={`ssh-host-card ssh-host-card--${tone}`}>
              <span className="ssh-host-card__terminal" aria-hidden="true">&gt;_</span>
              <div className="ssh-host-card__info">
                <div className="ssh-host-card__heading">
                  <div className="ssh-host-card__name">{host.name}</div>
                  {isTauri() && (
                    <span className={`ssh-status ssh-status--${tone}`}>
                      <span className="ssh-status__dot" aria-hidden="true" />
                      <span>{statusText(status)}</span>
                    </span>
                  )}
                </div>
                <div className="ssh-host-card__host">
                  <span className="ssh-host-card__target">{host.sshTarget}{host.port ? `:${host.port}` : ''}</span>
                  {status?.state === 'failed' && <span className="ssh-host-card__error" title={status.message}>{status.message}</span>}
                </div>
              </div>
              <div className="ssh-host-card__actions">
                {isTauri() && (
                  <>
                    <button
                      type="button"
                      className={`settings-mini-button ssh-host-card__primary${isConnected ? ' ssh-host-card__primary--disconnect' : ''}`}
                      disabled={busy}
                      onClick={() => runRemoteAction(
                        host.id,
                        isConnected ? 'disconnect' : 'connect',
                        () => isConnected ? disconnectServer(host.id) : connectServer(host.id),
                      )}
                    >
                      {isConnecting
                        ? t('settings.connecting', { defaultValue: '连接中...' })
                        : isConnected
                          ? t('settings.disconnect', { defaultValue: '断开' })
                          : t('settings.connect', { defaultValue: '连接' })}
                    </button>
                    <button
                      type="button"
                      className={`settings-mini-button${hooksPanelHost === host.id ? ' settings-mini-button--active' : ''}`}
                      disabled={!isConnected || busy}
                      title={!isConnected ? t('settings.connectFirst', { defaultValue: '请先连接远程主机' }) : undefined}
                      onClick={() => setHooksPanelHost(hooksPanelHost === host.id ? null : host.id)}
                    >
                      {t('settings.manageHooks', { defaultValue: 'Hooks 管理' })}
                      {isConnected && remoteHookStatuses[host.id] && (
                        <span className="ssh-hook-badge">{remoteHookStatuses[host.id].length}</span>
                      )}
                    </button>
                    <button
                      type="button"
                      className={`settings-mini-button${remoteProbeReports[host.id] ? ' settings-mini-button--active' : ''}`}
                      disabled={busy}
                      onClick={() => runRemoteProbe(host.id)}
                    >
                      {actionForHost === 'probe'
                        ? t('settings.remoteProbeRunning', { defaultValue: '诊断中...' })
                        : t('settings.remoteProbe', { defaultValue: '诊断' })}
                    </button>
                  </>
                )}
                <button
                  type="button"
                  className="ssh-host-card__remove"
                  aria-label={t('settings.removeHost')}
                  disabled={busy}
                  onClick={() => {
                    if (isTauri()) {
                      runRemoteAction(host.id, 'remove', () => removeRemoteHost(host.id))
                    } else {
                      config.removeSSHHost(host.id)
                    }
                  }}
                  title={t('settings.removeHost')}
                >
                  ×
                </button>
              </div>
              {remoteNotices[host.id] && (
                <div className={`ssh-host-card__notice ssh-host-card__notice--${remoteNotices[host.id].type}`}>
                  {remoteNotices[host.id].message}
                </div>
              )}
              {remoteProbeReports[host.id] && (
                <div className="ssh-probe-panel">
                  <div className="ssh-hooks-panel__header">
                    {t('settings.remoteProbeSummary', { defaultValue: '远程诊断' })}: {remoteProbeReports[host.id].summary}
                  </div>
                  <div className="ssh-probe-panel__list">
                    {remoteProbeReports[host.id].checks.length === 0 ? (
                      <div className="ssh-empty-state__text">
                        {t('settings.remoteProbeNoData', { defaultValue: '暂无诊断数据。' })}
                      </div>
                    ) : (
                      remoteProbeReports[host.id].checks.map((check) => (
                        <div className="ssh-probe-panel__row" key={check.id}>
                          <span className={`ssh-probe-panel__status ssh-probe-panel__status--${check.status}`} />
                          <span className="ssh-probe-panel__label">{check.label}</span>
                          <span className="ssh-probe-panel__detail">{check.detail}</span>
                        </div>
                      ))
                    )}
                  </div>
                </div>
              )}
              {hooksPanelHost === host.id && isConnected && (
                <div className="ssh-hooks-panel">
                  <div className="ssh-hooks-panel__header">
                    {t('settings.remoteHookAgents', { defaultValue: '选择要安装到远程的 Agent Hooks' })}
                  </div>
                  <div className="ssh-hooks-panel__list">
                    {installableAgents.map((agentId) => {
                      const installed = remoteHookStatuses[host.id]?.includes(agentId)
                      const isBusy = hookBusy?.hostId === host.id && hookBusy?.agentId === agentId
                      return (
                        <div key={agentId} className="ssh-hooks-panel__item">
                          <span className="ssh-hooks-panel__agent-name">{agentId}</span>
                          <span className={`ssh-hooks-panel__status ${installed ? 'ssh-hooks-panel__status--installed' : ''}`}>
                            {installed ? t('settings.installed', { defaultValue: '已安装' }) : t('settings.notInstalled', { defaultValue: '未安装' })}
                          </span>
                          {installed ? (
                            <button
                              type="button"
                              className="settings-mini-button settings-mini-button--danger settings-mini-button--sm"
                              disabled={isBusy}
                              onClick={async () => {
                                setHookBusy({ hostId: host.id, agentId })
                                try {
                                  await uninstallRemoteAgentHooks(host.id, agentId)
                                  setRemoteNotices((prev) => ({ ...prev, [host.id]: { type: 'success', message: `${agentId} hooks 已卸载` } }))
                                } catch (err) {
                                  setRemoteNotices((prev) => ({ ...prev, [host.id]: { type: 'error', message: `${agentId} 卸载失败${err instanceof Error ? `: ${err.message}` : ''}` } }))
                                } finally {
                                  setHookBusy(null)
                                  refreshHookStatus(host.id)
                                }
                              }}
                            >
                              {isBusy ? t('settings.uninstalling', { defaultValue: '卸载中...' }) : t('settings.uninstall', { defaultValue: '卸载' })}
                            </button>
                          ) : (
                            <button
                              type="button"
                              className="settings-mini-button settings-mini-button--sm"
                              disabled={isBusy}
                              onClick={async () => {
                                setHookBusy({ hostId: host.id, agentId })
                                try {
                                  await installRemoteAgentHooks(host.id, agentId)
                                  setRemoteNotices((prev) => ({ ...prev, [host.id]: { type: 'success', message: `${agentId} hooks 已安装` } }))
                                } catch (err) {
                                  setRemoteNotices((prev) => ({ ...prev, [host.id]: { type: 'error', message: `${agentId} 安装失败${err instanceof Error ? `: ${err.message}` : ''}` } }))
                                } finally {
                                  setHookBusy(null)
                                  refreshHookStatus(host.id)
                                }
                              }}
                            >
                              {isBusy ? t('settings.installing', { defaultValue: '安装中...' }) : t('settings.install', { defaultValue: '安装' })}
                            </button>
                          )}
                        </div>
                      )
                    })}
                  </div>
                </div>
              )}
            </div>
          )
        })}
      </SettingGroup>

      <SettingGroup label={t('settings.addRemoteHost', { defaultValue: '添加远程主机' })}>
        <div className="ssh-add-form">
          <GlassInput
            placeholder={t('settings.name')}
            value={newHostName}
            onChange={(e) => setNewHostName((e.target as HTMLInputElement).value)}
            style={{ flex: 1 }}
          />
          <GlassInput
            placeholder="user@host"
            value={newHostAddr}
            onChange={(e) => setNewHostAddr((e.target as HTMLInputElement).value)}
            style={{ flex: 1, fontFamily: 'var(--font-mono)', fontSize: 'var(--font-size-sm)' }}
          />
          <GlassButton variant="primary" onClick={addHost} disabled={!newHostName.trim() || !newHostAddr.trim()}>
            {t('settings.add')}
          </GlassButton>
        </div>
      </SettingGroup>

      {isTauri() && (
        <SettingGroup
          label={t('settings.importFromSshConfig', { defaultValue: 'Import from ~/.ssh/config' })}
          actions={(
            <button
              type="button"
              className="settings-mini-button"
              disabled={sshConfigRefreshing}
              onClick={refreshSshConfigImportList}
            >
              {t('settings.refresh', { defaultValue: '刷新' })}
            </button>
          )}
        >
          <div className="ssh-import-list">
            {importableSshConfigHosts.length === 0 ? (
              <div className="ssh-empty-state">
                <div className="ssh-empty-state__title">
                  {t('settings.noImportableSshHosts', { defaultValue: '暂无可导入的 SSH 主机。' })}
                </div>
                <div className="ssh-empty-state__text">
                  {t('settings.refreshSshConfigHint', { defaultValue: '修改 ~/.ssh/config 后点刷新重新读取。' })}
                </div>
              </div>
            ) : (
              importableSshConfigHosts.map((host) => (
                <div key={host.name} className="ssh-host-card ssh-host-card--import">
                  <div className="ssh-host-card__info">
                    <div className="ssh-host-card__name">{host.name}</div>
                    <div className="ssh-host-card__host">
                      {host.user ? `${host.user}@` : ''}{host.hostname || host.name}{host.port ? `:${host.port}` : ''}
                    </div>
                  </div>
                  <button
                    type="button"
                    className="settings-mini-button"
                    disabled={remoteBusyAction?.id === host.name}
                    onClick={() => runRemoteAction(host.name, 'import', () => importSshConfigHost(host))}
                  >
                    {t('settings.import', { defaultValue: 'Import' })}
                  </button>
                </div>
              ))
            )}
          </div>
        </SettingGroup>
      )}
    </>
  )
}
