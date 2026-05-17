import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'
import { useSessionStore } from '../../stores/sessionStore'
import type { DiffContent, PanelState, SessionState, TaskInfo } from '../../types/agent'
import './ClaudeHookUiLab.css'

type LabScenarioId =
  | 'pre-tool-use'
  | 'post-tool-use'
  | 'permission-request'
  | 'ask-user-question'
  | 'plan-approval'
  | 'stop'
  | 'stop-failure'

type LabViewMode = 'overlay' | 'list' | 'detail' | 'compact'

interface LabScenario {
  id: LabScenarioId
  hookName: string
  title: string
  caption: string
  buildSession: (now: number) => SessionState
}

interface ClaudeHookUiLabProps {
  children: ReactNode
}

const LAB_SESSION_ID = 'claude-hook-lab'
const LAB_CWD = '/Users/demo/projects/agentbro'
const VIEW_MODE_LABELS: Record<LabViewMode, string> = {
  overlay: 'Overlay',
  list: 'List',
  detail: 'Detail',
  compact: 'Compact',
}

const permissionDiff: DiffContent = {
  filePath: 'src/auth/middleware.ts',
  lines: [
    { type: 'context', lineNumber: 10, content: 'import { getToken } from "./auth"' },
    { type: 'remove', lineNumber: 12, content: 'const token = getToken()' },
    { type: 'add', lineNumber: 12, content: 'const token = await refreshToken()' },
    { type: 'context', lineNumber: 13, content: 'if (!token) return null' },
    { type: 'remove', lineNumber: 15, content: 'return { Authorization: token }' },
    { type: 'add', lineNumber: 15, content: 'return { Authorization: `Bearer ${token}` }' },
  ],
}

const baseTasks: TaskInfo[] = [
  { id: 'lab-task-1', name: 'Read auth middleware', status: 'completed' },
  { id: 'lab-task-2', name: 'Patch expired token refresh', status: 'in_progress' },
  { id: 'lab-task-3', name: 'Run focused regression test', status: 'pending' },
]

function baseSession(now: number, overrides: Partial<SessionState> = {}): SessionState {
  return {
    id: LAB_SESSION_ID,
    agentType: 'claude-code',
    engineLabel: 'Claude',
    project: 'agentbro',
    cwd: LAB_CWD,
    terminal: 'iTerm/tmux',
    phase: 'processing',
    startedAt: now - 110_000,
    duration: 110_000,
    tokens: { input: 12_480, output: 3_140, cacheRead: 8_100, cacheCreate: 0 },
    chatHistory: [
      {
        role: 'user',
        content: 'Fix the auth middleware to refresh expired tokens',
        timestamp: now - 105_000,
      },
      {
        role: 'assistant',
        content: 'I will inspect the middleware and update the token refresh path.',
        timestamp: now - 96_000,
      },
    ],
    subagents: [],
    activeTools: [],
    tasks: baseTasks,
    lastUserMessage: 'Fix expired token refresh',
    sessionTitle: 'Edit: auth middleware',
    pid: 4242,
    tty: '/dev/ttys042',
    termBundleId: 'com.googlecode.iterm2',
    lastActivityAt: now - 1_500,
    ...overrides,
  }
}

function permissionSession(now: number): SessionState {
  const toolInput = JSON.stringify({ file_path: 'src/auth/middleware.ts', old_string: 'getToken()', new_string: 'refreshToken()' }, null, 2)
  return baseSession(now, {
    phase: 'waiting_approval',
    description: 'Waiting for permission to edit src/auth/middleware.ts',
    pendingPermission: {
      toolName: 'Edit',
      toolInput,
      diff: permissionDiff,
    },
    lastToolName: 'Edit',
    lastToolTarget: 'src/auth/middleware.ts',
    lastToolStatus: 'running',
    activeTools: [
      { toolUseId: 'lab-tool-edit', toolName: 'Edit', status: 'running', startedAt: now - 4_000 },
    ],
    unattendedSince: now - 4_000,
  })
}

function questionSession(now: number): SessionState {
  return baseSession(now, {
    phase: 'waiting_input',
    description: 'Waiting for user choice from AskUserQuestion',
    pendingQuestion: {
      header: 'Deploy target',
      question: 'Where should Claude apply the migration?',
      options: ['Local branch only', 'Staging branch', 'Staging plus docs'],
      descriptions: [
        'Keep changes scoped to the current worktree.',
        'Update staging integration code as well.',
        'Also update the operator documentation.',
      ],
      multiSelect: false,
      questions: [],
    },
    lastToolName: 'AskUserQuestion',
    lastToolStatus: 'running',
    unattendedSince: now - 7_000,
  })
}

function planSession(now: number): SessionState {
  return baseSession(now, {
    phase: 'waiting_approval',
    description: 'Waiting for PlanApproval response',
    planTitle: 'Auth Middleware Refresh Plan',
    planContent: `Context
The current middleware reads cached tokens directly. Expired tokens can still be sent to downstream APIs.

Plan
1. Replace getToken() with refreshToken() in the request middleware.
2. Preserve the existing null-token early return behavior.
3. Prefix refreshed tokens with Bearer.
4. Add a focused middleware regression test.`,
    planPermissions: ['Edit', 'Bash'],
    lastToolName: 'ExitPlanMode',
    lastToolStatus: 'success',
    unattendedSince: now - 8_000,
  })
}

function preToolSession(now: number): SessionState {
  return baseSession(now, {
    phase: 'processing',
    description: 'Reading src/auth/middleware.ts',
    lastToolName: 'Read',
    lastToolTarget: 'src/auth/middleware.ts',
    lastToolStatus: 'running',
    activeTools: [
      { toolUseId: 'lab-tool-read', toolName: 'Read', status: 'running', startedAt: now - 2_200 },
    ],
  })
}

function postToolSession(now: number): SessionState {
  return baseSession(now, {
    phase: 'processing',
    description: 'Read completed, preparing the edit',
    lastToolName: 'Read',
    lastToolTarget: 'src/auth/middleware.ts',
    lastToolStatus: 'success',
    activeTools: [
      { toolUseId: 'lab-tool-read', toolName: 'Read', status: 'success', startedAt: now - 8_000, completedAt: now - 2_000 },
    ],
    chatHistory: [
      {
        role: 'user',
        content: 'Fix the auth middleware to refresh expired tokens',
        timestamp: now - 105_000,
      },
      {
        role: 'tool_use',
        toolName: 'Read',
        toolInput: 'src/auth/middleware.ts',
        status: 'success',
        timestamp: now - 2_000,
      },
      {
        role: 'assistant',
        content: 'I found the expired-token path. Next I will patch the middleware.',
        timestamp: now - 1_600,
      },
    ],
  })
}

function stopSession(now: number): SessionState {
  return baseSession(now, {
    phase: 'done',
    description: 'Updated token refresh flow and verified the focused middleware test.',
    responseText: 'Updated token refresh flow and verified the focused middleware test.',
    lastToolName: 'Bash',
    lastToolStatus: 'success',
    taskCompletedAt: now - 1_000,
    activeTools: [
      { toolUseId: 'lab-tool-test', toolName: 'Bash', status: 'success', startedAt: now - 16_000, completedAt: now - 3_000 },
    ],
    tasks: baseTasks.map((task) => ({ ...task, status: 'completed' })),
    chatHistory: [
      {
        role: 'user',
        content: 'Fix the auth middleware to refresh expired tokens',
        timestamp: now - 105_000,
      },
      {
        role: 'assistant',
        content: 'Updated token refresh flow and verified the focused middleware test.',
        timestamp: now - 1_000,
      },
    ],
  })
}

function stopFailureSession(now: number): SessionState {
  return baseSession(now, {
    phase: 'error',
    description: 'StopFailure: middleware.test.ts still fails because refreshToken() is not mocked.',
    lastToolName: 'Bash',
    lastToolStatus: 'error',
    activeTools: [
      {
        toolUseId: 'lab-tool-test-failure',
        toolName: 'Bash',
        status: 'error',
        startedAt: now - 14_000,
        completedAt: now - 2_000,
        error: 'ReferenceError: refreshToken is not defined',
      },
    ],
    chatHistory: [
      {
        role: 'user',
        content: 'Fix the auth middleware to refresh expired tokens',
        timestamp: now - 105_000,
      },
      {
        role: 'error',
        message: 'StopFailure: middleware.test.ts still fails because refreshToken() is not mocked.',
        timestamp: now - 2_000,
      },
    ],
  })
}

const CLAUDE_HOOK_LAB_SCENARIOS: LabScenario[] = [
  {
    id: 'permission-request',
    hookName: 'PermissionRequest',
    title: 'Permission',
    caption: 'pendingPermission + permission overlay',
    buildSession: permissionSession,
  },
  {
    id: 'ask-user-question',
    hookName: 'AskUserQuestion',
    title: 'Question',
    caption: 'pendingQuestion + question overlay',
    buildSession: questionSession,
  },
  {
    id: 'plan-approval',
    hookName: 'PlanApproval',
    title: 'Plan',
    caption: 'pendingPlan + plan overlay',
    buildSession: planSession,
  },
  {
    id: 'pre-tool-use',
    hookName: 'PreToolUse',
    title: 'Tool Start',
    caption: 'running activeTools entry',
    buildSession: preToolSession,
  },
  {
    id: 'post-tool-use',
    hookName: 'PostToolUse',
    title: 'Tool Done',
    caption: 'completed activeTools entry',
    buildSession: postToolSession,
  },
  {
    id: 'stop',
    hookName: 'Stop',
    title: 'Stop',
    caption: 'done session + completion overlay',
    buildSession: stopSession,
  },
  {
    id: 'stop-failure',
    hookName: 'StopFailure',
    title: 'Failure',
    caption: 'error session, no fake overlay',
    buildSession: stopFailureSession,
  },
]

function resetLabState() {
  useSessionStore.setState({
    sessions: {},
    sessionList: [],
    activeSessionId: null,
    panelState: 'collapsed',
    baseLayer: 'compact',
    overlayQueue: [],
    activeOverlay: null,
    rateLimits: undefined,
    hookNotification: null,
    wakeSilencedUntil: 0,
    focusedTerminal: null,
  })
}

function defaultPanelState(session: SessionState, viewMode: LabViewMode): PanelState {
  if (viewMode === 'detail') return 'expanded'
  if (viewMode === 'list') return 'hover'
  if (viewMode === 'compact') return 'collapsed'
  if (session.phase === 'done') return 'collapsed'
  return 'hover'
}

function applyClaudeHookLabScenario(scenarioId: LabScenarioId, viewMode: LabViewMode = 'overlay') {
  const scenario = CLAUDE_HOOK_LAB_SCENARIOS.find((candidate) => candidate.id === scenarioId)
  if (!scenario) return

  const session = scenario.buildSession(Date.now())
  resetLabState()
  useSessionStore.getState().replaceAllSessions([session], { suppressed: viewMode === 'compact' })
  const store = useSessionStore.getState()
  store.setActiveSession(session.id)

  if (viewMode === 'detail' || viewMode === 'list') {
    store.clearSessionOverlays(session.id)
  }

  store.setPanelState(defaultPanelState(session, viewMode))
}

export function ClaudeHookUiLab({ children }: ClaudeHookUiLabProps) {
  const [selectedScenarioId, setSelectedScenarioId] = useState<LabScenarioId>('permission-request')
  const [viewMode, setViewMode] = useState<LabViewMode>('overlay')
  const [replayToken, setReplayToken] = useState(0)
  const activeOverlay = useSessionStore((state) => state.activeOverlay)
  const activeSession = useSessionStore((state) => state.activeSessionId ? state.sessions[state.activeSessionId] : undefined)

  useEffect(() => {
    applyClaudeHookLabScenario(selectedScenarioId, viewMode)
  }, [selectedScenarioId, viewMode, replayToken])

  const activeScenario = useMemo(
    () => CLAUDE_HOOK_LAB_SCENARIOS.find((scenario) => scenario.id === selectedScenarioId) ?? CLAUDE_HOOK_LAB_SCENARIOS[0],
    [selectedScenarioId],
  )

  const applyScenario = useCallback((scenarioId: LabScenarioId) => {
    setSelectedScenarioId(scenarioId)
    setReplayToken((token) => token + 1)
  }, [])

  const applyViewMode = useCallback((mode: LabViewMode) => {
    setViewMode(mode)
    setReplayToken((token) => token + 1)
  }, [])

  return (
    <div className="claude-hook-lab">
      <div className="claude-hook-lab__stage">
        {children}
      </div>

      <aside className="claude-hook-lab__panel" aria-label="Claude Hook UI Lab">
        <header className="claude-hook-lab__header">
          <div>
            <div className="claude-hook-lab__eyebrow">Claude Hook UI Lab</div>
            <h1>{activeScenario.hookName}</h1>
          </div>
          <button className="claude-hook-lab__reset" type="button" onClick={() => setReplayToken((token) => token + 1)}>
            Reset
          </button>
        </header>

        <div className="claude-hook-lab__scenario-grid" role="list" aria-label="Hook scenarios">
          {CLAUDE_HOOK_LAB_SCENARIOS.map((scenario) => (
            <button
              key={scenario.id}
              className="claude-hook-lab__scenario"
              data-active={scenario.id === selectedScenarioId}
              type="button"
              onClick={() => applyScenario(scenario.id)}
            >
              <span className="claude-hook-lab__scenario-title">{scenario.title}</span>
              <span className="claude-hook-lab__scenario-hook">{scenario.hookName}</span>
            </button>
          ))}
        </div>

        <div className="claude-hook-lab__modes" aria-label="View mode">
          {(Object.keys(VIEW_MODE_LABELS) as LabViewMode[]).map((mode) => (
            <button
              key={mode}
              type="button"
              data-active={viewMode === mode}
              onClick={() => applyViewMode(mode)}
            >
              {VIEW_MODE_LABELS[mode]}
            </button>
          ))}
        </div>

        <footer className="claude-hook-lab__status">
          <span>{activeScenario.caption}</span>
          <span>phase: {activeSession?.phase ?? 'none'}</span>
          <span>overlay: {activeOverlay?.type ?? 'none'}</span>
        </footer>
      </aside>
    </div>
  )
}
