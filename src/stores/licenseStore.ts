/* AgentBro — License State Management (Zustand) */
import { create } from 'zustand'
import {
  getLicenseStatus,
  activateLicense as apiActivateLicense,
  deactivateLicense as apiDeactivateLicense,
  type BackendLicenseStatus,
} from '../services/tauriApi'

type LicenseStatus = 'active' | 'trial' | 'trial_expired' | 'invalid'

interface LicenseState {
  status: LicenseStatus
  daysRemaining: number
  licenseKey: string
  trialBannerDismissed: boolean
}

interface LicenseActions {
  checkLicense: () => Promise<void>
  activateLicense: (key: string) => Promise<void>
  deactivateLicense: () => Promise<void>
  dismissTrialBanner: () => void
  isFeatureEnabled: () => boolean
}

type LicenseStore = LicenseState & LicenseActions

/** Map backend tagged-union status to flat store fields. */
function applyBackendStatus(result: BackendLicenseStatus): Partial<LicenseState> {
  switch (result.type) {
    case 'active':
      return { status: 'active', licenseKey: result.licenseKey, daysRemaining: 365 }
    case 'trial':
      return { status: 'trial', licenseKey: '', daysRemaining: result.daysRemaining }
    case 'trialExpired':
      return { status: 'trial_expired', licenseKey: '', daysRemaining: 0 }
    case 'invalid':
      return { status: 'invalid', licenseKey: '', daysRemaining: 0 }
    case 'offlineGrace':
      return { status: 'active', licenseKey: result.licenseKey, daysRemaining: result.daysRemaining }
  }
}

export const useLicenseStore = create<LicenseStore>((set, get) => ({
  status: 'trial',
  daysRemaining: 14,
  licenseKey: '',
  trialBannerDismissed: false,

  checkLicense: async () => {
    try {
      const result = await getLicenseStatus()
      set(applyBackendStatus(result))
    } catch (e) {
      console.warn('[license] checkLicense failed:', e)
    }
  },

  activateLicense: async (key) => {
    try {
      const result = await apiActivateLicense(key)
      set(applyBackendStatus(result))
    } catch (e) {
      console.warn('[license] activateLicense failed:', e)
      set({ status: 'invalid' })
    }
  },

  deactivateLicense: async () => {
    try {
      const result = await apiDeactivateLicense()
      set(applyBackendStatus(result))
    } catch (e) {
      console.warn('[license] deactivateLicense failed:', e)
    }
  },

  dismissTrialBanner: () => {
    set({ trialBannerDismissed: true })
  },

  isFeatureEnabled: () => {
    const { status } = get()
    return status === 'active' || status === 'trial'
  },
}))

// Selectors
export const selectLicenseStatus = (s: LicenseStore) => s.status
export const selectDaysRemaining = (s: LicenseStore) => s.daysRemaining
export const selectIsFeatureEnabled = (s: LicenseStore) => s.isFeatureEnabled()
export const selectTrialBannerVisible = (s: LicenseStore) =>
  !s.trialBannerDismissed && (s.status === 'trial' || s.status === 'trial_expired')
