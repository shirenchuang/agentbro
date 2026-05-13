import { useState, useCallback } from 'react'
import { useTranslation } from 'react-i18next'
import { AnimatePresence, motion } from 'framer-motion'
import type { FileTreeNode } from '../../services/skillApi'
import { FileContentViewer } from './FileContentViewer'

const BINARY_EXTENSIONS = new Set([
  'png', 'jpg', 'jpeg', 'gif', 'bmp', 'ico', 'webp', 'svg',
  'zip', 'tar', 'gz', 'bz2', '7z', 'rar',
  'pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx',
  'exe', 'dll', 'so', 'dylib', 'bin', 'dat',
  'woff', 'woff2', 'ttf', 'eot', 'otf',
  'mp3', 'mp4', 'wav', 'avi', 'mov', 'webm',
])

function isBinaryFile(name: string): boolean {
  const ext = name.split('.').pop()?.toLowerCase() ?? ''
  return BINARY_EXTENSIONS.has(ext)
}

interface FileTreeViewerProps {
  tree: FileTreeNode
  onViewingFileChange?: (viewing: boolean) => void
}

export function FileTreeViewer({ tree, onViewingFileChange }: FileTreeViewerProps) {
  const { t } = useTranslation()
  const [selectedFile, setSelectedFile] = useState<{ name: string; path: string } | null>(null)
  const [binaryNotice, setBinaryNotice] = useState<string | null>(null)

  const handleFileClick = useCallback((node: FileTreeNode) => {
    if (node.nodeType === 'dir') return

    if (isBinaryFile(node.name)) {
      setBinaryNotice(node.name)
      return
    }

    setBinaryNotice(null)
    setSelectedFile({ name: node.name, path: node.path })
    onViewingFileChange?.(true)
  }, [onViewingFileChange])

  const handleBack = useCallback(() => {
    setSelectedFile(null)
    onViewingFileChange?.(false)
  }, [onViewingFileChange])

  return (
    <AnimatePresence mode="wait" initial={false}>
      {selectedFile === null ? (
        <motion.div
          key="tree"
          initial={{ opacity: 0, x: -20 }}
          animate={{ opacity: 1, x: 0 }}
          exit={{ opacity: 0, x: -20 }}
          transition={{ duration: 0.2, ease: 'easeOut' }}
        >
          <div className="file-tree-container">
            {tree.children?.map(child => (
              <TreeRow
                key={child.path}
                node={child}
                depth={0}
                onFileClick={handleFileClick}
              />
            )) ?? (
              <TreeRow node={tree} depth={0} onFileClick={handleFileClick} />
            )}
          </div>
          {binaryNotice && (
            <div className="file-viewer-warning" style={{ marginTop: 8 }}>
              {binaryNotice} — {t('skills.binaryFile')}
            </div>
          )}
        </motion.div>
      ) : (
        <motion.div
          key="viewer"
          initial={{ opacity: 0, x: 20 }}
          animate={{ opacity: 1, x: 0 }}
          exit={{ opacity: 0, x: 20 }}
          transition={{ duration: 0.2, ease: 'easeOut' }}
        >
          <FileContentViewer
            fileName={selectedFile.name}
            filePath={selectedFile.path}
            onBack={handleBack}
          />
        </motion.div>
      )}
    </AnimatePresence>
  )
}

function TreeRow({
  node,
  depth,
  onFileClick,
}: {
  node: FileTreeNode
  depth: number
  onFileClick: (node: FileTreeNode) => void
}) {
  const [expanded, setExpanded] = useState(depth < 1)
  const isDir = node.nodeType === 'dir'
  const paddingLeft = 8 + depth * 16

  if (isDir) {
    return (
      <>
        <button
          className="file-tree-row"
          style={{ paddingLeft }}
          onClick={() => setExpanded(e => !e)}
        >
          <span className={`file-tree-row__chevron ${expanded ? 'file-tree-row__chevron--open' : ''}`}>
            ›
          </span>
          <span className="file-tree-row__icon">
            {expanded ? '📂' : '📁'}
          </span>
          <span className="file-tree-row__name file-tree-row__name--dir">{node.name}</span>
        </button>
        {expanded && node.children?.map(child => (
          <TreeRow
            key={child.path}
            node={child}
            depth={depth + 1}
            onFileClick={onFileClick}
          />
        ))}
      </>
    )
  }

  return (
    <button
      className="file-tree-row"
      style={{ paddingLeft: paddingLeft + 18 }}
      onClick={() => onFileClick(node)}
    >
      <span className="file-tree-row__icon">📄</span>
      <span className="file-tree-row__name">{node.name}</span>
    </button>
  )
}
