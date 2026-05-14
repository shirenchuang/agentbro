import { useState, useEffect, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { skillApi } from '../../services/skillApi'

interface FileContentViewerProps {
  fileName: string
  filePath: string
  onBack: () => void
}

type RenderMode = 'source' | 'preview'
interface FileLoadState {
  filePath: string
  content: string | null
  loading: boolean
  error: string | null
}

const LARGE_FILE_THRESHOLD = 500 * 1024

function isMarkdownFile(name: string): boolean {
  const ext = name.split('.').pop()?.toLowerCase() ?? ''
  return ext === 'md' || ext === 'mdx'
}

function isJsonFile(name: string): boolean {
  const ext = name.split('.').pop()?.toLowerCase() ?? ''
  return ext === 'json'
}

function hasRenderablePreview(name: string): boolean {
  return isMarkdownFile(name)
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

export function FileContentViewer({ fileName, filePath, onBack }: FileContentViewerProps) {
  const { t } = useTranslation()
  const [fileState, setFileState] = useState<FileLoadState>({ filePath, content: null, loading: true, error: null })
  const [mode, setMode] = useState<RenderMode>(
    hasRenderablePreview(fileName) ? 'preview' : 'source'
  )

  useEffect(() => {
    let cancelled = false
    queueMicrotask(() => {
      if (cancelled) return
      setFileState({ filePath, content: null, loading: true, error: null })
      skillApi.readFileContent(filePath)
        .then(result => {
          if (!cancelled) setFileState({ filePath, content: result, loading: false, error: null })
        })
        .catch(() => {
          if (!cancelled) setFileState({ filePath, content: null, loading: false, error: t('skills.fileReadError') })
        })
    })
    return () => {
      cancelled = true
    }
  }, [filePath, t])

  const isCurrentFile = fileState.filePath === filePath
  const content = isCurrentFile ? fileState.content : null
  const loading = !isCurrentFile || fileState.loading
  const error = isCurrentFile ? fileState.error : null

  const pathParts = useMemo(() => {
    const parts = filePath.split('/')
    const last5 = parts.slice(-3)
    return last5
  }, [filePath])

  const showModeToggle = hasRenderablePreview(fileName)

  return (
    <div className="file-viewer">
      <div className="file-viewer-toolbar">
        <button className="file-viewer-back" onClick={onBack}>
          ←
        </button>
        <div className="file-viewer-breadcrumb">
          {pathParts.map((part, i) => (
            <span key={i} className="file-viewer-breadcrumb__segment">
              {i > 0 && <span className="file-viewer-breadcrumb__sep">/</span>}
              <span className={i === pathParts.length - 1 ? 'file-viewer-breadcrumb__active' : ''}>
                {part}
              </span>
            </span>
          ))}
        </div>
        <div style={{ flex: 1 }} />
        {showModeToggle && (
          <div className="file-viewer-mode-toggle">
            <button
              className={`file-viewer-mode-toggle__btn ${mode === 'source' ? 'file-viewer-mode-toggle__btn--active' : ''}`}
              onClick={() => setMode('source')}
            >
              Source
            </button>
            <button
              className={`file-viewer-mode-toggle__btn ${mode === 'preview' ? 'file-viewer-mode-toggle__btn--active' : ''}`}
              onClick={() => setMode('preview')}
            >
              Preview
            </button>
          </div>
        )}
      </div>

      <div className="file-viewer-content">
        {loading ? (
          <div className="file-viewer-status">
            <div className="skills-spinner" />
          </div>
        ) : error ? (
          <div className="file-viewer-status">{error}</div>
        ) : content !== null && content.length === 0 ? (
          <div className="file-viewer-status">{t('skills.fileEmpty')}</div>
        ) : content !== null && content.length > LARGE_FILE_THRESHOLD ? (
          <div>
            <div className="file-viewer-warning">
              {t('skills.fileLarge', { size: formatSize(content.length) })}
            </div>
            <SourceView content={content} />
          </div>
        ) : content !== null ? (
          <FileContentArea content={content} fileName={fileName} mode={mode} />
        ) : null}
      </div>
    </div>
  )
}

function FileContentArea({ content, fileName, mode }: { content: string; fileName: string; mode: RenderMode }) {
  if (mode === 'source') {
    let sourceContent = content
    if (isJsonFile(fileName)) {
      try {
        sourceContent = JSON.stringify(JSON.parse(content), null, 2)
      } catch { /* use raw */ }
    }
    return <SourceView content={sourceContent} />
  }

  if (isMarkdownFile(fileName)) {
    const fm = parseFrontmatter(content)
    const bodyContent = fm ? content.split('\n').slice(fm.endLine + 1).join('\n').trimStart() : content
    return (
      <div className="file-viewer-preview">
        {fm && (
          <div className="file-viewer-frontmatter">
            {fm.fields.name && <div className="file-viewer-frontmatter__name">{fm.fields.name}</div>}
            {fm.fields.description && <div className="file-viewer-frontmatter__desc">{fm.fields.description}</div>}
            {Object.entries(fm.fields).filter(([k]) => k !== 'name' && k !== 'description').length > 0 && (
              <div className="file-viewer-frontmatter__chips">
                {Object.entries(fm.fields)
                  .filter(([k]) => k !== 'name' && k !== 'description')
                  .map(([k, v]) => (
                    <span key={k} className="file-viewer-frontmatter__chip">
                      <em>{k}</em>{v}
                    </span>
                  ))}
              </div>
            )}
          </div>
        )}
        <ReactMarkdown remarkPlugins={[remarkGfm]}>{bodyContent}</ReactMarkdown>
      </div>
    )
  }

  return <SourceView content={content} />
}

function parseFrontmatter(content: string): { fields: Record<string, string>; endLine: number } | null {
  const lines = content.split('\n')
  if (lines[0]?.trim() !== '---') return null
  const end = lines.findIndex((l, i) => i > 0 && l.trim() === '---')
  if (end === -1) return null
  const fields: Record<string, string> = {}
  for (let i = 1; i < end; i++) {
    const m = lines[i].match(/^(\w+):\s*(.*)$/)
    if (m) fields[m[1]] = m[2]
  }
  return { fields, endLine: end }
}

function SourceView({ content }: { content: string }) {
  const lines = content.split('\n')

  return (
    <div className="file-viewer-source-wrap">
      <table className="file-viewer-source">
        <tbody>
          {lines.map((line, i) => (
            <tr key={i}>
              <td className="file-viewer-source__line-no">{i + 1}</td>
              <td className="file-viewer-source__code">{line || ' '}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
