import { useTranslation } from 'react-i18next'
import type { UpdateStatus } from '../../hooks/useUpdater'

interface UpdateDialogProps {
  version: string
  notes: string | null
  date: string | null
  status: UpdateStatus
  onInstall: () => void
  onDismiss: () => void
}

export function UpdateDialog({ version, notes, date, status, onInstall, onDismiss }: UpdateDialogProps) {
  const { t } = useTranslation()
  const isDownloading = status === 'downloading'
  const isReady = status === 'ready'

  return (
    <div className="skills-dialog-overlay" onClick={onDismiss}>
      <div className="skills-dialog" onClick={e => e.stopPropagation()}>
        <div className="skills-dialog__header">
          <div className="skills-dialog__title">
            {isReady ? t('update.readyTitle') : t('update.availableTitle')}
          </div>
        </div>

        <div className="skills-dialog__body">
          <div style={{ marginBottom: 12, fontSize: 14, color: 'var(--settings-text-primary)' }}>
            <strong>v{version}</strong>
            {date && (
              <span style={{ marginLeft: 8, color: 'var(--settings-text-secondary)', fontSize: 12 }}>
                {new Date(date).toLocaleDateString()}
              </span>
            )}
          </div>

          {notes && (
            <div style={{
              fontSize: 13,
              lineHeight: 1.6,
              color: 'var(--settings-text-secondary)',
              whiteSpace: 'pre-wrap',
            }}>
              {notes}
            </div>
          )}

          {isReady && (
            <div style={{ marginTop: 12, fontSize: 13, color: 'var(--settings-accent)' }}>
              {t('update.restartHint')}
            </div>
          )}
        </div>

        <div className="skills-dialog__footer">
          <button className="skills-btn" onClick={onDismiss} disabled={isDownloading}>
            {t('update.later')}
          </button>
          <button
            className="skills-btn skills-btn--primary"
            onClick={onInstall}
            disabled={isDownloading}
          >
            {isDownloading ? t('update.downloading') : isReady ? t('update.restart') : t('update.install')}
          </button>
        </div>
      </div>
    </div>
  )
}
