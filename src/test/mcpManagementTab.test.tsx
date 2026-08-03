import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import '@testing-library/jest-dom/vitest'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { McpManagementTab } from '../components/skills-v2/McpManagementTab'
import i18n from '../i18n'
import {
  skillApiV2,
  type AgentDetail,
  type McpInspectionReport,
  type McpInventory,
  type McpServerEntry,
} from '../services/skillApiV2'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(undefined) }))

const detail: AgentDetail = {
  id: 'codex',
  displayName: 'Codex',
  iconKey: 'codex',
  version: null,
  latestVersion: null,
  skillsDir: '/Users/me/.codex/skills',
  configPath: '/Users/me/.codex/config.toml',
  mcpConfigPath: '/Users/me/.codex/config.toml',
  pluginDir: null,
  skills: [],
  inheritedManagedSkills: [],
  inheritedUnmanagedSkills: [],
  appliedPacks: [],
  availablePacks: [],
  mcpServers: [],
  plugins: [],
  health: [],
}

function server(overrides: Partial<McpServerEntry> = {}): McpServerEntry {
  return {
    name: 'context7',
    transport: 'stdio',
    command: 'npx',
    args: ['-y', '@upstash/context7-mcp'],
    env: [{
      key: 'API_TOKEN',
      value: null,
      secret: true,
      configured: true,
    }],
    cwd: null,
    url: null,
    headers: [],
    enabled: true,
    disabledByAgentbro: false,
    valid: true,
    message: 'MCP configuration is valid',
    warnings: [],
    configPath: '/Users/me/.codex/config.toml',
    editable: true,
    sourceKind: 'configured',
    ...overrides,
  }
}

function inventory(servers: McpServerEntry[] = [server()]): McpInventory {
  return {
    agentId: 'codex',
    configPath: '/Users/me/.codex/config.toml',
    revision: 'sha256:one',
    capabilities: {
      editable: true,
      supportsStdio: true,
      supportsHttp: true,
      supportsSse: false,
      supportsNativeToggle: true,
    },
    servers,
  }
}

function inspectionReport(
  inspectionId = 'inspection-one',
  overrides: Partial<McpInspectionReport> = {},
): McpInspectionReport {
  return {
    inspectionId,
    status: 'connected',
    category: 'connected',
    summary: 'Connected · 1 tool · 1 resource · 1 prompt',
    inspectedAtMs: 1_700_000_000_000,
    durationMs: 42,
    protocolVersion: '2025-11-25',
    serverName: 'remote-mcp',
    serverVersion: '1.0.0',
    transport: 'http',
    capabilities: {
      tools: true,
      resources: true,
      prompts: true,
      logging: false,
    },
    tools: [{
      name: 'search',
      title: 'Search',
      description: 'Search connected data.',
      inputs: [{
        name: 'query',
        valueType: 'string',
        description: 'Search query',
        required: true,
      }],
      inputSchema: {
        type: 'object',
        properties: {
          query: { type: 'string' },
        },
        required: ['query'],
      },
      outputSchema: null,
      annotations: {
        readOnly: true,
        destructive: false,
        idempotent: true,
        openWorld: false,
      },
      hasAnnotations: true,
    }],
    resources: [{
      uri: 'demo://knowledge',
      name: 'Knowledge',
      title: null,
      description: 'Demo resource',
      mimeType: 'application/json',
      size: null,
    }],
    prompts: [{
      name: 'summarize',
      title: null,
      description: 'Summarize data',
      arguments: [{
        name: 'topic',
        description: null,
        required: true,
      }],
    }],
    steps: [
      { phase: 'connect', status: 'success', durationMs: 8, message: 'Connected' },
      { phase: 'tools', status: 'success', durationMs: 10, message: 'Discovered 1 tool' },
    ],
    warnings: [],
    suggestions: [],
    ...overrides,
  }
}

describe('McpManagementTab', () => {
  beforeEach(async () => {
    cleanup()
    vi.restoreAllMocks()
    await i18n.changeLanguage('zh')
  })

  afterEach(cleanup)

  it('edits without exposing a configured secret and preserves it on save', async () => {
    vi.spyOn(skillApiV2, 'listMcpInventory').mockResolvedValue(inventory())
    vi.spyOn(skillApiV2, 'validateMcpServerDraft').mockResolvedValue({
      valid: true,
      message: 'ok',
      warnings: [],
    })
    const save = vi.spyOn(skillApiV2, 'saveMcpServer').mockResolvedValue(inventory())

    render(<McpManagementTab detail={detail} />)
    expect(await screen.findByText('context7')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '编辑' }))

    expect(screen.queryByRole('combobox')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '本地命令' })).toHaveAttribute('aria-pressed', 'true')
    const secretInput = screen.getByLabelText('环境变量 值 1')
    expect(secretInput).toHaveAttribute('type', 'password')
    expect(secretInput).toHaveValue('')
    expect(secretInput).toHaveAttribute('placeholder', '已配置，留空则保持不变')
    fireEvent.click(screen.getByRole('button', { name: '保存' }))

    await waitFor(() => expect(save).toHaveBeenCalledWith(
      'codex',
      expect.objectContaining({
        name: 'context7',
        env: [expect.objectContaining({
          key: 'API_TOKEN',
          value: null,
          configured: true,
        })],
      }),
      'sha256:one',
      'context7',
    ))
    expect(await screen.findByText('已保存 MCP「context7」')).toBeInTheDocument()
  })

  it('adds HTTP servers and supports test, toggle, and confirmed deletion', async () => {
    const empty = inventory([])
    const remote = server({
      name: 'remote',
      transport: 'http',
      command: null,
      args: [],
      env: [],
      url: 'https://example.com/mcp',
      headers: [],
    })
    vi.spyOn(skillApiV2, 'listMcpInventory').mockResolvedValue(empty)
    vi.spyOn(skillApiV2, 'validateMcpServerDraft').mockResolvedValue({
      valid: true,
      message: 'ok',
      warnings: [],
    })
    const save = vi.spyOn(skillApiV2, 'saveMcpServer').mockResolvedValue(inventory([remote]))
    const inspect = vi.spyOn(skillApiV2, 'inspectMcpServer').mockImplementation(
      async (_agent, _serverName, inspectionId) => inspectionReport(inspectionId),
    )
    vi.spyOn(skillApiV2, 'cancelMcpInspection').mockResolvedValue()
    const toggle = vi.spyOn(skillApiV2, 'setMcpServerEnabled').mockResolvedValue(
      inventory([{ ...remote, enabled: false }]),
    )
    const remove = vi.spyOn(skillApiV2, 'deleteMcpServer').mockResolvedValue(inventory([]))

    render(<McpManagementTab detail={detail} />)
    fireEvent.click(await screen.findByRole('button', { name: '新增 MCP' }))
    fireEvent.change(screen.getByLabelText('名称'), { target: { value: 'remote' } })
    fireEvent.click(screen.getByRole('button', { name: 'HTTP' }))
    expect(screen.getByRole('button', { name: 'HTTP' })).toHaveAttribute('aria-pressed', 'true')
    fireEvent.change(screen.getByLabelText('服务地址'), {
      target: { value: 'https://example.com/mcp' },
    })
    fireEvent.click(screen.getByRole('button', { name: '保存' }))

    await waitFor(() => expect(save).toHaveBeenCalledWith(
      'codex',
      expect.objectContaining({
        name: 'remote',
        transport: 'http',
        command: null,
        url: 'https://example.com/mcp',
      }),
      'sha256:one',
      null,
    ))

    fireEvent.click(screen.getByRole('button', { name: '检查' }))
    await waitFor(() => expect(inspect).toHaveBeenCalledWith(
      'codex',
      'remote',
      expect.any(String),
    ))
    expect(
      (await screen.findAllByText('Connected · 1 tool · 1 resource · 1 prompt')).length,
    ).toBeGreaterThan(0)
    fireEvent.click(screen.getByRole('tab', { name: /Tools/ }))
    expect(screen.getByText('Search')).toBeInTheDocument()
    expect(screen.getByText('只读')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('switch', { name: '启用或停用 remote' }))
    await waitFor(() => expect(toggle).toHaveBeenCalledWith('codex', 'remote', 'sha256:one', false))

    fireEvent.click(screen.getByRole('button', { name: '删除' }))
    const dialog = screen.getByRole('dialog', { name: '删除 MCP「remote」？' })
    fireEvent.click(within(dialog).getByRole('button', { name: '确认删除' }))
    await waitFor(() => expect(remove).toHaveBeenCalledWith('codex', 'remote', 'sha256:one'))
    expect(await screen.findByText('还没有 MCP 服务器')).toBeInTheDocument()
  })

  it('renders resource, prompt, and structured log discovery without invoking tools', async () => {
    vi.spyOn(skillApiV2, 'listMcpInventory').mockResolvedValue(inventory())
    const inspect = vi.spyOn(skillApiV2, 'inspectMcpServer').mockImplementation(
      async (_agent, _serverName, inspectionId) => inspectionReport(inspectionId),
    )
    vi.spyOn(skillApiV2, 'cancelMcpInspection').mockResolvedValue()

    render(<McpManagementTab detail={detail} />)
    fireEvent.click(await screen.findByRole('button', { name: '检查' }))
    await waitFor(() => expect(inspect).toHaveBeenCalledTimes(1))

    fireEvent.click(screen.getByRole('tab', { name: /Resources/ }))
    expect(screen.getByText('Knowledge')).toBeInTheDocument()
    expect(screen.getByText('demo://knowledge')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('tab', { name: /Prompts/ }))
    expect(screen.getByText('summarize')).toBeInTheDocument()
    expect(screen.getByText('topic')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('tab', { name: '日志' }))
    expect(screen.getByText('启动或连接')).toBeInTheDocument()
    expect(screen.getByText('发现 Tools')).toBeInTheDocument()
  })

  it('renders Tools as a compact searchable list without repeating parameter prose', async () => {
    const baseTool = inspectionReport().tools[0]
    vi.spyOn(skillApiV2, 'listMcpInventory').mockResolvedValue(inventory())
    vi.spyOn(skillApiV2, 'inspectMcpServer').mockImplementation(
      async (_agent, _serverName, inspectionId) => inspectionReport(inspectionId, {
        tools: [{
          ...baseTool,
          name: 'get_scene_info',
          title: null,
          description: 'Get detailed scene information. Parameters: - user_prompt: telemetry context',
          inputs: [{
            name: 'user_prompt',
            valueType: 'string',
            description: 'Telemetry context',
            required: true,
          }],
          inputSchema: {
            type: 'object',
            properties: {
              user_prompt: { type: 'string' },
            },
            required: ['user_prompt'],
          },
          annotations: {
            readOnly: null,
            destructive: null,
            idempotent: null,
            openWorld: null,
          },
          hasAnnotations: false,
        }],
      }),
    )
    vi.spyOn(skillApiV2, 'cancelMcpInspection').mockResolvedValue()

    render(<McpManagementTab detail={detail} />)
    fireEvent.click(await screen.findByRole('button', { name: '检查' }))
    await waitFor(() => expect(screen.getByRole('tab', { name: /Tools/ })).toBeInTheDocument())
    fireEvent.click(screen.getByRole('tab', { name: /Tools/ }))

    expect(screen.getByText('Get detailed scene information.')).toBeInTheDocument()
    expect(screen.queryByText(/Parameters:/i)).not.toBeInTheDocument()
    expect(screen.getByText('user_prompt')).toBeInTheDocument()
    expect(document.querySelector('.sm2__mcp-tool-risk-summary')).toHaveTextContent('1 风险未知')

    fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'missing' } })
    expect(screen.queryByText('get_scene_info')).not.toBeInTheDocument()
    expect(screen.getByText('没有匹配的 Tool')).toBeInTheDocument()
  })

  it('runs a Tool in a one-shot operation and renders the result', async () => {
    vi.spyOn(skillApiV2, 'listMcpInventory').mockResolvedValue(inventory())
    vi.spyOn(skillApiV2, 'inspectMcpServer').mockImplementation(
      async (_agent, _serverName, inspectionId) => inspectionReport(inspectionId),
    )
    vi.spyOn(skillApiV2, 'cancelMcpInspection').mockResolvedValue()
    vi.spyOn(skillApiV2, 'cancelMcpOperation').mockResolvedValue()
    const call = vi.spyOn(skillApiV2, 'callMcpTool').mockResolvedValue({
      operationId: 'tool-operation',
      kind: 'tool',
      name: 'search',
      category: 'success',
      durationMs: 31,
      result: {
        content: [{ type: 'text', text: 'Found one result' }],
        structuredContent: { count: 1 },
        isError: false,
      },
      warnings: [],
    })

    render(<McpManagementTab detail={detail} />)
    fireEvent.click(await screen.findByRole('button', { name: '检查' }))
    await waitFor(() => expect(screen.getByRole('tab', { name: /Tools/ })).toBeInTheDocument())
    fireEvent.click(screen.getByRole('tab', { name: /Tools/ }))
    fireEvent.click(screen.getByRole('button', { name: '测试调用' }))
    fireEvent.change(screen.getByLabelText('调用参数（JSON）'), {
      target: { value: '{"query":"Blender"}' },
    })
    fireEvent.click(screen.getByRole('button', { name: '调用 Tool' }))

    await waitFor(() => expect(call).toHaveBeenCalledWith(
      'codex',
      'context7',
      expect.any(String),
      'search',
      { query: 'Blender' },
    ))
    expect(await screen.findByText('Found one result')).toBeInTheDocument()
    expect(screen.getByText('结构化结果')).toBeInTheDocument()
  })

  it('previews a Prompt without sending it to a model', async () => {
    vi.spyOn(skillApiV2, 'listMcpInventory').mockResolvedValue(inventory())
    vi.spyOn(skillApiV2, 'inspectMcpServer').mockImplementation(
      async (_agent, _serverName, inspectionId) => inspectionReport(inspectionId),
    )
    vi.spyOn(skillApiV2, 'cancelMcpInspection').mockResolvedValue()
    vi.spyOn(skillApiV2, 'cancelMcpOperation').mockResolvedValue()
    const preview = vi.spyOn(skillApiV2, 'getMcpPrompt').mockResolvedValue({
      operationId: 'prompt-operation',
      kind: 'prompt',
      name: 'summarize',
      category: 'success',
      durationMs: 17,
      result: {
        description: 'Generated prompt',
        messages: [{
          role: 'user',
          content: { type: 'text', text: 'Summarize Blender' },
        }],
      },
      warnings: [],
    })

    render(<McpManagementTab detail={detail} />)
    fireEvent.click(await screen.findByRole('button', { name: '检查' }))
    await waitFor(() => expect(screen.getByRole('tab', { name: /Prompts/ })).toBeInTheDocument())
    fireEvent.click(screen.getByRole('tab', { name: /Prompts/ }))
    fireEvent.click(screen.getByRole('button', { name: '预览 Prompt' }))
    fireEvent.change(screen.getByLabelText(/topic/), { target: { value: 'Blender' } })
    fireEvent.click(screen.getByRole('button', { name: '生成预览' }))

    await waitFor(() => expect(preview).toHaveBeenCalledWith(
      'codex',
      'context7',
      expect.any(String),
      'summarize',
      { topic: 'Blender' },
    ))
    expect(await screen.findByText('Summarize Blender')).toBeInTheDocument()
    expect(screen.getByText('user')).toBeInTheDocument()
  })

  it('cancels a running inspection when the drawer closes', async () => {
    vi.spyOn(skillApiV2, 'listMcpInventory').mockResolvedValue(inventory())
    let inspectionId = ''
    vi.spyOn(skillApiV2, 'inspectMcpServer').mockImplementation(
      async (_agent, _serverName, nextInspectionId) => {
        inspectionId = nextInspectionId
        return await new Promise<McpInspectionReport>(() => {})
      },
    )
    const cancel = vi.spyOn(skillApiV2, 'cancelMcpInspection').mockResolvedValue()

    render(<McpManagementTab detail={detail} />)
    fireEvent.click(await screen.findByRole('button', { name: '检查' }))
    await waitFor(() => expect(inspectionId).not.toBe(''))
    fireEvent.click(screen.getByRole('button', { name: '关闭' }))

    await waitFor(() => expect(cancel).toHaveBeenCalledWith(inspectionId))
  })
})
