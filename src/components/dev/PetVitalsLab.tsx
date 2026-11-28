import { useCallback } from 'react'
import { usePetVitalsDebug } from '../../stores/petVitalsDebugStore'
import { useConfigStore } from '../../stores/configStore'
import { updateIslandPreferences } from '../../services/tauriApi'
import './PetVitalsLab.css'

const PHASE_OPTIONS = [
  { value: 'idle', label: '空闲' },
  { value: 'working', label: '工作中' },
  { value: 'thinking', label: '思考中' },
  { value: 'done', label: '完成' },
  { value: 'error', label: '出错' },
  { value: 'attention', label: '需关注' },
] as const

const PRESETS = [
  { label: '刚开始', phase: 'idle', pressure: 0, energy: 10 },
  { label: '正常工作', phase: 'working', pressure: 60, energy: 30 },
  { label: '开始吃力', phase: 'working', pressure: 80, energy: 50 },
  { label: '快要 compact', phase: 'thinking', pressure: 92, energy: 70 },
  { label: '极限压力', phase: 'working', pressure: 98, energy: 85 },
  { label: '精疲力竭', phase: 'idle', pressure: 0, energy: 95 },
] as const

function pressureTierLabel(p: number): string {
  if (p <= 50) return '无'
  if (p <= 75) return '低（进度条）'
  if (p <= 85) return '中（+脸红）'
  if (p <= 92) return '高（+冒汗）'
  return '极限（脉冲）'
}

function energyTierLabel(e: number): string {
  if (e <= 20) return '充沛'
  if (e <= 50) return '良好'
  if (e <= 75) return '疲劳'
  if (e <= 90) return '虚弱'
  return '耗尽'
}

export function PetVitalsLab() {
  const { enabled, contextPressure, energyLevel, phaseOverride, setEnabled, setValues, setPhaseOverride } = usePetVitalsDebug()
  const updateLocalConfig = useConfigStore((s) => s.updateConfig)

  const closePanel = useCallback(() => {
    setEnabled(false)
    setPhaseOverride(null)
    updateLocalConfig('petVitalsDebugOpen', false)
    updateIslandPreferences({ petVitalsDebugOpen: false })
      .catch((error) => console.error('Failed to close pet vitals debug panel:', error))
  }, [setEnabled, setPhaseOverride, updateLocalConfig])

  const applyPreset = useCallback((preset: typeof PRESETS[number]) => {
    setEnabled(true)
    setValues(preset.pressure, preset.energy)
    setPhaseOverride(preset.phase)
  }, [setEnabled, setValues, setPhaseOverride])

  return (
    <div className="pvl pet-surface__interactive">
      <div className="pvl__header">
        <span className="pvl__title">宠物活力调试</span>
        <label className="pvl__enabled-toggle">
          <input type="checkbox" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />
          覆盖
        </label>
        <button type="button" className="pvl__close" onClick={closePanel}>x</button>
      </div>

      <div className="pvl__controls">
        <label className="pvl__label">
          <span>阶段：<strong>{phaseOverride ? PHASE_OPTIONS.find((o) => o.value === phaseOverride)?.label ?? phaseOverride : '（真实）'}</strong></span>
          <select
            value={phaseOverride ?? ''}
            onChange={(e) => setPhaseOverride(e.target.value || null)}
            className="pvl__select"
          >
            <option value="">使用真实 session 阶段</option>
            {PHASE_OPTIONS.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
          </select>
        </label>

        <label className="pvl__label">
          <span>上下文压力：<strong>{contextPressure}%</strong></span>
          <input
            type="range" min={0} max={100} step={1}
            value={contextPressure}
            onChange={(e) => setValues(Number(e.target.value), energyLevel)}
            className="pvl__slider"
          />
          <span className="pvl__tier-tag">{pressureTierLabel(contextPressure)}</span>
        </label>

        <label className="pvl__label">
          <span>体力消耗（5h 用量）：<strong>{energyLevel}%</strong></span>
          <input
            type="range" min={0} max={100} step={1}
            value={energyLevel}
            onChange={(e) => setValues(contextPressure, Number(e.target.value))}
            className="pvl__slider"
          />
          <span className="pvl__tier-tag">{energyTierLabel(energyLevel)}</span>
        </label>
      </div>

      <div className="pvl__presets">
        {PRESETS.map((preset) => (
          <button key={preset.label} type="button" className="pvl__preset" onClick={() => applyPreset(preset)}>
            {preset.label}
          </button>
        ))}
      </div>

      <div className="pvl__hint">
        覆盖真实 session 数据，关闭后恢复。
      </div>
    </div>
  )
}
