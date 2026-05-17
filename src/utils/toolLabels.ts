import { parseMcpTool } from './mcp'

type Translate = (key: string, options?: Record<string, unknown>) => string

const TOOL_LABEL_KEYS: Record<string, string> = {
  Edit: 'notch.tool.editing',
  Write: 'notch.tool.writing',
  Read: 'notch.tool.reading',
  ReadFile: 'notch.tool.reading',
  WriteFile: 'notch.tool.writing',
  EditFile: 'notch.tool.editing',
  Glob: 'notch.tool.searching',
  Grep: 'notch.tool.searching',
  GrepGrep: 'notch.tool.searching',
  GrepGlob: 'notch.tool.searching',
  Bash: 'notch.tool.running',
  Agent: 'notch.tool.tasking',
  Task: 'notch.tool.tasking',
  WebSearch: 'notch.tool.searching',
  WebFetch: 'notch.tool.fetching',
  LSP: 'notch.tool.analyzing',
  Skill: 'notch.tool.runningSkill',
  NotebookEdit: 'notch.tool.editing',
  EnterPlanMode: 'notch.tool.planning',
  ExitPlanMode: 'notch.tool.planning',
  AskUserQuestion: 'notch.tool.waitingForAnswer',
  TodoWrite: 'notch.tool.updatingTasks',
  TaskCreate: 'notch.tool.updatingTasks',
  TaskUpdate: 'notch.tool.updatingTasks',
  TaskList: 'notch.tool.updatingTasks',
  TaskGet: 'notch.tool.updatingTasks',
  TaskOutput: 'notch.tool.updatingTasks',
  TaskStop: 'notch.tool.updatingTasks',
  TeamCreate: 'notch.tool.coordinating',
  TeamDelete: 'notch.tool.coordinating',
  SendMessage: 'notch.tool.messaging',
  CronCreate: 'notch.tool.scheduling',
  CronDelete: 'notch.tool.scheduling',
  CronList: 'notch.tool.scheduling',
  ScheduleWakeup: 'notch.tool.scheduling',
  EnterWorktree: 'notch.tool.branching',
  ExitWorktree: 'notch.tool.branching',
  state_write: 'notch.tool.savingState',
  state_read: 'notch.tool.readingState',
  state_clear: 'notch.tool.clearingState',
  state_list_active: 'notch.tool.readingState',
  state_get_status: 'notch.tool.readingState',
  Compacting: 'notch.tool.compacting',
  'Compacting context': 'notch.tool.compactingContext',
}

export function getToolActivityLabel(t: Translate, toolName: string): string {
  const directKey = TOOL_LABEL_KEYS[toolName]
  if (directKey) return t(directKey)

  const parsed = parseMcpTool(toolName)
  if (!parsed.isMcp) return parsed.displayTool

  const mcpKey = TOOL_LABEL_KEYS[parsed.tool]
  const tool = mcpKey ? t(mcpKey) : parsed.displayTool
  return `${parsed.displayServer} — ${tool}`
}
