/* Diff View — Code diff with syntax highlighting colors and stats */
import type { DiffContent } from '../../types/agent'
import './DiffView.css'

interface DiffViewProps {
  diff: DiffContent
}

export function DiffView({ diff }: DiffViewProps) {
  const adds = diff.lines.filter(l => l.type === 'add').length
  const removes = diff.lines.filter(l => l.type === 'remove').length

  return (
    <div className="diff-view glass-card">
      <div className="diff-view__header">
        <span className="diff-view__filepath">{diff.filePath}</span>
        <span className="diff-view__stats">
          {adds > 0 && <span className="diff-view__stats-add">+{adds}</span>}
          {removes > 0 && <span className="diff-view__stats-remove">-{removes}</span>}
        </span>
      </div>
      <div className="diff-view__lines glass-scroll">
        {diff.lines.map((line, i) => (
          <div key={i} className={`diff-view__line diff-view__line--${line.type}`}>
            <span className="diff-view__line-num">{line.lineNumber}</span>
            <span className="diff-view__line-prefix">
              {line.type === 'add' ? '+' : line.type === 'remove' ? '-' : ' '}
            </span>
            <span className="diff-view__line-content selectable">{line.content}</span>
          </div>
        ))}
      </div>
    </div>
  )
}
