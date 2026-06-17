import type { TFunction } from 'i18next'
import type { TargetClaim } from '../../services/skillApiV2'

const SOURCE_TYPE_KEYS: Record<string, string> = {
  'skills.sh': 'skills_sh',
}

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
