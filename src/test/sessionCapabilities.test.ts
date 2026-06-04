import { describe, expect, it } from 'vitest'
import { getComposerCapability } from '../utils/sessionCapabilities'
import type { SessionState } from '../types/agent'

function session(overrides: Partial<SessionState> = {}): SessionState {
  return {
    id: 's1',
    agentType: 'claude-code',
    project: 'agentbro',
    terminal: 'iTerm2',
    phase: 'idle',
    startedAt: 0,
    duration: 0,
    tokens: { input: 0, output: 0, cacheRead: 0, cacheCreate: 0 },
    chatHistory: [],
    subagents: [],
    activeTools: [],
    pid: 1234,
    tty: '/dev/ttys001',
    ...overrides,
  }
}

describe('getComposerCapability', () => {
  it('allows sending for a Claude tmux session', () => {
    expect(getComposerCapability(session())).toEqual({ kind: 'sendable' })
  })

  it('locks Codex.app sessions detected by bundle id', () => {
    const s = session({
      agentType: 'codex',
      termBundleId: 'com.openai.codex',
      terminal: 'Codex',
      tty: undefined,
    })
    expect(getComposerCapability(s)).toEqual({ kind: 'locked', reason: 'codex-app' })
  })

  it('locks Codex sessions with no tty and Codex-looking terminal label', () => {
    const s = session({
      agentType: 'codex',
      terminal: 'Codex Desktop',
      tty: undefined,
      pid: 9001,
    })
    expect(getComposerCapability(s)).toEqual({ kind: 'locked', reason: 'codex-app' })
  })

  it('keeps Codex CLI tmux sessions sendable', () => {
    const s = session({
      agentType: 'codex',
      terminal: 'iTerm2',
      tty: '/dev/ttys003',
      pid: 9001,
    })
    expect(getComposerCapability(s)).toEqual({ kind: 'sendable' })
  })

  it('allows Codex.app follow-up composer when the app-server bridge is live', () => {
    const s = session({
      agentType: 'codex',
      termBundleId: 'com.openai.codex',
      codexAppServerThreadId: 'thread-1',
      terminal: 'Codex',
      tty: undefined,
    })
    expect(getComposerCapability(s, {
      codexAppServerLive: true,
      codexDesktopRepliesSupported: true,
    })).toEqual({ kind: 'sendable' })
  })

  it('keeps Codex.app locked when the bridge is live but the session is not app-server backed', () => {
    const s = session({
      agentType: 'codex',
      termBundleId: 'com.openai.codex',
      terminal: 'Codex',
      tty: undefined,
    })
    expect(getComposerCapability(s, { codexAppServerLive: true })).toEqual({
      kind: 'locked',
      reason: 'codex-app',
    })
  })

  it('keeps Codex.app locked when the platform cannot inject Desktop replies', () => {
    const s = session({
      agentType: 'codex',
      termBundleId: 'com.openai.codex',
      codexAppServerThreadId: 'thread-1',
      terminal: 'Codex',
      tty: undefined,
    })
    expect(getComposerCapability(s, {
      codexAppServerLive: true,
      codexDesktopRepliesSupported: false,
    })).toEqual({
      kind: 'locked',
      reason: 'codex-app',
    })
  })

  it('locks Qoder.app sessions', () => {
    const s = session({
      agentType: 'qoder',
      termBundleId: 'com.qoder.ide',
      terminal: 'Qoder',
    })
    expect(getComposerCapability(s)).toEqual({ kind: 'locked', reason: 'qoder-app' })
  })

  it('keeps Qoder.app locked even when the Codex app-server bridge is live', () => {
    const s = session({
      agentType: 'qoder',
      termBundleId: 'com.qoder.ide',
      terminal: 'Qoder',
    })
    expect(getComposerCapability(s)).toEqual({
      kind: 'locked',
      reason: 'qoder-app',
    })
  })

  it('locks remote sessions', () => {
    expect(getComposerCapability(session({ remoteHostId: 'host-1' }))).toEqual({
      kind: 'locked',
      reason: 'remote',
    })
  })

  it('keeps remote Codex app-server sessions locked', () => {
    const s = session({
      agentType: 'codex',
      codexAppServerThreadId: 'thread-1',
      remoteHostId: 'host-1',
      termBundleId: 'com.openai.codex',
      tty: undefined,
      pid: undefined,
    })
    expect(getComposerCapability(s, {
      codexAppServerLive: true,
      codexDesktopRepliesSupported: true,
    })).toEqual({
      kind: 'locked',
      reason: 'remote',
    })
  })

  it('locks sessions with no tty and no pid', () => {
    expect(getComposerCapability(session({ tty: undefined, pid: undefined }))).toEqual({
      kind: 'locked',
      reason: 'no-terminal',
    })
  })
})
