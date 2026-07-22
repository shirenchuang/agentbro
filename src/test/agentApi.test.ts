import { afterEach, describe, expect, it, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(() => Promise.resolve(undefined)),
}))

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
    vi.mocked(invoke).mockClear()
    delete (window as Window & { __TAURI_INTERNALS__?: unknown; isTauri?: boolean }).__TAURI_INTERNALS__
    delete (window as Window & { __TAURI_INTERNALS__?: unknown; isTauri?: boolean }).isTauri
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
    expect(agents.find((agent) => agent.id === 'zcode')?.appPath).toBe('/Applications/ZCode.app')
    expect(agents.find((agent) => agent.id === 'doubao')?.appPath).toBe('/Applications/Doubao.app')
  })

  it('checks Tauri runtime at call time for install actions', async () => {
    vi.resetModules()
    delete (window as Window & { __TAURI_INTERNALS__?: unknown; isTauri?: boolean }).__TAURI_INTERNALS__

    const { agentApi } = await import('../services/agentApi')
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      value: {},
    })

    await agentApi.install('codex')

    expect(invoke).toHaveBeenCalledWith('agent_install', { agentId: 'codex' })
  })
})
