import { useTranslation } from 'react-i18next'
import type { OverlayItem, SessionState } from '../../types/agent'
import { OverlayCard } from './OverlayCard'
import { DiffView } from '../notch/DiffView'
import './PermissionCard.css'

interface PermissionCardProps {
  overlay: OverlayItem
  session: SessionState
  onAllow: () => void
  onAllowAlways: () => void
  onDeny: () => void
  onDismiss: () => void
}

export function PermissionCard({ overlay, session, onAllow, onAllowAlways, onDeny, onDismiss }: PermissionCardProps) {
  const { t } = useTranslation()
  const data = overlay.data as { toolName: string; toolInput: string; diff?: import('../../types/agent').DiffContent; options?: string[] }

  return (
    <OverlayCard session={session} onDismiss={onDismiss}>
      {/* Tool info */}
      <div className="perm-card__tool">
        <div className="perm-card__tool-header">
          <span className="perm-card__tool-icon">{'\u26A0'}</span>
          <span className="perm-card__tool-name">{data.toolName}</span>
        </div>
        {data.toolInput && (
          <pre className="perm-card__tool-input">{data.toolInput}</pre>
        )}
      </div>

      {/* Diff view */}
      {data.diff && (
        <div className="perm-card__diff">
          <DiffView diff={data.diff} />
        </div>
      )}

      {/* Action buttons */}
      <div className="perm-card__actions">
        <button className="perm-card__btn perm-card__btn--deny" onClick={onDeny}>
          {t('notch.deny')}
        </button>
        <button className="perm-card__btn perm-card__btn--allow" onClick={onAllow}>
          {t('notch.allowOnce')}
        </button>
        <button className="perm-card__btn perm-card__btn--always" onClick={onAllowAlways}>
          {t('notch.allowAlways')}
        </button>
      </div>

      {/* Keyboard hints */}
      <div className="perm-card__hints">
        <kbd>{'\u2318'}Y</kbd> {t('notch.allowOnce')}
        <span className="perm-card__hint-sep">&middot;</span>
        <kbd>{'\u2318'}N</kbd> {t('notch.deny')}
      </div>
    </OverlayCard>
  )
}
