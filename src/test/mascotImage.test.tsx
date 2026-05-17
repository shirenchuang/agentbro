import { render } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { MascotRouter } from '../components/notch/mascots'

describe('Mascot image assets', () => {
  it('uses transparent CodeIsland PNG icons for matching agent sources', () => {
    const { container } = render(<MascotRouter toolType="gemini-cli" phase="processing" size={28} />)

    const wrapper = container.querySelector('.mascot-image')
    const image = container.querySelector('.mascot-image img') as HTMLImageElement | null

    expect(wrapper).toHaveAttribute('data-mascot-source', 'gemini-cli')
    expect(wrapper).toHaveAttribute('data-mascot-state', 'running')
    expect(image?.getAttribute('src')).toContain('/src/assets/cli-icons/gemini.png')
  })

  it('uses transparent PNG icons for sources without a preview GIF', () => {
    const { container } = render(<MascotRouter toolType="qwen" phase="waiting_input" size={28} />)

    const wrapper = container.querySelector('.mascot-image')
    const image = container.querySelector('.mascot-image img') as HTMLImageElement | null

    expect(wrapper).toHaveAttribute('data-mascot-source', 'qwen')
    expect(wrapper).toHaveAttribute('data-mascot-state', 'alert')
    expect(image?.getAttribute('src')).toContain('/src/assets/cli-icons/qwen.png')
  })
})
