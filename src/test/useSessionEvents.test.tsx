import { act, cleanup, renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useSessionEvents } from '../hooks/useTauri'
import type { BackendSession, BackendSessionSnapshot } from '../services/tauriApi'
import { useConfigStore } from '../stores/configStore'
import { useSessionStore } from '../stores/sessionStore'

type SessionListener = (event: { payload: BackendSessionSnapshot }) => void

const tauri = vi.hoisted(() => ({
  listen: vi.fn<(event: string, listener: SessionListener) => Promise<() => void>>(),
  getSessionSnapshot: vi.fn<() => Promise<BackendSessionSnapshot>>(),
  getAppStateFlags: vi.fn(),
}))

vi.mock('@tauri-apps/api/event', () => ({ listen: tauri.listen }))
vi.mock('../services/tauriApi', async (importOriginal) => ({
  ...await importOriginal<typeof import('../services/tauriApi')>(),
  isTauri: () => true,
  getSessionSnapshot: tauri.getSessionSnapshot,
  getAppStateFlags: tauri.getAppStateFlags,
}))

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((finish) => { resolve = finish })
  return { promise, resolve }
}

function makeSession(phase: string): BackendSession {
  return {
    id: 'session',
    agentType: 'codex',
    engineLabel: null,
    engineConfigRoot: null,
    project: 'AgentBro',
    cwd: '',
    terminal: '',
    phase,
    startedAt: Date.now(),
    duration: 0,
    tokens: { input: 0, output: 0, cacheRead: 0, cacheCreate: 0 },
    rateLimits: null,
    statusLineText: null,
    contextWindow: null,
    lastMainAgentAt: null,
    cacheTtlMs: null,
    pendingPermission: phase === 'waiting_approval'
      ? { toolName: 'Write', toolInput: '{}', diff: null, options: null }
      : null,
    pendingQuestion: null,
    pendingPlan: null,
    lastToolName: null,
    lastToolTarget: null,
    lastToolStatus: null,
    description: null,
    sessionTitle: null,
    pid: null,
    tty: null,
    termProgram: null,
    termBundleId: null,
    weztermPane: null,
    zellijPaneId: null,
    zellijSessionName: null,
    cmuxSurfaceId: null,
    cmuxWorkspaceId: null,
    subagents: [],
    activeTools: [],
    tasks: [],
    isYoloMode: false,
    model: null,
    lastUserMessage: null,
    lastResponse: null,
    lastThought: null,
  }
}

describe('useSessionEvents', () => {
  beforeEach(() => {
    vi.resetAllMocks()
    tauri.listen.mockResolvedValue(() => {})
    tauri.getAppStateFlags.mockResolvedValue({ codexAppServerLive: false })
    useConfigStore.setState({ usageQueryEnabled: false })
    useSessionStore.setState(useSessionStore.getInitialState(), true)
  })

  afterEach(() => {
    cleanup()
    vi.useRealTimers()
    vi.restoreAllMocks()
  })

  it('subscribes before reading and keeps a realtime approval over the startup snapshot', async () => {
    const subscription = deferred<() => void>()
    const snapshot = deferred<BackendSessionSnapshot>()
    const unlisten = vi.fn()
    tauri.listen.mockReturnValue(subscription.promise)
    tauri.getSessionSnapshot.mockReturnValue(snapshot.promise)

    const { unmount } = renderHook(() => useSessionEvents('events'))
    await waitFor(() => expect(tauri.listen).toHaveBeenCalledTimes(1))
    expect(tauri.getSessionSnapshot).not.toHaveBeenCalled()

    await act(async () => { subscription.resolve(unlisten) })
    expect(tauri.getSessionSnapshot).toHaveBeenCalledTimes(1)
    const emit = tauri.listen.mock.calls[0][1]
    act(() => emit({ payload: { revision: 2, sessions: [makeSession('waiting_approval')] } }))
    await act(async () => { snapshot.resolve({ revision: 1, sessions: [makeSession('processing')] }) })

    expect(useSessionStore.getState().sessions.session.phase).toBe('waiting_approval')
    expect(useSessionStore.getState().sessions.session.pendingPermission?.toolName).toBe('Write')
    unmount()
    expect(unlisten).toHaveBeenCalledTimes(1)
    emit({ payload: { revision: 3, sessions: [] } })
    expect(useSessionStore.getState().sessions.session.phase).toBe('waiting_approval')
  })

  it('keeps one fallback read in flight and drops it after a newer event', async () => {
    vi.useFakeTimers()
    const stalePoll = deferred<BackendSessionSnapshot>()
    tauri.getSessionSnapshot
      .mockResolvedValueOnce({ revision: 1, sessions: [makeSession('processing')] })
      .mockReturnValueOnce(stalePoll.promise)
      .mockResolvedValueOnce({ revision: 5, sessions: [makeSession('ready')] })
    renderHook(() => useSessionEvents())
    await vi.waitFor(() => expect(useSessionStore.getState().sessions.session?.phase).toBe('processing'))

    await act(async () => { await vi.advanceTimersByTimeAsync(20_000) })
    expect(tauri.getSessionSnapshot).toHaveBeenCalledTimes(2)
    const emit = tauri.listen.mock.calls[0][1]
    act(() => emit({ payload: { revision: 3, sessions: [makeSession('done')] } }))
    await act(async () => { stalePoll.resolve({ revision: 2, sessions: [makeSession('processing')] }) })
    expect(useSessionStore.getState().sessions.session.phase).toBe('done')

    await act(async () => { await vi.advanceTimersByTimeAsync(10_000) })
    expect(tauri.getSessionSnapshot).toHaveBeenCalledTimes(3)
    expect(useSessionStore.getState().sessions.session.phase).toBe('ready')
    act(() => emit({ payload: { revision: 4, sessions: [makeSession('waiting_approval')] } }))
    expect(useSessionStore.getState().sessions.session.phase).toBe('ready')
    expect(useSessionStore.getState().sessions.session.pendingPermission).toBeUndefined()
  })

  it('still reads sessions when the native listener fails to register', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => {})
    tauri.listen.mockRejectedValue(new Error('unavailable listener'))
    tauri.getSessionSnapshot.mockResolvedValue({ revision: 1, sessions: [makeSession('processing')] })

    renderHook(() => useSessionEvents('events'))
    await waitFor(() => expect(useSessionStore.getState().sessions.session?.phase).toBe('processing'))
    expect(tauri.getSessionSnapshot).toHaveBeenCalledTimes(1)
  })

  it('ignores an older native update after another surface has resolved the request', async () => {
    const resolved = makeSession('processing')
    tauri.getSessionSnapshot.mockResolvedValue({ revision: 1, sessions: [resolved] })
    renderHook(() => useSessionEvents('events'))
    await waitFor(() => expect(useSessionStore.getState().sessions.session?.phase).toBe('processing'))
    const emit = tauri.listen.mock.calls[0][1]

    act(() => emit({ payload: { revision: 3, sessions: [resolved] } }))
    act(() => emit({ payload: { revision: 2, sessions: [makeSession('waiting_approval')] } }))
    expect(useSessionStore.getState().sessions.session?.pendingPermission).toBeUndefined()
    expect(useSessionStore.getState().activeOverlay).toBeNull()

    act(() => emit({ payload: { revision: 4, sessions: [makeSession('waiting_approval')] } }))
    expect(useSessionStore.getState().sessions.session?.pendingPermission?.toolName).toBe('Write')
  })
})
