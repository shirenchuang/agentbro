import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useConfigStore } from '../../stores/configStore'
import { setAnalyticsEnabled as setAnalyticsEnabledBackend } from '../../services/tauriApi'

export function FirstRunWelcome() {
  const { t } = useTranslation()
  const updateConfig = useConfigStore((s) => s.updateConfig)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const complete = async () => {
    if (saving) return
    const previous = useConfigStore.getState()
    setSaving(true)
    setError(null)
    updateConfig('analyticsEnabled', true)
    updateConfig('analyticsConsentPromptCompleted', true)

    try {
      await setAnalyticsEnabledBackend(true)
    } catch (err) {
      updateConfig('analyticsEnabled', previous.analyticsEnabled)
      updateConfig('analyticsConsentPromptCompleted', previous.analyticsConsentPromptCompleted)
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="first-run-overlay">
      <section
        className="first-run-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="first-run-title"
      >
        <div className="first-run-dialog__brand">
          <img src="/agentbro-app-icon.png" alt="" />
          <span>AgentBro</span>
        </div>
        <div className="first-run-dialog__header">
          <div className="first-run-dialog__eyebrow">{t('settings.welcomeEyebrow')}</div>
          <h1 id="first-run-title">{t('settings.title')}</h1>
          <p>{t('notch.slogan')}</p>
        </div>

        {error && (
          <div className="first-run-dialog__error" role="alert">
            {t('settings.welcomeError', { message: error })}
          </div>
        )}

        <div className="first-run-dialog__footer">
          <button
            className="first-run-dialog__button"
            type="button"
            disabled={saving}
            onClick={complete}
          >
            {saving ? t('settings.welcomeSaving') : t('settings.welcomeContinue')}
          </button>
        </div>
      </section>
    </div>
  )
}
