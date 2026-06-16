import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import '@testing-library/jest-dom/vitest'
import { FileTreeViewer } from '../components/skills/FileTreeViewer'
import type { FileTreeNode } from '../services/skillApi'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}))

const tree: FileTreeNode = {
  name: 'pptx',
  path: '/skills/pptx',
  nodeType: 'dir',
  children: [
    { name: 'SKILL.md', path: '/skills/pptx/SKILL.md', nodeType: 'file', children: null },
    { name: 'clean.py', path: '/skills/pptx/scripts/clean.py', nodeType: 'file', children: null },
    { name: 'package.json', path: '/skills/pptx/package.json', nodeType: 'file', children: null },
  ],
}

describe('FileTreeViewer', () => {
  it('renders deterministic file-type icons instead of emoji document glyphs', () => {
    const { container } = render(<FileTreeViewer tree={tree} />)

    expect(screen.getByText('SKILL.md')).toBeInTheDocument()
    expect(container.querySelector('.file-tree-row__icon--markdown')).not.toBeNull()
    expect(container.querySelector('.file-tree-row__icon--python')).not.toBeNull()
    expect(container.querySelector('.file-tree-row__icon--json')).not.toBeNull()
    expect(container.querySelector('.file-tree-row__icon')?.textContent).not.toContain('📄')
  })
})
