import { useTranslation } from 'react-i18next'
import type { OverlayItem, SessionState } from '../../types/agent'
import { useConfigStore } from '../../stores/configStore'
import { OverlayFeedbackPanel } from './OverlayFeedbackPanel'
import './OverlayCompletionCard.css'

interface OverlayCompletionCardProps {
  overlay: OverlayItem
  session: SessionState
  onJumpToTerminal: () => void
  onDismiss: () => void
}

export function OverlayCompletionCard({ overlay, session, onJumpToTerminal, onDismiss }: OverlayCompletionCardProps) {
  const { t } = useTranslation()
  const data = overlay.data as { summary: string }
  const dwellSeconds = useConfigStore((s) => s.taskCompleteDwellSeconds) || 6
  const completionCardHeight = useConfigStore((s) => s.completionCardHeight)
  const dwellMs = dwellSeconds * 1000

  return (
    <OverlayFeedbackPanel
      session={session}
      text={data.summary}
      maxHeight={completionCardHeight}
      dwellMs={dwellMs}
      statusLabel={t('notch.completed', { defaultValue: '完成' })}
      onJumpToTerminal={onJumpToTerminal}
      onDismiss={onDismiss}
    />
  )
}
