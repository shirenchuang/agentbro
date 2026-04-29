import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { invoke } from '@tauri-apps/api/core'
import { SettingSection } from '../SettingSection'
import { SettingGroup } from '../SettingGroup'
import { SettingRow } from '../SettingRow'
import { Toggle } from '../Toggle'
import { GlassButton, GlassInput } from '../../shared'

interface WebhookConfig {
  enabled: boolean
  url: string
  secret?: string
  events: string[]
}

type WebhookProvider = 'dingtalk' | 'feishu'

const EVENT_OPTIONS = [
  'session_start',
  'task_complete',
  'error',
  'waiting_approval',
]

function WebhookProviderSection({
  provider,
  labelKey,
  descKey,
  urlPlaceholder,
  iconEmoji,
}: {
  provider: WebhookProvider
  labelKey: string
  descKey: string
  urlPlaceholder: string
  iconEmoji: string
}) {
  const { t } = useTranslation()
  const [config, setConfig] = useState<WebhookConfig>({
    enabled: false,
    url: '',
    secret: '',
    events: ['task_complete', 'error', 'waiting_approval'],
  })
  const [saving, setSaving] = useState(false)
  const [testResult, setTestResult] = useState<'success' | 'error' | null>(null)

  const save = async () => {
    setSaving(true)
    try {
      await invoke('save_webhook_config', { provider, config })
    } catch (e) {
      console.error('Failed to save webhook config:', e)
    }
    setSaving(false)
  }

  const test = async () => {
    setTestResult(null)
    try {
      await invoke('test_webhook', { provider, url: config.url, secret: config.secret })
      setTestResult('success')
    } catch {
      setTestResult('error')
    }
    setTimeout(() => setTestResult(null), 3000)
  }

  const toggleEvent = (event: string) => {
    setConfig(prev => ({
      ...prev,
      events: prev.events.includes(event)
        ? prev.events.filter(e => e !== event)
        : [...prev.events, event],
    }))
  }

  return (
    <SettingGroup label={`${iconEmoji} ${t(labelKey)}`}>
      <SettingRow label={t('settings.webhookEnabled')} description={t(descKey)}>
        <Toggle checked={config.enabled} onChange={(v) => setConfig(prev => ({ ...prev, enabled: v }))} />
      </SettingRow>
      {config.enabled && (
        <>
          <SettingRow label={t('settings.webhookUrl')}>
            <GlassInput
              placeholder={urlPlaceholder}
              value={config.url}
              onChange={(e) => setConfig(prev => ({ ...prev, url: (e.target as HTMLInputElement).value }))}
              style={{ width: 260, fontSize: 12 }}
            />
          </SettingRow>
          <SettingRow label={t('settings.webhookSecret')}>
            <GlassInput
              type="password"
              placeholder={t('settings.webhookSecretPlaceholder')}
              value={config.secret ?? ''}
              onChange={(e) => setConfig(prev => ({ ...prev, secret: (e.target as HTMLInputElement).value }))}
              style={{ width: 200, fontSize: 12 }}
            />
          </SettingRow>
          <SettingRow label={t('settings.webhookEvents')} description={t('settings.webhookEventsDesc')}>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 4, alignItems: 'flex-end' }}>
              {EVENT_OPTIONS.map(event => (
                <label key={event} style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 12, cursor: 'pointer' }}>
                  <input
                    type="checkbox"
                    checked={config.events.includes(event)}
                    onChange={() => toggleEvent(event)}
                  />
                  <span style={{ color: 'var(--settings-text-secondary)' }}>{t(`settings.webhookEvent_${event}`, { defaultValue: event })}</span>
                </label>
              ))}
            </div>
          </SettingRow>
          <div style={{ display: 'flex', gap: 8, paddingTop: 8, justifyContent: 'flex-end' }}>
            {testResult === 'success' && (
              <span style={{ fontSize: 12, color: 'var(--settings-status-active)', alignSelf: 'center' }}>
                {t('settings.webhookTestSuccess')}
              </span>
            )}
            {testResult === 'error' && (
              <span style={{ fontSize: 12, color: 'var(--settings-danger)', alignSelf: 'center' }}>
                {t('settings.webhookTestError')}
              </span>
            )}
            <GlassButton variant="ghost" onClick={test} disabled={!config.url}>
              {t('settings.webhookTest')}
            </GlassButton>
            <GlassButton variant="primary" onClick={save} disabled={saving || !config.url}>
              {saving ? '...' : t('settings.save')}
            </GlassButton>
          </div>
        </>
      )}
    </SettingGroup>
  )
}

export function WebhookSection() {
  const { t } = useTranslation()

  return (
    <SettingSection title={t('settings.webhooks')} description={t('settings.webhooksDesc')}>
      <div className="description-card">
        {t('settings.webhooksInfo')}
      </div>

      <WebhookProviderSection
        provider="dingtalk"
        labelKey="settings.dingtalk"
        descKey="settings.dingtalkDesc"
        urlPlaceholder="https://oapi.dingtalk.com/robot/send?access_token=..."
        iconEmoji="🔔"
      />

      <WebhookProviderSection
        provider="feishu"
        labelKey="settings.feishu"
        descKey="settings.feishuDesc"
        urlPlaceholder="https://open.feishu.cn/open-apis/bot/v2/hook/..."
        iconEmoji="🪶"
      />
    </SettingSection>
  )
}
