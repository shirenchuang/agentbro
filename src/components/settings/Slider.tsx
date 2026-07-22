import { useEffect, useRef } from 'react'

interface SliderProps {
  value: number
  min: number
  max: number
  step?: number
  onChange?: (value: number) => void
  onCommit?: (value: number) => void
  unit?: string
}

const COMMIT_DELAY_MS = 120
const PREVIEW_INTERVAL_MS = 50

export function Slider({ value, min, max, step = 1, onChange, onCommit, unit = '' }: SliderProps) {
  const inputRef = useRef<HTMLInputElement>(null)
  const valueRef = useRef<HTMLSpanElement>(null)
  const draftRef = useRef(value)
  const commitTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const previewTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const pendingPreviewRef = useRef<number | null>(null)
  const lastPreviewAtRef = useRef(Number.NEGATIVE_INFINITY)
  const lastCommittedRef = useRef(value)
  const onChangeRef = useRef(onChange)
  const onCommitRef = useRef(onCommit)
  const deferred = Boolean(onCommit)
  const fill = ((value - min) / (max - min)) * 100

  useEffect(() => {
    onChangeRef.current = onChange
    onCommitRef.current = onCommit
  }, [onChange, onCommit])

  useEffect(() => {
    draftRef.current = value
    lastCommittedRef.current = value
    if (inputRef.current) {
      inputRef.current.value = String(value)
      inputRef.current.style.setProperty('--slider-fill', `${((value - min) / (max - min)) * 100}%`)
    }
    if (valueRef.current) valueRef.current.textContent = `${value}${unit}`
  }, [max, min, unit, value])

  useEffect(() => () => {
    if (commitTimerRef.current) clearTimeout(commitTimerRef.current)
    if (previewTimerRef.current) clearTimeout(previewTimerRef.current)
  }, [])

  const emitPreview = (next: number) => {
    const callback = onChangeRef.current
    if (!callback) return
    const now = Date.now()
    const elapsed = now - lastPreviewAtRef.current
    if (elapsed >= PREVIEW_INTERVAL_MS) {
      lastPreviewAtRef.current = now
      callback(next)
      return
    }
    pendingPreviewRef.current = next
    if (previewTimerRef.current) return
    previewTimerRef.current = setTimeout(() => {
      previewTimerRef.current = null
      const pending = pendingPreviewRef.current
      pendingPreviewRef.current = null
      if (pending === null) return
      lastPreviewAtRef.current = Date.now()
      onChangeRef.current?.(pending)
    }, PREVIEW_INTERVAL_MS - elapsed)
  }

  const flushCommit = () => {
    if (!deferred) return
    if (commitTimerRef.current) {
      clearTimeout(commitTimerRef.current)
      commitTimerRef.current = null
    }
    const next = draftRef.current
    if (Object.is(next, lastCommittedRef.current)) return
    lastCommittedRef.current = next
    onCommitRef.current?.(next)
  }

  const scheduleCommit = () => {
    if (!deferred) return
    if (commitTimerRef.current) clearTimeout(commitTimerRef.current)
    commitTimerRef.current = setTimeout(flushCommit, COMMIT_DELAY_MS)
  }

  const handleChange = (next: number) => {
    if (!deferred) {
      onChangeRef.current?.(next)
      return
    }
    draftRef.current = next
    if (inputRef.current) {
      inputRef.current.style.setProperty('--slider-fill', `${((next - min) / (max - min)) * 100}%`)
    }
    if (valueRef.current) valueRef.current.textContent = `${next}${unit}`
    emitPreview(next)
    scheduleCommit()
  }

  return (
    <div className="glass-slider">
      <input
        ref={inputRef}
        type="range"
        className="glass-slider__input"
        defaultValue={value}
        min={min}
        max={max}
        step={step}
        onChange={(e) => handleChange(Number(e.target.value))}
        onPointerUp={flushCommit}
        onKeyUp={flushCommit}
        onBlur={flushCommit}
        style={{ '--slider-fill': `${fill}%` } as React.CSSProperties}
      />
      <span ref={valueRef} className="glass-slider__value">
        {value}{unit}
      </span>
    </div>
  )
}
