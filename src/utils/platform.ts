type NavigatorWithUserAgentData = Navigator & {
  userAgentData?: {
    platform?: string
  }
}

export function isApplePlatform(): boolean {
  if (typeof navigator === 'undefined') return false
  const nav = navigator as NavigatorWithUserAgentData
  const platform = nav.userAgentData?.platform || nav.platform || ''
  const userAgent = nav.userAgent || ''
  return /Mac|iPhone|iPad|iPod/i.test(`${platform} ${userAgent}`)
}

export function isWindowsPlatform(): boolean {
  if (typeof navigator === 'undefined') return false
  const nav = navigator as NavigatorWithUserAgentData
  const platform = nav.userAgentData?.platform || nav.platform || ''
  const userAgent = nav.userAgent || ''
  return /Win/i.test(`${platform} ${userAgent}`)
}

export function primaryModifierPressed(event: { metaKey: boolean; ctrlKey: boolean }): boolean {
  return isApplePlatform() ? event.metaKey : event.ctrlKey
}

export function primaryModifierName(): 'Cmd' | 'Ctrl' {
  return isApplePlatform() ? 'Cmd' : 'Ctrl'
}
