import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { agentApi, type RemoteAgentHookStatus } from '../../services/agentApi'
import { useRuntimeEnvironmentStore } from '../../stores/runtimeEnvironmentStore'
import { useSkillStoreV2 } from '../../stores/skillStoreV2'

const statusLabels = {
  installed: 'settings.hookInstalled',
  notInstalled: 'settings.hookNotInstalled',
  needsReinstall: 'settings.hookNeedsReinstall',
  settingsCorrupted: 'settings.hookSettingsCorrupted',
  unsupported: 'skills.agentManagement.remoteHooks.unsupported',
} as const satisfies Record<RemoteAgentHookStatus, string>

export function RemoteAgentHooksTab({
  agentId, agentName, agentRevision, environmentId, runtimeRevision, hostName, disabled, runHookMutation,
}: {
  agentId: string
  agentName: string
  agentRevision: number
  environmentId: string
  runtimeRevision: number
  hostName: string
  disabled: boolean
  runHookMutation: (agentId: string, mutate: () => Promise<void>) => Promise<void>
}) {
  const { t } = useTranslation()
  const [status, setStatus] = useState<RemoteAgentHookStatus | null>(null)
  const [busy, setBusy] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const isCurrent = useCallback(() => {
    const runtime = useRuntimeEnvironmentStore.getState()
    const skills = useSkillStoreV2.getState()
    return runtime.selectedEnvironmentId === environmentId
      && runtime.selectionRevision === runtimeRevision
      && skills.runtimeEnvironmentId === environmentId
      && skills.selectedAgentId === agentId
      && skills.agentSelectionRevision === agentRevision
  }, [agentId, agentRevision, environmentId, runtimeRevision])

  const run = useCallback(async (action: 'check' | 'install' | 'uninstall') => {
    if (!isCurrent()) return
    setBusy(true)
    setError(null)
    setStatus(null)
    try {
      if (action !== 'check') {
        const mutate = action === 'install' ? agentApi.installHook : agentApi.uninstallHook
        await runHookMutation(agentId, () => mutate(agentId, environmentId))
      }
      if (!isCurrent()) return
      const nextStatus = await agentApi.remoteHookStatus(agentId, environmentId)
      if (isCurrent()) setStatus(nextStatus)
    } catch (nextError) {
      if (isCurrent()) setError(String(nextError))
    } finally {
      if (isCurrent()) setBusy(false)
    }
  }, [agentId, environmentId, isCurrent, runHookMutation])

  useEffect(() => {
    void run('check')
  }, [run])

  function uninstall() {
    if (!isCurrent()) return
    if (window.confirm(t('skills.agentManagement.remoteHooks.confirmUninstall', { agent: agentName, host: hostName }))) {
      void run('uninstall')
    }
  }

  return (
    <div className="sm2__hooks-workspace">
      <section className="sm2__panel sm2__hook-summary" aria-busy={busy}>
        <div className="sm2__hook-summary-main">
          <strong>{t('settings.hooks')}</strong>
          <span>{t('skills.runtimeEnvironment.currentLabel', { name: hostName })}</span>
          {busy && <span role="status">{t('settings.detecting')}</span>}
          {status && <span role="status">{t(statusLabels[status])}</span>}
        </div>
        <div className="sm2__btn-row sm2__hook-actions">
          <button className="sm2__btn" type="button" disabled={busy} onClick={() => void run('check')}>
            {t('settings.detectNow')}
          </button>
          {status === 'installed' && (
            <button className="sm2__btn sm2__btn--danger" type="button" disabled={busy || disabled} onClick={uninstall}>
              {t('settings.uninstallHooks')}
            </button>
          )}
          {(status === 'notInstalled' || status === 'needsReinstall') && (
            <button className="sm2__btn sm2__btn--primary" type="button" disabled={busy || disabled} onClick={() => void run('install')}>
              {t(status === 'needsReinstall' ? 'settings.reinstall' : 'settings.installHooks')}
            </button>
          )}
        </div>
      </section>
      {error && <div className="sm2__error" role="alert">{error}</div>}
    </div>
  )
}
