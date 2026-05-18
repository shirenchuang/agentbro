import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'
import { useSessionStore } from '../../stores/sessionStore'
import { useConfigStore } from '../../stores/configStore'
import { COLOR_THEMES, useThemeStore } from '../../stores/themeStore'
import type { DiffContent, PanelState, SessionState, TaskInfo } from '../../types/agent'
import './ClaudeHookUiLab.css'

type LabScenarioId =
  | 'empty-idle'
  | 'multi-session-mixed'
  | 'response-overlay'
  | 'overlay-queue'
  | 'long-content'
  | 'status-badges'
  | 'pre-tool-use'
  | 'post-tool-use'
  | 'permission-request'
  | 'ask-user-question'
  | 'plan-approval'
  | 'stop'
  | 'stop-failure'
  | 'real-write'
  | 'real-read'
  | 'real-edit-pending'
  | 'real-edit-allowed'
  | 'real-bash-pending'
  | 'real-bash-failure'
  | 'real-plan'
  | 'real-question'
  | 'real-multi-question'
  | 'real-subagent'
  | 'real-compact'

type LabViewMode = 'overlay' | 'list' | 'detail' | 'compact'

interface LabScenario {
  id: LabScenarioId
  hookName: string
  title: string
  caption: string
  buildSession?: (now: number) => SessionState
  buildSessions?: (now: number) => SessionState[]
  configureStore?: (now: number, sessions: SessionState[]) => void
}

interface ClaudeHookUiLabProps {
  children: ReactNode
}

const LAB_SESSION_ID = 'claude-hook-lab'
const LAB_CWD = '/Users/demo/projects/agentbro'
const RECORDED_SESSION_ID = 'e336326d-ab6f-4002-bc6a-3ba66892b469'
const RECORDED_CWD = '/Users/shirenchuang/code/github/empty'
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

const recordedTasks: TaskInfo[] = [
  { id: 'recorded-init', name: 'Create minimal TypeScript test project', status: 'completed' },
  { id: 'recorded-read', name: 'Read package/auth/middleware files', status: 'completed' },
  { id: 'recorded-edit', name: 'Edit middleware to use refreshToken', status: 'completed' },
  { id: 'recorded-test', name: 'Run tests and capture failure', status: 'completed' },
  { id: 'recorded-questions', name: 'Capture single and multi AskUserQuestion', status: 'completed' },
  { id: 'recorded-compact', name: 'Capture PreCompact output', status: 'completed' },
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

function withSessionId(session: SessionState, id: string, overrides: Partial<SessionState> = {}): SessionState {
  return {
    ...session,
    id,
    ...overrides,
  }
}

function recordedSession(now: number, overrides: Partial<SessionState> = {}): SessionState {
  return {
    id: RECORDED_SESSION_ID,
    agentType: 'claude-code',
    engineLabel: 'Claude Code',
    project: 'empty',
    cwd: RECORDED_CWD,
    terminal: 'iTerm',
    phase: 'processing',
    startedAt: now - 14 * 60_000,
    duration: 14 * 60_000,
    tokens: { input: 33_802, output: 1_120, cacheRead: 33_437, cacheCreate: 0 },
    chatHistory: [
      {
        role: 'user',
        content: '我们要做 AgentBro Claude Code hook UI 真实采样。请严格按我每一轮指令执行。',
        timestamp: now - 13 * 60_000,
      },
      {
        role: 'assistant',
        content: '明白。等待你的第一步指令。',
        timestamp: now - 12 * 60_000,
      },
    ],
    subagents: [],
    activeTools: [],
    tasks: recordedTasks,
    lastUserMessage: '请修改 src/auth/middleware.ts：不再直接使用 getToken()，改成 await refreshToken()',
    sessionTitle: 'AgentBro Claude Code hook UI 真实采样',
    pid: 47839,
    tty: '/dev/ttys002',
    termBundleId: 'com.googlecode.iterm2',
    lastActivityAt: now - 1_000,
    ...overrides,
  }
}

function recordedEditInput() {
  return JSON.stringify({
    replace_all: false,
    file_path: `${RECORDED_CWD}/src/auth/middleware.ts`,
    old_string: 'import { getToken } from "./auth";\n\nexport function buildAuthHeaders(): Record<string, string> {\n  return { Authorization: `Bearer ${getToken()}` };\n}',
    new_string: 'import { refreshToken } from "./auth";\n\nexport async function buildAuthHeaders(): Promise<Record<string, string>> {\n  const token = await refreshToken();\n  if (!token) return {};\n  return { Authorization: `Bearer ${token}` };\n}',
  }, null, 2)
}

function recordedEditDiff(): DiffContent {
  return {
    filePath: `${RECORDED_CWD}/src/auth/middleware.ts`,
    lines: [
      { type: 'remove', lineNumber: 1, content: 'import { getToken } from "./auth";' },
      { type: 'add', lineNumber: 1, content: 'import { refreshToken } from "./auth";' },
      { type: 'context', lineNumber: 2, content: '' },
      { type: 'remove', lineNumber: 3, content: 'export function buildAuthHeaders(): Record<string, string> {' },
      { type: 'remove', lineNumber: 4, content: '  return { Authorization: `Bearer ${getToken()}` };' },
      { type: 'add', lineNumber: 3, content: 'export async function buildAuthHeaders(): Promise<Record<string, string>> {' },
      { type: 'add', lineNumber: 4, content: '  const token = await refreshToken();' },
      { type: 'add', lineNumber: 5, content: '  if (!token) return {};' },
      { type: 'add', lineNumber: 6, content: '  return { Authorization: `Bearer ${token}` };' },
      { type: 'context', lineNumber: 7, content: '}' },
    ],
  }
}

function recordedWriteSession(now: number): SessionState {
  return recordedSession(now, {
    phase: 'processing',
    description: '创建4个文件。',
    lastUserMessage: '请在当前空目录创建一个最小 TypeScript 测试项目',
    sessionTitle: 'Create minimal TypeScript project',
    lastToolName: 'Write',
    lastToolTarget: `${RECORDED_CWD}/src/auth/middleware.ts`,
    lastToolStatus: 'running',
    activeTools: [
      {
        toolUseId: 'tooluse_3wObTzgiqWYNnU4G7tMQwH',
        toolName: 'Write',
        status: 'running',
        startedAt: now - 2_000,
        diff: recordedEditDiff(),
      },
    ],
    chatHistory: [
      {
        role: 'user',
        content: '请在当前空目录创建一个最小 TypeScript 测试项目，只包含 package.json、auth.ts、middleware.ts、middleware.test.ts。',
        timestamp: now - 45_000,
      },
      {
        role: 'assistant',
        content: '创建4个文件。',
        timestamp: now - 38_000,
        toolCalls: [
          {
            toolUseId: 'tooluse_GMVOwdN2UBU1KTZtn9aLmq',
            toolName: 'Write',
            toolInput: `file_path: ${RECORDED_CWD}/package.json`,
            status: 'success',
          },
          {
            toolUseId: 'tooluse_3wObTzgiqWYNnU4G7tMQwH',
            toolName: 'Write',
            toolInput: `file_path: ${RECORDED_CWD}/src/auth/middleware.ts`,
            status: 'running',
          },
        ],
      },
    ],
  })
}

function recordedReadSession(now: number): SessionState {
  return recordedSession(now, {
    phase: 'done',
    description: '`auth.ts` 导出两个硬编码的 mock token 函数，`middleware.ts` 调用 getToken() 拼接 Bearer header。',
    responseText: '`auth.ts` 导出两个硬编码的 mock token 函数，`middleware.ts` 调用 getToken() 拼接 Bearer header。',
    lastUserMessage: '请只读取 package.json、src/auth/auth.ts、src/auth/middleware.ts，然后用三句话总结当前实现。',
    sessionTitle: 'Read package/auth/middleware',
    lastToolName: 'Read',
    lastToolTarget: `${RECORDED_CWD}/src/auth/middleware.ts`,
    lastToolStatus: 'success',
    activeTools: [
      { toolUseId: 'tooluse_6Z8UKEAredekq7774Bc8OH', toolName: 'Read', status: 'success', startedAt: now - 10_000, completedAt: now - 8_000 },
      { toolUseId: 'tooluse_oqOgggPHv3XcCZgd2rCvQZ', toolName: 'Read', status: 'success', startedAt: now - 9_000, completedAt: now - 7_000 },
      { toolUseId: 'tooluse_BHJWkz7TzzDUxbZFsHqzdd', toolName: 'Read', status: 'success', startedAt: now - 8_000, completedAt: now - 6_000 },
    ],
    chatHistory: [
      {
        role: 'user',
        content: '请只读取 package.json、src/auth/auth.ts、src/auth/middleware.ts，然后用三句话总结当前实现。不要修改任何文件。',
        timestamp: now - 16_000,
      },
      {
        role: 'assistant',
        content: '',
        timestamp: now - 10_000,
        toolCalls: [
          { toolUseId: 'tooluse_6Z8UKEAredekq7774Bc8OH', toolName: 'Read', toolInput: `file_path: ${RECORDED_CWD}/package.json`, status: 'success' },
          { toolUseId: 'tooluse_oqOgggPHv3XcCZgd2rCvQZ', toolName: 'Read', toolInput: `file_path: ${RECORDED_CWD}/src/auth/auth.ts`, status: 'success' },
          { toolUseId: 'tooluse_BHJWkz7TzzDUxbZFsHqzdd', toolName: 'Read', toolInput: `file_path: ${RECORDED_CWD}/src/auth/middleware.ts`, status: 'success' },
        ],
        trailingContent: '`auth.ts` 导出两个硬编码的 mock token 函数（`getToken` 返回 `"mock-token"`，`refreshToken` 返回 `"refreshed-token"`）。`middleware.ts` 的 `buildAuthHeaders()` 调用 `getToken()` 拼接成 `Bearer` header 返回。`package.json` 使用 `ts-jest` + Jest 29 作为测试框架，`npm test` 即可运行。',
      },
    ],
  })
}

function recordedEditPendingSession(now: number): SessionState {
  const diff = recordedEditDiff()
  return recordedSession(now, {
    phase: 'waiting_approval',
    description: 'PermissionRequest: Edit src/auth/middleware.ts',
    pendingPermission: {
      toolName: 'Edit',
      toolInput: recordedEditInput(),
      diff,
    },
    lastToolName: 'Edit',
    lastToolTarget: `${RECORDED_CWD}/src/auth/middleware.ts`,
    lastToolStatus: 'running',
    activeTools: [
      { toolUseId: 'tooluse_KpxejiPDybvgkOqLx2iNo4', toolName: 'Edit', status: 'running', startedAt: now - 47_000 },
    ],
    unattendedSince: now - 47_000,
    chatHistory: [
      {
        role: 'user',
        content: '请修改 src/auth/middleware.ts：不再直接使用 getToken()，改成 await refreshToken()，Authorization 返回值改成 Bearer ${token}。',
        timestamp: now - 55_000,
      },
      {
        role: 'permission',
        toolName: 'Edit',
        toolInput: recordedEditInput(),
        diff,
        timestamp: now - 47_000,
      },
    ],
  })
}

function recordedEditAllowedSession(now: number): SessionState {
  return recordedSession(now, {
    phase: 'done',
    description: 'hook_permission_decision: allow; buildAuthHeaders now awaits refreshToken().',
    responseText: '完成。`buildAuthHeaders` 现在是 async，调用 `await refreshToken()`，token 为空时返回 `{}`。等待下一步指令。',
    lastToolName: 'Edit',
    lastToolTarget: `${RECORDED_CWD}/src/auth/middleware.ts`,
    lastToolStatus: 'success',
    chatHistory: [
      {
        role: 'user',
        content: '请修改 src/auth/middleware.ts：不再直接使用 getToken()，改成 await refreshToken()。',
        timestamp: now - 60_000,
      },
      {
        role: 'assistant',
        content: '',
        timestamp: now - 52_000,
        toolCalls: [
          {
            toolUseId: 'tooluse_KpxejiPDybvgkOqLx2iNo4',
            toolName: 'Edit',
            toolInput: recordedEditInput(),
            diff: recordedEditDiff(),
            status: 'success',
            result: 'hook_permission_decision: allow\nThe file src/auth/middleware.ts has been updated successfully.',
          },
        ],
        trailingContent: '完成。`buildAuthHeaders` 现在是 async，调用 `await refreshToken()`，token 为空时返回 `{}`。等待下一步指令。',
      },
    ],
  })
}

function recordedBashPendingSession(now: number): SessionState {
  const toolInput = JSON.stringify({ command: 'npm install 2>&1 | tail -5', description: 'Install dependencies' })
  return recordedSession(now, {
    phase: 'waiting_approval',
    description: 'Processing user input: 请运行项目里的测试命令。如果通过，只总结测试结果，不要继续修改。',
    pendingPermission: {
      toolName: 'Bash',
      toolInput,
    },
    lastUserMessage: '请运行项目里的测试命令。如果通过，只总结测试结果，不要继续修改。',
    sessionTitle: 'Run project tests',
    lastToolName: 'Bash',
    lastToolTarget: 'npm install 2>&1 | tail -5',
    lastToolStatus: 'running',
    activeTools: [
      { toolUseId: 'tooluse_igIFg0ku5blKXCJNO9ZAh5', toolName: 'Bash', status: 'running', startedAt: now - 7_000 },
    ],
    unattendedSince: now - 7_000,
    chatHistory: [
      { role: 'user', content: '请运行项目里的测试命令。如果通过，只总结测试结果，不要继续修改。', timestamp: now - 14_000 },
      { role: 'assistant', content: '先安装依赖再运行测试。', timestamp: now - 8_000 },
      { role: 'permission', toolName: 'Bash', toolInput, timestamp: now - 7_000 },
    ],
  })
}

function recordedBashFailureSession(now: number): SessionState {
  return recordedSession(now, {
    phase: 'done',
    description: '命令以 exit code 7 退出，stderr 输出：intentional hook ui failure。',
    responseText: '命令以 exit code 7 退出，stderr 输出：`intentional hook ui failure`。',
    lastUserMessage: '请运行这个命令，并只报告结果，不要修复：node -e "console.error(...); process.exit(7)"',
    sessionTitle: 'Intentional Bash failure',
    lastToolName: 'Bash',
    lastToolStatus: 'error',
    activeTools: [
      {
        toolUseId: 'tooluse_intentional_failure',
        toolName: 'Bash',
        status: 'error',
        startedAt: now - 16_000,
        completedAt: now - 4_000,
        error: 'Exit code 7 intentional hook ui failure',
      },
    ],
    chatHistory: [
      { role: 'user', content: '请运行这个命令，并只报告结果，不要修复：\n\nnode -e "console.error(\'intentional hook ui failure\'); process.exit(7)"', timestamp: now - 22_000 },
      {
        role: 'assistant',
        content: '',
        timestamp: now - 16_000,
        toolCalls: [
          {
            toolUseId: 'tooluse_intentional_failure',
            toolName: 'Bash',
            toolInput: 'command: node -e "console.error(\'intentional hook ui failure\'); process.exit(7)"',
            status: 'error',
            result: 'Exit code 7\nintentional hook ui failure',
          },
        ],
        trailingContent: '命令以 exit code 7 退出，stderr 输出：`intentional hook ui failure`。',
      },
    ],
  })
}

function recordedPlanSession(now: number): SessionState {
  return recordedSession(now, {
    phase: 'waiting_approval',
    description: 'PlanApproval: ExitPlanMode generated a plan and is waiting for approval.',
    planTitle: 'AgentBro Hook UI Lab — 视觉统一计划',
    planContent: `# AgentBro Hook UI Lab — 视觉统一计划

## Context

为测试灵动岛交互，需要 Mock 三个 UI 组件的视觉表现，统一设计语言：
1. 权限详情卡
2. 底部审批栏
3. 浏览器 Hook UI Lab

## Plan

1. 用真实 Claude Code hook payload 构造回放 case。
2. 保持浏览器和 Tauri 走同一套 NotchPanel / Overlay / ChatView 组件。
3. 为 Permission、Question、Plan、Bash failure、Compact 增加稳定场景。`,
    planPermissions: ['Edit: src/components/dev/ClaudeHookUiLab.tsx', 'Bash: pnpm test:run'],
    lastToolName: 'ExitPlanMode',
    lastToolStatus: 'running',
    unattendedSince: now - 18_000,
    chatHistory: [
      { role: 'user', content: '请为“统一 AgentBro 权限详情卡、底部审批栏和浏览器 Hook UI Lab 的视觉表现”制定一个实现计划。', timestamp: now - 40_000 },
      { role: 'assistant', content: '这是一个纯 Mock UI 计划，不需要探索现有代码库。直接进入设计阶段。', timestamp: now - 24_000 },
    ],
  })
}

function recordedQuestionSession(now: number): SessionState {
  return recordedSession(now, {
    phase: 'waiting_input',
    description: 'AskUserQuestion: single choice UI sampling mode.',
    pendingQuestion: {
      header: 'UI 采样模式',
      question: '接下来优先采样哪个视图？',
      options: ['Overlay', 'Detail', 'Compact'],
      descriptions: ['检查弹层卡片', '检查详情页聊天流', '检查微药丸和折叠态'],
      multiSelect: false,
      questions: [
        {
          header: 'UI 采样模式',
          question: '接下来优先采样哪个视图？',
          multiSelect: false,
          options: [
            { label: 'Overlay', description: '检查弹层卡片' },
            { label: 'Detail', description: '检查详情页聊天流' },
            { label: 'Compact', description: '检查微药丸和折叠态' },
          ],
        },
      ],
    },
    lastToolName: 'AskUserQuestion',
    lastToolStatus: 'running',
    unattendedSince: now - 8_000,
    chatHistory: [
      { role: 'user', content: '请调用 AskUserQuestion，让我选择下一步 UI 采样模式。', timestamp: now - 18_000 },
      {
        role: 'tool_use',
        toolName: 'AskUserQuestion',
        toolUseId: 'tooluse_single_question',
        toolInput: 'question: 接下来优先采样哪个视图？\noptions: Overlay, Detail, Compact',
        status: 'running',
        timestamp: now - 8_000,
      },
    ],
  })
}

function recordedMultiQuestionSession(now: number): SessionState {
  return recordedSession(now, {
    phase: 'waiting_input',
    description: 'AskUserQuestion: multi-select component sampling.',
    pendingQuestion: {
      header: '组件选择',
      question: '请选择本轮需要采样的 UI 组件（可多选）：',
      options: ['权限详情卡', '底部审批栏', 'Hook 事件列表', '手动'],
      descriptions: [
        'PermissionCard - 风险等级、操作描述、diff',
        'ApprovalBar - 拒绝 / 允许一次 / 始终允许',
        '浏览器 Hook UI Lab 场景列表',
        '手动输入其他组件',
      ],
      multiSelect: true,
      questions: [
        {
          header: '组件选择',
          question: '请选择本轮需要采样的 UI 组件（可多选）：',
          multiSelect: true,
          options: [
            { label: '权限详情卡', description: 'PermissionCard - 风险等级、操作描述、diff' },
            { label: '底部审批栏', description: 'ApprovalBar - 拒绝 / 允许一次 / 始终允许' },
            { label: 'Hook 事件列表', description: '浏览器 Hook UI Lab 场景列表' },
            { label: '手动', description: '手动输入其他组件' },
          ],
        },
      ],
    },
    lastToolName: 'AskUserQuestion',
    lastToolStatus: 'running',
    unattendedSince: now - 9_000,
  })
}

function recordedSubagentSession(now: number): SessionState {
  return recordedSession(now, {
    phase: 'done',
    description: '子 Agent 报告：getToken、refreshToken、空 token 分支和 async Promise 路径缺少测试覆盖。',
    responseText: '子 Agent 报告：getToken、refreshToken、空 token 分支和 async Promise 路径缺少测试覆盖。',
    lastUserMessage: '请启动一个子 Agent 或等价的子任务来检查 src/auth 目录里的测试覆盖。',
    sessionTitle: 'Subagent coverage check',
    lastToolName: 'Agent',
    lastToolStatus: 'success',
    subagents: [
      {
        agentId: 'tooluse_agent_coverage',
        agentType: 'Explore',
        description: '检查 src/auth 测试覆盖缺口',
        startedAt: now - 22_000,
        completedAt: now - 8_000,
        status: 'completed',
        tools: ['Read', 'Grep'],
        lastAssistantMessage: '未覆盖 getToken()、refreshToken()、buildAuthHeaders() 空 token 分支。',
      },
    ],
    chatHistory: [
      { role: 'user', content: '请启动一个子 Agent 或等价的子任务来检查 src/auth 目录里的测试覆盖。子任务只需要总结缺口，不要修改文件。', timestamp: now - 26_000 },
      {
        role: 'assistant',
        content: '',
        timestamp: now - 20_000,
        toolCalls: [
          {
            toolUseId: 'tooluse_agent_coverage',
            toolName: 'Agent',
            toolInput: 'description: 检查 src/auth 测试覆盖缺口\nsubagent_type: Explore',
            status: 'success',
            result: '[object Object]',
          },
        ],
        trailingContent: '子 Agent 报告如下：\n\n**未覆盖：**\n- `getToken()` - 完全无测试\n- `refreshToken()` - 完全无测试\n- `buildAuthHeaders()` 空 token 分支\n- async Promise 路径导致当前测试失败',
      },
    ],
  })
}

function recordedCompactSession(now: number): SessionState {
  return recordedSession(now, {
    phase: 'compacting',
    description: 'PreCompact completed successfully after /compact.',
    statusLineText: 'Compacted (ctrl+o to see full summary)',
    lastToolName: 'Compacting',
    lastToolStatus: 'success',
    chatHistory: [
      { role: 'user', content: '接下来我要测试上下文压缩 hook。请在我触发压缩前，先总结当前会话里已经做过的 UI 采样场景。', timestamp: now - 58_000 },
      {
        role: 'assistant',
        content: '本次会话已完成的 UI 采样场景：文件读取、文件编辑、Bash 成功/失败、PlanApproval、AskUserQuestion、Subagent。',
        timestamp: now - 50_000,
      },
      {
        role: 'tool_use',
        toolName: 'Compacting',
        toolInput: '/compact\nPreCompact [/Users/shirenchuang/.evolab-desktop/bin/evolab-bridge --source claude-code] completed successfully',
        status: 'success',
        timestamp: now - 7_000,
      },
    ],
  })
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

function emptyIdleSessions(): SessionState[] {
  return []
}

function multiSessionMixedSessions(now: number): SessionState[] {
  return [
    withSessionId(preToolSession(now), 'lab-multi-processing', {
      project: 'agentbro',
      sessionTitle: 'Running focused tests',
      lastUserMessage: '跑一下灵动岛回归测试',
    }),
    withSessionId(permissionSession(now), 'lab-multi-permission', {
      project: 'frontend',
      sessionTitle: 'Edit island state styles',
      unattendedSince: now - 72_000,
    }),
    withSessionId(questionSession(now), 'lab-multi-question', {
      project: 'release',
      sessionTitle: 'Choose release target',
      unattendedSince: now - 42_000,
    }),
    withSessionId(stopFailureSession(now), 'lab-multi-error', {
      project: 'hooks',
      sessionTitle: 'Hook recovery failed',
    }),
    withSessionId(recordedSubagentSession(now), 'lab-multi-subagent', {
      project: 'audit',
      sessionTitle: 'Subagent coverage scan',
    }),
  ]
}

function responseOverlaySession(now: number): SessionState {
  return baseSession(now, {
    phase: 'processing',
    description: 'Assistant produced a response while the session stays active.',
    responseText: `我已经完成第一轮检查，发现两个风险点：

- 颜色外观只影响 CSS token，不应该覆盖 alert / feedback 状态。
- response overlay 在任务未结束时也会弹出，需要单独验证。

接下来继续跑 UI 场景矩阵。`,
    lastUserMessage: '检查灵动岛状态覆盖情况，先给我一个中间结论。',
    sessionTitle: 'Mid-run response overlay',
    lastToolName: 'Read',
    lastToolStatus: 'success',
    chatHistory: [
      { role: 'user', content: '检查灵动岛状态覆盖情况，先给我一个中间结论。', timestamp: now - 34_000 },
      {
        role: 'assistant',
        content: '我已经完成第一轮检查，发现两个风险点：颜色外观和状态样式需要分开验证。',
        timestamp: now - 3_000,
      },
    ],
  })
}

function overlayQueueSessions(now: number): SessionState[] {
  return [
    withSessionId(permissionSession(now), 'lab-queue-permission', {
      project: 'auth',
      sessionTitle: 'Permission has highest priority',
      unattendedSince: now - 66_000,
    }),
    withSessionId(planSession(now), 'lab-queue-plan', {
      project: 'planner',
      sessionTitle: 'Plan waits behind permission',
      unattendedSince: now - 54_000,
    }),
    withSessionId(questionSession(now), 'lab-queue-question', {
      project: 'release',
      sessionTitle: 'Question waits behind plan',
      unattendedSince: now - 38_000,
    }),
  ]
}

function longContentSession(now: number): SessionState {
  const longDiff: DiffContent = {
    filePath: '/Users/demo/projects/agentbro/src/components/notch/super-long-path-for-regression/ExtremelyLongPermissionCardAndDynamicIslandLayoutRegressionFixture.tsx',
    lines: [
      { type: 'context', lineNumber: 118, content: 'const previous = buildIslandAppearancePreview(colorTheme, scenario, viewport)' },
      { type: 'remove', lineNumber: 119, content: 'return previous.render({ compact: true })' },
      { type: 'add', lineNumber: 119, content: 'return previous.render({ compact: true, preserveAlertState: true, allowLongLocalizedLabels: true })' },
      { type: 'context', lineNumber: 120, content: '}' },
      { type: 'add', lineNumber: 121, content: 'const stressLabel = "这是一个非常长的中文说明，用来验证灵动岛授权卡片在长路径、长 diff、长按钮标签下不会溢出。"' },
    ],
  }

  return baseSession(now, {
    phase: 'waiting_approval',
    description: 'Stress test for long permission content, long file names, and wrapped copy.',
    pendingPermission: {
      toolName: 'MultiEdit',
      toolInput: JSON.stringify({
        file_path: longDiff.filePath,
        edits: [
          {
            old_string: 'return previous.render({ compact: true })',
            new_string: 'return previous.render({ compact: true, preserveAlertState: true, allowLongLocalizedLabels: true })',
          },
        ],
      }, null, 2),
      diff: longDiff,
    },
    lastUserMessage: '补齐灵动岛 UI Lab 的长内容压力场景，确认不要溢出。',
    sessionTitle: 'Long content permission stress',
    lastToolName: 'MultiEdit',
    lastToolTarget: longDiff.filePath,
    lastToolStatus: 'running',
    unattendedSince: now - 84_000,
  })
}

function statusBadgeSession(now: number): SessionState {
  return baseSession(now, {
    phase: 'error',
    project: 'agentbro',
    description: 'Rate limit warning, YOLO badge, error badge, and unattended red state.',
    sessionTitle: 'Status badge matrix',
    lastUserMessage: '验证折叠态的状态徽标是否都能看见。',
    lastToolName: 'Bash',
    lastToolTarget: 'pnpm test:run src/test/notchPanel.test.tsx',
    lastToolStatus: 'error',
    isYoloMode: true,
    unattendedSince: now - 125_000,
    rateLimits: {
      fiveHourUsage: 91,
      fiveHourRemaining: '23m',
      sevenDayUsage: 63,
      sevenDayRemaining: '2d4h',
    },
    activeTools: [
      {
        toolUseId: 'lab-status-test',
        toolName: 'Bash',
        status: 'error',
        startedAt: now - 55_000,
        completedAt: now - 14_000,
        error: 'Visual regression mismatch in collapsed badge row',
      },
    ],
  })
}

const CLAUDE_HOOK_LAB_SCENARIOS: LabScenario[] = [
  {
    id: 'empty-idle',
    hookName: 'Island Empty / Idle',
    title: 'Empty',
    caption: 'no sessions: brand empty, idle, micro/compact shell',
    buildSessions: emptyIdleSessions,
  },
  {
    id: 'multi-session-mixed',
    hookName: 'Island Multi Session',
    title: 'Multi Mix',
    caption: 'mixed processing, waiting, error, done, and subagent rows',
    buildSessions: multiSessionMixedSessions,
  },
  {
    id: 'response-overlay',
    hookName: 'Island Response Overlay',
    title: 'Response',
    caption: 'non-blocking response overlay while session is still processing',
    buildSession: responseOverlaySession,
  },
  {
    id: 'overlay-queue',
    hookName: 'Island Overlay Queue',
    title: 'Queue',
    caption: 'permission > plan > question blocking overlay priority',
    buildSessions: overlayQueueSessions,
  },
  {
    id: 'long-content',
    hookName: 'Island Long Content',
    title: 'Long',
    caption: 'long file path, long diff, and wrapped permission content',
    buildSession: longContentSession,
  },
  {
    id: 'status-badges',
    hookName: 'Island Status Badges',
    title: 'Badges',
    caption: 'rate limit, YOLO, error, and unattended collapsed badges',
    buildSession: statusBadgeSession,
  },
  {
    id: 'real-edit-pending',
    hookName: 'Real Edit Permission',
    title: 'Real Edit Wait',
    caption: 'recorded PermissionRequest from e336326d, replayed as pending Edit',
    buildSession: recordedEditPendingSession,
  },
  {
    id: 'real-edit-allowed',
    hookName: 'Real Edit Auto-Allow',
    title: 'Real Edit Done',
    caption: 'actual transcript result: hook_permission_decision allow',
    buildSession: recordedEditAllowedSession,
  },
  {
    id: 'real-bash-pending',
    hookName: 'Real Bash Permission',
    title: 'Real Bash Wait',
    caption: 'recorded npm install Bash permission checkpoint',
    buildSession: recordedBashPendingSession,
  },
  {
    id: 'real-plan',
    hookName: 'Real PlanApproval',
    title: 'Real Plan',
    caption: 'recorded ExitPlanMode pending plan approval',
    buildSession: recordedPlanSession,
  },
  {
    id: 'real-question',
    hookName: 'Real AskUserQuestion',
    title: 'Real Question',
    caption: 'recorded single-choice AskUserQuestion',
    buildSession: recordedQuestionSession,
  },
  {
    id: 'real-multi-question',
    hookName: 'Real Multi Question',
    title: 'Real Multi Q',
    caption: 'recorded multi-select AskUserQuestion',
    buildSession: recordedMultiQuestionSession,
  },
  {
    id: 'real-compact',
    hookName: 'Real PreCompact',
    title: 'Real Compact',
    caption: 'recorded /compact PreCompact success checkpoint',
    buildSession: recordedCompactSession,
  },
  {
    id: 'real-write',
    hookName: 'Real Write',
    title: 'Real Write',
    caption: 'recorded Write tool while creating the sample project',
    buildSession: recordedWriteSession,
  },
  {
    id: 'real-read',
    hookName: 'Real Read',
    title: 'Real Read',
    caption: 'recorded three-file Read summary',
    buildSession: recordedReadSession,
  },
  {
    id: 'real-bash-failure',
    hookName: 'Real Bash Failure',
    title: 'Real Bash Fail',
    caption: 'recorded intentional exit code 7 failure',
    buildSession: recordedBashFailureSession,
  },
  {
    id: 'real-subagent',
    hookName: 'Real Subagent',
    title: 'Real Agent',
    caption: 'recorded Agent subtask coverage summary',
    buildSession: recordedSubagentSession,
  },
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

function defaultPanelState(session: SessionState | undefined, viewMode: LabViewMode): PanelState {
  if (viewMode === 'detail') return 'expanded'
  if (viewMode === 'list') return 'hover'
  if (viewMode === 'compact') return 'collapsed'
  if (!session) return 'collapsed'
  if (session.phase === 'done') return 'collapsed'
  return 'hover'
}

function applyClaudeHookLabScenario(scenarioId: LabScenarioId, viewMode: LabViewMode = 'overlay') {
  const scenario = CLAUDE_HOOK_LAB_SCENARIOS.find((candidate) => candidate.id === scenarioId)
  if (!scenario) return

  const now = Date.now()
  const sessions = scenario.buildSessions?.(now) ?? (scenario.buildSession ? [scenario.buildSession(now)] : [])
  resetLabState()
  useSessionStore.getState().replaceAllSessions(sessions, { suppressed: viewMode === 'compact' })
  const store = useSessionStore.getState()
  store.setActiveSession(sessions[0]?.id ?? null)
  scenario.configureStore?.(now, sessions)

  if (viewMode === 'detail' || viewMode === 'list') {
    for (const session of sessions) {
      store.clearSessionOverlays(session.id)
    }
  }

  store.setPanelState(defaultPanelState(sessions[0], viewMode))
}

export function ClaudeHookUiLab({ children }: ClaudeHookUiLabProps) {
  const [selectedScenarioId, setSelectedScenarioId] = useState<LabScenarioId>('real-edit-pending')
  const [viewMode, setViewMode] = useState<LabViewMode>('overlay')
  const [replayToken, setReplayToken] = useState(0)
  const activeOverlay = useSessionStore((state) => state.activeOverlay)
  const activeSession = useSessionStore((state) => state.activeSessionId ? state.sessions[state.activeSessionId] : undefined)
  const colorTheme = useThemeStore((state) => state.colorTheme)
  const setColorTheme = useThemeStore((state) => state.setColorTheme)

  useEffect(() => {
    const previousAutoCollapse = useConfigStore.getState().autoCollapse
    useConfigStore.setState({ autoCollapse: false })

    return () => {
      useConfigStore.setState({ autoCollapse: previousAutoCollapse })
    }
  }, [])

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

  const applyColorAppearance = useCallback((themeId: string) => {
    setColorTheme(themeId)
    setReplayToken((token) => token + 1)
  }, [setColorTheme])

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

        <section className="claude-hook-lab__island-themes" aria-label="灵动岛颜色外观">
          <div className="claude-hook-lab__section-label">灵动岛颜色外观</div>
          <div className="claude-hook-lab__island-theme-grid">
            {COLOR_THEMES.map((theme) => (
              <button
                key={theme.id}
                className="claude-hook-lab__island-theme"
                type="button"
                data-active={colorTheme === theme.id}
                title={`${theme.labelZh} · ${theme.tag}`}
                onClick={() => applyColorAppearance(theme.id)}
              >
                <span className="claude-hook-lab__island-theme-swatch" style={{ background: theme.bg }}>
                  <span className="claude-hook-lab__island-theme-notch" style={{ background: theme.card }} />
                  <span className="claude-hook-lab__island-theme-dot" style={{ background: theme.accent }} />
                </span>
                <span className="claude-hook-lab__island-theme-name">{theme.labelZh}</span>
                <span className="claude-hook-lab__island-theme-tag">{theme.tag}</span>
              </button>
            ))}
          </div>
        </section>

        <footer className="claude-hook-lab__status">
          <span>{activeScenario.caption}</span>
          <span>灵动岛外观: {COLOR_THEMES.find((theme) => theme.id === colorTheme)?.labelZh ?? colorTheme}</span>
          <span>session: {activeSession?.id === RECORDED_SESSION_ID ? 'real e336326d' : activeSession?.id ?? 'none'}</span>
          <span>phase: {activeSession?.phase ?? 'none'}</span>
          <span>tool: {activeSession?.lastToolName ?? 'none'}</span>
          <span>overlay: {activeOverlay?.type ?? 'none'}</span>
        </footer>
      </aside>
    </div>
  )
}
