/* Diff View — Code diff with colored backgrounds, left borders, and collapsible long diffs */
import { useState } from 'react'
import type { DiffContent } from '../../types/agent'
import './DiffView.css'

const COLLAPSE_THRESHOLD = 20

interface DiffViewProps {
  diff: DiffContent
}

export function DiffView({ diff }: DiffViewProps) {
  const adds = diff.lines.filter(l => l.type === 'add').length
  const removes = diff.lines.filter(l => l.type === 'remove').length
  const isLong = diff.lines.length > COLLAPSE_THRESHOLD
  const [collapsed, setCollapsed] = useState(isLong)

  const visibleLines = collapsed ? diff.lines.slice(0, COLLAPSE_THRESHOLD) : diff.lines
  const hiddenCount = diff.lines.length - COLLAPSE_THRESHOLD

  return (
    <div className="diff-view glass-card">
      <div className="diff-view__header">
        <button
          className="diff-view__filepath-btn"
          onClick={isLong ? () => setCollapsed(c => !c) : undefined}
          aria-expanded={isLong ? !collapsed : undefined}
          style={{ cursor: isLong ? 'pointer' : 'default' }}
        >
          {isLong && (
            <svg
              className={`diff-view__chevron ${collapsed ? '' : 'diff-view__chevron--open'}`}
              width="9" height="9" viewBox="0 0 16 16" fill="none"
            >
              <path d="M6 4l4 4-4 4" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          )}
          <span className="diff-view__filepath">{diff.filePath}</span>
        </button>
        <span className="diff-view__stats">
          {adds > 0 && <span className="diff-view__stats-add">+{adds}</span>}
          {removes > 0 && <span className="diff-view__stats-remove">-{removes}</span>}
        </span>
      </div>

      <div className="diff-view__lines glass-scroll">
        {visibleLines.map((line, i) => (
          <div key={i} className={`diff-view__line diff-view__line--${line.type}`}>
            <span className="diff-view__line-num">{line.lineNumber}</span>
            <span className="diff-view__line-prefix">
              {line.type === 'add' ? '+' : line.type === 'remove' ? '-' : ' '}
            </span>
            <span className="diff-view__line-content selectable">{line.content}</span>
          </div>
        ))}
        {collapsed && hiddenCount > 0 && (
          <button className="diff-view__expand-btn" onClick={() => setCollapsed(false)}>
            ↕ Show {hiddenCount} more line{hiddenCount !== 1 ? 's' : ''}
          </button>
        )}
      </div>
    </div>
  )
}
