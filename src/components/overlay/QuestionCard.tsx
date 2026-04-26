import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { OverlayItem, SessionState } from '../../types/agent'
import { OverlayCard } from './OverlayCard'
import './QuestionCard.css'

interface QuestionOption {
  label: string
  description?: string
}

interface QuestionData {
  question: string
  options: (string | QuestionOption)[]
  multiSelect?: boolean
}

interface QuestionCardProps {
  overlay: OverlayItem
  session: SessionState
  onAnswer: (answer: string) => void
  onDismiss: () => void
}

function normalizeOption(opt: string | QuestionOption): QuestionOption {
  return typeof opt === 'string' ? { label: opt } : opt
}

export function QuestionCard({ overlay, session, onAnswer, onDismiss }: QuestionCardProps) {
  const { t } = useTranslation()
  const data = overlay.data as QuestionData
  const options = (data.options || []).map(normalizeOption)
  const isMulti = data.multiSelect ?? false

  const [selected, setSelected] = useState<Set<number>>(new Set())
  const [customText, setCustomText] = useState('')
  const [showCustom, setShowCustom] = useState(false)

  const handleOptionClick = (index: number) => {
    if (isMulti) {
      setSelected((prev) => {
        const next = new Set(prev)
        if (next.has(index)) next.delete(index)
        else next.add(index)
        return next
      })
    } else {
      onAnswer(options[index].label)
    }
  }

  const handleConfirm = () => {
    const selectedLabels = [...selected].map(i => options[i].label)
    onAnswer(selectedLabels.join(', '))
  }

  const handleCustomSubmit = () => {
    const val = customText.trim()
    if (val) onAnswer(val)
  }

  return (
    <OverlayCard session={session} onDismiss={onDismiss}>
      {/* Question text */}
      <div className="question-card__text">{data.question}</div>

      {/* Options */}
      <div className="question-card__options">
        {options.map((opt, i) => (
          <button
            key={i}
            className={`question-card__option ${isMulti && selected.has(i) ? 'question-card__option--selected' : ''}`}
            onClick={() => handleOptionClick(i)}
          >
            {isMulti && (
              <span className="question-card__check">
                {selected.has(i) ? '\u2611' : '\u2610'}
              </span>
            )}
            {i < 3 && !isMulti && <kbd className="question-card__shortcut">{'\u2318'}{i + 1}</kbd>}
            <span className="question-card__option-label">{opt.label}</span>
            {opt.description && (
              <span className="question-card__option-desc">{opt.description}</span>
            )}
          </button>
        ))}
      </div>

      {/* Multi-select confirm button */}
      {isMulti && selected.size > 0 && (
        <button className="question-card__confirm" onClick={handleConfirm}>
          {t('notch.confirmSelection', { defaultValue: 'Confirm' })} ({selected.size})
        </button>
      )}

      {/* Custom text input */}
      {showCustom ? (
        <div className="question-card__custom">
          <input
            className="question-card__input"
            placeholder={t('notch.typeReply')}
            value={customText}
            onChange={(e) => setCustomText(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter') handleCustomSubmit() }}
            autoFocus
          />
        </div>
      ) : (
        <button
          className="question-card__custom-toggle"
          onClick={() => setShowCustom(true)}
        >
          {t('notch.customAnswer', { defaultValue: 'Other...' })}
        </button>
      )}
    </OverlayCard>
  )
}
