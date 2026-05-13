import { useEffect, useMemo } from 'react'
import { SettingSection } from '../SettingSection'
import { agentApi } from '../../../services/agentApi'
import { useAgentStore, type AgentFilter } from '../../../stores/agentStore'
import { AgentRow } from './AgentRow'
import { AgentDetailSlider } from './AgentDetailSlider'
import './AgentsSection.css'

const filters: { id: AgentFilter; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'installed', label: 'Installed' },
  { id: 'available', label: 'Available' },
  { id: 'updates', label: 'Updates' },
]

export function AgentsSection() {
  const {
    agents,
    loading,
    filter,
    searchQuery,
    selectedAgentId,
    detailOpen,
    operations,
    loadAgents,
    refreshAgents,
    setFilter,
    setSearchQuery,
    selectAgent,
    closeDetail,
    runOperation,
    handleOutput,
    toggleOutput,
  } = useAgentStore()

  useEffect(() => {
    loadAgents()
  }, [loadAgents])

  useEffect(() => {
    let unlisten: (() => void) | undefined
    agentApi.onOutput(handleOutput).then((fn) => { unlisten = fn })
    return () => unlisten?.()
  }, [handleOutput])

  const filteredAgents = useMemo(() => {
    const q = searchQuery.trim().toLowerCase()
    return agents.filter((agent) => {
      const matchesSearch = !q
        || agent.displayName.toLowerCase().includes(q)
        || agent.id.toLowerCase().includes(q)
        || agent.packageName?.toLowerCase().includes(q)
      const matchesFilter =
        filter === 'all'
        || (filter === 'installed' && (agent.status === 'installed' || agent.status === 'updateAvailable'))
        || (filter === 'available' && agent.status === 'notInstalled')
        || (filter === 'updates' && agent.status === 'updateAvailable')
      return matchesSearch && matchesFilter
    })
  }, [agents, filter, searchQuery])

  const selectedAgent = agents.find((agent) => agent.id === selectedAgentId) ?? null
  const installedCount = agents.filter((agent) => agent.status === 'installed' || agent.status === 'updateAvailable').length
  const updateCount = agents.filter((agent) => agent.status === 'updateAvailable').length
  const cliCount = agents.filter((agent) => agent.kind === 'cli').length

  return (
    <SettingSection
      title="Agents"
      description="Install, update, detect, and open supported AI agent programs."
    >
      <div className="agents-section">
        <div className="agents-toolbar">
          <div className="agents-summary">
            <span><strong>{installedCount}</strong> installed</span>
            <span><strong>{updateCount}</strong> updates</span>
            <span><strong>{cliCount}</strong> CLI tools</span>
          </div>
          <button className="settings-mini-button" onClick={refreshAgents} disabled={loading}>
            {loading ? 'Refreshing...' : 'Refresh'}
          </button>
        </div>

        <div className="agents-controls">
          <input
            className="agents-search"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Search agents..."
          />
          <div className="agents-filter-tabs">
            {filters.map((item) => (
              <button
                key={item.id}
                className={`agents-filter-tab ${filter === item.id ? 'agents-filter-tab--active' : ''}`}
                onClick={() => setFilter(item.id)}
              >
                {item.label}
              </button>
            ))}
          </div>
        </div>

        <div className="agents-list">
          {filteredAgents.map((agent) => (
            <AgentRow
              key={agent.id}
              agent={agent}
              operation={operations[agent.id]}
              selected={selectedAgentId === agent.id}
              onSelect={() => selectAgent(agent.id)}
              onRun={(operation) => runOperation(agent.id, operation)}
              onToggleOutput={() => toggleOutput(agent.id)}
            />
          ))}
          {!loading && filteredAgents.length === 0 && (
            <div className="agents-empty">No agents match the current filters.</div>
          )}
        </div>
      </div>

      <AgentDetailSlider
        agent={selectedAgent}
        open={detailOpen}
        onClose={closeDetail}
        onRun={runOperation}
      />
    </SettingSection>
  )
}
