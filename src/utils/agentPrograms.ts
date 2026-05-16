import type { AgentProgramInfo } from '../services/agentApi'

const installedStatuses = new Set<AgentProgramInfo['status']>(['installed', 'updateAvailable'])

export function isAgentProgramInstalled(agent: AgentProgramInfo) {
  return installedStatuses.has(agent.status)
}

export function agentAliasIds(agentId: string) {
  if (agentId === 'gemini' || agentId === 'gemini-cli') return ['gemini', 'gemini-cli']
  if (agentId === 'cursor' || agentId === 'cursor-cli') return ['cursor', 'cursor-cli']
  if (agentId === 'droid' || agentId === 'factory-droid') return ['droid', 'factory-droid']
  if (agentId === 'traecn' || agentId === 'trae-cn') return ['traecn', 'trae-cn']
  if (agentId === 'qoder' || agentId === 'qoder-cli') return ['qoder', 'qoder-cli']
  return [agentId]
}

export function agentMatchesId(installedAgentId: string, filterAgentId: string) {
  return agentAliasIds(filterAgentId).includes(installedAgentId)
}

export function displayAgentName(agentId: string, agents: AgentProgramInfo[]) {
  if (agentId === 'central') return '中央技能库'

  const exact = agents.find((agent) => agent.id === agentId)
  if (exact) return exact.displayName

  const alias = agents.find((agent) => agentAliasIds(agentId).includes(agent.id))
  if (alias) return alias.displayName

  return agentId
}

export function shortAgentName(agentId: string, agents: AgentProgramInfo[]) {
  const name = displayAgentName(agentId, agents)
  return name
    .split(/\s|-/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase())
    .join('') || agentId.slice(0, 2).toUpperCase()
}

export function agentColor(agentId: string) {
  if (agentId === 'central') return '#1d1d1f'
  if (agentId === 'claude-code') return '#5856d6'
  if (agentId === 'codex') return '#34c759'
  if (agentId === 'gemini' || agentId === 'gemini-cli') return '#ff9500'
  if (agentId === 'cursor' || agentId === 'cursor-cli') return '#007aff'
  if (agentId === 'hermes') return '#ff2d55'
  if (agentId === 'opencode') return '#0a84ff'
  if (agentId === 'openclaw' || agentId === 'qclaw' || agentId === 'easyclaw' || agentId === 'autoclaw') return '#30b0c7'
  if (agentId === 'windsurf') return '#5ac8fa'
  if (agentId === 'workbuddy') return '#af52de'
  return '#8e8e93'
}

export function detectedAgentOptions(agents: AgentProgramInfo[], installedOnly = true) {
  const source = installedOnly ? agents.filter(isAgentProgramInstalled) : agents
  const unique = new Map<string, AgentProgramInfo>()
  for (const agent of source) {
    unique.set(agent.id, agent)
  }
  return Array.from(unique.values())
}
