import { describe, expect, it } from 'vitest'

import { isLocalImageSource } from '../services/tauriApi'

describe('tauriApi local image source detection', () => {
  it('recognizes Windows home-relative paths as local images', () => {
    expect(isLocalImageSource('~\\Pictures\\avatar.png')).toBe(true)
  })
})
