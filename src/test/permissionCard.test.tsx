import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { PermissionCard } from '../components/overlay/PermissionCard'
import type { SessionState } from '../types/agent'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

function renderPermissionCard(agentType: SessionState['agentType']) {
  const onDeny = vi.fn()
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
      onDeny={onDeny}
      onDismiss={vi.fn()}
    />,
  )
  return onDeny
}

describe('PermissionCard actions', () => {
  it.each([
    ['deepseek-harness', false],
    ['qoder-desktop', false],
    ['codex', false],
    ['claude-code', true],
  ] as const)('shows persistent actions for %s only when supported', (agentType, persistentActions) => {
    renderPermissionCard(agentType)

    expect(screen.getByRole('button', { name: 'notch.allowOnce' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'notch.deny' })).toBeInTheDocument()
    expect(Boolean(screen.queryByRole('button', { name: 'notch.allowAlways' }))).toBe(persistentActions)
    expect(Boolean(screen.queryByRole('button', { name: 'notch.autoApprove' }))).toBe(persistentActions)
  })

  it.each(['deepseek-harness', 'qoder-desktop'] as const)('denies %s requests on the first click', (agentType) => {
    const onDeny = renderPermissionCard(agentType)
    const deny = screen.getByRole('button', { name: 'notch.deny' })
    fireEvent.mouseDown(deny)
    fireEvent.click(deny, { detail: 1 })

    expect(onDeny).toHaveBeenCalledTimes(1)
    expect(onDeny).toHaveBeenCalledWith(undefined)
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument()
  })

  it('denies Qoder Desktop requests on keyboard activation', () => {
    const onDeny = renderPermissionCard('qoder-desktop')
    fireEvent.click(screen.getByRole('button', { name: 'notch.deny' }))

    expect(onDeny).toHaveBeenCalledExactlyOnceWith(undefined)
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument()
  })
})
