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
  descriptions?: string[]
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

function normalizeOption(opt: string | QuestionOption, description?: string): QuestionOption {
  const normalized = typeof opt === 'string' ? { label: opt } : opt
  return {
    ...normalized,
    description: normalized.description ?? description,
  }
}

function parseQuestionTag(question: string): { tag: string | null; text: string } {
  const match = question.match(/^\[([^\]]+)\]\s*(.*)/s)
  if (match) return { tag: match[1], text: match[2] }
  return { tag: null, text: question }
}

const SKIP_INTERVIEW_ANSWER = 'Skip interview and plan immediately'

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
  const [composing, setComposing] = useState(false)
  const hasDraft = Object.values(inputTexts).some((text) => text.trim().length > 0)

  useEffect(() => {
    onDraftStateChange?.(hasDraft)
  }, [hasDraft, onDraftStateChange])

  useEffect(() => () => onDraftStateChange?.(false), [onDraftStateChange])

  const getAnswer = (qi: number): string | undefined => {
    const q = allQuestions[qi]
    const custom = customAnswers[qi] || (inputTexts[qi] ?? '').trim() || null
    if (q.multiSelect) {
      const chipLabels = Array.isArray(selections[qi])
        ? (selections[qi] as number[]).map((idx) => q.options[idx].label)
        : []
      const parts = [...chipLabels]
      if (custom) parts.push(custom)
      return parts.length > 0 ? parts.join(', ') : undefined
    }
    if (custom) return custom
    return selections[qi] !== undefined ? q.options[selections[qi] as number].label : undefined
  }
  const allAnswered = allQuestions.every((_, i) => getAnswer(i) !== undefined)

  const handleSubmit = () => {
    if (!allAnswered) { setError(true); return }
    const answers: Record<string, string> = {}
    allQuestions.forEach((q, i) => { answers[q.question] = getAnswer(i)! })
    onAnswer(JSON.stringify(answers))
  }

  return (
    <div className="question-card__multi">
      <div className="question-card__header">
        <svg className="question-card__header-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2.4} d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2v10z"/>
        </svg>
        <span className="question-card__header-title">{t('notch.questionTitle', { defaultValue: "Claude's Question" })}</span>
        <span className="question-card__header-count">({allQuestions.length})</span>
      </div>

      <div className="question-card__multi-list">
        {allQuestions.map((q, qi) => {
          const parsed = parseQuestionTag(q.question)
          const tag = q.header ?? parsed.tag
          const text = parsed.text
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
                    className={`question-card__chip ${selectedSet.has(oi) && (q.multiSelect || !customAnswers[qi]) ? 'question-card__chip--selected' : ''} ${q.multiSelect ? 'question-card__chip--checkbox' : ''}`}
                    onMouseDown={() => {
                      setSelections(prev => {
                        if (!q.multiSelect) {
                          if (prev[qi] === oi) {
                            const clone = { ...prev }
                            delete clone[qi]
                            return clone
                          }
                          return { ...prev, [qi]: oi }
                        }
                        const current = Array.isArray(prev[qi]) ? [...prev[qi] as number[]] : []
                        const next = current.includes(oi) ? current.filter((idx) => idx !== oi) : [...current, oi]
                        if (next.length === 0) {
                          const clone = { ...prev }
                          delete clone[qi]
                          return clone
                        }
                        return { ...prev, [qi]: next }
                      })
                      if (!q.multiSelect) {
                        setCustomAnswers(prev => { const n = { ...prev }; delete n[qi]; return n })
                      }
                      setError(false)
                    }}
                  >
                    {q.multiSelect && (
                      <span className={`question-card__chip-check ${selectedSet.has(oi) ? 'question-card__chip-check--checked' : ''}`}>
                        {selectedSet.has(oi) && <svg width="10" height="10" viewBox="0 0 16 16" fill="none"><path d="M3 8l4 4 6-7" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round"/></svg>}
                      </span>
                    )}
                    {opt.label}
                  </button>
                ))}
                {customAnswers[qi] && (
                  <span className={`question-card__chip question-card__chip--selected ${q.multiSelect ? 'question-card__chip--checkbox' : ''}`}>
                    {q.multiSelect && <span className="question-card__chip-check question-card__chip-check--checked"><svg width="10" height="10" viewBox="0 0 16 16" fill="none"><path d="M3 8l4 4 6-7" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round"/></svg></span>}
                    {customAnswers[qi]}
                  </span>
                )}
                {inputStates[qi] ? (
                  <input
                    autoFocus
                    className="question-card__chip-input"
                    placeholder={t('notch.typePlaceholder', { defaultValue: 'Type your response...' })}
                    value={inputTexts[qi] ?? ''}
                    onChange={(e) => setInputTexts(prev => ({ ...prev, [qi]: e.target.value }))}
                    onMouseDown={(e) => e.stopPropagation()}
                    onClick={(e) => e.stopPropagation()}
                    onCompositionStart={() => setComposing(true)}
                    onCompositionEnd={() => setComposing(false)}
                    onKeyDown={(e) => {
                      e.stopPropagation()
                      if (e.nativeEvent.isComposing || composing) return
                      if (e.key === 'Enter') {
                        const trimmed = (inputTexts[qi] ?? '').trim()
                        if (trimmed) {
                          setCustomAnswers(prev => ({ ...prev, [qi]: trimmed }))
                          if (!q.multiSelect) {
                            setSelections(prev => { const n = { ...prev }; delete n[qi]; return n })
                          }
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
                  <button
                    className="question-card__chip question-card__chip--other"
                    onMouseDown={(e) => {
                      e.preventDefault()
                      setNotchFocusable(true).then(() => {
                        setInputStates(prev => ({ ...prev, [qi]: true }))
                      })
                    }}
                  >
                    {t('notch.typeHint', { defaultValue: 'Other' })}
                  </button>
                )}
              </div>
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
  const options = (data.options || []).map((opt, i) => normalizeOption(opt, data.descriptions?.[i]))
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

  const showCustomInput = async () => {
    setResponseState({ overlayId: overlay.id, selected, showCustom: true, customText })
    await setNotchFocusable(true)
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

        {/* Custom text input — uses same option-row style with index N+1 */}
        {!showCustom ? (
          <motion.div
            initial={{ opacity: 0, y: 4, scale: 0.98 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            transition={{ type: 'spring', duration: 0.25, bounce: 0.12, delay: options.length * 0.03 }}
          >
            <div
              className="question-card__option-row question-card__option-row--other"
              onMouseDown={() => { void showCustomInput() }}
            >
              <span className="question-card__option-index question-card__option-index--other">{options.length + 1}</span>
              <div className="question-card__option-body">
                <div className="question-card__option-label">{t('notch.typeHint', { defaultValue: 'Other' })}</div>
              </div>
              <svg className="question-card__option-chevron" width="12" height="12" viewBox="0 0 16 16" fill="none">
                <path d="M6 4l4 4-4 4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
              </svg>
            </div>
          </motion.div>
        ) : (
          <form className="question-card__option-row question-card__option-row--custom-active" onSubmit={(e) => { e.preventDefault(); handleCustomSubmit() }}>
            <span className="question-card__option-index">{options.length + 1}</span>
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

        <div className="question-card__secondary-actions" aria-label={t('notch.questionMoreActions', { defaultValue: 'More question actions' })}>
          <button
            type="button"
            className="question-card__secondary-action"
            onMouseDown={() => { void showCustomInput() }}
          >
            {t('notch.chatAboutThis', { defaultValue: 'Chat about this' })}
          </button>
          {!isMulti && (
            <button
              type="button"
              className="question-card__secondary-action"
              onMouseDown={() => onAnswer(SKIP_INTERVIEW_ANSWER)}
            >
              {t('notch.skipInterview', { defaultValue: 'Skip interview' })}
            </button>
          )}
        </div>

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
