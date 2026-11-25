import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useConfigStore } from '../../stores/configStore'
import { completeFirstRunSetup } from '../../services/tauriApi'

type SurfaceMode = 'island' | 'pet'

const surfaceOptions: Array<{ value: SurfaceMode; labelKey: string; mark: string }> = [
  { value: 'island', labelKey: 'settings.surfaceIsland', mark: 'I' },
  { value: 'pet', labelKey: 'settings.surfacePet', mark: 'P' },
]

export function FirstRunWelcome() {
  const { t } = useTranslation()
  const initialSurfaceMode = useConfigStore((s) => s.islandSurfaceMode)
  const [surfaceMode, setSurfaceMode] = useState<SurfaceMode>(initialSurfaceMode)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const complete = async () => {
    if (saving) return
    setSaving(true)
    setError(null)

    try {
      const config = await completeFirstRunSetup(surfaceMode)
      useConfigStore.setState({
        islandSurfaceMode: config.islandSurfaceMode,
        islandPetScale: config.islandPetScale,
        islandPetWindowOrigin: config.islandPetWindowOrigin ?? null,
        islandPetWindowAnchor: config.islandPetWindowAnchor ?? null,
        analyticsEnabled: config.analyticsEnabled,
        analyticsConsentPromptCompleted: config.analyticsConsentPromptCompleted,
      })
    } catch (err) {
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
        aria-busy={saving}
        aria-labelledby="first-run-title"
      >
        <div className="first-run-dialog__brand">
          <img src="/agentbro-app-icon.png" alt="" />
          <span>AgentBro</span>
        </div>
        <div className="first-run-dialog__header">
          <div className="first-run-dialog__eyebrow">{t('settings.welcomeEyebrow')}</div>
          <h1 id="first-run-title">{t('settings.welcomeTitle')}</h1>
          <p>{t('settings.welcomeSubtitle')}</p>
        </div>

        <div className="first-run-dialog__section">
          <div>
            <h2>{t('settings.welcomeSurface')}</h2>
            <p>{t('settings.welcomeSurfaceDesc')}</p>
          </div>
          <div className="first-run-surface" role="radiogroup" aria-label={t('settings.welcomeSurface')}>
            {surfaceOptions.map((option) => (
              <button
                key={option.value}
                className={`first-run-surface__option${surfaceMode === option.value ? ' first-run-surface__option--active' : ''}`}
                type="button"
                role="radio"
                aria-checked={surfaceMode === option.value}
                disabled={saving}
                onClick={() => setSurfaceMode(option.value)}
              >
                <span className="first-run-surface__mark" aria-hidden="true">{option.mark}</span>
                <span>{t(option.labelKey)}</span>
              </button>
            ))}
          </div>
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
