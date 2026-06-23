import { useState, useEffect, useCallback, useRef } from 'react'
import type { DownloadEvent, Update } from '@tauri-apps/plugin-updater'
import { getCurrentAppVersion, isHomebrewInstall, isTauri, restartApp } from '../services/tauriApi'
import { useConfigStore } from '../stores/configStore'
import { useSessionStore } from '../stores/sessionStore'
import { blockingBackgroundSessionCount } from '../utils/energyPolicy'
import { isWindowsPlatform } from '../utils/platform'

export type UpdateStatus = 'idle' | 'checking' | 'available' | 'downloading' | 'ready' | 'error' | 'up-to-date'
export type UpdateInstallChannel = 'direct' | 'homebrew'

const RELEASE_API_URL = 'https://api.github.com/repos/shirenchuang/agentbro/releases/latest'
const LATEST_DMG_URL = 'https://github.com/shirenchuang/agentbro/releases/latest/download/AgentBro_latest_universal.dmg'
const LATEST_WINDOWS_SETUP_URL = 'https://github.com/shirenchuang/agentbro/releases/latest/download/AgentBro_latest_x64-setup.exe'
const UPDATE_CHECK_TIMEOUT_MS = 8_000
const SETTINGS_AUTO_CHECK_DELAY_MS = 5_000
const BACKGROUND_AUTO_CHECK_DELAY_MS = 60_000
const BACKGROUND_AUTO_CHECK_INTERVAL_MS = 12 * 60 * 60 * 1000
const AUTO_RESTART_IDLE_GRACE_MS = 2 * 60 * 1000
export const HOMEBREW_UPDATE_COMMAND = 'brew upgrade --cask agentbro'

interface UpdateState {
  status: UpdateStatus
  installChannel: UpdateInstallChannel
  version: string | null
  notes: string | null
  date: string | null
  error: string | null
  manualDownloadUrl: string | null
  downloadProgress: {
    downloaded: number
    total: number | null
    percent: number | null
  } | null
  restartPending: boolean
  restartBlockedByActivity: boolean
  blockingSessionCount: number
}

interface UseUpdaterOptions {
  background?: boolean
}

export function useUpdater(options: UseUpdaterOptions = {}) {
  const { background = false } = options
  const updateRef = useRef<Update | null>(null)
  const manualDownloadUrlRef = useRef<string | null>(null)
  const installChannelRef = useRef<UpdateInstallChannel>('direct')
  const autoCheckUpdate = useConfigStore((s) => s.autoCheckUpdate)
  const autoInstallUpdate = useConfigStore((s) => s.autoInstallUpdate)
  const blockingSessionCount = useSessionStore((s) => blockingBackgroundSessionCount(s.sessionList))
  const hasBlockingUpdateActivity = blockingSessionCount > 0
  const autoInstallTriggeredRef = useRef(false)
  const hasBlockingUpdateActivityRef = useRef(false)
  const statusRef = useRef<UpdateStatus>('idle')
  const [state, setState] = useState<UpdateState>({
    status: 'idle',
    installChannel: 'direct',
    version: null,
    notes: null,
    date: null,
    error: null,
    manualDownloadUrl: null,
    downloadProgress: null,
    restartPending: false,
    restartBlockedByActivity: false,
    blockingSessionCount: 0,
  })

  const checkForUpdate = useCallback(async () => {
    if (!isTauri()) return

    setState(prev => ({ ...prev, status: 'checking', error: null }))

    try {
      // Fire the install-channel probe and the network check concurrently so the
      // homebrew round-trip doesn't add latency before the GitHub request starts.
      const homebrewPromise = isHomebrewInstall()
      const homebrewReleasePromise = homebrewPromise.then((isHomebrew) =>
        isHomebrew ? checkGitHubLatestRelease() : null,
      )

      const homebrewInstall = await homebrewPromise
      if (homebrewInstall) {
        const fallback = (await homebrewReleasePromise)!
        updateRef.current = null
        manualDownloadUrlRef.current = null
        installChannelRef.current = 'homebrew'
        autoInstallTriggeredRef.current = false

        if (fallback.available) {
          setState({
            status: 'available',
            installChannel: 'homebrew',
            version: fallback.version,
            notes: fallback.notes,
            date: fallback.date,
            error: null,
            manualDownloadUrl: null,
            downloadProgress: null,
            restartPending: false,
            restartBlockedByActivity: false,
            blockingSessionCount: 0,
          })
        } else {
          setState(createEmptyState('up-to-date', 'homebrew'))
        }
        return
      }

      installChannelRef.current = 'direct'
      const { check } = await import('@tauri-apps/plugin-updater')
      const update = await check({
        timeout: UPDATE_CHECK_TIMEOUT_MS,
        headers: { 'X-Update-Channel': 'stable' },
      })

      if (update) {
        updateRef.current = update
        manualDownloadUrlRef.current = null
        installChannelRef.current = 'direct'
        autoInstallTriggeredRef.current = false
        setState({
          status: 'available',
          installChannel: 'direct',
          version: update.version,
          notes: update.body ?? null,
          date: update.date ?? null,
          error: null,
          manualDownloadUrl: null,
          downloadProgress: null,
          restartPending: false,
          restartBlockedByActivity: false,
          blockingSessionCount: 0,
        })
      } else {
        updateRef.current = null
        manualDownloadUrlRef.current = null
        installChannelRef.current = 'direct'
        autoInstallTriggeredRef.current = false
        setState(createEmptyState('up-to-date', 'direct'))
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e)

      try {
        const fallback = await checkGitHubLatestRelease()
        if (fallback.available) {
          console.warn('[updater] direct check failed; using GitHub release fallback:', message)
          updateRef.current = null
          manualDownloadUrlRef.current = fallback.downloadUrl
          installChannelRef.current = 'direct'
          autoInstallTriggeredRef.current = false
          setState({
            status: 'available',
            installChannel: 'direct',
            version: fallback.version,
            notes: fallback.notes,
            date: fallback.date,
            error: null,
            manualDownloadUrl: fallback.downloadUrl,
            downloadProgress: null,
            restartPending: false,
            restartBlockedByActivity: false,
            blockingSessionCount: 0,
          })
          return
        }

        updateRef.current = null
        manualDownloadUrlRef.current = null
        installChannelRef.current = 'direct'
        autoInstallTriggeredRef.current = false
        setState(createEmptyState('up-to-date', 'direct'))
      } catch (fallbackError) {
        const fallbackMessage = fallbackError instanceof Error ? fallbackError.message : String(fallbackError)
        console.error('[updater] check failed:', message)
        console.error('[updater] fallback check failed:', fallbackMessage)
        setState(createEmptyState('error', 'direct'))
      }
    }
  }, [])

  const installUpdate = useCallback(async (installOptions?: { automatic?: boolean }) => {
    const automatic = installOptions?.automatic ?? false

    if (state.status === 'ready') {
      if (automatic && hasBlockingUpdateActivityRef.current) {
        setState(prev => ({
          ...prev,
          restartPending: true,
          restartBlockedByActivity: true,
          blockingSessionCount,
        }))
        return
      }
      await restartApp()
      return
    }

    if (installChannelRef.current === 'homebrew') {
      await copyText(HOMEBREW_UPDATE_COMMAND)
      return
    }

    const update = updateRef.current
    if (!update) {
      const manualDownloadUrl = manualDownloadUrlRef.current
      if (!manualDownloadUrl) return
      const { open } = await import('@tauri-apps/plugin-shell')
      await open(manualDownloadUrl)
      return
    }

    let downloaded = 0
    let total: number | null = null
    const onDownloadEvent = (event: DownloadEvent) => {
      if (event.event === 'Started') {
        downloaded = 0
        total = event.data.contentLength ?? null
      } else if (event.event === 'Progress') {
        downloaded += event.data.chunkLength
      } else if (event.event === 'Finished') {
        downloaded = total ?? downloaded
      }

      const percent = total && total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : null
      setState(prev => ({
        ...prev,
        status: 'downloading',
        restartPending: false,
        restartBlockedByActivity: false,
        downloadProgress: { downloaded, total, percent },
      }))
    }

    setState(prev => ({
      ...prev,
      status: 'downloading',
      error: null,
      restartPending: false,
      restartBlockedByActivity: false,
      downloadProgress: { downloaded: 0, total: null, percent: null },
    }))
    try {
      await update.downloadAndInstall(onDownloadEvent)
      setState(prev => ({
        ...prev,
        status: 'ready',
        downloadProgress: { downloaded, total, percent: 100 },
        restartPending: automatic,
        restartBlockedByActivity: automatic && hasBlockingUpdateActivityRef.current,
        blockingSessionCount: automatic ? blockingSessionCount : 0,
      }))
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e)
      console.error('[updater] install failed:', message)
      setState(prev => ({ ...prev, status: 'available', error: message, restartPending: false, restartBlockedByActivity: false, downloadProgress: null }))
    }
  }, [blockingSessionCount, state.status])

  const dismissUpdate = useCallback(() => {
    updateRef.current = null
    manualDownloadUrlRef.current = null
    installChannelRef.current = 'direct'
    autoInstallTriggeredRef.current = false
    setState(createEmptyState('idle', 'direct'))
  }, [])

  useEffect(() => {
    hasBlockingUpdateActivityRef.current = hasBlockingUpdateActivity
    setState(prev => {
      if (!prev.restartPending) return prev
      if (
        prev.restartBlockedByActivity === hasBlockingUpdateActivity
        && prev.blockingSessionCount === blockingSessionCount
      ) return prev
      return {
        ...prev,
        restartBlockedByActivity: hasBlockingUpdateActivity,
        blockingSessionCount,
      }
    })
  }, [blockingSessionCount, hasBlockingUpdateActivity])

  useEffect(() => {
    statusRef.current = state.status
  }, [state.status])

  useEffect(() => {
    if (!isTauri()) return
    if (!autoCheckUpdate) return

    const runCheck = () => {
      if (background && isUpdateFlowActive(statusRef.current)) return
      // The read-only check runs even while sessions are busy so a new version
      // surfaces promptly. Auto-download/install stays gated on idle separately.
      checkForUpdate()
    }

    const timer = setTimeout(() => {
      runCheck()
    }, background ? BACKGROUND_AUTO_CHECK_DELAY_MS : SETTINGS_AUTO_CHECK_DELAY_MS)

    const interval = background
      ? setInterval(() => {
          runCheck()
        }, BACKGROUND_AUTO_CHECK_INTERVAL_MS)
      : null

    return () => {
      clearTimeout(timer)
      if (interval) clearInterval(interval)
    }
  }, [autoCheckUpdate, background, checkForUpdate])

  useEffect(() => {
    if (state.status !== 'available') return
    if (!autoInstallUpdate) return
    if (autoInstallTriggeredRef.current) return
    if (!updateRef.current) return
    if (background && hasBlockingUpdateActivity) return
    autoInstallTriggeredRef.current = true
    installUpdate({ automatic: true })
  }, [background, hasBlockingUpdateActivity, state.status, autoInstallUpdate, installUpdate])

  useEffect(() => {
    if (state.status !== 'ready') return
    if (!state.restartPending) return
    if (!autoInstallUpdate) return
    if (hasBlockingUpdateActivity) return

    const timer = setTimeout(() => {
      if (!hasBlockingUpdateActivityRef.current) {
        restartApp().catch((error) => {
          console.error('[updater] automatic restart failed:', error)
          setState(prev => ({
            ...prev,
            error: error instanceof Error ? error.message : String(error),
            restartPending: false,
            restartBlockedByActivity: false,
          }))
        })
      }
    }, AUTO_RESTART_IDLE_GRACE_MS)

    return () => clearTimeout(timer)
  }, [autoInstallUpdate, hasBlockingUpdateActivity, state.restartPending, state.status])

  return { ...state, checkForUpdate, installUpdate, dismissUpdate }
}

function createEmptyState(status: UpdateStatus, installChannel: UpdateInstallChannel): UpdateState {
  return {
    status,
    installChannel,
    version: null,
    notes: null,
    date: null,
    error: null,
    manualDownloadUrl: null,
    downloadProgress: null,
    restartPending: false,
    restartBlockedByActivity: false,
    blockingSessionCount: 0,
  }
}

function isUpdateFlowActive(status: UpdateStatus): boolean {
  return status === 'checking'
    || status === 'available'
    || status === 'downloading'
    || status === 'ready'
}

interface GitHubRelease {
  tag_name: string
  body: string | null
  published_at: string | null
  assets: Array<{
    name: string
    browser_download_url: string
  }>
}

type UpdateDownloadPlatform = 'macos' | 'windows'

async function checkGitHubLatestRelease(): Promise<{
  available: boolean
  version: string | null
  notes: string | null
  date: string | null
  downloadUrl: string | null
}> {
  const controller = new AbortController()
  const timeoutId = setTimeout(() => controller.abort(), UPDATE_CHECK_TIMEOUT_MS)
  let response: Response
  try {
    response = await fetch(RELEASE_API_URL, {
      headers: {
        Accept: 'application/vnd.github+json',
      },
      signal: controller.signal,
    })
  } finally {
    clearTimeout(timeoutId)
  }

  if (!response.ok) {
    throw new Error(`GitHub release API returned ${response.status}`)
  }

  const release = await response.json() as GitHubRelease
  const latestVersion = release.tag_name.replace(/^v/, '')
  const currentVersion = await getCurrentAppVersion()
  const downloadPlatform = getUpdateDownloadPlatform()
  const downloadAsset = selectReleaseDownloadAsset(release.assets, downloadPlatform)

  return {
    available: compareVersions(latestVersion, currentVersion) > 0,
    version: latestVersion,
    notes: release.body,
    date: release.published_at,
    downloadUrl: downloadAsset?.browser_download_url ?? fallbackDownloadUrl(downloadPlatform),
  }
}

function getUpdateDownloadPlatform(): UpdateDownloadPlatform {
  return isWindowsPlatform() ? 'windows' : 'macos'
}

export function selectReleaseDownloadAsset(
  assets: GitHubRelease['assets'],
  platform: UpdateDownloadPlatform,
): GitHubRelease['assets'][number] | undefined {
  if (platform === 'windows') {
    return assets.find((asset) => asset.name === 'AgentBro_latest_x64-setup.exe')
      ?? assets.find((asset) => asset.name === 'AgentBro_latest_x64.msi')
      ?? assets.find((asset) => asset.name.endsWith('_x64-setup.exe'))
      ?? assets.find((asset) => asset.name.endsWith('_x64.msi'))
  }

  return assets.find((asset) => asset.name === 'AgentBro_latest_universal.dmg')
    ?? assets.find((asset) => asset.name.endsWith('_universal.dmg'))
}

function fallbackDownloadUrl(platform: UpdateDownloadPlatform): string {
  return platform === 'windows' ? LATEST_WINDOWS_SETUP_URL : LATEST_DMG_URL
}

function compareVersions(left: string, right: string): number {
  const leftParts = normalizeVersion(left)
  const rightParts = normalizeVersion(right)
  const length = Math.max(leftParts.length, rightParts.length)

  for (let index = 0; index < length; index += 1) {
    const leftPart = leftParts[index] ?? 0
    const rightPart = rightParts[index] ?? 0
    if (leftPart !== rightPart) return leftPart > rightPart ? 1 : -1
  }

  return 0
}

function normalizeVersion(version: string): number[] {
  return version
    .replace(/^v/, '')
    .split('-')[0]
    .split('.')
    .map((part) => Number.parseInt(part, 10))
    .map((part) => (Number.isFinite(part) ? part : 0))
}

async function copyText(text: string): Promise<void> {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text)
    return
  }

  const textarea = document.createElement('textarea')
  textarea.value = text
  textarea.setAttribute('readonly', 'true')
  textarea.style.position = 'fixed'
  textarea.style.opacity = '0'
  document.body.appendChild(textarea)
  textarea.select()
  document.execCommand('copy')
  document.body.removeChild(textarea)
}
