import { useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { AnimatePresence, motion } from 'framer-motion'
import type { OverlayItem, SessionState } from '../../types/agent'
import { OverlayCard } from './OverlayCard'
import { setNotchFocusable } from '../../services/tauriApi'
import './QuestionCard.css'

interface QuestionOption {
  label: string
  description?: string | null
}

interface QuestionData {
  question: string
  options: (string | QuestionOption)[]
  multiSelect?: boolean
  questions?: Array<{ question: string; header?: string | null; options: QuestionOption[]; multiSelect?: boolean }>
}

interface QuestionCardProps {
  overlay: OverlayItem
  session: SessionState
  onAnswer: (answer: string) => void
  onShowSessions?: () => void
  onDismiss: () => void
  onDraftStateChange?: (hasDraft: boolean) => void
  sessionCount?: number
}

function normalizeOption(opt: string | QuestionOption): QuestionOption {
  return typeof opt === 'string' ? { label: opt } : opt
}

function parseQuestionTag(question: string): { tag: string | null; text: string } {
  const match = question.match(/^\[([^\]]+)\]\s*(.*)/s)
  if (match) return { tag: match[1], text: match[2] }
  return { tag: null, text: question }
}

function QuestionOptionRow({
  index, selected = false, label, description, multiSelect = false, onMouseDown,
}: {
  index: number; selected?: boolean; label: string; description?: string; multiSelect?: boolean; onMouseDown: () => void
}) {
  return (
    <div
      className={`question-card__option-row ${selected ? 'question-card__option-row--selected' : ''}`}
      onMouseDown={onMouseDown}
    >
      <span className={`question-card__option-index ${selected ? 'question-card__option-index--selected' : ''}`}>
        {multiSelect && selected ? '✓' : index + 1}
      </span>
      <div className="question-card__option-body">
        <div className="question-card__option-label">{label}</div>
        {description && <div className="question-card__option-desc">{description}</div>}
      </div>
      <svg className="question-card__option-chevron" width="12" height="12" viewBox="0 0 16 16" fill="none">
        <path d="M6 4l4 4-4 4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
      </svg>
    </div>
  )
}

function MultiQuestionView({ data, onAnswer, onDraftStateChange }: { data: QuestionData; onAnswer: (answer: string) => void; onDraftStateChange?: (hasDraft: boolean) => void }) {
  const { t } = useTranslation()
  const allQuestions = data.questions!
  const [selections, setSelections] = useState<Record<number, number | number[]>>({})
  const [customAnswers, setCustomAnswers] = useState<Record<number, string>>({})
  const [inputStates, setInputStates] = useState<Record<number, boolean>>({})
  const [inputTexts, setInputTexts] = useState<Record<number, string>>({})
  const [error, setError] = useState(false)
  const hasDraft = Object.values(inputTexts).some((text) => text.trim().length > 0)

  useEffect(() => {
    onDraftStateChange?.(hasDraft)
  }, [hasDraft, onDraftStateChange])

  useEffect(() => () => onDraftStateChange?.(false), [onDraftStateChange])

  const getAnswer = (qi: number) =>
    customAnswers[qi] ??
    (Array.isArray(selections[qi])
      ? (selections[qi] as number[]).map((idx) => allQuestions[qi].options[idx].label).join(', ')
      : selections[qi] !== undefined ? allQuestions[qi].options[selections[qi] as number].label : undefined)
  const allAnswered = allQuestions.every((_, i) => getAnswer(i) !== undefined)

  const handleSubmit = () => {
    if (!allAnswered) { setError(true); return }
    const answers: Record<string, string> = {}
    allQuestions.forEach((q, i) => { answers[q.question] = getAnswer(i)! })
    onAnswer(JSON.stringify(answers))
  }

  return (
    <div className="question-card__multi">
      <div className="question-card__multi-badge">
        {allQuestions.length} {t('notch.questionsCount', { defaultValue: 'questions' })}
      </div>

      <div className="question-card__multi-list">
        {allQuestions.map((q, qi) => {
          const parsed = parseQuestionTag(q.question)
          const tag = q.header ?? parsed.tag
          const text = q.header ? q.question : parsed.text
          const selectedForQuestion = selections[qi]
          const selectedSet = new Set(Array.isArray(selectedForQuestion) ? selectedForQuestion : selectedForQuestion !== undefined ? [selectedForQuestion] : [])
          return (
            <div key={qi} className="question-card__multi-item">
              <p className="question-card__multi-q">
                <span className="question-card__multi-num">{qi + 1}.</span>
                {tag && <span className="question-card__multi-tag">[{tag}]</span>}
                {text}
              </p>
              <div className="question-card__multi-opts">
                {q.options.map((opt, oi) => (
                  <button
                    key={oi}
                    className={`question-card__chip ${selectedSet.has(oi) && !customAnswers[qi] ? 'question-card__chip--selected' : ''}`}
                    onMouseDown={() => {
                      setSelections(prev => {
                        if (!q.multiSelect) return { ...prev, [qi]: oi }
                        const current = Array.isArray(prev[qi]) ? [...prev[qi] as number[]] : []
                        const next = current.includes(oi) ? current.filter((idx) => idx !== oi) : [...current, oi]
                        if (next.length === 0) {
                          const clone = { ...prev }
                          delete clone[qi]
                          return clone
                        }
                        return { ...prev, [qi]: next }
                      })
                      setCustomAnswers(prev => { const n = { ...prev }; delete n[qi]; return n })
                      setError(false)
                    }}
                  >
                    {opt.label}
                  </button>
                ))}
                {customAnswers[qi] && (
                  <span className="question-card__chip question-card__chip--custom">{customAnswers[qi]}</span>
                )}
              </div>
              {inputStates[qi] ? (
                <input
                  autoFocus
                  className="question-card__inline-input"
                  data-has-draft={(inputTexts[qi] ?? '').trim() ? 'true' : 'false'}
                  placeholder="Custom answer, press Enter..."
                  value={inputTexts[qi] ?? ''}
                  onChange={(e) => setInputTexts(prev => ({ ...prev, [qi]: e.target.value }))}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                      const trimmed = (inputTexts[qi] ?? '').trim()
                      if (trimmed) {
                        setCustomAnswers(prev => ({ ...prev, [qi]: trimmed }))
                        setSelections(prev => { const n = { ...prev }; delete n[qi]; return n })
                        setInputStates(prev => ({ ...prev, [qi]: false }))
                        setInputTexts(prev => ({ ...prev, [qi]: '' }))
                        setError(false)
                      }
                    } else if (e.key === 'Escape') {
                      setInputStates(prev => ({ ...prev, [qi]: false }))
                      setInputTexts(prev => ({ ...prev, [qi]: '' }))
                    }
                  }}
                />
              ) : (
                <div
                  className="question-card__custom-link"
                  onMouseDown={() => {
                    setNotchFocusable(true)
                    setInputStates(prev => ({ ...prev, [qi]: true }))
                  }}
                >
                  {'✏️'} Custom input...
                </div>
              )}
            </div>
          )
        })}
      </div>

      <button
        className="question-card__submit-all"
        onMouseDown={handleSubmit}
      >
        {'✓'} {t('notch.submitAll', { defaultValue: 'Submit All' })}
      </button>
      {error && (
        <div className="question-card__error">{'⚠'} Please answer all questions</div>
      )}
    </div>
  )
}

export function QuestionCard({ overlay, session, onAnswer, onShowSessions, onDismiss, onDraftStateChange, sessionCount }: QuestionCardProps) {
  const { t } = useTranslation()
  const data = overlay.data as QuestionData
  const options = (data.options || []).map(normalizeOption)
  const isMulti = data.multiSelect ?? false
  const isMultiQuestionSet = Boolean(data.questions && data.questions.length > 1)

  const [responseState, setResponseState] = useState<{
    overlayId: string
    selected: Set<number>
    showCustom: boolean
    customText: string
  }>({ overlayId: overlay.id, selected: new Set(), showCustom: false, customText: '' })
  const isCurrentOverlay = responseState.overlayId === overlay.id
  const selected = isCurrentOverlay ? responseState.selected : new Set<number>()
  const showCustom = isCurrentOverlay && responseState.showCustom
  const customText = isCurrentOverlay ? responseState.customText : ''
  const hasDraft = customText.trim().length > 0

  useEffect(() => {
    if (isMultiQuestionSet) return
    onDraftStateChange?.(hasDraft)
  }, [hasDraft, isMultiQuestionSet, onDraftStateChange])

  useEffect(() => {
    if (isMultiQuestionSet) return
    return () => onDraftStateChange?.(false)
  }, [isMultiQuestionSet, onDraftStateChange])

  useEffect(() => {
    return () => { setNotchFocusable(false) }
  }, [])

  if (isMultiQuestionSet) {
    return (
      <OverlayCard
        session={session}
        onDismiss={onDismiss}
        onShowSessions={onShowSessions}
        sessionCount={sessionCount}
        className="overlay-card--question"
        bodyClassName="question-card"
      >
        <MultiQuestionView data={data} onAnswer={onAnswer} onDraftStateChange={onDraftStateChange} />
      </OverlayCard>
    )
  }

  const { tag, text } = parseQuestionTag(data.question)

  const toggleIndex = (index: number) => {
    setResponseState(prev => {
      const current = prev.overlayId === overlay.id ? prev : { overlayId: overlay.id, selected: new Set<number>(), showCustom: false, customText: '' }
      const next = new Set(current.selected)
      if (next.has(index)) next.delete(index)
      else next.add(index)
      return { ...current, selected: next }
    })
  }

  const handleConfirm = () => {
    if (selected.size > 0) {
      const labels = [...selected].map(i => options[i].label)
      onAnswer(labels.join(', '))
    }
  }

  const handleCustomSubmit = () => {
    const val = customText.trim()
    if (val) {
      onAnswer(val)
      setResponseState({ overlayId: overlay.id, selected, showCustom: false, customText: '' })
      setNotchFocusable(false)
    }
  }

  return (
    <OverlayCard
      session={session}
      onDismiss={onDismiss}
      onShowSessions={onShowSessions}
      sessionCount={sessionCount}
      className="overlay-card--question"
      bodyClassName="question-card"
    >
      {/* Header */}
      <div className="question-card__header">
        <svg className="question-card__header-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.4} d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2v10z"/>
        </svg>
        <span className="question-card__header-title">{t('notch.questionTitle', { defaultValue: "Claude's Question" })}</span>
        <div className="question-card__header-badges">
          {isMulti && <span className="question-card__multi-badge">Multi-select</span>}
        </div>
      </div>

      {/* Question text */}
      <div className="question-card__text">
        {tag && <span className="question-card__tag">[{tag}]</span>}
        {text}
      </div>

      {/* Options with animation */}
      <div className="question-card__options">
        <AnimatePresence>
          {options.map((opt, i) => (
            <motion.div
              key={i}
              initial={{ opacity: 0, y: 4, scale: 0.98 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              transition={{ type: 'spring', duration: 0.25, bounce: 0.12, delay: i * 0.03 }}
            >
              <QuestionOptionRow
                index={i}
                selected={isMulti && selected.has(i)}
                label={opt.label}
                description={opt.description ?? undefined}
                multiSelect={isMulti}
                onMouseDown={() => {
                  if (isMulti) toggleIndex(i)
                  else onAnswer(opt.label)
                }}
              />
            </motion.div>
          ))}
        </AnimatePresence>

        {/* Custom text input */}
        {!showCustom ? (
          <motion.div
            className="question-card__custom-trigger"
            initial={{ opacity: 0, y: 6 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ type: 'spring', duration: 0.3, bounce: 0.15, delay: options.length * 0.04 }}
            onMouseDown={async () => {
              setResponseState({ overlayId: overlay.id, selected, showCustom: true, customText })
              await setNotchFocusable(true)
            }}
          >
            {'✏️'} {t('notch.typeHint', { defaultValue: 'Type something...' })}
          </motion.div>
        ) : (
          <form onSubmit={(e) => { e.preventDefault(); handleCustomSubmit() }}>
            <input
              autoFocus
              type="text"
              className="question-card__input"
              data-has-draft={hasDraft ? 'true' : 'false'}
              value={customText}
              onChange={(e) => setResponseState({ overlayId: overlay.id, selected, showCustom: true, customText: e.target.value })}
              placeholder={t('notch.typePlaceholder', { defaultValue: 'Type your response...' })}
              onKeyDown={(e) => {
                if (e.key === 'Escape') {
                  setResponseState({ overlayId: overlay.id, selected, showCustom: false, customText: '' })
                  setNotchFocusable(false)
                }
              }}
            />
          </form>
        )}

        {/* Multi-select confirm */}
        {isMulti && (
          <button
            className={`question-card__confirm ${selected.size === 0 ? 'question-card__confirm--disabled' : ''}`}
            disabled={selected.size === 0}
            onMouseDown={handleConfirm}
          >
            Confirm ({selected.size})
          </button>
        )}
      </div>
    </OverlayCard>
  )
}
