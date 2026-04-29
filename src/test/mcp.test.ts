import { describe, it, expect } from 'vitest'
import { parseMcpTool, humanize } from '../utils/mcp'

describe('humanize', () => {
  it('capitalizes words separated by dashes', () => {
    expect(humanize('foo-bar')).toBe('Foo Bar')
  })

  it('capitalizes words separated by underscores', () => {
    expect(humanize('ast_grep_search')).toBe('Ast Grep Search')
  })

  it('handles already-capitalized strings', () => {
    expect(humanize('FooBar')).toBe('FooBar')
  })
})

describe('parseMcpTool — non-MCP tools', () => {
  it('returns isMcp=false for plain tool names', () => {
    const result = parseMcpTool('Bash')
    expect(result.isMcp).toBe(false)
    expect(result.tool).toBe('Bash')
    expect(result.server).toBe('')
  })

  it('applies alias for known non-MCP tool', () => {
    const result = parseMcpTool('AgentOutputTool')
    expect(result.displayTool).toBe('Await Agent')
  })

  it('falls back to raw tool name for unknown non-MCP tool', () => {
    const result = parseMcpTool('UnknownTool')
    expect(result.displayTool).toBe('UnknownTool')
  })
})

describe('parseMcpTool — MCP tools', () => {
  it('parses mcp__server__tool pattern', () => {
    const result = parseMcpTool('mcp__context7__query-docs')
    expect(result.isMcp).toBe(true)
    expect(result.server).toBe('context7')
    expect(result.tool).toBe('query-docs')
  })

  it('applies server alias', () => {
    const result = parseMcpTool('mcp__context7__query-docs')
    expect(result.displayServer).toBe('Context7')
  })

  it('applies tool alias for known MCP tool', () => {
    const result = parseMcpTool('mcp__context7__query-docs')
    expect(result.displayTool).toBe('Query Docs')
  })

  it('humanizes unknown MCP tool name', () => {
    const result = parseMcpTool('mcp__my-server__do-something')
    expect(result.displayTool).toBe('Do Something')
  })

  it('humanizes unknown server name', () => {
    const result = parseMcpTool('mcp__my-custom-server__tool')
    expect(result.displayServer).toBe('My Custom Server')
  })
})
