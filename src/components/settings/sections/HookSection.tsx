import { useState, useEffect, useCallback } from 'react'
import { useTranslation } from 'react-i18next'
import { invoke } from '@tauri-apps/api/core'
import { SettingSection } from '../SettingSection'
import { SettingGroup } from '../SettingGroup'
import { GlassButton, GlassInput } from '../../shared'
import './HookSection.css'

interface ToolHookStatus {
  toolId: string
  name: string
  installStatus: 'installed' | 'not_installed' | 'error'
  configPath: string
  version?: string
}

const TOOL_ICONS: Record<string, string> = {
  'claude-code': '🟠',
  'codex': '🟢',
  'gemini-cli': '🔵',
  'cursor': '⬛',
  'copilot': '🔷',
  'trae': '🩵',
  'qoder': '🟡',
  'codebuddy': '🔴',
  'qwen': '🟣',
  'kimi': '🌸',
  'opencode': '🌿',
  'droid': '🤖',
  'kiro': '🔵',
  'aider': '💚',
  'continue': '🔵',
  'amp': '🟠',
}

export function HookSection() {
  const { t } = useTranslation()
  const [tools, setTools] = useState<ToolHookStatus[]>([])
  const [loading, setLoading] = useState(false)
  const [actionLoading, setActionLoading] = useState<Record<string, string>>({})
  const [customToolId, setCustomToolId] = useState('')
  const [customToolPath, setCustomToolPath] = useState('')
  const [addingCustom, setAddingCustom] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const fetchStatus = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const status = await invoke<ToolHookStatus[]>('get_all_hook_status')
      setTools(status)
    } catch (e) {
      setError(String(e))
    }
    setLoading(false)
  }, [])

  const detectTools = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      await invoke('detect_tools')
      await fetchStatus()
    } catch (e) {
      setError(String(e))
      setLoading(false)
    }
  }, [fetchStatus])

  useEffect(() => { fetchStatus() }, [fetchStatus])

  const setToolAction = (toolId: string, action: string | null) =>
    setActionLoading(prev => {
      const next = { ...prev }
      if (action === null) delete next[toolId]
      else next[toolId] = action
      return next
    })

  const install = async (toolId: string) => {
    setToolAction(toolId, 'install')
    try {
      await invoke('install_agent_hook', { toolId })
      await fetchStatus()
    } catch (e) {
      setError(String(e))
    }
    setToolAction(toolId, null)
  }

  const uninstall = async (toolId: string) => {
    setToolAction(toolId, 'uninstall')
    try {
      await invoke('uninstall_agent_hook', { toolId })
      await fetchStatus()
    } catch (e) {
      setError(String(e))
    }
    setToolAction(toolId, null)
  }

  const reinstallAll = async () => {
    setLoading(true)
    try {
      for (const tool of tools.filter(t => t.installStatus === 'installed')) {
        await invoke('install_agent_hook', { toolId: tool.toolId })
      }
      await fetchStatus()
    } catch (e) {
      setError(String(e))
      setLoading(false)
    }
  }

  const addCustomTool = async () => {
    if (!customToolId.trim() || !customToolPath.trim()) return
    try {
      await invoke('install_agent_hook', { toolId: customToolId.trim(), configPath: customToolPath.trim() })
      setCustomToolId('')
      setCustomToolPath('')
      setAddingCustom(false)
      await fetchStatus()
    } catch (e) {
      setError(String(e))
    }
  }

  const installedCount = tools.filter(t => t.installStatus === 'installed').length

  return (
    <SettingSection title={t('settings.hooks')} description={t('settings.hooksDesc')}>
      {error && (
        <div className="hook-error-card">{error}</div>
      )}

      <div className="hook-actions-bar">
        <GlassButton variant="secondary" onClick={detectTools} disabled={loading}>
          {loading ? '...' : t('settings.detectTools')}
        </GlassButton>
        <GlassButton variant="secondary" onClick={reinstallAll} disabled={loading || installedCount === 0}>
          {t('settings.reinstallAll')}
        </GlassButton>
      </div>

      <SettingGroup label={t('settings.detectedTools')}>
        {tools.length === 0 && !loading && (
          <div className="hook-empty">{t('settings.noToolsDetected')}</div>
        )}
        {loading && tools.length === 0 && (
          <div className="hook-empty">{t('settings.detectingTools')}</div>
        )}
        {tools.map((tool) => {
          const busy = actionLoading[tool.toolId] !== undefined
          return (
            <div key={tool.toolId} className="hook-tool-row">
              <div className="hook-tool-row__icon">
                {TOOL_ICONS[tool.toolId] ?? '🔧'}
              </div>
              <div className="hook-tool-row__info">
                <div className="hook-tool-row__name">{tool.name}</div>
                <div className="hook-tool-row__path">{tool.configPath}</div>
              </div>
              <div className={`hook-status-badge hook-status-badge--${tool.installStatus}`}>
                {tool.installStatus === 'installed'
                  ? t('settings.hookInstalled')
                  : tool.installStatus === 'error'
                  ? t('settings.hookError')
                  : t('settings.hookNotInstalled')}
              </div>
              <div className="hook-tool-row__actions">
                {tool.installStatus === 'not_installed' && (
                  <GlassButton variant="primary" onClick={() => install(tool.toolId)} disabled={busy}>
                    {busy ? '...' : t('settings.install')}
                  </GlassButton>
                )}
                {tool.installStatus === 'installed' && (
                  <>
                    <GlassButton variant="secondary" onClick={() => install(tool.toolId)} disabled={busy}>
                      {busy ? '...' : t('settings.reinstall')}
                    </GlassButton>
                    <GlassButton variant="danger" onClick={() => uninstall(tool.toolId)} disabled={busy}>
                      {t('settings.uninstall')}
                    </GlassButton>
                  </>
                )}
                {tool.installStatus === 'error' && (
                  <GlassButton variant="primary" onClick={() => install(tool.toolId)} disabled={busy}>
                    {busy ? '...' : t('settings.retry')}
                  </GlassButton>
                )}
              </div>
            </div>
          )
        })}
      </SettingGroup>

      <SettingGroup label={t('settings.customCLI')}>
        {!addingCustom ? (
          <button className="engine-add-btn" onClick={() => setAddingCustom(true)}>
            + {t('settings.addCustomTool')}
          </button>
        ) : (
          <div className="engine-add-form">
            <div className="engine-add-form__row">
              <label>{t('settings.toolId')}</label>
              <GlassInput
                placeholder="e.g. my-ai-tool"
                value={customToolId}
                onChange={(e) => setCustomToolId((e.target as HTMLInputElement).value)}
                style={{ flex: 1 }}
              />
            </div>
            <div className="engine-add-form__row">
              <label>{t('settings.configPath')}</label>
              <GlassInput
                placeholder="~/.my-tool"
                value={customToolPath}
                onChange={(e) => setCustomToolPath((e.target as HTMLInputElement).value)}
                style={{ flex: 1 }}
              />
            </div>
            <div className="engine-add-form__actions">
              <button
                className="engine-add-form__cancel"
                onClick={() => { setAddingCustom(false); setCustomToolId(''); setCustomToolPath('') }}
              >
                {t('settings.cancel')}
              </button>
              <button
                className="engine-add-form__submit"
                disabled={!customToolId.trim() || !customToolPath.trim()}
                onClick={addCustomTool}
              >
                {t('settings.install')}
              </button>
            </div>
          </div>
        )}
      </SettingGroup>
    </SettingSection>
  )
}
