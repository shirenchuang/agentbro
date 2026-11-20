import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  getWslHookStatus, listWslDistributions, setWslHook, type WslHookStatus,
} from '../../../services/tauriApi'
import { GlassButton } from '../../shared'
import { SettingGroup } from '../SettingGroup'
import { SettingRow } from '../SettingRow'

export function WslHooksSection() {
  const { t } = useTranslation()
  const [distributions, setDistributions] = useState<string[]>([])
  const [distribution, setDistribution] = useState('')
  const [statuses, setStatuses] = useState<WslHookStatus[]>([])
  const [busy, setBusy] = useState(true)
  const [error, setError] = useState('')

  async function refresh() {
    setBusy(true)
    setError('')
    try {
      const list = await listWslDistributions()
      setDistributions(list)
      if (distribution && list.includes(distribution)) {
        setStatuses(await getWslHookStatus(distribution))
      } else {
        setDistribution('')
        setStatuses([])
      }
    } catch (error) {
      setError(String(error))
    } finally {
      setBusy(false)
    }
  }

  useEffect(() => {
    let active = true
    listWslDistributions().then((list) => {
      if (active) setDistributions(list)
    }).catch((error: unknown) => {
      if (active) setError(String(error))
    }).finally(() => {
      if (active) setBusy(false)
    })
    return () => { active = false }
  }, [])

  async function select(name: string) {
    setDistribution(name)
    setStatuses([])
    setError('')
    if (!name) return
    setBusy(true)
    try {
      setStatuses(await getWslHookStatus(name))
    } catch (error) {
      setError(String(error))
    } finally {
      setBusy(false)
    }
  }

  async function toggle(status: WslHookStatus) {
    setBusy(true)
    setError('')
    try {
      await setWslHook(distribution, status.agentId, !status.installed)
      setStatuses(await getWslHookStatus(distribution))
    } catch (error) {
      setError(String(error))
    } finally {
      setBusy(false)
    }
  }

  return (
    <SettingGroup
      label={t('settings.wslHooks.title')}
      actions={<GlassButton variant="ghost" disabled={busy} onClick={() => void refresh()}>{t('settings.wslHooks.refresh')}</GlassButton>}
    >
      <SettingRow
        label={t('settings.wslHooks.distribution')}
        description={!busy && !distributions.length ? t('settings.wslHooks.noDistributions') : undefined}
      >
        <select
          aria-label={t('settings.wslHooks.distribution')}
          className="glass-dropdown__trigger"
          value={distribution}
          disabled={busy || !distributions.length}
          onChange={(event) => void select(event.target.value)}
        >
          <option value="">{t('settings.wslHooks.select')}</option>
          {distributions.map((name) => <option value={name} key={name}>{name}</option>)}
        </select>
      </SettingRow>
      {statuses.map((status) => (
        <SettingRow
          key={status.agentId}
          label={status.agentId === 'codex' ? 'Codex CLI' : 'Claude Code'}
          description={status.error || (status.agentId === 'codex' && status.installed ? t('settings.wslHooks.codexTrust') : undefined)}
        >
          <GlassButton variant="secondary" disabled={busy || Boolean(status.error)} onClick={() => void toggle(status)}>
            {t(status.installed ? 'settings.uninstall' : 'settings.install')}
          </GlassButton>
        </SettingRow>
      ))}
      {busy && <div className="hook-empty" role="status">{t('settings.wslHooks.loading')}</div>}
      {error && <div className="hook-empty" role="alert">{error}</div>}
    </SettingGroup>
  )
}
