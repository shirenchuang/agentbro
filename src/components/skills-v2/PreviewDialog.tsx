import { createPortal } from 'react-dom'
import type { ReactNode, RefObject } from 'react'
import { useId, useLayoutEffect, useRef } from 'react'

interface ModalDialogProps {
  children: ReactNode
  titleId: string
  overlayClassName: string
  busy?: boolean
  initialFocusRef?: RefObject<HTMLElement | null>
  onCancel: () => void
}

export function ModalDialog({ children, titleId, overlayClassName, busy, initialFocusRef, onCancel }: ModalDialogProps) {
  const dialogRef = useRef<HTMLDialogElement>(null)

  useLayoutEffect(() => {
    const dialog = dialogRef.current
    if (!dialog) return
    dialog.showModal()
    initialFocusRef?.current?.focus({ preventScroll: true })
    return () => dialog.close()
  }, [initialFocusRef])

  if (typeof document === 'undefined') return null

  return createPortal(
    <dialog
      ref={dialogRef}
      className={`sm2__modal-overlay ${overlayClassName}`}
      aria-labelledby={titleId}
      onClick={(event) => {
        event.stopPropagation()
        if (event.target === event.currentTarget && !busy) onCancel()
      }}
      onKeyDown={(event) => {
        if (event.key === 'Escape') event.stopPropagation()
      }}
      onCancel={(event) => {
        event.preventDefault()
        event.stopPropagation()
        if (!busy) onCancel()
      }}
    >
      {children}
    </dialog>,
    document.body,
  )
}

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
  const cancelRef = useRef<HTMLButtonElement>(null)

  const modalClass = [
    'sm2__modal',
    destructive ? 'sm2__modal--destructive' : '',
    modalClassName ?? '',
  ].filter(Boolean).join(' ')

  return (
    <ModalDialog
      titleId={titleId}
      overlayClassName="sm2__overlay"
      busy={busy}
      initialFocusRef={cancelRef}
      onCancel={onCancel}
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
              <button type="button" ref={destructive ? cancelRef : undefined} className="sm2__btn" onClick={onCancel} disabled={busy}>
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
    </ModalDialog>
  )
}
