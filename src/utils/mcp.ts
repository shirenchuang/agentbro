/* AgentBro — MCP tool name parsing and display utilities */

export interface McpToolParsed {
  isMcp: boolean
  server: string
  tool: string
  displayServer: string
  displayTool: string
}

const MCP_ALIASES: Record<string, string> = {
  'AgentOutputTool': 'Await Agent',
  'query-docs': 'Query Docs',
  'resolve-library-id': 'Resolve Library',
  'ast_grep_search': 'AST Search',
  'ast_grep_replace': 'AST Replace',
  'lsp_diagnostics': 'LSP Diagnostics',
  'lsp_goto_definition': 'Go to Definition',
  'lsp_find_references': 'Find References',
  'lsp_hover': 'Hover Info',
  'lsp_rename': 'Rename Symbol',
  'python_repl': 'Python REPL',
  'notepad_read': 'Read Notepad',
  'notepad_write_working': 'Write Notepad',
  'session_search': 'Search Sessions',
  'project_memory_read': 'Read Memory',
  'project_memory_write': 'Write Memory',
}

const SERVER_ALIASES: Record<string, string> = {
  'context7': 'Context7',
  'oh-my-claudecode': 'OMC',
  'chrome-devtools-mcp': 'DevTools',
  'plugin_context7_context7': 'Context7',
  'plugin_oh-my-claudecode_t': 'OMC',
}

export function humanize(s: string): string {
  return s
    .replace(/[-_]+/g, ' ')
    .replace(/\b\w/g, (c) => c.toUpperCase())
}

export function parseMcpTool(toolName: string): McpToolParsed {
  // Match mcp__<server>__<tool> pattern
  const match = toolName.match(/^mcp__([^_]+(?:_[^_]+)*)__(.+)$/)
  if (!match) {
    return {
      isMcp: false,
      server: '',
      tool: toolName,
      displayServer: '',
      displayTool: MCP_ALIASES[toolName] ?? toolName,
    }
  }

  const server = match[1]
  const tool = match[2]
  const displayServer = SERVER_ALIASES[server] ?? humanize(server)
  const displayTool = MCP_ALIASES[tool] ?? humanize(tool)

  return { isMcp: true, server, tool, displayServer, displayTool }
}
