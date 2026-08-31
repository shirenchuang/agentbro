/* CodexUsageSection — Codex-only quota and token usage panel for the Usage view */
import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { CodexTokenBucket, CodexUsageSummary } from '../../../services/tauriApi'
import { getCodexUsageSummary, isTauri } from '../../../services/tauriApi'
import type { UsageRateWindow } from '../../../types/agent'
import { formatTokens } from '../../../utils/tokens'
import { SettingGroup } from '../SettingGroup'
import './CodexUsageSection.css'

function readableError(error: unknown): string {
  if (error instanceof Error) return error.message
  if (typeof error === 'string') return error
  if (error && typeof error === 'object' && 'message' in error) {
    return String((error as { message?: unknown }).message)
  }
  return String(error)
}

function formatClock(timestamp: number | undefined): string {
  if (!timestamp) return ''
  return new Date(timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

function formatResetTime(resetsAt: string | null | undefined): string {
  if (!resetsAt) return ''
  const date = new Date(resetsAt)
  if (Number.isNaN(date.getTime())) return ''
  return date.toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
}

function QuotaWindowRow({ window, t }: { window: UsageRateWindow; t: (key: string, opts?: { defaultValue?: string }) => string }) {
  const usedClass = window.usedPercent >= 80 ? 'codex-usage__bar--red' : window.usedPercent >= 50 ? 'codex-usage__bar--amber' : 'codex-usage__bar--green'
  const resetLabel = formatResetTime(window.resetsAt)
  return (
    <div className="codex-usage__window">
      <div className="codex-usage__window-head">
        <span className="codex-usage__window-title">{window.title}</span>
        <span className="codex-usage__window-used">{Math.round(window.usedPercent)}%</span>
      </div>
      <div className="codex-usage__bar-track">
        <div className={`codex-usage__bar ${usedClass}`} style={{ width: `${Math.min(100, Math.max(0, window.usedPercent))}%` }} />
      </div>
      <div className="codex-usage__window-foot">
        {window.remainingLabel && (
          <span className="codex-usage__window-remaining">
            {t('codexUsage.remainingLabel', { defaultValue: '{{label}} left' }).replace('{{label}}', window.remainingLabel)}
          </span>
        )}
        {resetLabel && (
          <span className="codex-usage__window-reset">
            {t('codexUsage.resetAt', { defaultValue: 'Resets {{time}}' }).replace('{{time}}', resetLabel)}
          </span>
        )}
      </div>
    </div>
  )
}

function TokenBucketRow({ label, bucket }: { label: string; bucket: CodexTokenBucket }) {
  const { t } = useTranslation()
  const total = bucket.input + bucket.output + bucket.cacheRead + bucket.cacheCreate
  return (
    <div className="codex-usage__bucket">
      <div className="codex-usage__bucket-head">
        <span className="codex-usage__bucket-label">{label}</span>
        <span className="codex-usage__bucket-total">{formatTokens(total)}</span>
      </div>
      <div className="codex-usage__bucket-metrics">
        <span>{t('codexUsage.input', { defaultValue: 'Input' })} <strong>{formatTokens(bucket.input)}</strong></span>
        <span>{t('codexUsage.output', { defaultValue: 'Output' })} <strong>{formatTokens(bucket.output)}</strong></span>
        <span>{t('codexUsage.cacheRead', { defaultValue: 'Cache read' })} <strong>{formatTokens(bucket.cacheRead)}</strong></span>
        {bucket.cacheCreate > 0 && (
          <span>{t('codexUsage.cacheCreate', { defaultValue: 'Cache write' })} <strong>{formatTokens(bucket.cacheCreate)}</strong></span>
        )}
        <span>{t('codexUsage.sessions', { defaultValue: '{{count}} sessions', count: bucket.sessions }).replace('{{count}}', String(bucket.sessions))}</span>
      </div>
    </div>
  )
}

export function CodexUsageSection() {
  const { t } = useTranslation()
  const [summary, setSummary] = useState<CodexUsageSummary | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const fetchSummary = useCallback(async () => {
    setError(null)
    if (!isTauri()) return
    setLoading(true)
    try {
      setSummary(await getCodexUsageSummary())
    } catch (err) {
      setError(readableError(err))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    fetchSummary()
  }, [fetchSummary])

  const quota = summary?.quota ?? null
  const quotaState = summary?.quotaState ?? null
  const tokenUsage = summary?.tokenUsage ?? null
  const windows = quota?.windows && quota.windows.length > 0
    ? quota.windows
    : quota
      ? [
        { id: 'five_hour', title: '5h', usedPercent: quota.fiveHourUsage, remainingLabel: quota.fiveHourRemaining, resetsAt: null, remainingPercent: null, windowMinutes: 300 },
        { id: 'seven_day', title: '7d', usedPercent: quota.sevenDayUsage, remainingLabel: quota.sevenDayRemaining, resetsAt: null, remainingPercent: null, windowMinutes: 10080 },
      ]
      : []
  const stateKind = quotaState?.state ?? 'unavailable'
  const stateLabel = quotaState
    ? quotaState.state === 'ok'
      ? t('settings.connected', { defaultValue: 'Connected' })
      : quotaState.state === 'failed'
        ? t('codexUsage.state.failed', { defaultValue: 'Failed' })
        : quotaState.state === 'disabled'
          ? t('settings.disabled', { defaultValue: 'Disabled' })
          : t('settings.waitingData', { defaultValue: 'Waiting for data' })
    : t('settings.waitingData', { defaultValue: 'Waiting for data' })

  return (
    <section className="setting-section">
      <h2>{t('settings.usage', { defaultValue: 'Usage' })}</h2>

      {!isTauri() && (
        <div className="hook-empty">{t('codexUsage.desktopOnly', { defaultValue: 'Codex usage is available in the desktop app' })}</div>
      )}

      {error && <div className="hook-error-card">{error}</div>}

      <SettingGroup
        actions={(
          <button className="settings-mini-button" disabled={loading} onClick={fetchSummary} type="button">
            {loading ? t('settings.detecting', { defaultValue: 'Checking...' }) : t('codexUsage.refresh', { defaultValue: 'Refresh' })}
          </button>
        )}
        label={t('codexUsage.quota', { defaultValue: 'Codex Quota' })}
      >
        <div className="usage-provider-row">
          <div className="usage-provider-row__main">
            <div className="usage-provider-row__title">
              <span>{t('settings.usage', { defaultValue: 'Usage' })}</span>
              <strong className={`codex-usage__state codex-usage__state--${stateKind}`}>{stateLabel}</strong>
            </div>
            {quotaState && <div className="usage-provider-row__detail">{quotaState.detail}</div>}
          </div>
        </div>
        {windows.length > 0 ? (
          <div className="codex-usage__windows">
            {windows.map((window) => <QuotaWindowRow key={window.id} window={window} t={t} />)}
          </div>
        ) : (
          <div className="hook-empty">
            {quotaState?.detail ?? t('codexUsage.noQuota', { defaultValue: 'No Codex quota data available' })}
          </div>
        )}
        {(quota?.source || quota?.updatedAt) && (
          <div className="codex-usage__meta">
            {quota?.source && (
              <span>{t('codexUsage.source', { defaultValue: 'Source' })}: <strong>{quota.source}</strong></span>
            )}
            {quota?.updatedAt && (
              <span>{t('codexUsage.updatedAt', { defaultValue: 'Updated {{time}}' }).replace('{{time}}', formatClock(quota.updatedAt))}</span>
            )}
          </div>
        )}
      </SettingGroup>

      <SettingGroup label={t('codexUsage.tokenUsage', { defaultValue: 'Codex Token Usage' })}>
        {tokenUsage?.available ? (
          <>
            <TokenBucketRow label={t('codexUsage.today', { defaultValue: 'Today' })} bucket={tokenUsage.today} />
            <TokenBucketRow label={t('codexUsage.days7', { defaultValue: 'Last 7 days' })} bucket={tokenUsage.days7} />
            <TokenBucketRow label={t('codexUsage.days30', { defaultValue: 'Last 30 days' })} bucket={tokenUsage.days30} />
            <div className="codex-usage__meta">
              {tokenUsage.source && (
                <span>{t('codexUsage.source', { defaultValue: 'Source' })}: <strong>{tokenUsage.source}</strong></span>
              )}
              <span>{t('codexUsage.sessionsScanned', { defaultValue: '{{count}} session file(s) scanned', count: tokenUsage.sessionsScanned }).replace('{{count}}', String(tokenUsage.sessionsScanned))}</span>
              <span>{t('codexUsage.tokenEvents', { defaultValue: '{{count}} token events', count: tokenUsage.tokenEvents }).replace('{{count}}', String(tokenUsage.tokenEvents))}</span>
            </div>
          </>
        ) : (
          <div className="hook-empty">
            {tokenUsage?.detail ?? t('codexUsage.noData', { defaultValue: 'No local Codex token usage data found yet' })}
          </div>
        )}
      </SettingGroup>
    </section>
  )
}