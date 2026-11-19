import { useCallback, useEffect, useRef, useState, type MouseEvent } from 'react'
import { useTranslation } from 'react-i18next'
import type { OverlayItem, SessionState } from '../../types/agent'
import { OverlayCard } from './OverlayCard'
import { DiffView } from '../notch/DiffView'
import { setNotchFocusable, jumpToTerminal } from '../../services/tauriApi'
import { getToolActivityLabel } from '../../utils/toolLabels'
import { getWritePermissionPreview, parseToolInput, shortenPath, WRITE_PERMISSION_PREVIEW_LINES } from '../../utils/permissionPreview'
import { isCompactPermissionPrompt } from '../../utils/notificationLayout'
import { supportsPersistentPermissionActions } from '../../utils/sessionCapabilities'
import './PermissionCard.css'

interface PermissionCardProps {
  overlay: OverlayItem
  session: SessionState
  onAllow: () => void
  onAllowAlways: () => void
  onAutoApprove: () => void
  onShowSessions?: () => void
  onDeny: (message?: string) => void
  onDismiss: () => void
  onDraftStateChange?: (hasDraft: boolean) => void
  queueLength?: number
  queueNext?: string
  sessionCount?: number
}

function ToolPreview({ toolName, toolInput }: { toolName: string; toolInput: Record<string, unknown> }) {
  switch (toolName) {
    case 'Bash':
    case 'run_shell_command': {
      const command = (toolInput.command as string) ?? (toolInput.raw as string) ?? ''
      const lines = command.split('\n')
      const maxLines = 5
      const truncated = lines.length > maxLines
      const displayLines = truncated ? lines.slice(0, maxLines) : lines

      return (
        <div>
          <div className="perm-card__preview-code perm-card__preview-code--bash">
            <pre className="perm-card__preview-command">
              <span className="perm-card__preview-prompt">$ </span>
              {displayLines.join('\n')}
            </pre>
            {toolInput.description != null && (
              <div className="perm-card__preview-desc">{String(toolInput.description)}</div>
            )}
          </div>
          {truncated && (
            <div className="perm-card__preview-more">+{lines.length - maxLines} lines</div>
          )}
        </div>
      )
    }

    case 'Edit':
    case 'MultiEdit':
    case 'replace': {
      const filePath = (toolInput.file_path as string) ?? (toolInput.filePath as string) ?? ''
      const oldStr = toolInput.old_string as string | undefined
      const newStr = toolInput.new_string as string | undefined

      return (
        <div>
          <div className="perm-card__preview-file">
            <span>{'✏️'}</span>
            <span className="perm-card__preview-path">{shortenPath(filePath)}</span>
          </div>
          {oldStr != null && newStr != null && (
            <div className="perm-card__preview-diff">
              {oldStr && oldStr.split('\n').slice(0, 3).map((line, i) => (
                <div key={`old-${i}`} className="perm-card__preview-diff--remove">- {line}</div>
              ))}
              {oldStr && oldStr.split('\n').length > 3 && (
                <div className="perm-card__preview-more">{'…'}+{oldStr.split('\n').length - 3}</div>
              )}
              {newStr && newStr.split('\n').slice(0, 3).map((line, i) => (
                <div key={`new-${i}`} className="perm-card__preview-diff--add">+ {line}</div>
              ))}
              {newStr && newStr.split('\n').length > 3 && (
                <div className="perm-card__preview-more">{'…'}+{newStr.split('\n').length - 3}</div>
              )}
            </div>
          )}
        </div>
      )
    }

    case 'Write':
    case 'write_file': {
      const preview = getWritePermissionPreview(toolInput, WRITE_PERMISSION_PREVIEW_LINES)

      return (
        <div>
          <div className="perm-card__preview-file">
            <span className="perm-card__preview-path">{preview.shortPath}</span>
            <span className="perm-card__preview-new-badge">new file</span>
          </div>
          {preview.content && (
            <pre className="perm-card__preview-code">
              {preview.visibleContent}
              {preview.hiddenLineCount > 0 ? '\n…' : ''}
            </pre>
          )}
        </div>
      )
    }

    case 'Read':
    case 'read_file': {
      const filePath = (toolInput.file_path as string) ?? (toolInput.filePath as string) ?? ''
      return (
        <div className="perm-card__preview-file">
          <span>{'📄'}</span>
          <span className="perm-card__preview-path">{shortenPath(filePath)}</span>
        </div>
      )
    }

    case 'Grep':
    case 'Glob':
    case 'grep_search': {
      const pattern = (toolInput.pattern as string) ?? ''
      const path = toolInput.path as string | undefined
      const dirPath = (toolInput.dir_path as string) || path
      return (
        <div className="perm-card__preview-file">
          <span>{'🔍'}</span>
          <code className="perm-card__preview-pattern">{pattern}</code>
          {dirPath && <span className="perm-card__preview-more">in {shortenPath(dirPath)}</span>}
        </div>
      )
    }

    case 'WebSearch':
    case 'google_web_search': {
      const query = (toolInput.query as string) ?? ''
      return (
        <div className="perm-card__preview-file">
          <span>{'🔍'}</span>
          <span className="perm-card__preview-desc">"{query}"</span>
        </div>
      )
    }

    case 'WebFetch':
    case 'web_fetch': {
      const url = (toolInput.url as string) ?? (toolInput.prompt as string) ?? ''
      return (
        <div className="perm-card__preview-file">
          <span>{'🔗'}</span>
          <span className="perm-card__preview-path">{url.length > 50 ? `${url.slice(0, 50)}…` : url}</span>
        </div>
      )
    }

    default: {
      const entries = Object.entries(toolInput).filter(([, v]) => v != null && typeof v !== 'object')
      if (entries.length > 0) {
        return (
          <div className="perm-card__preview-kv">
            {entries.slice(0, 4).map(([key, val]) => (
              <div key={key} className="perm-card__preview-kv-row">
                <span className="perm-card__preview-kv-key">{key}:</span>
                <span className="perm-card__preview-kv-val">{String(val)}</span>
              </div>
            ))}
            {entries.length > 4 && (
              <div className="perm-card__preview-more">+{entries.length - 4} more</div>
            )}
          </div>
        )
      }
      return null
    }
  }
}

export function PermissionCard({ overlay, session, onAllow, onAllowAlways, onAutoApprove, onShowSessions, onDeny, onDismiss, onDraftStateChange, queueLength = 1, queueNext, sessionCount = 1 }: PermissionCardProps) {
  const { t } = useTranslation()
  const data = overlay.data as { toolName: string; toolInput: string; diff?: import('../../types/agent').DiffContent; options?: string[] }
  const [feedbackState, setFeedbackState] = useState({ overlayId: overlay.id, text: '', show: false })
  const feedbackRef = useRef<HTMLInputElement>(null)
  const ignoreNextClickRef = useRef(false)
  const feedbackText = feedbackState.overlayId === overlay.id ? feedbackState.text : ''
  const showFeedback = feedbackState.overlayId === overlay.id && feedbackState.show
  const hasDraft = feedbackText.trim().length > 0

  let parsedInput: Record<string, unknown> = {}
  parsedInput = parseToolInput(data.toolInput)
  const toolLabel = getToolActivityLabel(t, data.toolName)
  const supportsPersistentActions = supportsPersistentPermissionActions(session.agentType)
  const isCompactPermission = isCompactPermissionPrompt(overlay.data)
  const cardClassName = `overlay-card--permission${isCompactPermission ? ' overlay-card--permission-compact' : ''}`
  const actionsClassName = [
    'perm-card__actions',
    isCompactPermission ? 'perm-card__actions--compact' : undefined,
    supportsPersistentActions ? undefined : 'perm-card__actions--two',
  ].filter(Boolean).join(' ')

  useEffect(() => {
    if (showFeedback) feedbackRef.current?.focus()
  }, [showFeedback])

  useEffect(() => {
    onDraftStateChange?.(hasDraft)
  }, [hasDraft, onDraftStateChange])

  useEffect(() => () => onDraftStateChange?.(false), [onDraftStateChange])

  const handleReject = useCallback(() => {
    const msg = feedbackText.trim()
    onDeny(msg || undefined)
  }, [feedbackText, onDeny])

  const handleJump = useCallback(() => {
    jumpToTerminal(session.id)
  }, [session.id])

  const runActionOnMouseDown = useCallback((event: MouseEvent<HTMLButtonElement>, action: () => void) => {
    event.preventDefault()
    event.stopPropagation()
    ignoreNextClickRef.current = true
    action()
  }, [])

  const runActionOnKeyboardClick = useCallback((event: MouseEvent<HTMLButtonElement>, action: () => void) => {
    if (ignoreNextClickRef.current) {
      ignoreNextClickRef.current = false
      return
    }
    if (event.detail !== 0) return
    action()
  }, [])

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const isInput = (e.target as HTMLElement).tagName === 'INPUT'

      if (e.key === 'Escape') {
        e.preventDefault()
        if (showFeedback) {
          setFeedbackState(current => ({ overlayId: overlay.id, text: current.overlayId === overlay.id ? current.text : '', show: false }))
          setNotchFocusable(false)
        }
        return
      }

      if (isInput) {
        if (e.key === 'Enter' && showFeedback) {
          e.preventDefault()
          handleReject()
        }
        return
      }

      if (e.key === 't' || e.key === 'T') { e.preventDefault(); handleJump() }
    }

    document.addEventListener('keydown', handler)
    return () => document.removeEventListener('keydown', handler)
  }, [showFeedback, handleReject, handleJump, overlay.id])

  return (
    <OverlayCard session={session} onDismiss={onDismiss} onShowSessions={onShowSessions} sessionCount={sessionCount} className={cardClassName} bodyClassName="perm-card">
      <div className="perm-card__scroll">
        {/* Queue progress bar */}
        {queueLength > 1 && (
          <div className="perm-card__queue">
            <div className="perm-card__queue-info">
              <span className="perm-card__queue-text">1 / {queueLength}</span>
              {queueNext && <span className="perm-card__queue-text">Next: {queueNext}</span>}
            </div>
            <div className="perm-card__queue-bar">
              <div className="perm-card__queue-fill" style={{ width: `${(1 / queueLength) * 100}%` }} />
            </div>
          </div>
        )}

        <div className="perm-card__tool-row">
          <div className="perm-card__tool-icon-wrap">
            <svg className="perm-card__tool-icon-svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
            </svg>
          </div>
          <div className="perm-card__tool-name">{data.toolName || toolLabel}</div>
        </div>

        {/* Tool detail box with preview */}
        <div className="perm-card__detail-box">
          <ToolPreview toolName={data.toolName} toolInput={parsedInput} />
        </div>

        {/* Diff view */}
        {data.diff && (
          <div className="perm-card__diff">
            <DiffView diff={data.diff} />
          </div>
        )}
      </div>

      {/* Feedback input */}
      {showFeedback && (
        <div className="perm-card__feedback">
          <input
            ref={feedbackRef}
            type="text"
            className="perm-card__feedback-input"
            data-has-draft={hasDraft ? 'true' : 'false'}
            placeholder="Tell Claude why (optional)..."
            value={feedbackText}
            onChange={(e) => setFeedbackState({ overlayId: overlay.id, text: e.target.value, show: true })}
            onMouseDown={(e) => {
              e.stopPropagation()
              setNotchFocusable(true).then(() => feedbackRef.current?.focus())
            }}
            onFocus={() => setNotchFocusable(true)}
            onBlur={() => setNotchFocusable(false)}
          />
        </div>
      )}

      {/* Action buttons */}
      <div className={actionsClassName}>
        <button
          type="button"
          className="perm-card__btn perm-card__btn--deny"
          onMouseDown={(event) => runActionOnMouseDown(event, () => {
            if (showFeedback) {
              handleReject()
            } else {
              setFeedbackState({ overlayId: overlay.id, text: feedbackText, show: true })
              setNotchFocusable(true)
            }
          })}
          onClick={(event) => runActionOnKeyboardClick(event, () => {
            if (showFeedback) {
              handleReject()
            } else {
              setFeedbackState({ overlayId: overlay.id, text: feedbackText, show: true })
              setNotchFocusable(true)
            }
          })}
        >
          <span>{t('notch.deny')}</span>
        </button>
        <button
          type="button"
          className="perm-card__btn perm-card__btn--allow"
          onMouseDown={(event) => runActionOnMouseDown(event, onAllow)}
          onClick={(event) => runActionOnKeyboardClick(event, onAllow)}
        >
          <span>{t('notch.allowOnce')}</span>
        </button>
        {supportsPersistentActions && (
          <>
            <button
              type="button"
              className="perm-card__btn perm-card__btn--always"
              onMouseDown={(event) => runActionOnMouseDown(event, onAllowAlways)}
              onClick={(event) => runActionOnKeyboardClick(event, onAllowAlways)}
            >
              <span>{t('notch.allowAlways')}</span>
            </button>
            <button
              type="button"
              className="perm-card__btn perm-card__btn--auto"
              onMouseDown={(event) => runActionOnMouseDown(event, onAutoApprove)}
              onClick={(event) => runActionOnKeyboardClick(event, onAutoApprove)}
            >
              <span>{t('notch.autoApprove')}</span>
            </button>
          </>
        )}
      </div>
    </OverlayCard>
  )
}
