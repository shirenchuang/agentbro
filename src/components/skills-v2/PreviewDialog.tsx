import { createPortal } from 'react-dom'
import type { ReactNode } from 'react'

interface PreviewDialogProps {
  title: string
  children: ReactNode
  confirmLabel?: string
  cancelLabel?: string
  modalClassName?: string
  destructive?: boolean
  busy?: boolean
  disabled?: boolean
  onConfirm: () => void
  onCancel: () => void
}

export function PreviewDialog({
  title,
  children,
  confirmLabel = '确认执行',
  cancelLabel = '取消',
  modalClassName,
  destructive = false,
  busy = false,
  disabled = false,
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
          <button className="sm2__btn" onClick={onCancel} disabled={busy}>
            {cancelLabel}
          </button>
          <button
            className={`sm2__btn sm2__btn--primary${destructive ? ' sm2__btn--danger' : ''}`}
            onClick={onConfirm}
            disabled={busy || disabled}
          >
            {busy ? '处理中…' : confirmLabel}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  )
}
