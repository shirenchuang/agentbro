import { useEffect, useRef } from 'react'
import type { ThemeConfig } from '../../types/theme'
import type { Priority } from '../../types/priority'
import { priorityName } from '../../types/priority'

interface SpriteCanvasProps {
  theme: ThemeConfig
  priority: Priority
  size: number
}

export function SpriteCanvas({ theme, priority, size }: SpriteCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const frameRef = useRef(0)
  const imageRef = useRef<HTMLImageElement | null>(null)

  const character = theme.character
  const pName = priorityName(priority)
  const animName = character ? (theme.stateMapping?.[pName] ?? 'idle') : 'idle'
  const anim = character ? (character.animations[animName] ?? character.animations['idle']) : undefined
  const spriteSrc = character?.spriteSheetDataUrl ?? character?.spriteSheet

  useEffect(() => {
    if (!spriteSrc) return

    const img = new Image()
    img.src = spriteSrc
    img.onload = () => { imageRef.current = img }

    return () => { imageRef.current = null }
  }, [spriteSrc])

  useEffect(() => {
    if (!anim || !character) return

    const canvas = canvasRef.current
    if (!canvas) return
    const ctx = canvas.getContext('2d')
    if (!ctx) return

    const fps = anim.fps || 8
    const interval = 1000 / fps
    let lastTime = 0
    let animId: number

    const render = (time: number) => {
      animId = requestAnimationFrame(render)
      if (time - lastTime < interval) return
      lastTime = time

      const img = imageRef.current
      if (!img) return

      const { width, height } = character.frameSize
      const frame = frameRef.current % anim.frames

      ctx.clearRect(0, 0, size, size)
      ctx.drawImage(
        img,
        frame * width,
        anim.row * height,
        width,
        height,
        0,
        0,
        size,
        size,
      )

      frameRef.current = frame + 1
    }

    animId = requestAnimationFrame(render)
    return () => cancelAnimationFrame(animId)
  }, [anim, size, character])

  if (!character) return null

  return (
    <canvas
      ref={canvasRef}
      width={size}
      height={size}
      style={{ width: size, height: size }}
      aria-hidden
    />
  )
}
