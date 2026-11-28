import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { open } from '@tauri-apps/plugin-shell'
import { save } from '@tauri-apps/plugin-dialog'
import { exportDiagnostics, getCurrentAppVersion } from '../../../services/tauriApi'
import { HOMEBREW_UPDATE_COMMAND } from '../../../hooks/useUpdater'
import type { UpdateInstallChannel, UpdateStatus } from '../../../hooks/useUpdater'
import { useConfigStore } from '../../../stores/configStore'
import { SettingSection } from '../SettingSection'
import { SettingGroup } from '../SettingGroup'
import { SettingRow } from '../SettingRow'
import { Toggle } from '../Toggle'
import { GlassButton } from '../../shared'

interface AboutSectionProps {
  updateStatus?: UpdateStatus
  updateInstallChannel?: UpdateInstallChannel
  updateVersion?: string | null
  updateError?: string | null
  updateRestartPending?: boolean
  updateRestartBlockedByActivity?: boolean
  updateBlockingSessionCount?: number
  onCheckForUpdate?: () => void
}

const DEVELOPER_GITHUB_URL = 'https://github.com/shirenchuang'
const REPO_ISSUES_URL = 'https://github.com/shirenchuang/agentbro/issues/new'
const REPO_RELEASES_URL = 'https://github.com/shirenchuang/agentbro/releases'
const WEBSITE_URL = 'https://www.agentbro.net'

function RowIcon({ tone, children }: { tone: string; children: React.ReactNode }) {
  return (
    <span className="about-row-icon" style={{ background: tone }} aria-hidden="true">
      {children}
    </span>
  )
}

function ExternalArrow() {
  return (
    <svg aria-hidden="true" className="about-row-arrow" viewBox="0 0 16 16" fill="none">
      <path d="M5 11l6-6M6 5h5v5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

export function AboutSection({
  updateStatus,
  updateInstallChannel = 'direct',
  updateVersion,
  updateError,
  updateRestartPending = false,
  updateRestartBlockedByActivity = false,
  updateBlockingSessionCount = 0,
  onCheckForUpdate,
}: AboutSectionProps) {
  const { t } = useTranslation()
  const [diagStatus, setDiagStatus] = useState<'idle' | 'saving' | 'saved' | 'error'>('idle')
  const [communityOpen, setCommunityOpen] = useState(false)
  const [appVersion, setAppVersion] = useState<string>('...')
  const communityBtnRef = useRef<HTMLButtonElement>(null)
  const communityPopoverRef = useRef<HTMLDivElement>(null)
  const [popoverPos, setPopoverPos] = useState<{ top: number; left: number } | null>(null)

  const autoCheckUpdate = useConfigStore((s) => s.autoCheckUpdate)
  const autoInstallUpdate = useConfigStore((s) => s.autoInstallUpdate)
  const updateConfig = useConfigStore((s) => s.updateConfig)

  const updatePopoverPos = useCallback(() => {
    const btn = communityBtnRef.current
    if (!btn) return
    const rect = btn.getBoundingClientRect()
    const popoverWidth = 380
    const popoverHeight = 320
    const margin = 12
    const anchorCenterX = rect.left + rect.width / 2
    let left = anchorCenterX - popoverWidth / 2
    left = Math.max(margin, Math.min(left, window.innerWidth - popoverWidth - margin))
    let top = rect.bottom + 8
    if (top + popoverHeight + margin > window.innerHeight) {
      top = Math.max(margin, rect.top - popoverHeight - 8)
    }
    setPopoverPos({ top, left })
  }, [])

  useEffect(() => {
    let cancelled = false
    getCurrentAppVersion()
      .then((version) => {
        if (!cancelled) setAppVersion(version)
      })
      .catch(() => {
        if (!cancelled) setAppVersion('dev')
      })
    return () => {
      cancelled = true
    }
  }, [])

  useEffect(() => {
    if (!communityOpen) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setCommunityOpen(false)
    }
    const onScroll = () => updatePopoverPos()
    window.addEventListener('keydown', onKey)
    window.addEventListener('resize', onScroll)
    window.addEventListener('scroll', onScroll, true)
    return () => {
      window.removeEventListener('keydown', onKey)
      window.removeEventListener('resize', onScroll)
      window.removeEventListener('scroll', onScroll, true)
    }
  }, [communityOpen, updatePopoverPos])

  useLayoutEffect(() => {
    if (communityOpen) updatePopoverPos()
  }, [communityOpen, updatePopoverPos])

  const openExternalLink = (url: string) => {
    open(url).catch((err) => console.warn('[AboutSection] open link:', err))
  }

  const handleExportDiagnostics = async () => {
    setDiagStatus('saving')
    try {
      const now = new Date()
      const pad = (n: number) => String(n).padStart(2, '0')
      const defaultName = `AgentBro-Diagnostics-${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}-${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}.zip`
      const targetPath = await save({
        defaultPath: defaultName,
        filters: [{ name: 'ZIP Archive', extensions: ['zip'] }],
      })
      if (!targetPath) {
        setDiagStatus('idle')
        return
      }
      await exportDiagnostics(targetPath)
      setDiagStatus('saved')
      setTimeout(() => setDiagStatus('idle'), 2000)
    } catch {
      setDiagStatus('error')
      setTimeout(() => setDiagStatus('idle'), 2000)
    }
  }

  const updateDescription = (() => {
    switch (updateStatus) {
      case 'checking': return t('settings.updateChecking', { defaultValue: '正在检查更新...' })
      case 'installer-pending': return t('settings.updateInstallerPending', { version: updateVersion })
      case 'available':
        return updateInstallChannel === 'homebrew'
          ? t('settings.updateAvailableHomebrew', { version: updateVersion, command: HOMEBREW_UPDATE_COMMAND, defaultValue: `发现新版本 ${updateVersion}。请运行 ${HOMEBREW_UPDATE_COMMAND}` })
          : t('settings.updateAvailable', { version: updateVersion, defaultValue: `发现新版本 ${updateVersion}` })
      case 'downloading': return t('settings.updateDownloading', { version: updateVersion, defaultValue: `正在下载 ${updateVersion}...` })
      case 'ready':
        if (updateRestartBlockedByActivity) {
          return t('settings.updateReadyWaitingIdle', {
            version: updateVersion,
            count: updateBlockingSessionCount,
            defaultValue: `新版本 ${updateVersion} 已就绪，会在当前会话空闲后自动重启安装`,
          })
        }
        if (updateRestartPending) {
          return t('settings.updateReadyAuto', {
            version: updateVersion,
            defaultValue: `新版本 ${updateVersion} 已就绪，会在短暂空闲后自动重启安装`,
          })
        }
        return t('settings.updateReady', { version: updateVersion, defaultValue: `新版本 ${updateVersion} 已就绪，重启后生效` })
      case 'error': return updateError ?? t('settings.updateCheckFailed', { defaultValue: '无法连接更新服务，请稍后重试，或通过“发布版本”手动下载最新版。' })
      case 'up-to-date': return t('settings.latestVersion')
      default: return t('settings.latestVersion')
    }
  })()

  return (
    <SettingSection title={t('settings.aboutTitle')}>
      <div className="about-header">
        <img className="about-header__icon" src="/agentbro-logo.png" alt="" aria-hidden="true" />
        <div className="about-header__name">AgentBro</div>
        <div className="about-header__slogan">{t('notch.slogan')}</div>
        <div className="about-header__version">Version {appVersion}</div>
      </div>

      <SettingGroup>
        <SettingRow label={t('settings.checkForUpdates')} description={updateDescription}>
          <GlassButton
            variant="secondary"
            onClick={() => onCheckForUpdate?.()}
            disabled={updateStatus === 'checking' || updateStatus === 'downloading'}
          >
            {updateStatus === 'checking' || updateStatus === 'downloading' ? '...' : t('settings.checkNow')}
          </GlassButton>
        </SettingRow>
        <SettingRow
          label={t('settings.autoCheckUpdate')}
          description={t('settings.autoCheckUpdateDesc')}
        >
          <Toggle checked={autoCheckUpdate} onChange={(v) => updateConfig('autoCheckUpdate', v)} />
        </SettingRow>
        <SettingRow
          label={t('settings.autoInstallUpdate')}
          description={t('settings.autoInstallUpdateDesc')}
        >
          <Toggle checked={autoInstallUpdate} onChange={(v) => updateConfig('autoInstallUpdate', v)} />
        </SettingRow>
      </SettingGroup>

      <SettingGroup>
        <button type="button" className="about-link-row" onClick={() => openExternalLink(WEBSITE_URL)}>
          <RowIcon tone="#34C759">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth="2">
              <circle cx="12" cy="12" r="9" />
              <path d="M3.6 9h16.8M3.6 15h16.8M12 3c2.1 2.35 3.15 5.35 3.15 9S14.1 18.65 12 21M12 3C9.9 5.35 8.85 8.35 8.85 12S9.9 18.65 12 21" />
            </svg>
          </RowIcon>
          <span className="about-link-row__label">{t('settings.website')}</span>
          <span className="about-link-row__value">agentbro.net</span>
          <ExternalArrow />
        </button>
        <button type="button" className="about-link-row" onClick={() => openExternalLink(DEVELOPER_GITHUB_URL)}>
          <RowIcon tone="#8E8E93">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth="2">
              <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
              <circle cx="12" cy="7" r="4" />
            </svg>
          </RowIcon>
          <span className="about-link-row__label">{t('settings.developer')}</span>
          <span className="about-link-row__value">{t('settings.developerName')}</span>
          <ExternalArrow />
        </button>
        <button type="button" className="about-link-row" onClick={() => openExternalLink(REPO_RELEASES_URL)}>
          <RowIcon tone="#AF52DE">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth="2">
              <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z" />
              <path d="m3.27 6.96 8.73 5.05 8.73-5.05M12 22.08V12" />
            </svg>
          </RowIcon>
          <span className="about-link-row__label">{t('settings.releases')}</span>
          <span className="about-link-row__value">GitHub</span>
          <ExternalArrow />
        </button>
        <button ref={communityBtnRef} type="button" className="about-link-row" onClick={() => setCommunityOpen((open) => !open)}>
          <RowIcon tone="#5856D6">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth="2">
              <path d="M4 5.5A3.5 3.5 0 0 1 7.5 2h9A3.5 3.5 0 0 1 20 5.5v7A3.5 3.5 0 0 1 16.5 16H12l-4.5 4v-4A3.5 3.5 0 0 1 4 12.5v-7Z" />
            </svg>
          </RowIcon>
          <span className="about-link-row__label">{t('settings.community')}</span>
          <ExternalArrow />
        </button>
        <button type="button" className="about-link-row" onClick={() => openExternalLink(REPO_ISSUES_URL)}>
          <RowIcon tone="#FF9500">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth="2">
              <path d="M8 2v3M16 2v3M5 8h14a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-9a2 2 0 0 1 2-2Z" />
              <path d="M9 13l2 2 4-4" />
            </svg>
          </RowIcon>
          <span className="about-link-row__label">{t('settings.reportBug')}</span>
          <span className="about-link-row__value">GitHub</span>
          <ExternalArrow />
        </button>
        <button
          type="button"
          className="about-link-row"
          onClick={() => openExternalLink(`mailto:${t('settings.feedbackEmail')}`)}
        >
          <RowIcon tone="#007AFF">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth="2">
              <rect x="3" y="5" width="18" height="14" rx="2" />
              <path d="m3 7 9 6 9-6" />
            </svg>
          </RowIcon>
          <span className="about-link-row__label">{t('settings.sendFeedback')}</span>
          <span className="about-link-row__value about-link-row__value--accent">{t('settings.feedbackEmail')}</span>
          <ExternalArrow />
        </button>
      </SettingGroup>

      {communityOpen && (
        <>
          <div className="about-community-backdrop" onClick={() => setCommunityOpen(false)} />
          <div
            ref={communityPopoverRef}
            className="about-community-popover"
            role="dialog"
            aria-modal="true"
            aria-labelledby="about-community-title"
            style={popoverPos ? { top: popoverPos.top, left: popoverPos.left, visibility: 'visible' } : { visibility: 'hidden' }}
          >
            <div className="about-community-popover__header">
              <h3 id="about-community-title">{t('settings.community')}</h3>
              <button
                type="button"
                className="about-community-popover__close"
                aria-label={t('settings.communityClose', { defaultValue: '关闭' })}
                onClick={() => setCommunityOpen(false)}
              >
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
                  <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
                </svg>
              </button>
            </div>
            <div className="about-community-popover__grid">
              <div className="about-community-popover__item">
                <img src="/agentbro-wechat-qr.jpg" alt={t('settings.communityWechatAlt', { defaultValue: 'WeChat QR code' })} />
                <span className="about-community-popover__label">{t('settings.communityWechat', { defaultValue: 'WeChat' })}</span>
              </div>
              <div className="about-community-popover__item">
                <img src="/agentbro-group-qr.png" alt={t('settings.communityGroupAlt', { defaultValue: 'WeChat Group QR code' })} />
                <span className="about-community-popover__label">{t('settings.communityGroup', { defaultValue: 'WeChat Group' })}</span>
              </div>
            </div>
          </div>
        </>
      )}

      <SettingGroup label={t('settings.logoMeaning', { defaultValue: 'Logo Meaning' })}>
        <div className="about-logo-meaning">
          <img src="/agentbro-logo.png" alt="" aria-hidden="true" />
          <div>
            <strong>{t('settings.logoMeaningTitle', { defaultValue: 'A handshake between people and agents' })}</strong>
            <span>{t('settings.logoMeaningDesc', { defaultValue: 'The center handshake represents collaboration between humans and AI agents. The outer A/B shape comes from AgentBro and resembles two connected agent nodes.' })}</span>
          </div>
        </div>
      </SettingGroup>

      <SettingGroup>
        <SettingRow label={t('settings.exportDiagnostics')} description={t('settings.exportDiagnosticsDesc')}>
          <GlassButton
            variant="secondary"
            onClick={handleExportDiagnostics}
            disabled={diagStatus === 'saving'}
          >
            {diagStatus === 'saved' ? t('settings.exportSaved', { defaultValue: 'Saved!' }) : diagStatus === 'error' ? t('settings.exportFailed', { defaultValue: 'Failed' }) : t('settings.export')}
          </GlassButton>
        </SettingRow>
      </SettingGroup>

      <SettingGroup label={t('settings.credits')}>
        <div className="credits-list" style={{ padding: 'var(--space-sm) 0' }}>
          <div>{t('settings.builtWith')}</div>
          <div>{t('settings.designSystem')}</div>
          <div style={{ marginTop: 'var(--space-sm)', color: '#aeaeb2' }}>
            {t('settings.copyright')}
          </div>
        </div>
      </SettingGroup>

    </SettingSection>
  )
}
