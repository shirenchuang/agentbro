import type { TFunction } from 'i18next'
import type { TargetClaim } from '../../services/skillApiV2'

const SOURCE_TYPE_KEYS: Record<string, string> = {
  'skills.sh': 'skills_sh',
}

const STALE_UNMANAGED_ERROR_CODE = 'SKILL_UNMANAGED_STALE'
const ADOPT_OPTION_UNAVAILABLE_PREFIX = 'Adopt option '
const ADOPT_OPTION_UNAVAILABLE_HINT = 'Re-run preview and choose one of the suggested actions.'

export function skillModeLabel(t: TFunction, mode?: string | null): string {
  if (mode === 'link') return t('skills.mode.link', { defaultValue: 'Symlink' })
  if (mode === 'copy') return t('skills.mode.copy', { defaultValue: 'Copy' })
  return mode || ''
}

export function skillSourceTypeLabel(t: TFunction, sourceType?: string | null): string {
  if (!sourceType) return ''
  return t(`skills.sourceType.${SOURCE_TYPE_KEYS[sourceType] || sourceType}`, { defaultValue: sourceType })
}

export function skillStatusLabel(t: TFunction, status?: string | null): string {
  if (!status) return ''
  return t(`skills.status.${status}`, { defaultValue: status })
}

export function targetClaimLabel(t: TFunction, claim?: Pick<TargetClaim, 'claimType' | 'packName'> | null): string {
  if (!claim) return t('skills.claim.direct', { defaultValue: 'Direct distribution' })
  if (claim.claimType === 'pack') {
    return claim.packName
      ? t('skills.claim.packNamed', { name: claim.packName, defaultValue: 'Pack: {{name}}' })
      : t('skills.claim.pack', { defaultValue: 'Pack' })
  }
  return t('skills.claim.direct', { defaultValue: 'Direct distribution' })
}

export function unmanagedReasonLabel(t: TFunction, reason?: string | null): string {
  if (!reason) return ''
  return t(`skills.reason.${reason}`, { defaultValue: reason })
}

export function skillErrorMessage(t: TFunction, error: unknown): string {
  const raw = error instanceof Error ? error.message : String(error)
  const message = raw.replace(/^Error:\s*/, '')
  if (message.startsWith(STALE_UNMANAGED_ERROR_CODE) || message === 'Query returned no rows') {
    return t('skills.errors.unmanagedStale', {
      defaultValue: 'This Skill is no longer in the pending list. Rescan and try again.',
    })
  }
  if (message.startsWith(ADOPT_OPTION_UNAVAILABLE_PREFIX) && message.endsWith(ADOPT_OPTION_UNAVAILABLE_HINT)) {
    return t('skills.errors.adoptOptionUnavailable', {
      defaultValue: 'The selected adoption method is no longer available. Reopen the adoption preview and choose one of the suggested actions.',
    })
  }
  return raw
}
