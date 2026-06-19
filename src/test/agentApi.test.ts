import { afterEach, describe, expect, it, vi } from 'vitest'

async function seedWithPlatform(platform: string) {
  vi.resetModules()
  Object.defineProperty(window.navigator, 'platform', {
    configurable: true,
    value: platform,
  })
  Object.defineProperty(window.navigator, 'userAgentData', {
    configurable: true,
    value: undefined,
  })
  const { seedAgentPrograms } = await import('../services/agentApi')
  return seedAgentPrograms()
}

describe('agentApi seedAgentPrograms', () => {
  afterEach(() => {
    vi.resetModules()
  })

  it('does not expose macOS app bundle paths on Windows', async () => {
    const agents = await seedWithPlatform('Win32')
    const appAgents = agents.filter((agent) => agent.kind === 'app')

    expect(appAgents.length).toBeGreaterThan(0)
    expect(appAgents.every((agent) => !agent.appPath?.startsWith('/Applications/'))).toBe(true)
    expect(agents.find((agent) => agent.id === 'cursor')?.appPath).toBeNull()
  })

  it('keeps macOS app bundle hints on macOS', async () => {
    const agents = await seedWithPlatform('MacIntel')

    expect(agents.find((agent) => agent.id === 'cursor')?.appPath).toBe('/Applications/Cursor.app')
  })
})
