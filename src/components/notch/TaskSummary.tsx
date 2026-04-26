/* TaskSummary — Checkbox-style task list with collapsible completed items */
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { TaskInfo } from '../../types/agent'
import './TaskSummary.css'

interface TaskSummaryProps {
  tasks: TaskInfo[]
}

export function TaskSummary({ tasks }: TaskSummaryProps) {
  const { t } = useTranslation()
  const [showCompleted, setShowCompleted] = useState(false)

  const completed = tasks.filter(t => t.status === 'completed')
  const inProgress = tasks.filter(t => t.status === 'in_progress')
  const pending = tasks.filter(t => t.status === 'pending')

  // Show: in-progress first, then pending, then completed (collapsed by default)
  const visibleCompleted = showCompleted ? completed : completed.slice(0, 1)
  const hiddenCompletedCount = completed.length - visibleCompleted.length

  return (
    <div className="task-summary">
      <div className="task-summary__header">
        <span className="task-summary__label">
          {t('notch.tasks', {
            completed: completed.length,
            inProgress: inProgress.length,
            pending: pending.length,
          })}
        </span>
      </div>

      <div className="task-summary__list">
        {/* In-progress tasks */}
        {inProgress.map((task) => (
          <div key={task.id} className="task-summary__item task-summary__item--in-progress">
            <span className="task-summary__bullet task-summary__bullet--active" />
            <span className="task-summary__name">{task.name}</span>
          </div>
        ))}

        {/* Pending tasks */}
        {pending.map((task) => (
          <div key={task.id} className="task-summary__item task-summary__item--pending">
            <span className="task-summary__checkbox" />
            <span className="task-summary__name">{task.name}</span>
          </div>
        ))}

        {/* Completed tasks (collapsible) */}
        {visibleCompleted.map((task) => (
          <div key={task.id} className="task-summary__item task-summary__item--completed">
            <span className="task-summary__checkbox task-summary__checkbox--checked">
              <svg width="10" height="10" viewBox="0 0 12 12" fill="none">
                <path d="M2 6l3 3 5-5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
              </svg>
            </span>
            <span className="task-summary__name task-summary__name--done">{task.name}</span>
          </div>
        ))}

        {/* "+N completed" toggle */}
        {hiddenCompletedCount > 0 && (
          <button
            className="task-summary__toggle"
            onClick={(e) => { e.stopPropagation(); setShowCompleted(!showCompleted) }}
          >
            +{hiddenCompletedCount} {t('notch.completedCollapsed', { defaultValue: '\u5DF2\u5B8C\u6210' })}
          </button>
        )}

        {/* Collapse button when expanded */}
        {showCompleted && completed.length > 1 && (
          <button
            className="task-summary__toggle"
            onClick={(e) => { e.stopPropagation(); setShowCompleted(false) }}
          >
            {t('notch.collapseCompleted', { defaultValue: '\u6536\u8D77' })}
          </button>
        )}
      </div>
    </div>
  )
}
