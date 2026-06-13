import type { ReactNode } from 'react'

interface PreviewDialogProps {
  title: string
  children: ReactNode
  confirmLabel?: string
  cancelLabel?: string
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
  destructive = false,
  busy = false,
  disabled = false,
  onConfirm,
  onCancel,
}: PreviewDialogProps) {
  return (
    <div className="sm2__overlay" onClick={onCancel}>
      <div className="sm2__modal" onClick={(e) => e.stopPropagation()}>
        <h3>{title}</h3>
        {children}
        <div className="sm2__btn-row" style={{ justifyContent: 'flex-end' }}>
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
    </div>
  )
}
