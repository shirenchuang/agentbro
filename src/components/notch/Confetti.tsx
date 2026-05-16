import { useState, useEffect } from 'react'
import './Confetti.css'

const COLORS = ['#22c55e', '#3b82f6', '#f97316', '#a855f7', '#eab308', '#ec4899']

interface Particle {
  id: number
  color: string
  size: number
  x: number
  rotation: number
  delay: number
}

interface ConfettiProps {
  trigger: boolean
}

export function Confetti({ trigger }: ConfettiProps) {
  const [particles, setParticles] = useState<Particle[]>([])

  useEffect(() => {
    if (!trigger) return
    const newParticles: Particle[] = Array.from({ length: 50 }, (_, i) => ({
      id: i,
      color: COLORS[Math.floor(Math.random() * COLORS.length)],
      size: 4 + Math.random() * 4,
      x: Math.random() * 100,
      rotation: Math.random() * 360,
      delay: Math.random() * 0.5,
    }))
    const frame = window.requestAnimationFrame(() => setParticles(newParticles))
    const timer = setTimeout(() => setParticles([]), 3500)
    return () => {
      window.cancelAnimationFrame(frame)
      clearTimeout(timer)
    }
  }, [trigger])

  if (particles.length === 0) return null

  return (
    <div className="confetti-container" aria-hidden="true">
      {particles.map(p => (
        <div
          key={p.id}
          className="confetti-particle"
          style={{
            '--init-rot': `${p.rotation}deg`,
            backgroundColor: p.color,
            width: p.size,
            height: p.size,
            left: `${p.x}%`,
            animationDelay: `${p.delay}s`,
          } as React.CSSProperties}
        />
      ))}
    </div>
  )
}
