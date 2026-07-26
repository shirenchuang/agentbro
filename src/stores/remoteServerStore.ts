import { create } from 'zustand'
import {
  connectRemote,
  disconnectRemote,
  getRemoteStatus,
  isTauri,
  listRemoteHosts,
} from '../services/tauriApi'
import type { ConnectionStatus, RemoteHost } from '../services/tauriApi'

interface RemoteServerState {
  remoteHosts: RemoteHost[]
  remoteStatuses: Record<string, ConnectionStatus>
  loaded: boolean
  loading: boolean
  error: string | null
}

interface RemoteServerActions {
  refreshServers: () => Promise<void>
  connectServer: (id: string) => Promise<void>
  disconnectServer: (id: string) => Promise<void>
}

type RemoteServerStore = RemoteServerState & RemoteServerActions

export const useRemoteServerStore = create<RemoteServerStore>((set, get) => ({
  remoteHosts: [],
  remoteStatuses: {},
  loaded: false,
  loading: false,
  error: null,

  refreshServers: async () => {
    if (!isTauri()) {
      set({ loaded: true, loading: false, error: null })
      return
    }

    set({ loading: true, error: null })
    try {
      const remoteHosts = await listRemoteHosts()
      const statusEntries = await Promise.all(remoteHosts.map(async (host) => {
        try {
          return [host.id, await getRemoteStatus(host.id)] as const
        } catch (error) {
          return [
            host.id,
            { state: 'failed', message: String(error) } satisfies ConnectionStatus,
          ] as const
        }
      }))

      set({
        remoteHosts,
        remoteStatuses: Object.fromEntries(statusEntries),
        loaded: true,
        loading: false,
        error: null,
      })
    } catch (error) {
      set({
        loaded: true,
        loading: false,
        error: String(error),
      })
    }
  },

  connectServer: async (id) => {
    if (!id || !isTauri()) return
    set((state) => ({
      remoteStatuses: {
        ...state.remoteStatuses,
        [id]: { state: 'connecting' },
      },
      error: null,
    }))
    try {
      await connectRemote(id)
      window.setTimeout(() => {
        void get().refreshServers()
      }, 500)
    } catch (error) {
      set((state) => ({
        remoteStatuses: {
          ...state.remoteStatuses,
          [id]: { state: 'failed', message: String(error) },
        },
        error: String(error),
      }))
    }
  },

  disconnectServer: async (id) => {
    if (!id || !isTauri()) return
    try {
      await disconnectRemote(id)
      await get().refreshServers()
    } catch (error) {
      set((state) => ({
        remoteStatuses: {
          ...state.remoteStatuses,
          [id]: { state: 'failed', message: String(error) },
        },
        error: String(error),
      }))
    }
  },
}))
