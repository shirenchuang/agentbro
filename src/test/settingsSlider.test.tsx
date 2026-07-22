import { act, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { Slider } from '../components/settings/Slider'

describe('settings Slider', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('updates its draft immediately and defers the persisted commit', () => {
    const preview = vi.fn()
    const commit = vi.fn()
    render(<Slider value={10} min={0} max={100} onChange={preview} onCommit={commit} unit="%" />)

    fireEvent.change(screen.getByRole('slider'), { target: { value: '35' } })

    expect(screen.getByText('35%')).toBeInTheDocument()
    expect(preview).toHaveBeenCalledWith(35)
    expect(commit).not.toHaveBeenCalled()

    act(() => {
      vi.advanceTimersByTime(120)
    })

    expect(commit).toHaveBeenCalledTimes(1)
    expect(commit).toHaveBeenCalledWith(35)
  })

  it('flushes the final value when pointer interaction ends', () => {
    const commit = vi.fn()
    render(<Slider value={10} min={0} max={100} onCommit={commit} />)
    const slider = screen.getByRole('slider')

    fireEvent.pointerDown(slider)
    fireEvent.change(slider, { target: { value: '70' } })
    fireEvent.pointerUp(slider)

    expect(commit).toHaveBeenCalledTimes(1)
    expect(commit).toHaveBeenCalledWith(70)
  })
})
