import { create } from 'zustand'
import type { AgentOutputEvent, AgentProgramInfo } from '../services/agentApi'
import { agentApi } from '../services/agentApi'

export type AgentFilter = 'all' | 'installed' | 'available' | 'updates'
export type AgentOperationName = 'install' | 'update' | 'uninstall' | 'open'
export type AgentOperationStatus = 'idle' | 'running' | 'success' | 'error'

export interface AgentOperationState {
  name: AgentOperationName
  status: AgentOperationStatus
  lines: { stream: string; text: string }[]
  error: string | null
  expanded: boolean
}

interface AgentState {
  agents: AgentProgramInfo[]
  loading: boolean
  searchQuery: string
  filter: AgentFilter
  selectedAgentId: string | null
  detailOpen: boolean
  operations: Record<string, AgentOperationState>
}

interface AgentActions {
  loadAgents: () => Promise<void>
  refreshAgents: () => Promise<void>
  setSearchQuery: (query: string) => void
  setFilter: (filter: AgentFilter) => void
  selectAgent: (id: string) => void
  closeDetail: () => void
  runOperation: (agentId: string, operation: AgentOperationName) => Promise<void>
  handleOutput: (event: AgentOutputEvent) => void
  toggleOutput: (agentId: string) => void
}

const emptyOperation = (name: AgentOperationName): AgentOperationState => ({
  name,
  status: 'running',
  lines: [],
  error: null,
  expanded: true,
})

export const useAgentStore = create<AgentState & AgentActions>()((set, get) => ({
  agents: [],
  loading: false,
  searchQuery: '',
  filter: 'all',
  selectedAgentId: null,
  detailOpen: false,
  operations: {},

  loadAgents: async () => {
    set({ loading: true })
    try {
      const agents = await agentApi.list()
      set({ agents, loading: false })
    } catch (e) {
      console.error('Failed to load agents:', e)
      set({ loading: false })
    }
  },

  refreshAgents: async () => {
    set({ loading: true })
    try {
      const agents = await agentApi.refresh()
      set({ agents, loading: false })
    } catch (e) {
      console.error('Failed to refresh agents:', e)
      set({ loading: false })
    }
  },

  setSearchQuery: (searchQuery) => set({ searchQuery }),
  setFilter: (filter) => set({ filter }),
  selectAgent: (id) => set({ selectedAgentId: id, detailOpen: true }),
  closeDetail: () => set({ selectedAgentId: null, detailOpen: false }),

  runOperation: async (agentId, operation) => {
    set((state) => ({
      operations: {
        ...state.operations,
        [agentId]: emptyOperation(operation),
      },
    }))

    try {
      if (operation === 'install') await agentApi.install(agentId)
      if (operation === 'update') await agentApi.update(agentId)
      if (operation === 'uninstall') await agentApi.uninstall(agentId)
      if (operation === 'open') {
        const agent = get().agents.find((item) => item.id === agentId)
        if (agent?.status === 'installed' && agent.kind === 'app') await agentApi.openApp(agentId)
        else await agentApi.openDownload(agentId)
      }

      if (operation === 'open') {
        set((state) => ({
          operations: {
            ...state.operations,
            [agentId]: {
              name: operation,
              status: 'success',
              lines: [{ stream: 'info', text: 'Opened' }],
              error: null,
              expanded: false,
            },
          },
        }))
      }

      await get().refreshAgents()
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e)
      set((state) => {
        const current = state.operations[agentId] ?? emptyOperation(operation)
        return {
          operations: {
            ...state.operations,
            [agentId]: {
              ...current,
              status: 'error',
              error: message,
              expanded: true,
              lines: [...current.lines, { stream: 'stderr', text: message }],
            },
          },
        }
      })
    }
  },

  handleOutput: (event) => {
    set((state) => {
      const name = event.operation as AgentOperationName
      const current = state.operations[event.agentId] ?? emptyOperation(name)
      const nextStatus: AgentOperationStatus = event.done
        ? event.success ? 'success' : 'error'
        : 'running'

      return {
        operations: {
          ...state.operations,
          [event.agentId]: {
            ...current,
            name,
            status: nextStatus,
            error: nextStatus === 'error' ? event.line : current.error,
            expanded: current.expanded || nextStatus === 'error',
            lines: event.line
              ? [...current.lines, { stream: event.stream, text: event.line }]
              : current.lines,
          },
        },
      }
    })
  },

  toggleOutput: (agentId) => {
    set((state) => {
      const current = state.operations[agentId]
      if (!current) return state
      return {
        operations: {
          ...state.operations,
          [agentId]: {
            ...current,
            expanded: !current.expanded,
          },
        },
      }
    })
  },
}))
