/* ResponseCard — Response display with "完成" badge */
import { useTranslation } from 'react-i18next'
import './ResponseCard.css'

interface ResponseCardProps {
  userMessage?: string
  responseText: string
}

export function ResponseCard({ userMessage, responseText }: ResponseCardProps) {
  const { t } = useTranslation()

  return (
    <div className="response-card">
      {userMessage && (
        <div className="response-card__user">
          <span className="response-card__user-prefix">{t('notch.you')}:</span>
          <span className="response-card__user-text">{userMessage}</span>
          <span className="response-card__done-badge">{t('notch.complete')}</span>
        </div>
      )}
      <div className="response-card__content">
        <span className="response-card__text">{responseText}</span>
      </div>
    </div>
  )
}
