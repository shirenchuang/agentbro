import { useEffect, useState } from 'react'
import { useSwitchStore } from '../../../../stores/switchStore'
import type { ProviderHealthInfo, SpeedTestResult } from '../../../../services/switchApi'
import { switchApi } from '../../../../services/switchApi'

const MAX_LATENCY = 3000

export function SwitchHealthPanel() {
  const { activeAppType } = useSwitchStore()
  const [providers, setProviders] = useState<ProviderHealthInfo[]>([])
  const [results, setResults] = useState<Record<string, SpeedTestResult>>({})
  const [testing, setTesting] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [tested, setTested] = useState(0)
  const [total, setTotal] = useState(0)

  useEffect(() => {
    let cancelled = false
    const id = window.setTimeout(() => {
      setLoading(true)
      switchApi
        .getProviderHealth(activeAppType)
        .then((nextProviders) => {
          if (!cancelled) setProviders(nextProviders)
        })
        .finally(() => {
          if (!cancelled) setLoading(false)
        })
    }, 0)

    return () => {
      cancelled = true
      window.clearTimeout(id)
    }
  }, [activeAppType])

  const runTest = async (providerId: string) => {
    setTesting(providerId)
    try {
      const result = await switchApi.speedTest(activeAppType, providerId)
      setResults((prev) => ({ ...prev, [providerId]: result }))
    } catch {
      // silent
    }
    setTesting(null)
  }

  const runAll = async () => {
    setTested(0)
    setTotal(providers.length)
    for (const p of providers) {
      await runTest(p.provider_id)
      setTested((prev) => prev + 1)
    }
    setTotal(0)
  }

  const latencyColor = (ms: number) => {
    if (ms < 500) return '#34C759'
    if (ms < 1500) return '#FF9500'
    return '#FF3B30'
  }

  const latencyPercent = (ms: number) => Math.min((ms / MAX_LATENCY) * 100, 100)

  return (
    <div>
      <div className="switch-provider-list__header">
        <h3>供应商健康检测</h3>
        <button
          type="button"
          className="switch-btn switch-btn--primary"
          disabled={testing !== null}
          onClick={runAll}
        >
          {total > 0 ? `测试中 (${tested}/${total})` : '全部测试'}
        </button>
      </div>

      {loading && <div className="switch-loading">加载中...</div>}

      {!loading && providers.length === 0 && (
        <div className="switch-empty">当前未配置任何供应商。</div>
      )}

      <div className="switch-health-cards">
        {providers.map((p) => {
          const result = results[p.provider_id]
          return (
            <div key={p.provider_id} className="switch-health-card">
              <div className="switch-health-card__info">
                <div className="switch-health-card__name">
                  <strong>{p.provider_name}</strong>
                  {p.is_current && <span className="switch-provider-card__badge">当前</span>}
                  {!p.has_api_key && <span className="switch-health-badge switch-health-badge--warn">无密钥</span>}
                </div>
                <span className="switch-health-card__url">{p.base_url || '未配置接口地址'}</span>
              </div>

              <div className="switch-health-card__result">
                {result && result.success && (
                  <>
                    <div className="switch-health-latency-bar">
                      <div
                        className="switch-health-latency-bar__fill"
                        style={{
                          width: `${latencyPercent(result.latency_ms)}%`,
                          background: latencyColor(result.latency_ms),
                        }}
                      />
                    </div>
                    <span className="switch-health-latency-label" style={{ color: latencyColor(result.latency_ms) }}>
                      {result.latency_ms}ms
                    </span>
                  </>
                )}
                {result && !result.success && (
                  <span className="switch-health-badge switch-health-badge--fail">
                    {result.error || '失败'}
                  </span>
                )}
                <button
                  type="button"
                  className="switch-btn switch-btn--small"
                  disabled={testing === p.provider_id}
                  onClick={() => runTest(p.provider_id)}
                >
                  {testing === p.provider_id ? '测试中...' : '测试'}
                </button>
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}
