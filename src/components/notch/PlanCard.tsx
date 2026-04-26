/* PlanCard — Dark card with green "计划" label and monospace content */
import { useTranslation } from 'react-i18next'
import './PlanCard.css'

interface PlanCardProps {
  title?: string
  content: string
}

export function PlanCard({ title, content }: PlanCardProps) {
  const { t } = useTranslation()

  return (
    <div className="plan-card">
      <div className="plan-card__header">
        {title && <span className="plan-card__title">{title}</span>}
        <span className="plan-card__label">{t('notch.plan', { defaultValue: '\u8BA1\u5212' })}</span>
      </div>
      <div className="plan-card__content">
        <pre className="plan-card__text">{content}</pre>
      </div>
    </div>
  )
}
