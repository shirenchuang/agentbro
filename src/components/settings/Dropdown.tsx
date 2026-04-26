import { useState, useRef, useEffect } from 'react'
import { motion, AnimatePresence } from 'framer-motion'

interface DropdownOption {
  value: string
  label: string
}

interface DropdownProps {
  value: string
  options: DropdownOption[]
  onChange: (value: string) => void
  minWidth?: number
}

export function Dropdown({ value, options, onChange, minWidth = 120 }: DropdownProps) {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)
  const selected = options.find((o) => o.value === value)

  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false)
      }
    }
    if (open) {
      document.addEventListener('mousedown', handleClickOutside)
      return () => document.removeEventListener('mousedown', handleClickOutside)
    }
  }, [open])

  return (
    <div className="glass-dropdown" ref={ref} style={{ minWidth }}>
      <button
        className={`glass-dropdown__trigger ${open ? 'glass-dropdown__trigger--open' : ''}`}
        onClick={() => setOpen(!open)}
      >
        <span className="glass-dropdown__label">{selected?.label ?? value}</span>
        <span className={`glass-dropdown__arrow ${open ? 'glass-dropdown__arrow--open' : ''}`}>
          ▾
        </span>
      </button>
      <AnimatePresence>
        {open && (
          <motion.div
            className="glass-dropdown__menu"
            role="listbox"
            initial={{ opacity: 0, y: -4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={{ duration: 0.15 }}
          >
            {options.map((opt) => (
              <div
                key={opt.value}
                role="option"
                aria-selected={opt.value === value}
                className={`glass-dropdown__option ${opt.value === value ? 'glass-dropdown__option--selected' : ''}`}
                onClick={() => {
                  onChange(opt.value)
                  setOpen(false)
                }}
              >
                {opt.label}
              </div>
            ))}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}
