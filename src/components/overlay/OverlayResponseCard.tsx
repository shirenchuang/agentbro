import { useTranslation } from 'react-i18next'
import type { OverlayItem, SessionState } from '../../types/agent'
import { useConfigStore } from '../../stores/configStore'
import { OverlayFeedbackPanel } from './OverlayFeedbackPanel'
import './OverlayResponseCard.css'

interface OverlayResponseCardProps {
  overlay: OverlayItem
  session: SessionState
  onJumpToTerminal: () => void
  onDismiss: () => void
}

export function OverlayResponseCard({ overlay, session, onJumpToTerminal, onDismiss }: OverlayResponseCardProps) {
  const { t } = useTranslation()
  const data = overlay.data as { responseText: string; userMessage?: string }
  const dwellSeconds = useConfigStore((s) => s.taskCompleteDwellSeconds) || 6
  const completionCardHeight = useConfigStore((s) => s.completionCardHeight)
  const dwellMs = dwellSeconds * 1000

  return (
    <OverlayFeedbackPanel
      session={session}
      userMessage={data.userMessage}
      text={data.responseText}
      maxHeight={completionCardHeight}
      dwellMs={dwellMs}
      statusLabel={t('notch.completed', { defaultValue: '完成' })}
      onJumpToTerminal={onJumpToTerminal}
      onDismiss={onDismiss}
    />
  )
}
