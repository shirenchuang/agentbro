import { AnimatePresence, motion } from 'framer-motion'
import type { AgentProgramInfo } from '../../../services/agentApi'
import { InlineConfirmAction } from '../../skills/InlineConfirmAction'

interface AgentDetailSliderProps {
  agent: AgentProgramInfo | null
  open: boolean
  onClose: () => void
  onRun: (agentId: string, operation: 'install' | 'update' | 'uninstall' | 'open') => void
}

function InfoRow({ label, value }: { label: string; value: string | null | undefined }) {
  if (!value) return null
  return (
    <div className="agent-detail-info-row">
      <span className="agent-detail-info-row__label">{label}</span>
      <span className="agent-detail-info-row__value">{value}</span>
    </div>
  )
}

function statusText(status: AgentProgramInfo['status']) {
  if (status === 'installed') return 'Installed'
  if (status === 'updateAvailable') return 'Update Available'
  if (status === 'notInstalled') return 'Not Installed'
  return 'Unavailable'
}

export function AgentDetailSlider({ agent, open, onClose, onRun }: AgentDetailSliderProps) {
  const installed = agent?.status === 'installed' || agent?.status === 'updateAvailable'

  return (
    <AnimatePresence>
      {open && agent && (
        <motion.div
          className="agent-detail-overlay"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
        >
          <div className="agent-detail-backdrop" onClick={onClose} />
          <motion.aside
            className="agent-detail-panel"
            initial={{ x: 420 }}
            animate={{ x: 0 }}
            exit={{ x: 420 }}
            transition={{ type: 'spring', damping: 28, stiffness: 320 }}
          >
            <button className="agent-detail-panel__close" onClick={onClose}>×</button>
            <div className="agent-detail-panel__header">
              <div className="agent-detail-panel__icon">{agent.displayName.slice(0, 2).toUpperCase()}</div>
              <div>
                <div className="agent-detail-panel__title">{agent.displayName}</div>
                <div className="agent-detail-panel__desc">
                  {agent.kind === 'cli' ? 'Command-line agent' : 'Desktop application'} · {statusText(agent.status)}
                </div>
              </div>
            </div>

            <section className="agent-detail-section">
              <div className="agent-detail-section__title">Program</div>
              <InfoRow label="Package Manager" value={agent.packageManager} />
              <InfoRow label="Package" value={agent.packageName} />
              <InfoRow label="Installed Version" value={agent.installedVersion} />
              <InfoRow label="Latest Version" value={agent.latestVersion} />
              <InfoRow label="Binary Path" value={agent.binaryPath} />
              <InfoRow label="App Path" value={agent.appPath} />
              <InfoRow label="Config Dir" value={agent.configDir} />
              <InfoRow label="Download URL" value={agent.downloadUrl} />
            </section>

            <section className="agent-detail-section">
              <div className="agent-detail-section__title">Commands</div>
              <InfoRow label="Install" value={agent.installCommand} />
              <InfoRow label="Update" value={agent.updateCommand} />
              <InfoRow label="Uninstall" value={agent.uninstallCommand} />
              {!agent.installCommand && !agent.updateCommand && !agent.uninstallCommand && (
                <div className="agent-detail-empty">No direct command is configured for this agent.</div>
              )}
            </section>

            <section className="agent-detail-section">
              <div className="agent-detail-section__title">AgentBro Hooks</div>
              <div className="agent-detail-hook-state">
                <span className={`agent-detail-hook-dot ${agent.hooksInstalled ? 'agent-detail-hook-dot--on' : ''}`} />
                {agent.hooksInstalled ? 'Hooks installed' : 'Hooks not installed'}
              </div>
            </section>

            <div className="agent-detail-panel__footer">
              {installed && agent.updateCommand && (
                <button className="settings-mini-button" onClick={() => onRun(agent.id, 'update')}>Update</button>
              )}
              {!installed && agent.installCommand && (
                <button className="settings-mini-button agent-detail-primary" onClick={() => onRun(agent.id, 'install')}>Install</button>
              )}
              {!installed && !agent.installCommand && agent.downloadUrl && (
                <button className="settings-mini-button agent-detail-primary" onClick={() => onRun(agent.id, 'open')}>Download</button>
              )}
              {installed && agent.kind === 'app' && (
                <button className="settings-mini-button" onClick={() => onRun(agent.id, 'open')}>Open</button>
              )}
              {installed && agent.uninstallCommand && (
                <InlineConfirmAction
                  label="Uninstall"
                  confirmLabel="Confirm"
                  icon="−"
                  onConfirm={() => onRun(agent.id, 'uninstall')}
                />
              )}
            </div>
          </motion.aside>
        </motion.div>
      )}
    </AnimatePresence>
  )
}
