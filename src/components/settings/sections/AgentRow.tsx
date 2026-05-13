import type { MouseEvent } from 'react'
import type { AgentOperationState } from '../../../stores/agentStore'
import type { AgentProgramInfo } from '../../../services/agentApi'
import { InlineConfirmAction } from '../../skills/InlineConfirmAction'

interface AgentRowProps {
  agent: AgentProgramInfo
  operation: AgentOperationState | undefined
  selected: boolean
  onSelect: () => void
  onRun: (operation: 'install' | 'update' | 'uninstall' | 'open') => void
  onToggleOutput: () => void
}

const statusLabel: Record<AgentProgramInfo['status'], string> = {
  installed: 'Installed',
  notInstalled: 'Not Installed',
  updateAvailable: 'Update Available',
  unavailable: 'Unavailable',
}

const kindLabel: Record<AgentProgramInfo['kind'], string> = {
  cli: 'CLI',
  app: 'App',
}

function agentInitials(agent: AgentProgramInfo) {
  const words = agent.displayName.split(/\s+/).filter(Boolean)
  if (words.length >= 2) return `${words[0][0]}${words[1][0]}`.toUpperCase()
  return agent.displayName.slice(0, 2).toUpperCase()
}

function stopAndRun(e: MouseEvent, fn: () => void) {
  e.stopPropagation()
  fn()
}

export function AgentRow({ agent, operation, selected, onSelect, onRun, onToggleOutput }: AgentRowProps) {
  const running = operation?.status === 'running'
  const failed = operation?.status === 'error'
  const installed = agent.status === 'installed' || agent.status === 'updateAvailable'
  const supportsInstall = Boolean(agent.installCommand)
  const supportsUpdate = Boolean(agent.updateCommand)
  const supportsUninstall = Boolean(agent.uninstallCommand)
  const meta = [
    agent.packageManager,
    agent.packageName,
    agent.installedVersion ? `v${agent.installedVersion}` : null,
  ].filter(Boolean).join(' · ')

  return (
    <div
      className={[
        'agent-row',
        selected ? 'agent-row--selected' : '',
        running ? 'agent-row--running' : '',
        failed ? 'agent-row--error' : '',
      ].filter(Boolean).join(' ')}
      onClick={onSelect}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault()
          onSelect()
        }
      }}
    >
      <div className="agent-row__main">
        <div className="agent-row__icon">{agentInitials(agent)}</div>
        <div className="agent-row__body">
          <div className="agent-row__title-line">
            <span className="agent-row__name">{agent.displayName}</span>
            <span className="agent-row__kind">{kindLabel[agent.kind]}</span>
            <span className={`agent-row__status agent-row__status--${agent.status}`}>
              {statusLabel[agent.status]}
            </span>
          </div>
          <div className="agent-row__meta">
            {meta || agent.binaryPath || agent.appPath || agent.downloadUrl || 'No package metadata'}
          </div>
        </div>
        <div className="agent-row__actions">
          {installed && supportsUpdate && (
            <button
              type="button"
              className="settings-mini-button"
              disabled={running}
              onClick={(e) => stopAndRun(e, () => onRun('update'))}
            >
              Update
            </button>
          )}
          {installed && agent.kind === 'app' && (
            <button
              type="button"
              className="settings-mini-button"
              disabled={running}
              onClick={(e) => stopAndRun(e, () => onRun('open'))}
            >
              Open
            </button>
          )}
          {!installed && supportsInstall && (
            <button
              type="button"
              className="settings-mini-button agent-row__primary"
              disabled={running}
              onClick={(e) => stopAndRun(e, () => onRun('install'))}
            >
              Install
            </button>
          )}
          {!installed && !supportsInstall && agent.downloadUrl && (
            <button
              type="button"
              className="settings-mini-button agent-row__primary"
              disabled={running}
              onClick={(e) => stopAndRun(e, () => onRun('open'))}
            >
              Download
            </button>
          )}
          {installed && supportsUninstall && (
            <InlineConfirmAction
              label="Uninstall"
              confirmLabel="Confirm"
              icon="−"
              disabled={running}
              onConfirm={() => onRun('uninstall')}
            />
          )}
          {operation && operation.lines.length > 0 && (
            <button
              type="button"
              className="agent-row__output-toggle"
              onClick={(e) => stopAndRun(e, onToggleOutput)}
            >
              {operation.expanded ? 'Hide' : 'Output'}
            </button>
          )}
        </div>
      </div>

      {operation?.expanded && operation.lines.length > 0 && (
        <div className="agent-row__terminal" onClick={(e) => e.stopPropagation()}>
          {operation.lines.map((line, index) => (
            <div key={`${line.stream}-${index}`} className={`agent-row__terminal-line agent-row__terminal-line--${line.stream}`}>
              <span>{line.stream}</span>
              <code>{line.text}</code>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
