import type { SwitchAppType } from '../../../../services/switchApi'
import { useSwitchStore } from '../../../../stores/switchStore'

const APP_TYPES: { id: SwitchAppType; label: string }[] = [
  { id: 'claude', label: 'Claude' },
  { id: 'codex', label: 'Codex' },
  { id: 'gemini', label: 'Gemini' },
  { id: 'opencode', label: 'OpenCode' },
  { id: 'hermes', label: 'Hermes' },
]

export function SwitchAppTabs() {
  const { activeAppType, setActiveAppType } = useSwitchStore()

  return (
    <div className="switch-app-tabs">
      {APP_TYPES.map((app) => (
        <button
          key={app.id}
          type="button"
          className={`switch-app-tab${activeAppType === app.id ? ' switch-app-tab--active' : ''}`}
          onClick={() => setActiveAppType(app.id)}
        >
          {app.label}
        </button>
      ))}
    </div>
  )
}
