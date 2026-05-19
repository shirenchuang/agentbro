import { useEffect, useState } from 'react'
import { useSwitchStore } from '../../../../stores/switchStore'
import { SwitchProviderEditor } from './SwitchProviderEditor'
import { switchApi } from '../../../../services/switchApi'
import type { SwitchProvider } from '../../../../services/switchApi'

const AVATAR_COLORS = ['#007AFF', '#5856D6', '#FF9500', '#34C759', '#FF3B30', '#AF52DE', '#FF2D55', '#5AC8FA']

function avatarBg(name: string, color?: string | null) {
  if (color) return color
  let hash = 0
  for (const ch of name) hash = ch.charCodeAt(0) + ((hash << 5) - hash)
  return AVATAR_COLORS[Math.abs(hash) % AVATAR_COLORS.length]
}

const CATEGORY_LABELS: Record<string, string> = {
  official: '官方',
  cn_official: '国内官方',
  aggregator: '聚合服务',
  third_party: '第三方',
}

function getProviderUrl(p: SwitchProvider): string | null {
  if (p.notes) return p.notes
  const cfg = p.settings_config as Record<string, unknown>
  const env = cfg?.env as Record<string, unknown> | undefined
  if (env?.ANTHROPIC_BASE_URL) return String(env.ANTHROPIC_BASE_URL)
  if (cfg?.baseUrl) return String(cfg.baseUrl)
  if (p.website_url) return p.website_url
  return null
}

export function SwitchProviderList() {
  const { providers, loading, error, loadProviders, setCurrent, deleteProvider, duplicateProvider, activeAppType, clearError } = useSwitchStore()
  const [editingProvider, setEditingProvider] = useState<SwitchProvider | null>(null)
  const [showEditor, setShowEditor] = useState(false)
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null)
  const [testingId, setTestingId] = useState<string | null>(null)

  useEffect(() => {
    loadProviders()
  }, [activeAppType])

  useEffect(() => {
    if (error) {
      const t = setTimeout(clearError, 5000)
      return () => clearTimeout(t)
    }
  }, [error])

  const handleNew = () => {
    setEditingProvider(null)
    setShowEditor(true)
  }

  const handleEdit = (provider: SwitchProvider) => {
    setEditingProvider(provider)
    setShowEditor(true)
  }

  const handleEditorClose = () => {
    setShowEditor(false)
    setEditingProvider(null)
  }

  const confirmDelete = () => {
    if (confirmDeleteId) {
      deleteProvider(confirmDeleteId)
      setConfirmDeleteId(null)
    }
  }

  const handleSpeedTest = async (id: string) => {
    setTestingId(id)
    try {
      const result = await switchApi.speedTest(activeAppType, id)
      if (result.success) {
        alert(`测速成功: ${result.latency_ms}ms`)
      } else {
        alert(`测速失败: ${result.error || '未知错误'}`)
      }
    } catch (e) {
      alert(`测速出错: ${e}`)
    }
    setTestingId(null)
  }

  if (showEditor) {
    return <SwitchProviderEditor provider={editingProvider} onClose={handleEditorClose} />
  }

  return (
    <div>
      {error && <div className="switch-error">{error}</div>}

      <div className="switch-provider-list__header">
        <h3>供应商列表</h3>
        <button type="button" className="switch-btn switch-btn--primary" onClick={handleNew}>
          + 添加供应商
        </button>
      </div>

      {loading && <div className="switch-loading">加载中...</div>}

      {!loading && providers.length === 0 && (
        <div className="switch-empty">
          <span>当前未配置任何供应商</span>
          <button type="button" className="switch-btn switch-btn--primary" onClick={handleNew}>
            + 添加供应商
          </button>
        </div>
      )}

      <div className="switch-provider-cards">
        {providers.map((p) => {
          const url = getProviderUrl(p)
          const categoryLabel = p.category ? (CATEGORY_LABELS[p.category] || p.category) : null

          return (
            <div
              key={p.id}
              className={`switch-provider-card${p.is_current ? ' switch-provider-card--current' : ''}`}
            >
              <div
                className="switch-provider-card__avatar"
                style={{ background: avatarBg(p.name, p.icon_color) }}
              >
                {p.name.charAt(0).toUpperCase()}
              </div>
              <div className="switch-provider-card__info">
                <div className="switch-provider-card__name">
                  <strong>{p.name}</strong>
                  {p.is_current && <span className="switch-provider-card__badge">当前</span>}
                  {categoryLabel && <span className="switch-provider-card__category-badge">{categoryLabel}</span>}
                </div>
                {url && (
                  <span className="switch-provider-card__url" title={url}>
                    {url}
                  </span>
                )}
              </div>
              <div className="switch-provider-card__actions">
                {p.is_current ? (
                  <button
                    type="button"
                    className="switch-icon-btn switch-icon-btn--active"
                    title="已启用"
                    disabled
                  >
                    <span>✓</span>
                  </button>
                ) : (
                  <button
                    type="button"
                    className="switch-icon-btn switch-icon-btn--enable"
                    title="启用"
                    onClick={() => setCurrent(p.id)}
                  >
                    <span>▶</span>
                  </button>
                )}
                <button
                  type="button"
                  className="switch-icon-btn"
                  title="编辑"
                  onClick={() => handleEdit(p)}
                >
                  <span>✏️</span>
                </button>
                <button
                  type="button"
                  className="switch-icon-btn"
                  title="复制"
                  onClick={() => duplicateProvider(p.id)}
                >
                  <span>📋</span>
                </button>
                <button
                  type="button"
                  className="switch-icon-btn"
                  title="测速"
                  disabled={testingId === p.id}
                  onClick={() => handleSpeedTest(p.id)}
                >
                  <span>{testingId === p.id ? '⏳' : '🔬'}</span>
                </button>
                <button
                  type="button"
                  className="switch-icon-btn switch-icon-btn--danger"
                  title="删除"
                  onClick={() => setConfirmDeleteId(p.id)}
                >
                  <span>🗑</span>
                </button>
              </div>
            </div>
          )
        })}
      </div>

      {confirmDeleteId && (
        <div className="switch-confirm-overlay">
          <div className="switch-confirm-dialog">
            <p>确定删除该供应商？此操作不可撤销。</p>
            <div className="switch-confirm-dialog__actions">
              <button type="button" className="switch-btn" onClick={() => setConfirmDeleteId(null)}>
                取消
              </button>
              <button type="button" className="switch-btn switch-btn--danger" onClick={confirmDelete}>
                删除
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
