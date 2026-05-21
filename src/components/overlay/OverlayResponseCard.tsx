import { useTranslation } from 'react-i18next'
import type { OverlayItem, SessionState } from '../../types/agent'
import { useConfigStore } from '../../stores/configStore'
import { getReadableNotificationHeight } from '../../utils/notificationLayout'
import { OverlayFeedbackPanel } from './OverlayFeedbackPanel'
import './OverlayResponseCard.css'

interface OverlayResponseCardProps {
  overlay: OverlayItem
  session: SessionState
  onJumpToTerminal: () => void
  onShowSessions?: () => void
  onDismiss: () => void
  onDraftStateChange?: (hasDraft: boolean) => void
  sessionCount?: number
}

export function OverlayResponseCard({ overlay, session, onJumpToTerminal, onShowSessions, onDismiss, onDraftStateChange, sessionCount }: OverlayResponseCardProps) {
  const { t } = useTranslation()
  const data = overlay.data as { responseText: string; userMessage?: string }
  const dwellSeconds = useConfigStore((s) => s.taskCompleteDwellSeconds) || 6
  const completionCardHeight = useConfigStore((s) => s.completionCardHeight)
  const maxPanelHeight = useConfigStore((s) => s.maxPanelHeight)
  const readableCardHeight = getReadableNotificationHeight(completionCardHeight, maxPanelHeight, {
    text: data.responseText,
    userMessage: data.userMessage || session.lastUserMessage,
  })
  const dwellMs = dwellSeconds * 1000

  return (
    <OverlayFeedbackPanel
      session={session}
      userMessage={data.userMessage}
      text={data.responseText}
      kind="response"
      maxHeight={readableCardHeight}
      dwellMs={dwellMs}
      startedAt={overlay.createdAt}
      statusLabel={t('notch.replied', { defaultValue: 'New reply' })}
      onJumpToTerminal={onJumpToTerminal}
      onShowSessions={onShowSessions}
      onDismiss={onDismiss}
      onDraftStateChange={onDraftStateChange}
      sessionCount={sessionCount}
    />
  )
}
