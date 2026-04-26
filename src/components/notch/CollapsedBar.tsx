/* Collapsed Bar — Pill-shaped header with pixel art, info, and controls */
import { useTranslation } from 'react-i18next'
import type { PanelState, RateLimitInfo, SessionState } from '../../types/agent'
import { computePriority, PRIORITY } from '../../types/priority'
import { PixelIndicator } from './PixelIndicator'
import { RateLimitBar } from './RateLimitBar'
import { useTick } from '../../hooks/useTick'
import { isTauri } from '../../services/tauriApi'
import './CollapsedBar.css'

interface CollapsedBarProps {
  sessions: SessionState[]
  panelState: PanelState
  rateLimits?: RateLimitInfo
  onCollapse: () => void
}

function getLeadSession(sessions: SessionState[]): SessionState | undefined {
  return [...sessions].sort((a, b) => computePriority(b) - computePriority(a))[0]
}

function getSessionInfo(session: SessionState | undefined, t: (key: string, opts?: Record<string, string>) => string): string {
  if (!session) return t('notch.waitingForSessions')
  const parts: string[] = [session.project]
  if (session.sessionTitle) parts.push(session.sessionTitle)
  else if (session.phase === 'processing' && session.description) parts.push(session.description.split('\n')[0])
  else if (session.phase === 'waiting_approval') parts.push(t('notch.needsApproval'))
  return parts.join(' \u00B7 ')
}

export function CollapsedBar({ sessions, panelState, rateLimits, onCollapse }: CollapsedBarProps) {
  const { t } = useTranslation()
  useTick(1000, sessions.length > 0)
  const lead = getLeadSession(sessions)
  const info = getSessionInfo(lead, t)
  const count = sessions.length
  const isExpanded = panelState !== 'collapsed'
  const alertCount = sessions.filter(s => computePriority(s) === PRIORITY.attention).length
  const leadPriority = lead ? computePriority(lead) : PRIORITY.dormant

  return (
    <div className={`collapsed-bar ${isExpanded ? 'collapsed-bar--expanded' : ''}`} onClick={panelState === 'expanded' ? onCollapse : undefined}>
      {/* Top row: rate limits (left) + icons (right) — only in expanded */}
      {isExpanded && (
        <div className="collapsed-bar__status-row">
          <div className="collapsed-bar__rate-limits">
            <RateLimitBar rateLimits={rateLimits} />
          </div>
          <div className="collapsed-bar__icons">
            <button
              className="collapsed-bar__icon-btn"
              title="Toggle Sound"
              onClick={async (e) => {
                e.stopPropagation()
                const { useConfigStore } = await import('../../stores/configStore')
                const config = useConfigStore.getState()
                const newVal = !config.soundEnabled
                config.updateConfig('soundEnabled', newVal)
                try {
                  const { setSoundEnabled } = await import('../../services/tauriApi')
                  setSoundEnabled(newVal)
                } catch {}
              }}
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none">
                <path d="M11 5L6 9H2v6h4l5 4V5z" fill="currentColor" opacity="0.8"/>
                <path d="M15.54 8.46a5 5 0 010 7.07M19.07 4.93a10 10 0 010 14.14" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" fill="none"/>
              </svg>
            </button>
            <button
              className="collapsed-bar__icon-btn"
              title={t('notch.settings')}
              onClick={async (e) => {
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
              }}
            >
              <svg width="14" height="14" viewBox="0 0 20 20" fill="none">
                <path d="M10 12.5a2.5 2.5 0 100-5 2.5 2.5 0 000 5z" fill="currentColor"/>
                <path fillRule="evenodd" clipRule="evenodd" d="M8.5 1.5A1.5 1.5 0 007 3v.34a1.1 1.1 0 01-.65.99l-.12.05a1.1 1.1 0 01-1.18-.16l-.24-.2a1.5 1.5 0 00-2.12.13l-.7.77a1.5 1.5 0 00.12 2.12l.2.18c.37.34.5.86.34 1.34l-.04.12a1.1 1.1 0 01-1.04.72H1.5A1.5 1.5 0 000 10.5v1A1.5 1.5 0 001.5 13h.07a1.1 1.1 0 011.04.72l.04.12c.16.48.03 1-.34 1.34l-.2.18a1.5 1.5 0 00-.12 2.12l.7.77a1.5 1.5 0 002.12.13l.24-.2a1.1 1.1 0 011.18-.16l.12.05c.39.18.65.57.65.99V19.5A1.5 1.5 0 008.5 21h1a1.5 1.5 0 001.5-1.5v-.34a1.1 1.1 0 01.65-.99l.12-.05a1.1 1.1 0 011.18.16l.24.2a1.5 1.5 0 002.12-.13l.7-.77a1.5 1.5 0 00-.12-2.12l-.2-.18a1.1 1.1 0 01-.34-1.34l.04-.12a1.1 1.1 0 011.04-.72h.07A1.5 1.5 0 0020 11.5v-1a1.5 1.5 0 00-1.5-1.5h-.07a1.1 1.1 0 01-1.04-.72l-.04-.12a1.1 1.1 0 01.34-1.34l.2-.18a1.5 1.5 0 00.12-2.12l-.7-.77a1.5 1.5 0 00-2.12-.13l-.24.2a1.1 1.1 0 01-1.18.16l-.12-.05A1.1 1.1 0 0111 3.34V3a1.5 1.5 0 00-1.5-1.5h-1zM10 14a4 4 0 100-8 4 4 0 000 8z" fill="currentColor"/>
              </svg>
            </button>
          </div>
        </div>
      )}

      {/* Main row: pixel art + info + count */}
      <div className="collapsed-bar__main">
        <div className="collapsed-bar__left">
          {lead ? (
            <>
              <PixelIndicator priority={leadPriority} size={14} />
              <span className="collapsed-bar__info">{info}</span>
            </>
          ) : (
            <span className="collapsed-bar__info collapsed-bar__info--empty">
              {t('notch.waitingForSessions')}
            </span>
          )}
        </div>

        <div className="collapsed-bar__right">
          {alertCount > 0 && (
            <span className="collapsed-bar__alert-badge">{alertCount}</span>
          )}
          {count > 0 && (
            <span className="collapsed-bar__count">{count}</span>
          )}
          {/* Settings gear only in collapsed state */}
          {!isExpanded && (
            <button
              className="collapsed-bar__icon-btn"
              title={t('notch.settings')}
              onClick={async (e) => {
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
              }}
            >
              <svg width="12" height="12" viewBox="0 0 20 20" fill="none">
                <path d="M10 12.5a2.5 2.5 0 100-5 2.5 2.5 0 000 5z" fill="currentColor"/>
                <path fillRule="evenodd" clipRule="evenodd" d="M8.5 1.5A1.5 1.5 0 007 3v.34a1.1 1.1 0 01-.65.99l-.12.05a1.1 1.1 0 01-1.18-.16l-.24-.2a1.5 1.5 0 00-2.12.13l-.7.77a1.5 1.5 0 00.12 2.12l.2.18c.37.34.5.86.34 1.34l-.04.12a1.1 1.1 0 01-1.04.72H1.5A1.5 1.5 0 000 10.5v1A1.5 1.5 0 001.5 13h.07a1.1 1.1 0 011.04.72l.04.12c.16.48.03 1-.34 1.34l-.2.18a1.5 1.5 0 00-.12 2.12l.7.77a1.5 1.5 0 002.12.13l.24-.2a1.1 1.1 0 011.18-.16l.12.05c.39.18.65.57.65.99V19.5A1.5 1.5 0 008.5 21h1a1.5 1.5 0 001.5-1.5v-.34a1.1 1.1 0 01.65-.99l.12-.05a1.1 1.1 0 011.18.16l.24.2a1.5 1.5 0 002.12-.13l.7-.77a1.5 1.5 0 00-.12-2.12l-.2-.18a1.1 1.1 0 01-.34-1.34l.04-.12a1.1 1.1 0 011.04-.72h.07A1.5 1.5 0 0020 11.5v-1a1.5 1.5 0 00-1.5-1.5h-.07a1.1 1.1 0 01-1.04-.72l-.04-.12a1.1 1.1 0 01.34-1.34l.2-.18a1.5 1.5 0 00.12-2.12l-.7-.77a1.5 1.5 0 00-2.12-.13l-.24.2a1.1 1.1 0 01-1.18.16l-.12-.05A1.1 1.1 0 0111 3.34V3a1.5 1.5 0 00-1.5-1.5h-1zM10 14a4 4 0 100-8 4 4 0 000 8z" fill="currentColor"/>
              </svg>
            </button>
          )}
        </div>
      </div>
    </div>
  )
}
