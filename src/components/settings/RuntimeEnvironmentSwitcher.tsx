import { useEffect, useRef, useState } from 'react'
import type { MouseEvent as ReactMouseEvent } from 'react'
import { useTranslation } from 'react-i18next'
import {
  LOCAL_RUNTIME_ENVIRONMENT_ID,
  useRuntimeEnvironmentStore,
} from '../../stores/runtimeEnvironmentStore'
import type { ConnectionStatus, RemoteHost } from '../../services/tauriApi'
import { useRuntimeEnvironmentView } from '../../hooks/useRuntimeEnvironment'
import { useRemoteServerStore } from '../../stores/remoteServerStore'

interface RuntimeEnvironmentSwitcherProps {
  collapsed: boolean
  onManageRemote: () => void
}

export function RuntimeEnvironmentSwitcher({
  collapsed,
  onManageRemote,
}: RuntimeEnvironmentSwitcherProps) {
  const { t } = useTranslation()
  const rootRef = useRef<HTMLDivElement>(null)
  const [open, setOpen] = useState(false)
  const selectedEnvironmentId = useRuntimeEnvironmentStore((state) => state.selectedEnvironmentId)
  const selectEnvironment = useRuntimeEnvironmentStore((state) => state.selectEnvironment)
  const refreshEnvironments = useRuntimeEnvironmentStore((state) => state.refreshEnvironments)
  const connectEnvironment = useRuntimeEnvironmentStore((state) => state.connectEnvironment)
  const loading = useRemoteServerStore((state) => state.loading)
  const { hosts, statuses } = useRuntimeEnvironmentView()
  const selectedHost = hosts.find((host) => host.id === selectedEnvironmentId) ?? null
  const isRemoteSelection = selectedEnvironmentId !== LOCAL_RUNTIME_ENVIRONMENT_ID
  const selectedStatus = selectedHost ? statuses[selectedHost.id] : null
  const selectedName = selectedHost?.name
    ?? (isRemoteSelection ? selectedEnvironmentId : t('skills.runtimeEnvironment.localName'))
  const selectedMeta = selectedHost
    ? statusLabel(t, selectedStatus)
    : isRemoteSelection
      ? t('skills.runtimeEnvironment.loading')
      : t('skills.runtimeEnvironment.localMeta')

  useEffect(() => {
    void refreshEnvironments()
    const timer = window.setInterval(() => {
      void refreshEnvironments()
    }, 10_000)
    return () => window.clearInterval(timer)
  }, [refreshEnvironments])

  useEffect(() => {
    if (!open) return
    const handlePointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false)
    }
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false)
    }
    window.addEventListener('pointerdown', handlePointerDown)
    window.addEventListener('keydown', handleKeyDown)
    return () => {
      window.removeEventListener('pointerdown', handlePointerDown)
      window.removeEventListener('keydown', handleKeyDown)
    }
  }, [open])

  const chooseEnvironment = (id: string) => {
    selectEnvironment(id)
    setOpen(false)
  }

  const reconnect = (event: ReactMouseEvent, id: string) => {
    event.stopPropagation()
    void connectEnvironment(id)
  }

  return (
    <div className={`runtime-environment${collapsed ? ' runtime-environment--collapsed' : ''}`} ref={rootRef}>
      {open && (
        <div className="runtime-environment__popover" role="listbox" aria-label={t('skills.runtimeEnvironment.switchLabel')}>
          <div className="runtime-environment__popover-head">
            <div>
              <strong>{t('skills.runtimeEnvironment.switchLabel')}</strong>
              <span>{t('skills.runtimeEnvironment.switchHint')}</span>
            </div>
            {loading && <span className="runtime-environment__spinner" aria-label={t('skills.runtimeEnvironment.loading')} />}
          </div>

          <div className="runtime-environment__section-label">{t('skills.runtimeEnvironment.localSection')}</div>
          <EnvironmentOption
            selected={selectedEnvironmentId === LOCAL_RUNTIME_ENVIRONMENT_ID}
            name={t('skills.runtimeEnvironment.localName')}
            meta={t('skills.runtimeEnvironment.localMeta')}
            status="connected"
            kind="local"
            onClick={() => chooseEnvironment(LOCAL_RUNTIME_ENVIRONMENT_ID)}
          />

          <div className="runtime-environment__section-label runtime-environment__section-label--remote">
            <span>{t('skills.runtimeEnvironment.remoteSection')}</span>
            <em>{hosts.length}</em>
          </div>
          {hosts.length === 0 ? (
            <div className="runtime-environment__empty">{t('skills.runtimeEnvironment.empty')}</div>
          ) : hosts.map((host) => {
            const status = statuses[host.id]
            return (
              <EnvironmentOption
                key={host.id}
                selected={selectedEnvironmentId === host.id}
                name={host.name}
                meta={hostMeta(host, status, t)}
                status={status?.state ?? 'disconnected'}
                kind="remote"
                onClick={() => chooseEnvironment(host.id)}
                reconnectLabel={status?.state === 'disconnected' || status?.state === 'failed'
                  ? t('skills.runtimeEnvironment.reconnect')
                  : undefined}
                onReconnect={(event) => reconnect(event, host.id)}
              />
            )
          })}

          <div className="runtime-environment__footer">
            <button
              type="button"
              onClick={() => {
                setOpen(false)
                onManageRemote()
              }}
            >
              <span aria-hidden="true">＋</span>
              {hosts.length === 0
                ? t('skills.runtimeEnvironment.addRemote')
                : t('skills.runtimeEnvironment.manageRemote')}
            </button>
          </div>
        </div>
      )}

      <button
        type="button"
        className="runtime-environment__trigger"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={t('skills.runtimeEnvironment.currentLabel', { name: selectedName })}
        title={collapsed ? selectedName : undefined}
        onClick={() => setOpen((value) => !value)}
      >
        <span className={`runtime-environment__terminal runtime-environment__terminal--${isRemoteSelection ? 'remote' : 'local'}`} aria-hidden="true">
          {isRemoteSelection ? '>_' : '⌂'}
          <i className={`runtime-environment__status-dot runtime-environment__status-dot--${selectedStatus?.state ?? (isRemoteSelection ? 'connecting' : 'connected')}`} />
        </span>
        <span className="runtime-environment__copy">
          <span className="runtime-environment__name">{selectedName}</span>
          <span className="runtime-environment__meta">{selectedMeta}</span>
        </span>
        <span className={`runtime-environment__chevron${open ? ' runtime-environment__chevron--open' : ''}`} aria-hidden="true">⌃</span>
      </button>
    </div>
  )
}

export function RuntimeEnvironmentBadge() {
  const { t } = useTranslation()
  const selectedEnvironmentId = useRuntimeEnvironmentStore((state) => state.selectedEnvironmentId)
  const connectEnvironment = useRuntimeEnvironmentStore((state) => state.connectEnvironment)
  const { hosts, statuses } = useRuntimeEnvironmentView()
  const isRemoteSelection = selectedEnvironmentId !== LOCAL_RUNTIME_ENVIRONMENT_ID
  const selectedHost = hosts.find((host) => host.id === selectedEnvironmentId) ?? null
  const status = selectedHost ? statuses[selectedHost.id] : null
  const name = selectedHost?.name
    ?? (isRemoteSelection ? selectedEnvironmentId : t('skills.runtimeEnvironment.localName'))
  const isConnecting = status?.state === 'connecting'
  const showConnect = isRemoteSelection && selectedHost && status?.state !== 'connected'
  const connectLabel = isConnecting
    ? t('skills.runtimeEnvironment.connecting')
    : t('settings.connect', { defaultValue: '连接' })
  const connectHint = t('skills.runtimeEnvironment.liveConnectHint')

  return (
    <div className="runtime-environment-badge-group">
      <div
        className={`runtime-environment-badge${isRemoteSelection ? ' runtime-environment-badge--remote' : ''}`}
        role="status"
        aria-label={t('skills.runtimeEnvironment.currentLabel', { name })}
        title={selectedHost?.sshTarget}
      >
        <i className={`runtime-environment__status-dot runtime-environment__status-dot--${status?.state ?? (isRemoteSelection ? 'disconnected' : 'connected')}`} />
        <span>{isRemoteSelection ? t('skills.runtimeEnvironment.remoteMeta') : t('skills.runtimeEnvironment.localMeta')}</span>
        <strong>{name}</strong>
      </div>
      {showConnect && (
        <button
          type="button"
          className="runtime-environment-badge__connect"
          aria-label={t('skills.runtimeEnvironment.liveConnectLabel', { name })}
          title={connectHint}
          disabled={isConnecting}
          onClick={() => void connectEnvironment(selectedHost.id)}
        >
          {isConnecting && <span className="runtime-environment-badge__spinner" aria-hidden="true" />}
          {connectLabel}
        </button>
      )}
    </div>
  )
}

interface EnvironmentOptionProps {
  selected: boolean
  name: string
  meta: string
  status: ConnectionStatus['state']
  kind: 'local' | 'remote'
  onClick: () => void
  reconnectLabel?: string
  onReconnect?: (event: ReactMouseEvent) => void
}

function EnvironmentOption({
  selected,
  name,
  meta,
  status,
  kind,
  onClick,
  reconnectLabel,
  onReconnect,
}: EnvironmentOptionProps) {
  return (
    <div className={`runtime-environment__option${selected ? ' runtime-environment__option--selected' : ''}`}>
      <button
        type="button"
        role="option"
        aria-selected={selected}
        className="runtime-environment__option-main"
        onClick={onClick}
      >
        <span className={`runtime-environment__option-icon runtime-environment__option-icon--${kind}`} aria-hidden="true">
          {kind === 'local' ? '⌂' : '>_'}
        </span>
        <span className="runtime-environment__option-copy">
          <strong>{name}</strong>
          <span>{meta}</span>
        </span>
        <i className={`runtime-environment__status-dot runtime-environment__status-dot--${status}`} />
        {selected && <span className="runtime-environment__check" aria-hidden="true">✓</span>}
      </button>
      {reconnectLabel && onReconnect && (
        <button type="button" className="runtime-environment__reconnect" onClick={onReconnect}>
          {reconnectLabel}
        </button>
      )}
    </div>
  )
}

function statusLabel(
  t: ReturnType<typeof useTranslation>['t'],
  status: ConnectionStatus | null | undefined,
) {
  switch (status?.state) {
    case 'connected':
      return t('skills.runtimeEnvironment.liveConnected')
    case 'connecting':
      return t('skills.runtimeEnvironment.liveConnecting')
    case 'failed':
      return t('skills.runtimeEnvironment.liveFailed')
    default:
      return t('skills.runtimeEnvironment.liveDisconnected')
  }
}

function hostMeta(
  host: RemoteHost,
  status: ConnectionStatus | undefined,
  t: ReturnType<typeof useTranslation>['t'],
) {
  const connection = statusLabel(t, status)
  const address = host.port ? `${host.sshTarget}:${host.port}` : host.sshTarget
  return address ? `${address} · ${connection}` : connection
}
