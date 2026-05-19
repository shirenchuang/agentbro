import { useEffect, useState } from 'react'
import { useSwitchStore } from '../../../../stores/switchStore'
import type { UsageSummary, ProviderUsage, ModelUsage, DailyCost } from '../../../../services/switchApi'
import { switchApi } from '../../../../services/switchApi'

const PERIOD_OPTIONS = [
  { label: '7 天', days: 7 },
  { label: '30 天', days: 30 },
  { label: '90 天', days: 90 },
]

function formatCost(usd: number): string {
  return usd < 0.01 && usd > 0 ? '<$0.01' : `$${usd.toFixed(2)}`
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
  return String(n)
}

export function SwitchUsagePanel() {
  const { activeAppType } = useSwitchStore()
  const [days, setDays] = useState(30)
  const [summary, setSummary] = useState<UsageSummary | null>(null)
  const [byProvider, setByProvider] = useState<ProviderUsage[]>([])
  const [byModel, setByModel] = useState<ModelUsage[]>([])
  const [daily, setDaily] = useState<DailyCost[]>([])
  const [loading, setLoading] = useState(false)

  const loadData = async () => {
    setLoading(true)
    try {
      const [s, p, m, d] = await Promise.all([
        switchApi.getUsageSummary(activeAppType, days),
        switchApi.getUsageByProvider(activeAppType, days),
        switchApi.getUsageByModel(activeAppType, days),
        switchApi.getDailyCost(activeAppType, days),
      ])
      setSummary(s)
      setByProvider(p)
      setByModel(m)
      setDaily(d)
    } catch {
      // silent
    }
    setLoading(false)
  }

  useEffect(() => {
    loadData()
  }, [activeAppType, days])

  const maxDailyCost = Math.max(...daily.map((d) => d.cost_usd), 0.01)

  return (
    <div>
      <div className="switch-usage-panel__header">
        <h3>用量统计</h3>
        <div className="switch-usage-period">
          {PERIOD_OPTIONS.map((opt) => (
            <button
              key={opt.days}
              type="button"
              className={`switch-app-tab${days === opt.days ? ' switch-app-tab--active' : ''}`}
              onClick={() => setDays(opt.days)}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>

      {loading && <div className="switch-loading">加载中...</div>}

      {!loading && summary && (
        <>
          <div className="switch-usage-summary">
            <div className="switch-usage-stat">
              <span className="switch-usage-stat__value">{formatCost(summary.total_cost_usd)}</span>
              <span className="switch-usage-stat__label">总费用</span>
            </div>
            <div className="switch-usage-stat">
              <span className="switch-usage-stat__value">{summary.total_requests.toLocaleString()}</span>
              <span className="switch-usage-stat__label">请求数</span>
            </div>
            <div className="switch-usage-stat">
              <span className="switch-usage-stat__value">{formatTokens(summary.total_input_tokens)}</span>
              <span className="switch-usage-stat__label">输入 Token</span>
            </div>
            <div className="switch-usage-stat">
              <span className="switch-usage-stat__value">{formatTokens(summary.total_output_tokens)}</span>
              <span className="switch-usage-stat__label">输出 Token</span>
            </div>
          </div>

          {daily.length > 0 && (
            <div className="switch-usage-chart">
              <h4>每日费用</h4>
              <div className="switch-usage-bars">
                {daily.map((d) => (
                  <div key={d.date} className="switch-usage-bar" title={`${d.date}: ${formatCost(d.cost_usd)} (${d.request_count} 次请求)`}>
                    <div
                      className="switch-usage-bar__fill"
                      style={{ height: `${Math.max((d.cost_usd / maxDailyCost) * 100, 2)}%` }}
                    />
                    <span className="switch-usage-bar__label">{d.date.slice(5)}</span>
                  </div>
                ))}
              </div>
            </div>
          )}

          {byModel.length > 0 && (
            <div className="switch-usage-table">
              <h4>按模型</h4>
              <table>
                <thead>
                  <tr>
                    <th>模型</th>
                    <th>请求数</th>
                    <th>输入</th>
                    <th>输出</th>
                    <th>费用</th>
                  </tr>
                </thead>
                <tbody>
                  {byModel.map((m) => (
                    <tr key={m.model_id}>
                      <td>{m.model_id}</td>
                      <td>{m.request_count}</td>
                      <td>{formatTokens(m.input_tokens)}</td>
                      <td>{formatTokens(m.output_tokens)}</td>
                      <td>{formatCost(m.cost_usd)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}

          {byProvider.length > 0 && (
            <div className="switch-usage-table">
              <h4>按供应商</h4>
              <table>
                <thead>
                  <tr>
                    <th>供应商</th>
                    <th>请求数</th>
                    <th>输入</th>
                    <th>输出</th>
                    <th>费用</th>
                  </tr>
                </thead>
                <tbody>
                  {byProvider.map((p) => (
                    <tr key={p.provider_id}>
                      <td>{p.provider_id}</td>
                      <td>{p.request_count}</td>
                      <td>{formatTokens(p.input_tokens)}</td>
                      <td>{formatTokens(p.output_tokens)}</td>
                      <td>{formatCost(p.cost_usd)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}

          {summary.total_requests === 0 && (
            <div className="switch-empty">
              暂无用量数据。使用供应商后将自动记录。
            </div>
          )}
        </>
      )}
    </div>
  )
}
