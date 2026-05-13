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
  const [content, setContent] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [mode, setMode] = useState<RenderMode>(
    hasRenderablePreview(fileName) ? 'preview' : 'source'
  )

  useEffect(() => {
    setLoading(true)
    setError(null)
    skillApi.readFileContent(filePath)
      .then(result => setContent(result))
      .catch(() => setError(t('skills.fileReadError')))
      .finally(() => setLoading(false))
  }, [filePath, t])

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
    if (isJsonFile(fileName)) {
      try {
        const formatted = JSON.stringify(JSON.parse(content), null, 2)
        return <SourceView content={formatted} />
      } catch { /* use raw */ }
    }
    return <SourceView content={content} />
  }

  if (isMarkdownFile(fileName)) {
    return (
      <div className="file-viewer-preview">
        <ReactMarkdown remarkPlugins={[remarkGfm]}>{content}</ReactMarkdown>
      </div>
    )
  }

  return <SourceView content={content} />
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
