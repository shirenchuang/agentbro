interface SliderProps {
  value: number
  min: number
  max: number
  step?: number
  onChange: (value: number) => void
  unit?: string
}

export function Slider({ value, min, max, step = 1, onChange, unit = '' }: SliderProps) {
  const fill = ((value - min) / (max - min)) * 100

  return (
    <div className="glass-slider">
      <input
        type="range"
        className="glass-slider__input"
        value={value}
        min={min}
        max={max}
        step={step}
        onChange={(e) => onChange(Number(e.target.value))}
        style={{ '--slider-fill': `${fill}%` } as React.CSSProperties}
      />
      <span className="glass-slider__value">
        {value}{unit}
      </span>
    </div>
  )
}
