/* Trial Banner — subtle notification for trial/expired status */
import { useTranslation } from 'react-i18next'
import { motion, AnimatePresence } from 'framer-motion'
import { useLicenseStore, selectTrialBannerVisible, selectLicenseStatus, selectDaysRemaining } from '../../stores/licenseStore'

export function TrialBanner() {
  const { t } = useTranslation()
  const visible = useLicenseStore(selectTrialBannerVisible)
  const status = useLicenseStore(selectLicenseStatus)
  const daysRemaining = useLicenseStore(selectDaysRemaining)
  const dismiss = useLicenseStore((s) => s.dismissTrialBanner)

  return (
    <AnimatePresence>
      {visible && (
        <motion.div
          className="trial-banner"
          initial={{ opacity: 0, height: 0 }}
          animate={{ opacity: 1, height: 'auto' }}
          exit={{ opacity: 0, height: 0 }}
          transition={{ duration: 0.2 }}
        >
          <span className="trial-banner__text">
            {status === 'trial_expired'
              ? t('trial.expired')
              : t('trial.daysRemaining', { days: daysRemaining })}
          </span>
          <button className="trial-banner__dismiss" onClick={dismiss} title={t('trial.dismiss')}>
            ✕
          </button>
        </motion.div>
      )}
    </AnimatePresence>
  )
}
