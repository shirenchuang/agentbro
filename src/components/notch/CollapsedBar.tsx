/* Collapsed Bar — Pill-shaped header with pixel art, info, and controls */
import { useState, useEffect, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { AnimatePresence, motion } from 'framer-motion'
import type { PanelState, RateLimitInfo, SessionPhase, SessionState } from '../../types/agent'
import { computePriority, PRIORITY } from '../../types/priority'
import { MascotRouter } from './mascots'
import { TipDisplay } from './TipDisplay'
import { useTick } from '../../hooks/useTick'
import { isTauri, setSoundEnabled } from '../../services/tauriApi'
import { useConfigStore } from '../../stores/configStore'
import { useThemeStore } from '../../stores/themeStore'
import { sessionNeedsAttention } from '../../utils/islandInteraction'
import { getToolActivityLabel } from '../../utils/toolLabels'
import { SpriteCanvas } from './SpriteCanvas'
import './CollapsedBar.css'

interface CollapsedBarProps {
  sessions: SessionState[]
  panelState: PanelState
  rateLimits?: RateLimitInfo
  onCollapse: () => void
  isMicro?: boolean
  focusFilteredEmpty?: boolean
}

function getLeadSession(sessions: SessionState[]): SessionState | undefined {
  return [...sessions].sort((a, b) => computePriority(b) - computePriority(a))[0]
}

const PHASE_LABELS: Record<SessionPhase, string> = {
  idle: 'notch.idle',
  processing: 'notch.working',
  waiting_approval: 'notch.needsApproval',
  waiting_input: 'notch.waitingInput',
  compacting: 'notch.compactingShort',
  done: 'notch.taskComplete',
  error: 'notch.error',
  interrupted: 'notch.interrupted',
}

function getCarouselSlides(session: SessionState, t: (key: string) => string): string[] {
  const slides: string[] = [session.project]

  if (session.lastToolName) {
    const target = session.lastToolTarget ? `: ${session.lastToolTarget}` : ''
    slides.push(`${getToolActivityLabel(t, session.lastToolName)}${target}`)
  } else if (session.description) {
    slides.push(session.description.split('\n')[0])
  }

  const statusKey = PHASE_LABELS[session.phase]
  const status = statusKey ? t(statusKey) : session.phase
  if (status && status !== slides[0]) slides.push(status)

  return slides.filter(Boolean)
}

function getUnattendedLevel(unattendedSince: number | undefined): 'none' | 'amber' | 'red' {
  if (!unattendedSince) return 'none'
  const elapsed = Date.now() - unattendedSince
  if (elapsed >= 60000) return 'red'
  if (elapsed >= 30000) return 'amber'
  return 'none'
}

function formatElapsed(unattendedSince: number | undefined): string {
  if (!unattendedSince) return ''
  const secs = Math.floor((Date.now() - unattendedSince) / 1000)
  if (secs < 60) return `${secs}s`
  const m = Math.floor(secs / 60)
  const s = secs % 60
  return s > 0 ? `${m}m${s}s` : `${m}m`
}

export function CollapsedBar({ sessions, panelState, onCollapse, isMicro, focusFilteredEmpty = false }: CollapsedBarProps) {
  const { t } = useTranslation()
  const showToolStatus = useConfigStore((s) => s.showToolStatus)
  const defaultMascotSource = useConfigStore((s) => s.defaultMascotSource)
  const tipsEnabled = useConfigStore((s) => s.tipsEnabled)
  const activeTheme = useThemeStore((s) => s.activeTheme)

  const lead = getLeadSession(sessions)
  useTick(1000, Boolean(lead?.unattendedSince))
  const slides = lead ? getCarouselSlides(lead, t) : []
  const slidesCount = slides.length

  const [slideIndex, setSlideIndex] = useState(0)
  const leadId = lead?.id

  // Reset to slide 0 when the lead session changes
  useEffect(() => {
    setSlideIndex(0)
  }, [leadId])

  const safeIndex = slidesCount > 0 ? slideIndex % slidesCount : 0
  const currentSlide = slides[safeIndex] ?? ''

  const count = sessions.length
  const isExpanded = panelState !== 'collapsed'
  const alertCount = sessions.filter(s => computePriority(s) >= PRIORITY.attention).length
  const workingCount = sessions.filter(s => s.phase === 'processing' || s.phase === 'compacting').length
  const waitingCount = sessions.filter(sessionNeedsAttention).length
  const allIdle = sessions.length > 0 && sessions.every(s => computePriority(s) <= PRIORITY.idle)
  const showTips = tipsEnabled && (sessions.length === 0 || allIdle)
  const emptyText = focusFilteredEmpty ? t('notch.noSessionInFocus') : t('notch.waitingForSessions')
  const isThinking = lead?.phase === 'processing' && !lead?.lastToolName
  const isYolo = lead?.isYoloMode
  const hasError = lead?.phase === 'error'
  const ratePct = lead?.rateLimits?.fiveHourUsage
  const rateColor = ratePct != null
    ? ratePct > 80 ? '#ef4444' : ratePct > 50 ? '#f59e0b' : '#4ade80'
    : undefined

  const unattendedLevel = getUnattendedLevel(lead?.unattendedSince)
  const elapsedText = unattendedLevel !== 'none' ? formatElapsed(lead?.unattendedSince) : ''
  const renderMascot = (session: SessionState | undefined, size: number) => {
    if (activeTheme.character) {
      return (
        <span className="collapsed-bar__theme-avatar" style={{ width: size, height: size }}>
          <SpriteCanvas
            priority={session ? computePriority(session) : PRIORITY.idle}
            size={size}
            theme={activeTheme}
          />
        </span>
      )
    }

    return <MascotRouter toolType={session?.agentType || defaultMascotSource} phase={session?.phase || 'idle'} size={size} />
  }

  const settingsButtonRef = useRef<HTMLButtonElement>(null)
  async function openSettings(e: React.MouseEvent) {
    e.stopPropagation()
    if (isTauri()) {
      try {
        const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow')
        const settingsWin = await WebviewWindow.getByLabel('settings')
        if (settingsWin) {
          await settingsWin.show()
          await settingsWin.setFocus()
        }
      } catch (err) {
        console.error('[settings] Failed to open settings window:', err)
      }
    }
  }

  if (isMicro) {
    return (
      <div className="collapsed-bar collapsed-bar--micro">
        <div className="collapsed-bar__micro-main">
          {renderMascot(lead, 22)}
          <span className="collapsed-bar__micro-count">{count}</span>
        </div>
      </div>
    )
  }

  return (
    <div className={`collapsed-bar ${isExpanded ? 'collapsed-bar--expanded' : ''} ${isThinking ? 'collapsed-bar--shimmer' : ''}`} onClick={panelState === 'expanded' ? onCollapse : undefined}>
      {/* Top row: rate limits (left) + icons (right) — only in expanded */}
      {isExpanded && (
        <div className="collapsed-bar__status-row">
          <div className="collapsed-bar__left" style={{ gap: 8 }}>
            {renderMascot(lead, 20)}
            <div className="collapsed-bar__counter-pills">
              <span className={`collapsed-bar__counter-pill${count > 0 ? ' collapsed-bar__counter-pill--active' : ''}`}>
                <span>ALL</span><span className="collapsed-bar__counter-pill-val">{count}</span>
              </span>
              <span className={`collapsed-bar__counter-pill${workingCount > 0 ? ' collapsed-bar__counter-pill--active collapsed-bar__counter-pill--act' : ''}`}>
                <span>ACT</span><span className="collapsed-bar__counter-pill-val">{workingCount}</span>
              </span>
              <span className={`collapsed-bar__counter-pill${waitingCount > 0 ? ' collapsed-bar__counter-pill--active collapsed-bar__counter-pill--wait' : ''}`}>
                <span>WAIT</span><span className="collapsed-bar__counter-pill-val">{waitingCount}</span>
              </span>
            </div>
          </div>
          {showTips && !focusFilteredEmpty && (
            <div className="collapsed-bar__header-tip">
              <TipDisplay show />
            </div>
          )}
          <div className="collapsed-bar__icons">
            <span className="collapsed-bar__esc-hint">ESC</span>
            <button
              className="collapsed-bar__icon-btn"
              title="Toggle Sound"
              onClick={async (e) => {
                e.stopPropagation()
                const config = useConfigStore.getState()
                const newVal = !config.soundEnabled
                config.updateConfig('soundEnabled', newVal)
                try {
                  setSoundEnabled(newVal)
                } catch (error) {
                  console.warn('[notch] setSoundEnabled failed:', error)
                }
              }}
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none">
                <path d="M11 5L6 9H2v6h4l5 4V5z" fill="currentColor" opacity="0.8"/>
                <path d="M15.54 8.46a5 5 0 010 7.07" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" fill="none"/>
              </svg>
            </button>
            <button
              className="collapsed-bar__icon-btn"
              title={t('notch.settings')}
              onClick={openSettings}
            >
              <svg width="14" height="14" viewBox="0 0 20 20" fill="none">
                <path d="M10 12.5a2.5 2.5 0 100-5 2.5 2.5 0 000 5z" fill="currentColor"/>
                <path fillRule="evenodd" clipRule="evenodd" d="M8.5 1.5A1.5 1.5 0 007 3v.34a1.1 1.1 0 01-.65.99l-.12.05a1.1 1.1 0 01-1.18-.16l-.24-.2a1.5 1.5 0 00-2.12.13l-.7.77a1.5 1.5 0 00.12 2.12l.2.18c.37.34.5.86.34 1.34l-.04.12a1.1 1.1 0 01-1.04.72H1.5A1.5 1.5 0 000 10.5v1A1.5 1.5 0 001.5 13h.07a1.1 1.1 0 011.04.72l.04.12c.16.48.03 1-.34 1.34l-.2.18a1.5 1.5 0 00-.12 2.12l.7.77a1.5 1.5 0 002.12.13l.24-.2a1.1 1.1 0 011.18-.16l.12.05c.39.18.65.57.65.99V19.5A1.5 1.5 0 008.5 21h1a1.5 1.5 0 001.5-1.5v-.34a1.1 1.1 0 01.65-.99l.12-.05a1.1 1.1 0 011.18.16l.24.2a1.5 1.5 0 002.12-.13l.7-.77a1.5 1.5 0 00-.12-2.12l-.2-.18a1.1 1.1 0 01-.34-1.34l.04-.12a1.1 1.1 0 011.04-.72h.07A1.5 1.5 0 0020 11.5v-1a1.5 1.5 0 00-1.5-1.5h-.07a1.1 1.1 0 01-1.04-.72l-.04-.12a1.1 1.1 0 01.34-1.34l.2-.18a1.5 1.5 0 00.12-2.12l-.7-.77a1.5 1.5 0 00-2.12-.13l-.24.2a1.1 1.1 0 01-1.18.16l-.12-.05A1.1 1.1 0 0111 3.34V3a1.5 1.5 0 00-1.5-1.5h-1zM10 14a4 4 0 100-8 4 4 0 000 8z" fill="currentColor"/>
              </svg>
            </button>
          </div>
        </div>
      )}

      {/* Main row: mascot + carousel info + unattended + count */}
      {!isExpanded && (
      <div className="collapsed-bar__main">
        <div className="collapsed-bar__left">
          {lead ? (
            <>
              {renderMascot(lead, 22)}
              <div className="collapsed-bar__carousel">
                {showTips ? (
                  <TipDisplay show />
                ) : (
                  <AnimatePresence mode="wait" initial={false}>
                    <motion.span
                      key={`${leadId}-${safeIndex}`}
                      className="collapsed-bar__info"
                      initial={{ y: 8, opacity: 0 }}
                      animate={{ y: 0, opacity: 1 }}
                      exit={{ y: -8, opacity: 0 }}
                      transition={{ duration: 0.18, ease: 'easeOut' }}
                    >
                      {currentSlide}
                    </motion.span>
                  </AnimatePresence>
                )}
              </div>
            </>
          ) : (
            <>
              {renderMascot(undefined, 22)}
              {showTips && !focusFilteredEmpty ? (
                <div className="collapsed-bar__carousel">
                  <TipDisplay show />
                </div>
              ) : (
                <span className="collapsed-bar__info collapsed-bar__info--empty">
                  {emptyText}
                </span>
              )}
            </>
          )}
        </div>

        <div className="collapsed-bar__right">
          {/* Tool status display */}
          {showToolStatus && lead?.lastToolName && !isExpanded && (
            <span className="collapsed-bar__tool-status">
              {getToolActivityLabel(t, lead.lastToolName)}
              {lead.lastToolTarget ? `: ${lead.lastToolTarget}` : ''}
            </span>
          )}
          {/* Rate limit percentage */}
          {ratePct != null && !isExpanded && (
            <span className="collapsed-bar__rate-pct" style={{ color: rateColor }}>
              {ratePct}%
            </span>
          )}
          {/* Error warning badge */}
          {hasError && (
            <span className="collapsed-bar__error-badge">!</span>
          )}
          {/* YOLO mode badge */}
          {isYolo && (
            <span className="collapsed-bar__yolo-badge">YOLO</span>
          )}
          {/* Unattended timer badge */}
          {unattendedLevel !== 'none' && (
            <span className={`collapsed-bar__unattended collapsed-bar__unattended--${unattendedLevel}`}>
              {elapsedText}
            </span>
          )}
          {alertCount > 0 && (
            <span className="collapsed-bar__alert-badge">{alertCount}</span>
          )}
          {count > 0 && (
            <span className="collapsed-bar__count">{count}</span>
          )}
          {/* Settings gear only in collapsed state */}
          {!isExpanded && (
            <button
              ref={settingsButtonRef}
              className="collapsed-bar__icon-btn"
              title={t('notch.settings')}
              onClick={openSettings}
            >
              <svg width="12" height="12" viewBox="0 0 20 20" fill="none">
                <path d="M10 12.5a2.5 2.5 0 100-5 2.5 2.5 0 000 5z" fill="currentColor"/>
                <path fillRule="evenodd" clipRule="evenodd" d="M8.5 1.5A1.5 1.5 0 007 3v.34a1.1 1.1 0 01-.65.99l-.12.05a1.1 1.1 0 01-1.18-.16l-.24-.2a1.5 1.5 0 00-2.12.13l-.7.77a1.5 1.5 0 00.12 2.12l.2.18c.37.34.5.86.34 1.34l-.04.12a1.1 1.1 0 01-1.04.72H1.5A1.5 1.5 0 000 10.5v1A1.5 1.5 0 001.5 13h.07a1.1 1.1 0 011.04.72l.04.12c.16.48.03 1-.34 1.34l-.2.18a1.5 1.5 0 00-.12 2.12l.7.77a1.5 1.5 0 002.12.13l.24-.2a1.1 1.1 0 011.18-.16l.12.05c.39.18.65.57.65.99V19.5A1.5 1.5 0 008.5 21h1a1.5 1.5 0 001.5-1.5v-.34a1.1 1.1 0 01.65-.99l.12-.05a1.1 1.1 0 011.18.16l.24.2a1.5 1.5 0 002.12-.13l.7-.77a1.5 1.5 0 00-.12-2.12l-.2-.18a1.1 1.1 0 01-.34-1.34l.04-.12a1.1 1.1 0 011.04-.72h.07A1.5 1.5 0 0020 11.5v-1a1.5 1.5 0 00-1.5-1.5h-.07a1.1 1.1 0 01-1.04-.72l-.04-.12a1.1 1.1 0 01.34-1.34l.2-.18a1.5 1.5 0 00.12-2.12l-.7-.77a1.5 1.5 0 00-2.12-.13l-.24.2a1.1 1.1 0 01-1.18.16l-.12-.05A1.1 1.1 0 0111 3.34V3a1.5 1.5 0 00-1.5-1.5h-1zM10 14a4 4 0 100-8 4 4 0 000 8z" fill="currentColor"/>
              </svg>
            </button>
          )}
        </div>
      </div>
      )}
    </div>
  )
}
