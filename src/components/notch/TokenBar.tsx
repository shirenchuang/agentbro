/* Token Bar — bottom bar with token counts, cache, and cost */
import { useTranslation } from 'react-i18next'
import type { TokenUsage } from '../../types/agent'
import { useConfigStore } from '../../stores/configStore'
import { formatTokens, estimateCost, formatCost, MODEL_PRICING } from '../../utils/tokens'
import './TokenBar.css'

interface TokenBarProps {
  tokens: TokenUsage
}

export function TokenBar({ tokens }: TokenBarProps) {
  const { t } = useTranslation()
  const displayMode = useConfigStore((s) => s.tokenDisplayMode)
  const costModel = useConfigStore((s) => s.costModel)
  const updateConfig = useConfigStore((s) => s.updateConfig)

  const total = tokens.input + tokens.output
  if (total === 0 || displayMode === 'hidden') return null

  const cachePercent = tokens.input > 0
    ? Math.round((tokens.cacheRead / (tokens.input + tokens.cacheRead)) * 100)
    : 0

  const cost = estimateCost(tokens, costModel)
  const showTokens = displayMode === 'tokens' || displayMode === 'both'
  const showCost = displayMode === 'cost' || displayMode === 'both'

  return (
    <div className="token-bar">
      {showTokens && (
        <span className="token-bar__counts">
          {formatTokens(tokens.input)} in · {formatTokens(tokens.output)} out
          {cachePercent > 0 && (
            <span className="token-bar__cache"> · {cachePercent}% {t('notch.cached')}</span>
          )}
        </span>
      )}

      <span className="token-bar__right">
        {showCost && (
          <span className="token-bar__cost">{formatCost(cost)}</span>
        )}
        <button
          className="token-bar__model-btn"
          onClick={() => {
            const models = Object.keys(MODEL_PRICING)
            const idx = models.indexOf(costModel)
            const next = models[(idx + 1) % models.length]
            updateConfig('costModel', next)
          }}
          title={`Cost model: ${MODEL_PRICING[costModel]?.label ?? costModel}. Click to cycle.`}
        >
          {MODEL_PRICING[costModel]?.label.split(' ').pop() ?? costModel}
        </button>
      </span>
    </div>
  )
}
