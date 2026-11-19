import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { PermissionCard } from '../components/overlay/PermissionCard'
import type { SessionState } from '../types/agent'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

describe('PermissionCard actions', () => {
  it.each([
    ['deepseek-harness', false],
    ['codex', false],
    ['claude-code', true],
  ] as const)('shows persistent actions for %s only when supported', (agentType, persistentActions) => {
    const session: SessionState = {
      id: 'permission-session',
      agentType,
      project: 'agentbro',
      terminal: 'Terminal',
      phase: 'waiting_approval',
      startedAt: 0,
      duration: 0,
      tokens: { input: 0, output: 0, cacheRead: 0, cacheCreate: 0 },
      chatHistory: [],
      subagents: [],
      activeTools: [],
    }

    render(
      <PermissionCard
        session={session}
        overlay={{
          id: 'permission',
          sessionId: session.id,
          type: 'permission',
          data: { toolName: 'Bash', toolInput: '{"command":"pwd"}' },
          createdAt: 0,
        }}
        onAllow={vi.fn()}
        onAllowAlways={vi.fn()}
        onAutoApprove={vi.fn()}
        onDeny={vi.fn()}
        onDismiss={vi.fn()}
      />,
    )

    expect(screen.getByRole('button', { name: 'notch.allowOnce' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'notch.deny' })).toBeInTheDocument()
    expect(Boolean(screen.queryByRole('button', { name: 'notch.allowAlways' }))).toBe(persistentActions)
    expect(Boolean(screen.queryByRole('button', { name: 'notch.autoApprove' }))).toBe(persistentActions)
  })
})
