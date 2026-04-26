import { forwardRef, type InputHTMLAttributes } from 'react'

interface GlassInputProps extends Omit<InputHTMLAttributes<HTMLInputElement>, 'onSubmit'> {
  onSubmit?: (value: string) => void
}

export const GlassInput = forwardRef<HTMLInputElement, GlassInputProps>(
  function GlassInput({ onSubmit, className = '', ...props }, ref) {
    return (
      <input
        ref={ref}
        className={`glass-input ${className}`}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && onSubmit) {
            onSubmit((e.target as HTMLInputElement).value)
            ;(e.target as HTMLInputElement).value = ''
          }
        }}
        {...props}
      />
    )
  }
)
