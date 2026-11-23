import { createPortal } from 'react-dom'
import type { ReactNode } from 'react'
import { useId, useLayoutEffect, useRef } from 'react'

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
  const titleId = useId()
  const dialogRef = useRef<HTMLDialogElement>(null)
  const cancelRef = useRef<HTMLButtonElement>(null)

  useLayoutEffect(() => {
    const dialog = dialogRef.current
    if (!dialog) return
    dialog.showModal()
    cancelRef.current?.focus({ preventScroll: true })
    return () => dialog.close()
  }, [])

  if (typeof document === 'undefined') return null

  const modalClass = [
    'sm2__modal',
    destructive ? 'sm2__modal--destructive' : '',
    modalClassName ?? '',
  ].filter(Boolean).join(' ')

  const requestCancel = () => {
    if (!busy) onCancel()
  }

  return createPortal(
    <dialog
      ref={dialogRef}
      className="sm2__overlay sm2__preview-dialog"
      aria-labelledby={titleId}
      onClick={(event) => {
        event.stopPropagation()
        if (event.target === event.currentTarget) requestCancel()
      }}
      onKeyDown={(event) => {
        if (event.key === 'Escape') event.stopPropagation()
      }}
      onCancel={(event) => {
        event.preventDefault()
        event.stopPropagation()
        requestCancel()
      }}
    >
      <div className={modalClass}>
        <div className="sm2__modal-head">
          <h3 id={titleId}>{title}</h3>
        </div>
        <div className="sm2__modal-body">
          {children}
        </div>
        <div className="sm2__modal-actions">
          {actions ?? (
            <>
              <button type="button" ref={destructive ? cancelRef : undefined} className="sm2__btn" onClick={requestCancel} disabled={busy}>
                {cancelLabel}
              </button>
              <button
                type="button"
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
    </dialog>,
    document.body,
  )
}
