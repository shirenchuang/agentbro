import { useState, useCallback } from 'react'
import type { FileTreeNode } from '../../services/skillApi'
import { useSkillStore } from '../../stores/skillStore'

interface FileTreeViewerProps {
  tree: FileTreeNode
}

export function FileTreeViewer({ tree }: FileTreeViewerProps) {
  const { fileContent, selectedFilePath, loadFileContent } = useSkillStore()

  return (
    <div>
      <div className="file-tree">
        <TreeNode node={tree} depth={0} onSelect={loadFileContent} selectedPath={selectedFilePath} />
      </div>
      {fileContent && (
        <div className="file-preview">{fileContent}</div>
      )}
    </div>
  )
}

interface TreeNodeProps {
  node: FileTreeNode
  depth: number
  onSelect: (path: string) => void
  selectedPath: string
}

function TreeNode({ node, depth, onSelect, selectedPath }: TreeNodeProps) {
  const [expanded, setExpanded] = useState(depth < 2)
  const isDir = node.nodeType === 'dir'
  const isSelected = node.path === selectedPath

  const handleClick = useCallback(() => {
    if (isDir) {
      setExpanded(e => !e)
    } else {
      onSelect(node.path)
    }
  }, [isDir, node.path, onSelect])

  return (
    <div>
      <div
        className={`file-tree-item ${isSelected ? 'file-tree-item--selected' : ''}`}
        style={{ paddingLeft: depth * 16 }}
        onClick={handleClick}
      >
        <span className="file-tree-item__icon">
          {isDir ? (expanded ? '📂' : '📁') : '📄'}
        </span>
        {node.name}
      </div>
      {isDir && expanded && node.children && (
        <div className="file-tree-children">
          {node.children.map(child => (
            <TreeNode
              key={child.path}
              node={child}
              depth={depth + 1}
              onSelect={onSelect}
              selectedPath={selectedPath}
            />
          ))}
        </div>
      )}
    </div>
  )
}
