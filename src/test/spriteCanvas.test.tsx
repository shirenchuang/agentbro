import { act, cleanup, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { SpriteCanvas } from '../components/notch/SpriteCanvas'
import { PRIORITY } from '../types/priority'
import type { PetOption } from '../types/pet'

function makePet(): PetOption {
  return {
    id: 'codex:test',
    displayName: 'Test Pet',
    provider: 'codex',
    builtin: true,
    spritesheetPath: '/tmp/test-pets/codex-test/spritesheet.webp',
    spritesheetUrl: 'asset://localhost/tmp/test-pets/codex-test/spritesheet.webp',
    frameSize: { width: 16, height: 16 },
    animations: {
      idle: { row: 0, frames: 1, fps: 10 },
      running: { row: 7, frames: 2, fps: 10 },
      'running-left': { row: 2, frames: 2, fps: 10 },
    },
    stateMapping: { working: 'running' },
  }
}

describe('SpriteCanvas', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.stubGlobal('Image', MockImage)
  })

  afterEach(() => {
    cleanup()
    vi.useRealTimers()
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it('plays active work briefly, then settles back to idle frames', () => {
    render(
      <SpriteCanvas
        pet={makePet()}
        priority={PRIORITY.working}
        size={32}
        enableIdleBehaviors={false}
      />,
    )

    const sprite = screen.getByTestId('sprite-canvas')
    expect(sprite).toHaveAttribute('data-pet-rendered-animation', 'running')

    act(() => {
      vi.advanceTimersByTime(600)
    })

    expect(sprite).toHaveAttribute('data-pet-rendered-animation', 'idle')
    expect(sprite).toHaveStyle({ backgroundPosition: '0% 0%' })
  })

  it('keeps continuous overrides looping instead of settling to idle', () => {
    render(
      <SpriteCanvas
        pet={makePet()}
        priority={PRIORITY.working}
        size={32}
        animationOverride="running-left"
        animationOverrideMode="continuous"
        enableIdleBehaviors={false}
      />,
    )

    const sprite = screen.getByTestId('sprite-canvas')

    act(() => {
      vi.advanceTimersByTime(700)
    })

    expect(sprite).toHaveAttribute('data-pet-rendered-animation', 'running-left')
    expect(sprite).toHaveStyle({ backgroundPosition: '100% 28.57142857142857%' })
  })

  it('pauses frame updates while the document is hidden', () => {
    let visibilityState: DocumentVisibilityState = 'visible'
    vi.spyOn(document, 'visibilityState', 'get').mockImplementation(() => visibilityState)

    render(
      <SpriteCanvas
        pet={makePet()}
        priority={PRIORITY.working}
        size={32}
        enableIdleBehaviors={false}
      />,
    )

    const sprite = screen.getByTestId('sprite-canvas')
    act(() => {
      vi.advanceTimersByTime(100)
    })
    const frameBeforeHide = sprite.getAttribute('data-pet-rendered-frame')

    act(() => {
      visibilityState = 'hidden'
      document.dispatchEvent(new Event('visibilitychange'))
    })
    act(() => {
      vi.advanceTimersByTime(600)
    })

    expect(sprite).toHaveAttribute('data-pet-rendered-frame', frameBeforeHide)
    expect(sprite).toHaveAttribute('data-pet-rendered-animation', 'running')
  })
})

class MockImage {
  naturalWidth = 32
  naturalHeight = 128
  width = 32
  height = 128

  private loaded = false
  private handler: ((this: GlobalEventHandlers, ev: Event) => unknown) | null = null

  set src(_value: string) {
    this.loaded = true
    this.handler?.call(this as unknown as GlobalEventHandlers, new Event('load'))
  }

  get onload() {
    return this.handler
  }

  set onload(handler: ((this: GlobalEventHandlers, ev: Event) => unknown) | null) {
    this.handler = handler
    if (this.loaded) {
      handler?.call(this as unknown as GlobalEventHandlers, new Event('load'))
    }
  }
}
