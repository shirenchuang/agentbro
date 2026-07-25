import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import '@testing-library/jest-dom/vitest'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { PluginManagementTab } from '../components/skills-v2/PluginManagementTab'
import i18n from '../i18n'
import {
  skillApiV2,
  type AgentDetail,
  type PluginDetail,
  type PluginInventory,
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
  pluginDir: '/Users/me/.codex/plugins/cache',
  skills: [],
  appliedPacks: [],
  availablePacks: [],
  mcpServers: [],
  plugins: [],
  health: [],
}

function inventory(overrides: Partial<PluginInventory> = {}): PluginInventory {
  return {
    agentId: 'codex',
    configPath: '/Users/me/.codex/config.toml',
    revision: 'sha256:one',
    capabilities: {
      editable: true,
      requiresNewSession: true,
    },
    plugins: [
      {
        id: 'documents@openai-primary-runtime',
        name: 'Documents',
        version: '26.723.12215',
        enabled: true,
        source: 'codex-plugin:openai-primary-runtime',
      },
      {
        id: 'sites@openai-bundled',
        name: 'Sites',
        version: '0.1.31',
        enabled: false,
        source: 'codex-plugin:openai-bundled',
      },
    ],
    ...overrides,
  }
}

function pluginDetail(overrides: Partial<PluginDetail> = {}): PluginDetail {
  return {
    ...inventory().plugins[0],
    description: 'Create, edit, and inspect documents.',
    author: 'OpenAI',
    homepage: 'https://openai.com/',
    license: 'Proprietary',
    installPath: '/Users/me/.codex/plugins/cache/openai-primary-runtime/documents/26.723.12215',
    manifestPath: '/Users/me/.codex/plugins/cache/openai-primary-runtime/documents/26.723.12215/.codex-plugin/plugin.json',
    files: {
      name: '26.723.12215',
      nodeType: 'directory',
      path: '.',
      omittedCount: null,
      children: [
        {
          name: '.codex-plugin',
          nodeType: 'directory',
          path: '.codex-plugin',
          omittedCount: null,
          children: [
            {
              name: 'backend-specific-skill.md',
              nodeType: 'file',
              path: '.codex-plugin/backend-specific-skill.md',
              children: null,
              omittedCount: null,
            },
          ],
        },
        {
          name: 'node_modules',
          nodeType: 'directory',
          path: 'node_modules',
          children: [],
          omittedCount: 27,
        },
        {
          name: 'preview.png',
          nodeType: 'file',
          path: 'preview.png',
          children: null,
          omittedCount: null,
        },
      ],
    },
    fileCount: 4,
    truncated: false,
    ...overrides,
  }
}

describe('PluginManagementTab', () => {
  beforeEach(async () => {
    cleanup()
    vi.restoreAllMocks()
    await i18n.changeLanguage('zh')
  })

  afterEach(cleanup)

  it('loads installed plugins, filters them, and toggles with the inventory revision', async () => {
    vi.spyOn(skillApiV2, 'listPluginInventory').mockResolvedValue(inventory())
    const getPluginDetail = vi.spyOn(skillApiV2, 'getPluginDetail').mockResolvedValue(pluginDetail())
    const readPluginFile = vi.spyOn(skillApiV2, 'readPluginFile').mockImplementation(
      async (_agentId, _pluginId, relativePath) => relativePath === 'preview.png'
        ? {
            path: relativePath,
            kind: 'image',
            mimeType: 'image/png',
            content: null,
            dataBase64: 'iVBORw0KGgo=',
            size: 8,
            truncated: false,
          }
        : {
            path: relativePath,
            kind: 'text',
            mimeType: 'text/plain; charset=utf-8',
            content: [
              '---',
              'name: control-in-app-browser',
              'description: Plugin document tools',
              '---',
              '# Document tools',
              '',
              'Use **documents** safely.',
            ].join('\n'),
            dataBase64: null,
            size: 92,
            truncated: false,
          },
    )
    const toggle = vi.spyOn(skillApiV2, 'setPluginEnabled').mockResolvedValue(inventory({
      revision: 'sha256:two',
      plugins: inventory().plugins.map((plugin) => (
        plugin.id === 'documents@openai-primary-runtime'
          ? { ...plugin, enabled: false }
          : plugin
      )),
    }))

    render(<PluginManagementTab detail={detail} />)
    expect(await screen.findByText('Documents')).toBeInTheDocument()
    expect(screen.getByText('Sites')).toBeInTheDocument()
    expect(screen.getByText('新会话生效')).toBeInTheDocument()
    const summary = screen.getByLabelText('插件状态摘要')
    expect(within(summary).getByText('2')).toBeInTheDocument()

    expect(screen.queryByText('详情')).not.toBeInTheDocument()
    fireEvent.click(screen.getByText('Documents'))
    await waitFor(() => expect(getPluginDetail).toHaveBeenCalledWith(
      'codex',
      'documents@openai-primary-runtime',
    ))
    expect(await screen.findByText('Create, edit, and inspect documents.')).toBeInTheDocument()
    expect(screen.getByText('/Users/me/.codex/plugins/cache/openai-primary-runtime/documents/26.723.12215')).toBeInTheDocument()
    expect(document.querySelector('.sm2__slideover--plugin-detail')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('tab', { name: '文件 (4)' }))
    expect(screen.getByText('backend-specific-skill.md')).toBeInTheDocument()
    expect(screen.getByText('node_modules')).toBeInTheDocument()
    expect(screen.getByText('27 项')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'backend-specific-skill.md' }))
    await waitFor(() => expect(readPluginFile).toHaveBeenCalledWith(
      'codex',
      'documents@openai-primary-runtime',
      '.codex-plugin/backend-specific-skill.md',
    ))
    expect(await screen.findByRole('heading', { name: 'Document tools' })).toBeInTheDocument()
    expect(screen.getByText('documents')).toHaveProperty('tagName', 'STRONG')
    expect(screen.getByText('文档元信息')).toBeInTheDocument()
    expect(screen.getByText('control-in-app-browser')).toBeInTheDocument()
    expect(screen.getByText('Plugin document tools')).toBeInTheDocument()
    expect(screen.queryByText('description: Plugin document tools')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '预览' })).toHaveAttribute('aria-pressed', 'true')

    fireEvent.click(screen.getByRole('button', { name: '源码' }))
    expect(await screen.findByText(/description: Plugin document tools/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '源码' })).toHaveAttribute('aria-pressed', 'true')

    fireEvent.click(screen.getByRole('button', { name: 'preview.png' }))
    await waitFor(() => expect(readPluginFile).toHaveBeenCalledWith(
      'codex',
      'documents@openai-primary-runtime',
      'preview.png',
    ))
    expect(await screen.findByAltText('preview.png')).toHaveAttribute(
      'src',
      'data:image/png;base64,iVBORw0KGgo=',
    )

    fireEvent.click(screen.getByRole('button', { name: '关闭' }))
    await waitFor(() => expect(document.querySelector('.sm2__slideover--plugin-detail')).not.toBeInTheDocument())

    fireEvent.change(screen.getByRole('searchbox', { name: '搜索插件' }), {
      target: { value: 'bundled' },
    })
    expect(screen.queryByText('Documents')).not.toBeInTheDocument()
    expect(screen.getByText('Sites')).toBeInTheDocument()

    fireEvent.change(screen.getByRole('searchbox', { name: '搜索插件' }), {
      target: { value: '' },
    })
    fireEvent.click(screen.getByRole('button', { name: '已停用' }))
    expect(screen.queryByText('Documents')).not.toBeInTheDocument()
    expect(screen.getByText('Sites')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '全部' }))
    fireEvent.click(screen.getByRole('switch', { name: '启用或停用 Documents' }))
    await waitFor(() => expect(toggle).toHaveBeenCalledWith(
      'codex',
      'documents@openai-primary-runtime',
      'sha256:one',
      false,
    ))
    expect(getPluginDetail).toHaveBeenCalledTimes(1)
    const feedback = await screen.findByText('已停用插件「Documents」')
    expect(feedback.closest('.sm2__agent-toast-stack')).toHaveClass('sm2__agent-toast-stack--local')
  })

  it('shows an explicit read-only state without rendering fake switches', async () => {
    vi.spyOn(skillApiV2, 'listPluginInventory').mockResolvedValue(inventory({
      capabilities: {
        editable: false,
        requiresNewSession: true,
      },
    }))

    render(<PluginManagementTab detail={{ ...detail, id: 'custom-agent' }} />)
    expect(await screen.findByText(/插件配置暂时只读/)).toBeInTheDocument()
    expect(screen.queryAllByRole('switch')).toHaveLength(0)
    expect(screen.getAllByText('只读')).toHaveLength(2)
  })
})
