import { useEffect, useMemo, useState } from 'react'
import { useSwitchStore } from '../../../../stores/switchStore'
import type { SwitchProvider, ProviderPreset } from '../../../../services/switchApi'

interface Props {
  provider: SwitchProvider | null
  onClose: () => void
}

type ClaudeApiFormat = 'anthropic' | 'openai_chat' | 'openai_responses' | 'gemini_native'
type ClaudeApiKeyField = 'ANTHROPIC_AUTH_TOKEN' | 'ANTHROPIC_API_KEY'

const API_FORMAT_OPTIONS: { value: ClaudeApiFormat; label: string }[] = [
  { value: 'anthropic', label: 'Anthropic Messages' },
  { value: 'openai_chat', label: 'OpenAI Chat' },
  { value: 'openai_responses', label: 'OpenAI Responses' },
  { value: 'gemini_native', label: 'Gemini Native' },
]

const API_KEY_FIELD_OPTIONS: { value: ClaudeApiKeyField; label: string }[] = [
  { value: 'ANTHROPIC_AUTH_TOKEN', label: 'ANTHROPIC_AUTH_TOKEN' },
  { value: 'ANTHROPIC_API_KEY', label: 'ANTHROPIC_API_KEY' },
]

const MODEL_ROWS = [
  { role: 'Sonnet', modelField: 'ANTHROPIC_DEFAULT_SONNET_MODEL', displayField: 'ANTHROPIC_DEFAULT_SONNET_MODEL_NAME', default: 'claude-sonnet-4-20250514' },
  { role: 'Opus', modelField: 'ANTHROPIC_DEFAULT_OPUS_MODEL', displayField: 'ANTHROPIC_DEFAULT_OPUS_MODEL_NAME', default: 'claude-opus-4-20250514' },
  { role: 'Haiku', modelField: 'ANTHROPIC_DEFAULT_HAIKU_MODEL', displayField: 'ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME', default: 'claude-haiku-4-20250506' },
]

const CATEGORY_OPTIONS = [
  { value: '', label: '未分类' },
  { value: 'official', label: '官方' },
  { value: 'cn_official', label: '国内官方' },
  { value: 'aggregator', label: '聚合服务' },
  { value: 'third_party', label: '第三方' },
]

function deepGet(obj: Record<string, unknown>, path: string): unknown {
  return path.split('.').reduce((cur: unknown, key) => {
    if (cur && typeof cur === 'object') return (cur as Record<string, unknown>)[key]
    return undefined
  }, obj)
}

function deepSet(obj: Record<string, unknown>, path: string, value: unknown): Record<string, unknown> {
  const result = JSON.parse(JSON.stringify(obj)) as Record<string, unknown>
  const keys = path.split('.')
  let cur = result as Record<string, unknown>
  for (let i = 0; i < keys.length - 1; i++) {
    if (!cur[keys[i]] || typeof cur[keys[i]] !== 'object') cur[keys[i]] = {}
    cur = cur[keys[i]] as Record<string, unknown>
  }
  if (value === undefined || value === '') {
    delete cur[keys[keys.length - 1]]
  } else {
    cur[keys[keys.length - 1]] = value
  }
  return result
}

export function SwitchProviderEditor({ provider, onClose }: Props) {
  const { activeAppType, presets, createProvider, updateProvider, loadPresets } = useSwitchStore()
  const isNew = !provider
  const isClaude = activeAppType === 'claude'

  // --- Base fields ---
  const [name, setName] = useState(provider?.name ?? '')
  const [notes, setNotes] = useState(provider?.notes ?? '')
  const [websiteUrl, setWebsiteUrl] = useState(provider?.website_url ?? '')
  const [icon, setIcon] = useState(provider?.icon ?? '')
  const [iconColor, setIconColor] = useState(provider?.icon_color ?? '')
  const [category, setCategory] = useState(provider?.category ?? '')

  // --- settings_config (deep object) ---
  const [settingsConfig, setSettingsConfig] = useState<Record<string, unknown>>(
    () => JSON.parse(JSON.stringify(provider?.settings_config ?? {}))
  )

  // --- meta ---
  const [meta, setMeta] = useState<Record<string, unknown>>(
    () => JSON.parse(JSON.stringify(provider?.meta ?? {}))
  )

  // --- UI state ---
  const [showApiKey, setShowApiKey] = useState(false)
  const [saving, setSaving] = useState(false)
  const [showPresets, setShowPresets] = useState(isNew)
  const [showAdvanced, setShowAdvanced] = useState(false)

  useEffect(() => {
    if (isNew && presets.length === 0) loadPresets()
  }, [])

  const filteredPresets = presets.filter((p) => p.supported_apps.includes(activeAppType))

  // --- Derived fields ---
  const apiFormat = (meta.apiFormat as ClaudeApiFormat) || 'anthropic'
  const apiKeyField = (meta.apiKeyField as ClaudeApiKeyField) || 'ANTHROPIC_AUTH_TOKEN'
  const isFullUrl = !!meta.isFullUrl

  const apiKey = String(deepGet(settingsConfig, `env.${apiKeyField}`) ?? deepGet(settingsConfig, 'primaryApiKey') ?? '')
  const baseUrl = String(deepGet(settingsConfig, 'env.ANTHROPIC_BASE_URL') ?? deepGet(settingsConfig, 'baseUrl') ?? '')

  // --- Setters ---
  const setApiKey = (v: string) => {
    let next = deepSet(settingsConfig, `env.${apiKeyField}`, v || undefined)
    next = deepSet(next, 'primaryApiKey', v || undefined)
    setSettingsConfig(next)
  }

  const setBaseUrl = (v: string) => {
    let next = deepSet(settingsConfig, 'env.ANTHROPIC_BASE_URL', v || undefined)
    next = deepSet(next, 'baseUrl', v || undefined)
    setSettingsConfig(next)
  }

  const setApiFormat = (v: ClaudeApiFormat) => {
    setMeta({ ...meta, apiFormat: v })
  }

  const setApiKeyFieldValue = (v: ClaudeApiKeyField) => {
    const oldKey = apiKey
    let next = deepSet(settingsConfig, `env.${apiKeyField}`, undefined)
    next = deepSet(next, `env.${v}`, oldKey || undefined)
    next = deepSet(next, 'primaryApiKey', oldKey || undefined)
    setSettingsConfig(next)
    setMeta({ ...meta, apiKeyField: v })
  }

  const setIsFullUrl = (v: boolean) => {
    setMeta({ ...meta, isFullUrl: v })
  }

  const getModelEnv = (field: string) => String(deepGet(settingsConfig, `env.${field}`) ?? '')
  const setModelEnv = (field: string, v: string) => {
    setSettingsConfig(deepSet(settingsConfig, `env.${field}`, v || undefined))
  }

  // Feature flags
  const featureFlags = useMemo(() => {
    const env = (settingsConfig.env ?? {}) as Record<string, unknown>
    return {
      hideAiSignature: env.CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC === '1',
      maxThinking: !!env.ANTHROPIC_THINK_BUDGET_TOKENS,
      disableAutoUpgrade: env.CLAUDE_CODE_DISABLE_AUTO_UPGRADE === '1',
    }
  }, [settingsConfig])

  const toggleFeature = (key: string, envKey: string, enableValue: string) => {
    const env = (settingsConfig.env ?? {}) as Record<string, unknown>
    const current = env[envKey]
    if (current === enableValue) {
      setSettingsConfig(deepSet(settingsConfig, `env.${envKey}`, undefined))
    } else {
      setSettingsConfig(deepSet(settingsConfig, `env.${envKey}`, enableValue))
    }
  }

  // Config preview
  const configPreview = useMemo(() => {
    try { return JSON.stringify(settingsConfig, null, 2) } catch { return '{}' }
  }, [settingsConfig])

  // --- Preset ---
  const applyPreset = (preset: ProviderPreset) => {
    setName(preset.name)
    setWebsiteUrl(preset.website_url)
    setIcon(preset.icon)
    setIconColor(preset.icon_color)
    setCategory(preset.category)
    const template = JSON.parse(JSON.stringify(preset.settings_template)) as Record<string, unknown>
    if (template.baseUrl) {
      const next = deepSet(template, 'env.ANTHROPIC_BASE_URL', template.baseUrl)
      delete next.baseUrl
      setSettingsConfig(next)
    } else {
      setSettingsConfig(template)
    }
    setShowPresets(false)
  }

  const fillDefaultModels = () => {
    let next = { ...settingsConfig }
    for (const row of MODEL_ROWS) {
      next = deepSet(next, `env.${row.modelField}`, row.default)
    }
    setSettingsConfig(next)
  }

  // --- Save ---
  const handleSave = async () => {
    if (!name.trim()) return
    setSaving(true)

    const data: SwitchProvider = {
      id: provider?.id ?? crypto.randomUUID(),
      app_type: activeAppType,
      name: name.trim(),
      settings_config: settingsConfig,
      website_url: websiteUrl || null,
      category: category || null,
      icon: icon || null,
      icon_color: iconColor || null,
      meta,
      is_current: provider?.is_current ?? false,
      in_failover_queue: provider?.in_failover_queue ?? false,
      created_at: provider?.created_at ?? Math.floor(Date.now() / 1000),
      sort_index: provider?.sort_index ?? null,
      notes: notes || null,
    }

    try {
      if (isNew) {
        await createProvider(data)
      } else {
        await updateProvider(data)
      }
      onClose()
    } finally {
      setSaving(false)
    }
  }

  return (
    <div>
      <div className="switch-editor__header">
        <button type="button" className="switch-btn" onClick={onClose}>← 返回</button>
        <h3>{isNew ? '新建供应商' : `编辑「${provider!.name}」`}</h3>
      </div>

      {/* Preset picker */}
      {isNew && showPresets && filteredPresets.length > 0 && (
        <div>
          <div className="switch-presets__header">
            <span>从预设快速创建</span>
            <button type="button" className="switch-btn switch-btn--small" onClick={() => setShowPresets(false)}>
              跳过，手动创建
            </button>
          </div>
          <div className="switch-presets__grid">
            {filteredPresets.map((preset) => (
              <button
                key={preset.id}
                type="button"
                className="switch-preset-card"
                onClick={() => applyPreset(preset)}
              >
                <div className="switch-preset-card__icon" style={{ background: preset.icon_color || '#007AFF' }}>
                  {preset.name.charAt(0).toUpperCase()}
                </div>
                <div className="switch-preset-card__info">
                  <strong>{preset.name}</strong>
                  <span>{preset.description}</span>
                </div>
              </button>
            ))}
          </div>
        </div>
      )}

      {/* Main form */}
      {(!showPresets || !isNew || filteredPresets.length === 0) && (
        <div className="switch-editor__form">
          {isNew && filteredPresets.length > 0 && (
            <button type="button" className="switch-btn switch-btn--small" onClick={() => setShowPresets(true)}>
              ← 从预设选择
            </button>
          )}

          {/* Row: name + notes */}
          <div className="switch-editor__row">
            <label className="switch-field">
              <span>名称</span>
              <input type="text" value={name} onChange={(e) => setName(e.target.value)} placeholder="供应商名称" />
            </label>
            <label className="switch-field">
              <span>备注</span>
              <input type="text" value={notes} onChange={(e) => setNotes(e.target.value)} placeholder="API 地址或备注..." />
            </label>
          </div>

          {/* Website */}
          <label className="switch-field">
            <span>官网链接</span>
            <input type="text" value={websiteUrl} onChange={(e) => setWebsiteUrl(e.target.value)} placeholder="https://..." />
          </label>

          {/* API Key with show/hide */}
          <div className="switch-field">
            <span>API 密钥</span>
            <div className="switch-field__input-group">
              <input
                type={showApiKey ? 'text' : 'password'}
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                placeholder="sk-..."
              />
              <button
                type="button"
                className="switch-field__toggle"
                onClick={() => setShowApiKey(!showApiKey)}
              >
                {showApiKey ? '隐藏' : '显示'}
              </button>
            </div>
          </div>

          {/* Base URL with full URL toggle */}
          <div className="switch-field">
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <span>请求地址</span>
              <label className="switch-mini-toggle">
                <input type="checkbox" checked={isFullUrl} onChange={(e) => setIsFullUrl(e.target.checked)} />
                <span>完整 URL</span>
              </label>
            </div>
            <input
              type="text"
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
              placeholder={isFullUrl ? 'https://api.example.com/v1/messages' : 'https://api.anthropic.com'}
            />
          </div>

          {/* Claude-specific fields */}
          {isClaude && (
            <>
              <div className="switch-editor__section-title">Claude 专属配置</div>

              {/* API Format + Auth Field */}
              <div className="switch-editor__row">
                <label className="switch-field">
                  <span>API 格式</span>
                  <select
                    value={apiFormat}
                    onChange={(e) => setApiFormat(e.target.value as ClaudeApiFormat)}
                    className="switch-select"
                  >
                    {API_FORMAT_OPTIONS.map((o) => (
                      <option key={o.value} value={o.value}>{o.label}</option>
                    ))}
                  </select>
                </label>
                <label className="switch-field">
                  <span>认证字段</span>
                  <select
                    value={apiKeyField}
                    onChange={(e) => setApiKeyFieldValue(e.target.value as ClaudeApiKeyField)}
                    className="switch-select"
                  >
                    {API_KEY_FIELD_OPTIONS.map((o) => (
                      <option key={o.value} value={o.value}>{o.label}</option>
                    ))}
                  </select>
                </label>
              </div>

              {/* Model mapping */}
              <div className="switch-field">
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                  <span>模型映射</span>
                  <button type="button" className="switch-btn switch-btn--small" onClick={fillDefaultModels}>
                    一键填充默认
                  </button>
                </div>
                <div className="switch-model-table">
                  <div className="switch-model-table__header">
                    <span>角色</span>
                    <span>显示名称</span>
                    <span>实际模型</span>
                  </div>
                  {MODEL_ROWS.map((row) => (
                    <div key={row.role} className="switch-model-table__row">
                      <span className="switch-model-table__role">{row.role}</span>
                      <input
                        type="text"
                        value={getModelEnv(row.displayField)}
                        onChange={(e) => setModelEnv(row.displayField, e.target.value)}
                        placeholder={`${row.role} 显示名`}
                      />
                      <input
                        type="text"
                        value={getModelEnv(row.modelField)}
                        onChange={(e) => setModelEnv(row.modelField, e.target.value)}
                        placeholder={row.default}
                      />
                    </div>
                  ))}
                </div>
              </div>

              {/* Feature flags */}
              <div className="switch-field">
                <span>功能开关</span>
                <div className="switch-feature-flags">
                  <label className="switch-feature-flag">
                    <input
                      type="checkbox"
                      checked={featureFlags.hideAiSignature}
                      onChange={() => toggleFeature('hideAiSignature', 'CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC', '1')}
                    />
                    <span>隐藏 AI 签名</span>
                  </label>
                  <label className="switch-feature-flag">
                    <input
                      type="checkbox"
                      checked={featureFlags.maxThinking}
                      onChange={() => toggleFeature('maxThinking', 'ANTHROPIC_THINK_BUDGET_TOKENS', '50000')}
                    />
                    <span>最大强度思考</span>
                  </label>
                  <label className="switch-feature-flag">
                    <input
                      type="checkbox"
                      checked={featureFlags.disableAutoUpgrade}
                      onChange={() => toggleFeature('disableAutoUpgrade', 'CLAUDE_CODE_DISABLE_AUTO_UPGRADE', '1')}
                    />
                    <span>禁用自动升级</span>
                  </label>
                </div>
              </div>
            </>
          )}

          {/* Config JSON preview */}
          <div className="switch-field">
            <span>配置 JSON 预览</span>
            <textarea
              className="switch-textarea switch-config-preview"
              rows={8}
              value={configPreview}
              readOnly
            />
          </div>

          {/* Advanced options toggle */}
          <button
            type="button"
            className="switch-advanced-toggle"
            onClick={() => setShowAdvanced(!showAdvanced)}
          >
            {showAdvanced ? '▼' : '▶'} 高级选项
          </button>

          {showAdvanced && (
            <div className="switch-editor__advanced">
              <div className="switch-editor__row">
                <label className="switch-field">
                  <span>图标</span>
                  <input type="text" value={icon} onChange={(e) => setIcon(e.target.value)} placeholder="图标标识" />
                </label>
                <label className="switch-field">
                  <span>图标颜色</span>
                  <div className="switch-field__input-group">
                    <input type="text" value={iconColor} onChange={(e) => setIconColor(e.target.value)} placeholder="#007AFF" />
                    {iconColor && (
                      <div className="switch-color-swatch" style={{ background: iconColor }} />
                    )}
                  </div>
                </label>
              </div>
              <label className="switch-field">
                <span>分类</span>
                <select value={category} onChange={(e) => setCategory(e.target.value)} className="switch-select">
                  {CATEGORY_OPTIONS.map((o) => (
                    <option key={o.value} value={o.value}>{o.label}</option>
                  ))}
                </select>
              </label>
            </div>
          )}

          {/* Save / Cancel */}
          <div className="switch-editor__actions">
            <button type="button" className="switch-btn" onClick={onClose}>取消</button>
            <button
              type="button"
              className="switch-btn switch-btn--primary"
              disabled={!name.trim() || saving}
              onClick={handleSave}
            >
              {saving ? '保存中...' : '保存'}
            </button>
          </div>
        </div>
      )}
    </div>
  )
}
