import { useState } from 'react'
import { useTranslation } from 'react-i18next'

interface FrontmatterCardProps {
  data: Record<string, string>
}

const SUMMARY_KEYS = ['name', 'description', 'version']

export function FrontmatterCard({ data }: FrontmatterCardProps) {
  const { t } = useTranslation()
  const [expanded, setExpanded] = useState(false)

  const entries = Object.entries(data)
  if (entries.length === 0) return null

  const summaryName = data.name?.trim() || null
  const summaryDesc = data.description?.trim() || null
  const summaryVersion = data.version?.trim() || null
  const additionalEntries = entries.filter(([key]) => !SUMMARY_KEYS.includes(key))
  const hasSummary = summaryName || summaryDesc || summaryVersion

  return (
    <div className="frontmatter-card">
      {hasSummary && (
        <div className="frontmatter-card__summary">
          <div className="frontmatter-card__summary-left">
            {summaryName && (
              <div className="frontmatter-card__name">{summaryName}</div>
            )}
            {summaryDesc && (
              <div className="frontmatter-card__desc">{summaryDesc}</div>
            )}
          </div>
          {summaryVersion && (
            <span className="frontmatter-card__version">v{summaryVersion}</span>
          )}
        </div>
      )}

      {additionalEntries.length > 0 && (
        <div className="frontmatter-card__extra">
          <button
            type="button"
            className="frontmatter-card__toggle"
            onClick={() => setExpanded(v => !v)}
          >
            <span className={`frontmatter-card__chevron ${expanded ? 'frontmatter-card__chevron--open' : ''}`}>›</span>
            {t('skills.frontmatterAdditional', { count: additionalEntries.length })}
          </button>
          {expanded && (
            <div className="frontmatter-card__fields">
              {additionalEntries.map(([key, value]) => (
                <div key={key} className="frontmatter-card__field">
                  <span className="frontmatter-card__field-key">{key}</span>
                  <span className="frontmatter-card__field-value">{value}</span>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  )
}
