import { useMemo } from 'react'
import { isTauri } from '../services/tauriApi'
import type { ConnectionStatus, RemoteHost } from '../services/tauriApi'
import { useConfigStore } from '../stores/configStore'
import {
  LOCAL_RUNTIME_ENVIRONMENT_ID,
  useRuntimeEnvironmentStore,
} from '../stores/runtimeEnvironmentStore'
import { useRemoteServerStore } from '../stores/remoteServerStore'

export function useRuntimeEnvironmentView() {
  const remoteHosts = useRemoteServerStore((state) => state.remoteHosts)
  const remoteStatuses = useRemoteServerStore((state) => state.remoteStatuses)
  const previewHosts = useConfigStore((state) => state.remoteHostEntries)
  const legacyPreviewHosts = useConfigStore((state) => state.sshHosts)

  return useMemo(() => {
    if (isTauri()) return { hosts: remoteHosts, statuses: remoteStatuses }

    const hosts: RemoteHost[] = previewHosts.length > 0
      ? previewHosts.map((host) => ({
          id: host.id,
          name: host.name,
          sshTarget: host.sshTarget,
          port: host.port,
          identityFile: null,
          authSocket: null,
          remoteSocketPath: host.remoteSocketPath,
          autoConnect: host.autoConnect,
        }))
      : legacyPreviewHosts.map((host) => ({
          id: host.id,
          name: host.name,
          sshTarget: host.host,
          port: null,
          identityFile: null,
          authSocket: null,
          remoteSocketPath: '',
          autoConnect: host.enabled,
        }))
    const statuses = previewHosts.length > 0
      ? Object.fromEntries(previewHosts.map((host) => [
          host.id,
          host.connectionStatus === 'failed'
            ? { state: 'failed', message: '' }
            : { state: host.connectionStatus },
        ])) as Record<string, ConnectionStatus>
      : Object.fromEntries(legacyPreviewHosts.map((host) => [
          host.id,
          { state: host.enabled ? 'connected' : 'disconnected' },
        ])) as Record<string, ConnectionStatus>

    return { hosts, statuses }
  }, [legacyPreviewHosts, previewHosts, remoteHosts, remoteStatuses])
}

export function useSelectedRuntimeEnvironment() {
  const selectedEnvironmentId = useRuntimeEnvironmentStore((state) => state.selectedEnvironmentId)
  const { hosts, statuses } = useRuntimeEnvironmentView()
  const remoteHost = hosts.find((host) => host.id === selectedEnvironmentId) ?? null

  return {
    selectedEnvironmentId,
    isLocal: selectedEnvironmentId === LOCAL_RUNTIME_ENVIRONMENT_ID,
    remoteHost,
    status: remoteHost ? statuses[remoteHost.id] ?? { state: 'disconnected' as const } : null,
  }
}
