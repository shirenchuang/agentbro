import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
  type WheelEvent as ReactWheelEvent,
} from 'react'
import { useTranslation } from 'react-i18next'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { skillApiV2 } from '../../services/skillApiV2'
import type { AgentSummary, SkillPackSummary } from '../../services/skillApiV2'
import { isTauri } from '../../services/tauriApi'
import { useSelectedRuntimeEnvironment } from '../../hooks/useRuntimeEnvironment'
import { useRuntimeEnvironmentStore } from '../../stores/runtimeEnvironmentStore'
import { AgentIconBadge } from '../skills-v2/AgentIconBadge'
import './SkillPackPicker.css'

const SHARED_SKILLS_AGENT_ID = 'agents'
const EMPTY_PACK_IDS = new Set<string>()
const AGENT_DRAG_THRESHOLD = 4

interface AgentDragState {
  pointerId: number
  startX: number
  startScrollLeft: number
  dragged: boolean
}

function sortAgents(agents: AgentSummary[]) {
  return [...agents].sort((left, right) => {
    const priority = Number(left.id !== 'codex') - Number(right.id !== 'codex')
    return priority || left.displayName.localeCompare(right.displayName)
  })
}

function initialPackOrder(packs: SkillPackSummary[], appliedPackIds: string[]) {
  const applied = new Set(appliedPackIds)
  return [
    ...packs.filter((pack) => applied.has(pack.id)),
    ...packs.filter((pack) => !applied.has(pack.id)),
  ].map((pack) => pack.id)
}

async function closePicker() {
  try {
    await getCurrentWindow().hide()
  } catch {
    // The focus-loss handler may have already hidden the popover.
  }
}

export function SkillPackPicker() {
  const { t } = useTranslation()
  const refreshEnvironments = useRuntimeEnvironmentStore((state) => state.refreshEnvironments)
  const {
    selectedEnvironmentId,
    isLocal,
    remoteHost,
  } = useSelectedRuntimeEnvironment()
  const [agents, setAgents] = useState<AgentSummary[]>([])
  const [packs, setPacks] = useState<SkillPackSummary[]>([])
  const [appliedByAgent, setAppliedByAgent] = useState<Record<string, Set<string>>>({})
  const [packOrderByAgent, setPackOrderByAgent] = useState<Record<string, string[]>>({})
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null)
  const [mode, setMode] = useState<'link' | 'copy'>('link')
  const [query, setQuery] = useState('')
  const [busy, setBusy] = useState<Set<string>>(new Set())
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const interactionRevision = useRef(0)
  const agentDrag = useRef<AgentDragState | null>(null)
  const suppressAgentClick = useRef(false)
  const environmentName = remoteHost?.name ?? selectedEnvironmentId

  const loadPickerData = useCallback(async (foreground: boolean) => {
    const revision = interactionRevision.current
    if (foreground) setLoading(true)
    try {
      const data = await skillApiV2.getSkillPackPickerData()
      if (revision !== interactionRevision.current) return
      const availableAgents = sortAgents(data.agents.filter((agent) => (
        agent.id !== SHARED_SKILLS_AGENT_ID
        && agent.installed
        && agent.enabled
        && Boolean(agent.skillsDir)
      )))
      const nextApplied = Object.fromEntries(
        availableAgents.map((agent) => [agent.id, new Set(data.appliedByAgent[agent.id] ?? [])]),
      )
      const nextPackOrder = Object.fromEntries(
        availableAgents.map((agent) => [
          agent.id,
          initialPackOrder(data.packs, data.appliedByAgent[agent.id] ?? []),
        ]),
      )
      setAgents(availableAgents)
      setPacks(data.packs)
      setAppliedByAgent(nextApplied)
      setPackOrderByAgent(nextPackOrder)
      setMode(data.defaultDistributeMode)
      setSelectedAgentId((current) => (
        availableAgents.some((agent) => agent.id === current)
          ? current
          : availableAgents.find((agent) => agent.id === 'codex')?.id ?? availableAgents[0]?.id ?? null
      ))
      setError(null)
    } catch (loadError) {
      if (revision === interactionRevision.current) setError(String(loadError))
    } finally {
      if (foreground) setLoading(false)
    }
  }, [])

  useEffect(() => {
    document.documentElement.classList.add('skill-pack-picker-body')
    document.body.classList.add('skill-pack-picker-body')
    return () => {
      document.documentElement.classList.remove('skill-pack-picker-body')
      document.body.classList.remove('skill-pack-picker-body')
    }
  }, [])

  useEffect(() => {
    void loadPickerData(true)
  }, [loadPickerData])

  useEffect(() => {
    if (isTauri()) void refreshEnvironments()
  }, [refreshEnvironments])

  useEffect(() => {
    if (!isTauri()) return
    let cancelled = false
    let unlisten: (() => void) | undefined
    void import('@tauri-apps/api/event')
      .then(({ listen }) => listen('skill-pack-picker-shown', () => void loadPickerData(false)))
      .then((stopListening) => {
        if (cancelled) stopListening()
        else unlisten = stopListening
      })
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [loadPickerData])

  useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') void closePicker()
    }
    window.addEventListener('keydown', closeOnEscape)
    return () => window.removeEventListener('keydown', closeOnEscape)
  }, [])

  const applied = selectedAgentId ? appliedByAgent[selectedAgentId] ?? EMPTY_PACK_IDS : EMPTY_PACK_IDS
  const filteredPacks = useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase()
    if (!normalizedQuery) return packs
    return packs.filter((pack) => (
      `${pack.name} ${pack.description} ${pack.tags.join(' ')}`
        .toLocaleLowerCase()
        .includes(normalizedQuery)
    ))
  }, [packs, query])
  const orderedPacks = useMemo(() => {
    const order = selectedAgentId ? packOrderByAgent[selectedAgentId] ?? [] : []
    const rank = new Map(order.map((packId, index) => [packId, index]))
    return filteredPacks
      .map((pack, index) => ({ pack, index }))
      .sort((left, right) => (
        (rank.get(left.pack.id) ?? Number.MAX_SAFE_INTEGER)
        - (rank.get(right.pack.id) ?? Number.MAX_SAFE_INTEGER)
        || left.index - right.index
      ))
      .map(({ pack }) => pack)
  }, [filteredPacks, packOrderByAgent, selectedAgentId])

  const updateApplied = (agentId: string, packId: string, enabled: boolean) => {
    setAppliedByAgent((current) => {
      const next = new Set(current[agentId] ?? [])
      if (enabled) next.add(packId)
      else next.delete(packId)
      return { ...current, [agentId]: next }
    })
  }

  const togglePack = async (pack: SkillPackSummary) => {
    if (!selectedAgentId || !pack.healthy || pack.memberCount === 0) return
    const actionKey = `${selectedAgentId}\u0000${pack.id}`
    if (busy.has(actionKey)) return
    interactionRevision.current += 1
    const wasApplied = applied.has(pack.id)
    setError(null)
    setBusy((current) => new Set(current).add(actionKey))
    updateApplied(selectedAgentId, pack.id, !wasApplied)
    try {
      if (wasApplied) {
        await skillApiV2.removePackFromAgent(pack.id, selectedAgentId)
      } else {
        const result = await skillApiV2.executeApplyPack(pack.id, [selectedAgentId], mode)
        if (result.blockers.length > 0) {
          updateApplied(selectedAgentId, pack.id, false)
          setError(t('tray.skillPickerConflict', { count: result.blockers.length }))
        }
      }
    } catch (actionError) {
      updateApplied(selectedAgentId, pack.id, wasApplied)
      setError(t('tray.skillPickerError', { error: String(actionError) }))
    } finally {
      setBusy((current) => {
        const next = new Set(current)
        next.delete(actionKey)
        return next
      })
    }
  }

  const openManager = async () => {
    try {
      await invoke('open_settings_window')
      await closePicker()
    } catch (managerError) {
      setError(t('tray.skillPickerError', { error: String(managerError) }))
    }
  }

  const handleAgentWheel = (event: ReactWheelEvent<HTMLDivElement>) => {
    const list = event.currentTarget
    if (list.scrollWidth <= list.clientWidth) return
    const delta = Math.abs(event.deltaX) > Math.abs(event.deltaY) ? event.deltaX : event.deltaY
    const nextScrollLeft = Math.max(
      0,
      Math.min(list.scrollWidth - list.clientWidth, list.scrollLeft + delta),
    )
    if (nextScrollLeft === list.scrollLeft) return
    list.scrollLeft = nextScrollLeft
    event.preventDefault()
  }

  const handleAgentPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.pointerType === 'mouse' && event.button !== 0) return
    agentDrag.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startScrollLeft: event.currentTarget.scrollLeft,
      dragged: false,
    }
  }

  const handleAgentPointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = agentDrag.current
    if (!drag || drag.pointerId !== event.pointerId) return
    const delta = event.clientX - drag.startX
    if (!drag.dragged && Math.abs(delta) < AGENT_DRAG_THRESHOLD) return
    if (!drag.dragged) event.currentTarget.setPointerCapture?.(event.pointerId)
    drag.dragged = true
    event.currentTarget.classList.add('skill-pack-picker__agents--dragging')
    event.currentTarget.scrollLeft = drag.startScrollLeft - delta
    event.preventDefault()
  }

  const finishAgentDrag = (event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = agentDrag.current
    if (!drag || drag.pointerId !== event.pointerId) return
    suppressAgentClick.current = drag.dragged
    event.currentTarget.classList.remove('skill-pack-picker__agents--dragging')
    if (event.currentTarget.hasPointerCapture?.(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId)
    }
    agentDrag.current = null
  }

  const cancelAgentDrag = (event: ReactPointerEvent<HTMLDivElement>) => {
    suppressAgentClick.current = false
    event.currentTarget.classList.remove('skill-pack-picker__agents--dragging')
    agentDrag.current = null
  }

  const handleAgentClickCapture = (event: ReactMouseEvent<HTMLDivElement>) => {
    if (!suppressAgentClick.current) return
    suppressAgentClick.current = false
    event.preventDefault()
    event.stopPropagation()
  }

  return (
    <main className="skill-pack-picker" aria-label={t('tray.skillPickerTitle')}>
      <div className="skill-pack-picker__pointer" aria-hidden="true" />
      <section className="skill-pack-picker__surface">
        <header className="skill-pack-picker__header">
          <div>
            <div className="skill-pack-picker__context">
              <span className="skill-pack-picker__kicker">{t('tray.skillPickerKicker')}</span>
              {!isLocal && (
                <span
                  className="skill-pack-picker__environment"
                  role="status"
                  aria-label={t('skills.runtimeEnvironment.currentLabel', { name: environmentName })}
                  title={t('tray.skillPickerRemoteHint', { name: environmentName })}
                >
                  <span aria-hidden="true">&gt;_</span>
                  <em>{t('skills.runtimeEnvironment.remoteMeta')}</em>
                  <strong>{environmentName}</strong>
                </span>
              )}
            </div>
            <h1>{t('tray.skillPickerTitle')}</h1>
            <p>{t('tray.skillPickerSubtitle')}</p>
          </div>
          <button
            className="skill-pack-picker__close"
            type="button"
            aria-label={t('common.close', { defaultValue: 'Close' })}
            onClick={() => void closePicker()}
          >
            ×
          </button>
        </header>

        {agents.length > 0 && (
          <div
            className="skill-pack-picker__agents"
            role="tablist"
            aria-label={t('skills.allAgents')}
            onWheel={handleAgentWheel}
            onPointerDown={handleAgentPointerDown}
            onPointerMove={handleAgentPointerMove}
            onPointerUp={finishAgentDrag}
            onPointerCancel={cancelAgentDrag}
            onClickCapture={handleAgentClickCapture}
          >
            {agents.map((agent) => {
              const count = appliedByAgent[agent.id]?.size ?? 0
              return (
                <button
                  key={agent.id}
                  type="button"
                  role="tab"
                  aria-selected={selectedAgentId === agent.id}
                  className={`skill-pack-picker__agent${selectedAgentId === agent.id ? ' skill-pack-picker__agent--active' : ''}`}
                  onClick={() => {
                    setSelectedAgentId(agent.id)
                    setError(null)
                  }}
                >
                  <AgentIconBadge iconKey={agent.iconKey} size={24} />
                  <span>{agent.displayName}</span>
                  <em>{count}</em>
                </button>
              )
            })}
          </div>
        )}

        <div className="skill-pack-picker__toolbar">
          <label className="skill-pack-picker__search">
            <span aria-hidden="true">⌕</span>
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t('tray.skillPickerSearch')}
            />
          </label>
          <span className="skill-pack-picker__count">
            <i aria-hidden="true" />
            {t('tray.skillPickerEnabled', { count: applied.size })}
          </span>
        </div>

        {error && (
          <div className="skill-pack-picker__error" role="alert">
            <span>{error}</span>
            <button type="button" onClick={() => void openManager()}>{t('tray.skillPickerManage')}</button>
          </div>
        )}

        <div className="skill-pack-picker__list">
          {loading ? (
            <div className="skill-pack-picker__empty skill-pack-picker__empty--loading">
              <i aria-hidden="true" />
              <span>{t('tray.skillPickerLoading')}</span>
            </div>
          ) : agents.length === 0 ? (
            <div className="skill-pack-picker__empty">{t('tray.skillPickerNoAgents')}</div>
          ) : packs.length === 0 ? (
            <div className="skill-pack-picker__empty">{t('tray.skillPickerEmpty')}</div>
          ) : filteredPacks.length === 0 ? (
            <div className="skill-pack-picker__empty">{t('tray.skillPickerNoMatch')}</div>
          ) : (
            orderedPacks.map((pack) => {
              const enabled = applied.has(pack.id)
              const actionKey = selectedAgentId ? `${selectedAgentId}\u0000${pack.id}` : ''
              const isBusy = busy.has(actionKey)
              const unavailable = !pack.healthy || pack.memberCount === 0
              return (
                <button
                  key={pack.id}
                  type="button"
                  role="checkbox"
                  aria-checked={enabled}
                  disabled={unavailable}
                  className={`skill-pack-picker__pack${enabled ? ' skill-pack-picker__pack--active' : ''}`}
                  onClick={() => void togglePack(pack)}
                >
                  <span className="skill-pack-picker__check" aria-hidden="true">
                    {isBusy ? <i /> : enabled ? '✓' : ''}
                  </span>
                  <span className="skill-pack-picker__pack-copy">
                    <strong>{pack.name}</strong>
                    <span>{pack.description || t('tray.skillPickerSkills', { count: pack.memberCount })}</span>
                  </span>
                  <span className="skill-pack-picker__pack-meta">
                    {unavailable
                      ? t('tray.skillPickerUnavailable')
                      : t('tray.skillPickerSkills', { count: pack.memberCount })}
                  </span>
                </button>
              )
            })
          )}
        </div>

        <footer className="skill-pack-picker__footer">
          <span><i aria-hidden="true" />{t('tray.skillPickerImmediate')}</span>
          <div>
            <button type="button" className="skill-pack-picker__manage" onClick={() => void openManager()}>
              {t('tray.skillPickerManage')}
            </button>
            <button type="button" className="skill-pack-picker__done" onClick={() => void closePicker()}>
              {t('tray.skillPickerDone')}
            </button>
          </div>
        </footer>
      </section>
    </main>
  )
}
