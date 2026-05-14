import { AnimatePresence, motion } from 'framer-motion'
import type { AgentProgramInfo } from '../../../services/agentApi'
import { InlineConfirmAction } from '../../skills/InlineConfirmAction'

interface AgentDetailSliderProps {
  agent: AgentProgramInfo | null
  open: boolean
  onClose: () => void
  onRefresh: () => void
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
  if (status === 'installed') return '已安装'
  if (status === 'updateAvailable') return '可更新'
  if (status === 'notInstalled') return '未安装'
  return '不可用'
}

export function AgentDetailSlider({ agent, open, onClose, onRefresh, onRun }: AgentDetailSliderProps) {
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
                  {agent.kind === 'cli' ? '命令行 Agent' : '桌面应用'} · {statusText(agent.status)}
                </div>
              </div>
            </div>

            <section className="agent-detail-section">
              <div className="agent-detail-section__title">程序信息</div>
              <InfoRow label="包管理器" value={agent.packageManager} />
              <InfoRow label="包名" value={agent.packageName} />
              <InfoRow label="已安装版本" value={agent.installedVersion} />
              <InfoRow label="最新版本" value={agent.latestVersion} />
              <InfoRow label="执行文件" value={agent.binaryPath} />
              <InfoRow label="应用路径" value={agent.appPath} />
              <InfoRow label="配置目录" value={agent.configDir} />
              <InfoRow label="下载地址" value={agent.downloadUrl} />
            </section>

            <section className="agent-detail-section">
              <div className="agent-detail-section__title">命令</div>
              <InfoRow label="安装" value={agent.installCommand} />
              <InfoRow label="更新" value={agent.updateCommand} />
              <InfoRow label="卸载" value={agent.uninstallCommand} />
              {!agent.installCommand && !agent.updateCommand && !agent.uninstallCommand && (
                <div className="agent-detail-empty">当前 Agent 未配置可直接执行的命令。</div>
              )}
            </section>

            <section className="agent-detail-section">
              <div className="agent-detail-section__title">AgentBro Hooks</div>
              <div className="agent-detail-hook-state">
                <span className={`agent-detail-hook-dot ${agent.hooksInstalled ? 'agent-detail-hook-dot--on' : ''}`} />
                {agent.hooksInstalled ? 'Hooks 已安装' : 'Hooks 未安装'}
              </div>
            </section>

            <div className="agent-detail-panel__footer">
              {installed && agent.status !== 'updateAvailable' && (
                <button className="settings-mini-button" onClick={onRefresh}>检查更新</button>
              )}
              {agent.status === 'updateAvailable' && (
                <button className="settings-mini-button" onClick={() => agent.updateCommand ? onRun(agent.id, 'update') : onRefresh()}>
                  ⬆ 更新版本
                </button>
              )}
              {!installed && agent.installCommand && (
                <button className="settings-mini-button agent-detail-primary" onClick={() => onRun(agent.id, 'install')}>安装</button>
              )}
              {!installed && !agent.installCommand && agent.downloadUrl && (
                <button className="settings-mini-button agent-detail-primary" onClick={() => onRun(agent.id, 'open')}>下载</button>
              )}
              {installed && agent.kind === 'app' && (
                <button className="settings-mini-button" onClick={() => onRun(agent.id, 'open')}>打开</button>
              )}
              {installed && agent.uninstallCommand && (
                <InlineConfirmAction
                  label="卸载"
                  confirmLabel="确认"
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
