import type { MarketplaceSkill, SkillSummary } from '../../services/skillApiV2'

export function marketSkillId(skill: MarketplaceSkill) {
  const rawPath = skill.downloadUrl.replace(/^(github|skillssh):/, '')
  const source = skill.source || ''
  if (source && rawPath.startsWith(`${source}/`)) {
    return rawPath.slice(source.length + 1) || skill.name
  }
  const parts = rawPath.split('/').filter(Boolean)
  return parts.length > 2 ? parts.slice(2).join('/') : skill.name
}

export function isMarketItemInstalled(skill: MarketplaceSkill, centerSkills: SkillSummary[]) {
  const skillId = marketSkillId(skill)
  const sourceUris = marketSourceUriCandidates(skill)
  return centerSkills.some((centerSkill) => {
    if (centerSkill.id === skillId) return true
    const sourceUri = centerSkill.sourceUri?.trim()
    return Boolean(sourceUri && sourceUris.has(sourceUri))
  })
}

function marketSourceUriCandidates(skill: MarketplaceSkill) {
  const candidates = new Set<string>()
  const downloadUrl = skill.downloadUrl.trim()
  if (downloadUrl) candidates.add(downloadUrl)

  const source = skill.source?.trim()
  const skillId = marketSkillId(skill)
  if (source && skillId) {
    candidates.add(`skillssh:${source}/${skillId}`)
    candidates.add(`github:${source}/${skillId}`)
  }

  if (downloadUrl.startsWith('skillssh:')) {
    candidates.add(`github:${downloadUrl.slice('skillssh:'.length)}`)
  } else if (downloadUrl.startsWith('github:')) {
    candidates.add(`skillssh:${downloadUrl.slice('github:'.length)}`)
  }

  return candidates
}
