import type { SessionState } from '../types/agent'

export type ComposerLockReason =
  | 'codex-app'
  | 'qoder-app'
  | 'remote'
  | 'no-terminal'

export type ComposerCapability =
  | { kind: 'sendable' }
  | { kind: 'locked'; reason: ComposerLockReason }

export interface ComposerCapabilityOptions {
  codexAppServerLive?: boolean
  codexDesktopRepliesSupported?: boolean
}

function bundleIdMatchesCodexApp(termBundleId: string | undefined): boolean {
  if (!termBundleId) return false
  return termBundleId.toLowerCase().includes('openai.codex')
}

function bundleIdMatchesQoderApp(termBundleId: string | undefined): boolean {
  if (!termBundleId) return false
  const lower = termBundleId.toLowerCase()
  return lower === 'com.qoder.ide' || lower === 'com.qoder.ide.helper'
}

function isCodexDesktopSession(session: SessionState): boolean {
  if (session.agentType !== 'codex') return false
  if (bundleIdMatchesCodexApp(session.termBundleId)) return true

  // Mirror the tail of the Rust heuristic (commands/mod.rs:2364) — no tty,
  // terminal label looks like a Codex.app label rather than a /dev/tty path.
  // We deliberately skip the disk-backed `read_codex_session_meta` branch;
  // false negatives just mean "let the backend try and surface any error".
  const tty = session.tty?.trim() ?? ''
  const terminal = session.terminal?.trim() ?? ''
  const missingTty = tty.length === 0
  const terminalLooksLikeApp = !terminal.startsWith('/dev/')
    && (terminal.length === 0 || terminal.toLowerCase().includes('codex'))
  return missingTty && terminalLooksLikeApp
}

function isQoderAppSession(session: SessionState): boolean {
  if (session.agentType !== 'qoder') return false
  return bundleIdMatchesQoderApp(session.termBundleId)
    || (session.terminal?.toLowerCase().includes('qoder') ?? false)
}

function isRemoteSession(session: SessionState): boolean {
  return Boolean(session.remoteHostId || session.remoteHostName)
}

function isCodexAppServerBackedSession(session: SessionState): boolean {
  return Boolean(session.codexAppServerThreadId)
}

function hasNoTerminalAffinity(session: SessionState): boolean {
  // Sessions with neither a tty nor a pid have nowhere local to type into.
  // Codex.app / Qoder.app are caught earlier; this is the residual case for
  // app-hosted agents we don't recognize.
  return !session.tty && session.pid === undefined
}

export function getComposerCapability(
  session: SessionState,
  options: ComposerCapabilityOptions = {},
): ComposerCapability {
  if (isRemoteSession(session)) {
    return { kind: 'locked', reason: 'remote' }
  }

  if (isCodexDesktopSession(session)) {
    return options.codexDesktopRepliesSupported
      && options.codexAppServerLive
      && isCodexAppServerBackedSession(session)
      ? { kind: 'sendable' }
      : { kind: 'locked', reason: 'codex-app' }
  }

  if (isQoderAppSession(session)) {
    return { kind: 'locked', reason: 'qoder-app' }
  }

  if (hasNoTerminalAffinity(session)) {
    return { kind: 'locked', reason: 'no-terminal' }
  }

  return { kind: 'sendable' }
}
