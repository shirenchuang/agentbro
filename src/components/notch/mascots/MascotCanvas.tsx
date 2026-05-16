import { useEffect, useRef } from 'react'
import type { AgentType } from '../../../types/agent'

export type MascotAnimState = 'idle' | 'processing' | 'running' | 'alert'

interface MascotConfig {
  primary: string
  secondary: string
  accent: string
  eyeStyle: 'dot' | 'wide' | 'star' | 'slash'
  bodyShape: 'round' | 'square' | 'hex'
  antenna: boolean
  ears: 'none' | 'round' | 'pointy' | 'wide'
}

const MASCOT_CONFIGS: Record<string, MascotConfig> = {
  'claude-code': { primary: '#E07B39', secondary: '#7B4A2D', accent: '#FFF3E0', eyeStyle: 'wide', bodyShape: 'round', antenna: true, ears: 'round' },
  'codex':       { primary: '#10A37F', secondary: '#0D6B5E', accent: '#E0F7F4', eyeStyle: 'dot',  bodyShape: 'square', antenna: true, ears: 'none' },
  'gemini-cli':  { primary: '#4285F4', secondary: '#8B5CF6', accent: '#E8F0FE', eyeStyle: 'star', bodyShape: 'hex',    antenna: false, ears: 'wide' },
  'cursor':      { primary: '#E2E8F0', secondary: '#1E293B', accent: '#64748B', eyeStyle: 'slash', bodyShape: 'square', antenna: false, ears: 'none' },
  'copilot':     { primary: '#0D84FF', secondary: '#006FCC', accent: '#E0F0FF', eyeStyle: 'wide', bodyShape: 'round', antenna: false, ears: 'round' },
  'trae':        { primary: '#00BCD4', secondary: '#007C8A', accent: '#E0F7FA', eyeStyle: 'dot',  bodyShape: 'round', antenna: true, ears: 'none' },
  'qoder':       { primary: '#FFC107', secondary: '#FF8F00', accent: '#FFF8E1', eyeStyle: 'wide', bodyShape: 'square', antenna: true, ears: 'pointy' },
  'codebuddy':   { primary: '#F44336', secondary: '#B71C1C', accent: '#FFEBEE', eyeStyle: 'wide', bodyShape: 'round', antenna: false, ears: 'round' },
  'qwen':        { primary: '#9C27B0', secondary: '#6A0080', accent: '#F3E5F5', eyeStyle: 'star', bodyShape: 'hex',    antenna: true, ears: 'pointy' },
  'kimi':        { primary: '#EC407A', secondary: '#AD1457', accent: '#FCE4EC', eyeStyle: 'dot',  bodyShape: 'round', antenna: false, ears: 'wide' },
  'opencode':    { primary: '#4CAF50', secondary: '#1B5E20', accent: '#E8F5E9', eyeStyle: 'wide', bodyShape: 'square', antenna: false, ears: 'none' },
  'droid':       { primary: '#78909C', secondary: '#37474F', accent: '#ECEFF1', eyeStyle: 'wide', bodyShape: 'square', antenna: true, ears: 'wide' },
  'kiro':        { primary: '#00E5FF', secondary: '#006064', accent: '#E0F7FA', eyeStyle: 'dot',  bodyShape: 'hex',    antenna: true, ears: 'none' },
  'stepfun':     { primary: '#7C4DFF', secondary: '#4A148C', accent: '#EDE7F6', eyeStyle: 'star', bodyShape: 'hex',    antenna: true, ears: 'wide' },
  'antigravity': { primary: '#00BFA5', secondary: '#004D40', accent: '#E0F2F1', eyeStyle: 'dot',  bodyShape: 'round', antenna: false, ears: 'round' },
  'workbuddy':   { primary: '#FF7043', secondary: '#BF360C', accent: '#FBE9E7', eyeStyle: 'wide', bodyShape: 'square', antenna: false, ears: 'pointy' },
  'hermes':      { primary: '#5C6BC0', secondary: '#283593', accent: '#E8EAF6', eyeStyle: 'slash', bodyShape: 'round', antenna: true, ears: 'none' },
  'pi':          { primary: '#26A69A', secondary: '#004D40', accent: '#E0F2F1', eyeStyle: 'dot',  bodyShape: 'round', antenna: false, ears: 'wide' },
  'cursor-cli':  { primary: '#E2E8F0', secondary: '#1E293B', accent: '#64748B', eyeStyle: 'slash', bodyShape: 'square', antenna: false, ears: 'none' },
  'qoder-cli':   { primary: '#FFC107', secondary: '#FF8F00', accent: '#FFF8E1', eyeStyle: 'wide', bodyShape: 'square', antenna: true, ears: 'pointy' },
  'codebuddycn': { primary: '#F44336', secondary: '#B71C1C', accent: '#FFEBEE', eyeStyle: 'wide', bodyShape: 'round', antenna: false, ears: 'round' },
  'traecn':      { primary: '#00BCD4', secondary: '#007C8A', accent: '#E0F7FA', eyeStyle: 'dot',  bodyShape: 'round', antenna: true, ears: 'none' },
  'aider':       { primary: '#8BC34A', secondary: '#33691E', accent: '#F1F8E9', eyeStyle: 'wide', bodyShape: 'round', antenna: false, ears: 'pointy' },
  'continue':    { primary: '#1565C0', secondary: '#0D47A1', accent: '#E3F2FD', eyeStyle: 'dot',  bodyShape: 'round', antenna: false, ears: 'round' },
  'amp':         { primary: '#FF6D00', secondary: '#E65100', accent: '#FFF3E0', eyeStyle: 'wide', bodyShape: 'hex',    antenna: true, ears: 'none' },
}

const DEFAULT_CONFIG: MascotConfig = {
  primary: '#8E8E93', secondary: '#636366', accent: '#AEAEB2',
  eyeStyle: 'dot', bodyShape: 'round', antenna: false, ears: 'none',
}

function hexToRgb(hex: string): [number, number, number] {
  const r = parseInt(hex.slice(1, 3), 16)
  const g = parseInt(hex.slice(3, 5), 16)
  const b = parseInt(hex.slice(5, 7), 16)
  return [r, g, b]
}

function withAlpha(hex: string, alpha: number): string {
  const [r, g, b] = hexToRgb(hex)
  return `rgba(${r},${g},${b},${alpha})`
}

function drawMascot(
  ctx: CanvasRenderingContext2D,
  config: MascotConfig,
  animState: MascotAnimState,
  frame: number,
  canvasSize: number,
) {
  ctx.clearRect(0, 0, canvasSize, canvasSize)
  const s = canvasSize / 32
  const cx = 16 * s

  const { primary, secondary, accent, eyeStyle, bodyShape, antenna, ears } = config

  // Animation transforms
  let bobY = 0
  let breathScale = 1
  let eyeVisible = true

  if (animState === 'idle') {
    const t0 = Math.sin((frame / 60) * Math.PI * 2)
    breathScale = 1 + t0 * 0.02
    bobY = t0 * 0.3 * s
  } else if (animState === 'processing') {
    bobY = Math.sin((frame / 20) * Math.PI) * 0.8 * s
  } else if (animState === 'running') {
    bobY = Math.abs(Math.sin((frame / 12) * Math.PI)) * 1.2 * s - 0.6 * s
  } else if (animState === 'alert') {
    eyeVisible = Math.floor(frame / 10) % 4 !== 0
  }

  ctx.save()
  ctx.translate(cx, 16 * s + bobY)
  ctx.scale(1, breathScale)
  ctx.translate(-cx, -(16 * s + bobY))

  const headY = antenna ? 8 * s : 6 * s
  const headW = 12 * s
  const headH = 10 * s

  // Antenna
  if (antenna) {
    ctx.fillStyle = secondary
    ctx.fillRect(cx - s, 3 * s, 2 * s, 5 * s)
    ctx.fillStyle = primary
    ctx.beginPath()
    ctx.arc(cx, 3 * s, 2.5 * s, 0, Math.PI * 2)
    ctx.fill()
    ctx.strokeStyle = secondary
    ctx.lineWidth = 0.8 * s
    ctx.beginPath()
    ctx.arc(cx, 3 * s, 2.5 * s, 0, Math.PI * 2)
    ctx.stroke()
  }

  // Ears
  if (ears !== 'none') {
    const earY = headY + 2 * s
    ctx.fillStyle = primary
    if (ears === 'round') {
      ctx.beginPath()
      ctx.arc(cx - headW / 2 - 1.5 * s, earY, 2 * s, 0, Math.PI * 2)
      ctx.arc(cx + headW / 2 + 1.5 * s, earY, 2 * s, 0, Math.PI * 2)
      ctx.fill()
    } else if (ears === 'pointy') {
      // Left pointy ear
      ctx.beginPath()
      ctx.moveTo(cx - headW / 2 + 1 * s, earY + 2 * s)
      ctx.lineTo(cx - headW / 2 - 3 * s, earY - 3 * s)
      ctx.lineTo(cx - headW / 2 + 3 * s, earY - 1 * s)
      ctx.closePath()
      ctx.fill()
      // Right pointy ear
      ctx.beginPath()
      ctx.moveTo(cx + headW / 2 - 1 * s, earY + 2 * s)
      ctx.lineTo(cx + headW / 2 + 3 * s, earY - 3 * s)
      ctx.lineTo(cx + headW / 2 - 3 * s, earY - 1 * s)
      ctx.closePath()
      ctx.fill()
    } else if (ears === 'wide') {
      ctx.fillRect(cx - headW / 2 - 3 * s, earY - s, 3 * s, 5 * s)
      ctx.fillRect(cx + headW / 2, earY - s, 3 * s, 5 * s)
    }
  }

  // Head
  ctx.fillStyle = primary
  if (bodyShape === 'round') {
    ctx.beginPath()
    ctx.roundRect(cx - headW / 2, headY, headW, headH, 4 * s)
    ctx.fill()
  } else if (bodyShape === 'square') {
    ctx.fillRect(cx - headW / 2, headY, headW, headH)
    ctx.fillStyle = withAlpha(accent, 0.2)
    ctx.fillRect(cx - headW / 2, headY, headW, 2 * s)
  } else {
    // Hex
    const hx = cx, hy = headY + headH / 2
    const hw = headW / 2, hh = headH / 2
    ctx.beginPath()
    ctx.moveTo(hx, hy - hh)
    ctx.lineTo(hx + hw * 0.85, hy - hh * 0.5)
    ctx.lineTo(hx + hw * 0.85, hy + hh * 0.5)
    ctx.lineTo(hx, hy + hh)
    ctx.lineTo(hx - hw * 0.85, hy + hh * 0.5)
    ctx.lineTo(hx - hw * 0.85, hy - hh * 0.5)
    ctx.closePath()
    ctx.fill()
  }

  // Head outline
  ctx.strokeStyle = withAlpha(secondary, 0.6)
  ctx.lineWidth = 0.8 * s
  if (bodyShape === 'round') {
    ctx.beginPath()
    ctx.roundRect(cx - headW / 2, headY, headW, headH, 4 * s)
    ctx.stroke()
  } else if (bodyShape === 'square') {
    ctx.strokeRect(cx - headW / 2, headY, headW, headH)
  } else {
    const hx = cx, hy = headY + headH / 2, hw = headW / 2, hh = headH / 2
    ctx.beginPath()
    ctx.moveTo(hx, hy - hh)
    ctx.lineTo(hx + hw * 0.85, hy - hh * 0.5)
    ctx.lineTo(hx + hw * 0.85, hy + hh * 0.5)
    ctx.lineTo(hx, hy + hh)
    ctx.lineTo(hx - hw * 0.85, hy + hh * 0.5)
    ctx.lineTo(hx - hw * 0.85, hy - hh * 0.5)
    ctx.closePath()
    ctx.stroke()
  }

  // Eyes
  const eyeY = headY + 3.5 * s
  if (!eyeVisible) {
    ctx.strokeStyle = secondary
    ctx.lineWidth = 1.5 * s
    ctx.lineCap = 'round'
    ctx.beginPath()
    ctx.moveTo(cx - 4 * s, eyeY)
    ctx.lineTo(cx - 2 * s, eyeY)
    ctx.moveTo(cx + 2 * s, eyeY)
    ctx.lineTo(cx + 4 * s, eyeY)
    ctx.stroke()
  } else if (eyeStyle === 'dot') {
    ctx.fillStyle = secondary
    ctx.beginPath()
    ctx.arc(cx - 2.5 * s, eyeY, 1.5 * s, 0, Math.PI * 2)
    ctx.arc(cx + 2.5 * s, eyeY, 1.5 * s, 0, Math.PI * 2)
    ctx.fill()
  } else if (eyeStyle === 'wide') {
    ctx.fillStyle = accent
    ctx.fillRect(cx - 4.5 * s, eyeY - 1.5 * s, 3.5 * s, 3 * s)
    ctx.fillRect(cx + 1 * s, eyeY - 1.5 * s, 3.5 * s, 3 * s)
    ctx.fillStyle = secondary
    ctx.fillRect(cx - 3.5 * s, eyeY - 0.5 * s, 1.5 * s, 1.5 * s)
    ctx.fillRect(cx + 2 * s, eyeY - 0.5 * s, 1.5 * s, 1.5 * s)
  } else if (eyeStyle === 'star') {
    ctx.fillStyle = secondary
    // Left star
    ctx.fillRect(cx - 4 * s, eyeY - 0.5 * s, 2.5 * s, 1 * s)
    ctx.fillRect(cx - 3 * s, eyeY - 1.5 * s, 1 * s, 2.5 * s)
    // Right star
    ctx.fillRect(cx + 1.5 * s, eyeY - 0.5 * s, 2.5 * s, 1 * s)
    ctx.fillRect(cx + 2.5 * s, eyeY - 1.5 * s, 1 * s, 2.5 * s)
  } else if (eyeStyle === 'slash') {
    ctx.strokeStyle = secondary
    ctx.lineWidth = 1.5 * s
    ctx.lineCap = 'round'
    ctx.beginPath()
    ctx.moveTo(cx - 4.5 * s, eyeY - 1.5 * s)
    ctx.lineTo(cx - 2 * s, eyeY + 1.5 * s)
    ctx.moveTo(cx + 2 * s, eyeY - 1.5 * s)
    ctx.lineTo(cx + 4.5 * s, eyeY + 1.5 * s)
    ctx.stroke()
  }

  // Mouth (small curve)
  ctx.strokeStyle = withAlpha(secondary, 0.7)
  ctx.lineWidth = 1 * s
  ctx.lineCap = 'round'
  ctx.beginPath()
  if (animState === 'alert') {
    // Surprised O
    ctx.arc(cx, headY + 7 * s, 1.5 * s, 0, Math.PI * 2)
    ctx.stroke()
  } else {
    ctx.moveTo(cx - 2 * s, headY + 7 * s)
    ctx.quadraticCurveTo(cx, headY + 8.5 * s, cx + 2 * s, headY + 7 * s)
    ctx.stroke()
  }

  // Body
  const bodyY = headY + headH + 1 * s
  const bodyW = 8 * s
  const bodyH = 5 * s

  ctx.fillStyle = secondary
  ctx.fillRect(cx - bodyW / 2, bodyY, bodyW, bodyH)
  ctx.fillStyle = withAlpha(primary, 0.7)
  ctx.fillRect(cx - bodyW / 2 + 1 * s, bodyY + 1 * s, bodyW - 2 * s, bodyH - 2 * s)
  // Chest detail
  ctx.fillStyle = withAlpha(accent, 0.5)
  ctx.fillRect(cx - 1.5 * s, bodyY + 1.5 * s, 3 * s, 2 * s)

  // Arms
  const armBusy = animState === 'processing'
  const armPhase = armBusy ? Math.sin((frame / 8) * Math.PI) : 0
  ctx.fillStyle = primary
  ctx.fillRect(cx - bodyW / 2 - 2 * s, bodyY + 1 * s - armPhase * s, 2 * s, 3 * s)
  ctx.fillRect(cx + bodyW / 2, bodyY + 1 * s + armPhase * s, 2 * s, 3 * s)

  // Legs
  const legY = bodyY + bodyH
  const legPhase = animState === 'running' ? frame : 0
  const leftLegY = legY + Math.sin((legPhase / 8) * Math.PI) * 1.5 * s
  const rightLegY = legY + Math.cos((legPhase / 8) * Math.PI) * 1.5 * s
  ctx.fillStyle = secondary
  ctx.fillRect(cx - 3.5 * s, leftLegY, 2.5 * s, 3 * s)
  ctx.fillRect(cx + 1 * s, rightLegY, 2.5 * s, 3 * s)
  // Feet
  ctx.fillStyle = withAlpha(primary, 0.8)
  ctx.fillRect(cx - 4 * s, leftLegY + 2.5 * s, 3.5 * s, 1.5 * s)
  ctx.fillRect(cx + 0.5 * s, rightLegY + 2.5 * s, 3.5 * s, 1.5 * s)

  ctx.restore()
}

interface MascotCanvasProps {
  toolType: AgentType | string
  animState: MascotAnimState
  size?: number
}

export function MascotCanvas({ toolType, animState, size = 32 }: MascotCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const frameRef = useRef(0)
  const rafRef = useRef<number | undefined>(undefined)
  const config = MASCOT_CONFIGS[toolType] ?? DEFAULT_CONFIG

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return

    const dpr = window.devicePixelRatio || 1
    const canvasSize = size * dpr
    canvas.width = canvasSize
    canvas.height = canvasSize
    canvas.style.width = `${size}px`
    canvas.style.height = `${size}px`

    const ctx = canvas.getContext('2d')
    if (!ctx) return

    ctx.scale(dpr, dpr)

    if (animState === 'idle') {
      frameRef.current = 0
      drawMascot(ctx, config, animState, frameRef.current, size)
      return
    }

    const animate = () => {
      frameRef.current = (frameRef.current + 1) % 240
      drawMascot(ctx, config, animState, frameRef.current, size)
      rafRef.current = requestAnimationFrame(animate)
    }

    rafRef.current = requestAnimationFrame(animate)
    return () => {
      if (rafRef.current !== undefined) cancelAnimationFrame(rafRef.current)
    }
  }, [toolType, animState, size, config])

  return (
    <canvas
      ref={canvasRef}
      style={{ imageRendering: 'pixelated', display: 'block' }}
      aria-hidden="true"
    />
  )
}
