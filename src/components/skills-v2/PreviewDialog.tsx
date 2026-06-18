import { createPortal } from 'react-dom'
import type { ReactNode } from 'react'

interface PreviewDialogProps {
  title: string
  children: ReactNode
  confirmLabel?: string
  cancelLabel?: string
  busyLabel?: string
  modalClassName?: string
  destructive?: boolean
  busy?: boolean
  disabled?: boolean
  actions?: ReactNode
  onConfirm: () => void
  onCancel: () => void
}

export function PreviewDialog({
  title,
  children,
  confirmLabel = '确认执行',
  cancelLabel = '取消',
  busyLabel = '处理中…',
  modalClassName,
  destructive = false,
  busy = false,
  disabled = false,
  actions,
  onConfirm,
  onCancel,
}: PreviewDialogProps) {
  if (typeof document === 'undefined') return null

  const modalClass = [
    'sm2__modal',
    destructive ? 'sm2__modal--destructive' : '',
    modalClassName ?? '',
  ].filter(Boolean).join(' ')

  return createPortal(
    <div className="sm2__overlay" onClick={onCancel}>
      <div className={modalClass} onClick={(e) => e.stopPropagation()}>
        <div className="sm2__modal-head">
          <h3>{title}</h3>
        </div>
        <div className="sm2__modal-body">
          {children}
        </div>
        <div className="sm2__modal-actions">
          {actions ?? (
            <>
              <button className="sm2__btn" onClick={onCancel} disabled={busy}>
                {cancelLabel}
              </button>
              <button
                className={`sm2__btn sm2__btn--primary${destructive ? ' sm2__btn--danger' : ''}`}
                onClick={onConfirm}
                disabled={busy || disabled}
                aria-busy={busy || undefined}
                data-busy={busy ? 'true' : undefined}
              >
                {busy && <span className="sm2__spinner" aria-hidden="true" />}
                <span className="sm2__btn-label">{busy ? busyLabel : confirmLabel}</span>
              </button>
            </>
          )}
        </div>
      </div>
    </div>,
    document.body,
  )
}
