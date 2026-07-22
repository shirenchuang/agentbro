import { useTranslation } from 'react-i18next'
import { useSkillStoreV2 } from '../../stores/skillStoreV2'

export function MarketplaceInstallTaskDock({
  hidden = false,
  onOpen,
}: {
  hidden?: boolean
  onOpen: () => void
}) {
  const { t } = useTranslation()
  const task = useSkillStoreV2((state) => state.marketplaceInstallTask)
  const cancelTask = useSkillStoreV2((state) => state.cancelMarketplaceInstallTask)
  const dismissTask = useSkillStoreV2((state) => state.dismissMarketplaceInstallTask)

  if (!task || hidden) return null

  const items = Object.values(task.items)
  const completed = items.filter((item) => ['success', 'failed', 'cancelled'].includes(item.status)).length
  const failed = items.filter((item) => item.status === 'failed').length
  const progress = items.length === 0 ? 0 : Math.round((completed / items.length) * 100)
  const phaseLabel = t(`skills.marketInstall.backgroundPhase.${task.phase}`)

  return (
    <aside
      className={`sm2__global-install-dock${task.busy ? ' sm2__global-install-dock--busy' : ''}${task.result?.cancelled ? ' sm2__global-install-dock--cancelled' : ''}`}
      role="status"
      aria-label={t('skills.marketInstall.globalTaskTitle')}
    >
      <div className="sm2__global-install-dock-head">
        <span className={`sm2__global-install-dock-pulse sm2__global-install-dock-pulse--${task.phase}`} aria-hidden="true" />
        <div>
          <strong>{phaseLabel}</strong>
          <small>{task.source}</small>
        </div>
        <span className="sm2__global-install-dock-count">{completed}/{items.length}</span>
      </div>

      <div
        className="sm2__global-install-dock-track"
        role="progressbar"
        aria-label={t('skills.marketInstall.installProgress')}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={progress}
      >
        <span
          className={['cloning', 'preparing'].includes(task.phase) ? 'sm2__global-install-dock-bar sm2__global-install-dock-bar--indeterminate' : 'sm2__global-install-dock-bar'}
          style={['cloning', 'preparing'].includes(task.phase) ? undefined : { width: `${progress}%` }}
        />
      </div>

      <div className="sm2__global-install-dock-actions">
        <button type="button" className="sm2__global-install-dock-open" onClick={onOpen}>
          {t('skills.marketInstall.returnToTask')}
        </button>
        {task.busy ? (
          <button
            type="button"
            className="sm2__global-install-dock-cancel"
            disabled={task.phase === 'cancelling' || task.phase === 'organizing'}
            onClick={() => void cancelTask()}
          >
            {task.phase === 'cancelling'
              ? t('skills.marketInstall.cancelling')
              : task.phase === 'organizing'
                ? t('skills.marketInstall.organizing')
                : t('skills.marketInstall.cancelInstall')}
          </button>
        ) : (
          <button type="button" className="sm2__global-install-dock-dismiss" onClick={dismissTask}>
            {t('skills.marketInstall.dismissProgress')}
          </button>
        )}
      </div>

      {failed > 0 && (
        <small className="sm2__global-install-dock-note">
          {t('skills.marketInstall.backgroundProgress', { completed, total: items.length, failed })}
        </small>
      )}
      {task.error && (
        <small className="sm2__global-install-dock-note sm2__global-install-dock-note--error">
          {task.error === 'cancel-not-accepted' ? t('skills.marketInstall.cancelNotAccepted') : task.error}
        </small>
      )}
    </aside>
  )
}
