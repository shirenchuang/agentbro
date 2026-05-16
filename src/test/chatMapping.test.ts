import { describe, expect, it } from 'vitest'
import { mapParsedMessages } from '../hooks/useTauri'
import type { ParsedMessage } from '../services/tauriApi'

describe('mapParsedMessages', () => {
  it('attaches image blocks to the user message text', () => {
    const parsed: ParsedMessage[] = [{
      id: 'm1',
      role: 'user',
      timestamp: '2026-05-13T07:00:00Z',
      blocks: [
        { type: 'text', text: 'What is in this image?' },
        { type: 'image', source: 'data:image/png;base64,abc123' },
      ],
    }]

    expect(mapParsedMessages(parsed)).toMatchObject([
      {
        role: 'user',
        content: 'What is in this image?',
        images: ['data:image/png;base64,abc123'],
      },
    ])
  })

  it('creates an image-only message when no text block is present', () => {
    const parsed: ParsedMessage[] = [{
      id: 'm2',
      role: 'user',
      timestamp: null,
      blocks: [
        { type: 'image', source: 'https://example.com/image.png' },
      ],
    }]

    expect(mapParsedMessages(parsed)).toMatchObject([
      {
        role: 'user',
        content: '',
        images: ['https://example.com/image.png'],
      },
    ])
  })

  it('groups consecutive assistant process blocks like the island chat history', () => {
    const parsed: ParsedMessage[] = [
      {
        id: 'a1',
        role: 'assistant',
        timestamp: '2026-05-13T07:00:00Z',
        blocks: [
          { type: 'thinking', thinking: 'Need to inspect files.' },
          { type: 'tool_use', id: 'tool-1', name: 'Read', input: { file_path: 'src/App.tsx' } },
        ],
      },
      {
        id: 'u-tool',
        role: 'user',
        timestamp: '2026-05-13T07:00:01Z',
        blocks: [
          { type: 'tool_result', toolUseId: 'tool-1', content: 'file contents', isError: false },
        ],
      },
      {
        id: 'a2',
        role: 'assistant',
        timestamp: '2026-05-13T07:00:02Z',
        blocks: [
          { type: 'text', text: 'Done.' },
        ],
      },
    ]

    expect(mapParsedMessages(parsed)).toMatchObject([
      {
        role: 'assistant',
        content: '',
        thinking: 'Need to inspect files.',
        thinkingCount: 1,
        trailingContent: 'Done.',
        toolCalls: [{
          toolUseId: 'tool-1',
          toolName: 'Read',
          result: 'file contents',
          status: 'success',
        }],
      },
    ])
  })

  it('adds inline diffs for edit and write tool history', () => {
    const parsed: ParsedMessage[] = [{
      id: 'a-edit',
      role: 'assistant',
      timestamp: '2026-05-13T07:00:00Z',
      blocks: [
        {
          type: 'tool_use',
          id: 'tool-edit',
          name: 'Edit',
          input: {
            file_path: 'src/App.tsx',
            old_string: 'old line',
            new_string: 'new line',
          },
        },
      ],
    }]

    expect(mapParsedMessages(parsed)).toMatchObject([
      {
        role: 'assistant',
        toolCalls: [{
          toolName: 'Edit',
          diff: {
            filePath: 'src/App.tsx',
            lines: [
              { type: 'remove', content: 'old line' },
              { type: 'add', content: 'new line' },
            ],
          },
        }],
      },
    ])
  })
})
