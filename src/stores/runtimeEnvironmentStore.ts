import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import { useRemoteServerStore } from './remoteServerStore'

export const LOCAL_RUNTIME_ENVIRONMENT_ID = 'local'

interface RuntimeEnvironmentState {
  selectedEnvironmentId: string
}

interface RuntimeEnvironmentActions {
  selectEnvironment: (id: string) => void
  refreshEnvironments: () => Promise<void>
  connectEnvironment: (id: string) => Promise<void>
}

type RuntimeEnvironmentStore = RuntimeEnvironmentState & RuntimeEnvironmentActions

export const useRuntimeEnvironmentStore = create<RuntimeEnvironmentStore>()(
  persist(
    (set, get) => ({
      selectedEnvironmentId: LOCAL_RUNTIME_ENVIRONMENT_ID,

      selectEnvironment: (id) => {
        const nextId = id.trim() || LOCAL_RUNTIME_ENVIRONMENT_ID
        set({ selectedEnvironmentId: nextId })
      },

      refreshEnvironments: async () => {
        await useRemoteServerStore.getState().refreshServers()
        const { error, remoteHosts } = useRemoteServerStore.getState()
        if (error) return

        const selectedEnvironmentId = get().selectedEnvironmentId
        const selectedStillExists = selectedEnvironmentId === LOCAL_RUNTIME_ENVIRONMENT_ID
          || remoteHosts.some((host) => host.id === selectedEnvironmentId)
        if (!selectedStillExists) set({ selectedEnvironmentId: LOCAL_RUNTIME_ENVIRONMENT_ID })
      },

      connectEnvironment: async (id) => {
        if (!id || id === LOCAL_RUNTIME_ENVIRONMENT_ID) return
        await useRemoteServerStore.getState().connectServer(id)
      },
    }),
    {
      name: 'agentbro-runtime-environment',
      partialize: (state) => ({
        selectedEnvironmentId: state.selectedEnvironmentId,
      }),
    },
  ),
)
