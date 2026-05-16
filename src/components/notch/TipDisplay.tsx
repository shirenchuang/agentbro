import { AnimatePresence, motion } from 'framer-motion'
import { useCallback, useEffect, useRef, useState } from 'react'

const TIPS = [
  '⌘⇧I 切换灵动岛可见性',
  '⌘K 打开命令面板',
  '⌘J 切换终端面板',
  '⌘⌥← / → 在 session 之间切换',
  '⌘⌥T 切换亮色/暗色主题',
  '⌘, 快速打开设置',
  '拖拽面板分隔线可调整布局宽度',
  '右键 session 查看更多操作',
  'Playground 模式适合快速测试 prompt',
]

function shuffleTips(tips: string[]): string[] {
  const shuffled = [...tips]
  for (let i = shuffled.length - 1; i > 0; i -= 1) {
    const j = Math.floor(Math.random() * (i + 1))
    const tmp = shuffled[i]
    shuffled[i] = shuffled[j]
    shuffled[j] = tmp
  }
  return shuffled
}

function useTipRotation(active: boolean): string | null {
  const [tip, setTip] = useState<string | null>(() => TIPS[0] ?? null)
  const shuffledRef = useRef<string[]>([])
  const indexRef = useRef(0)

  const nextTip = useCallback(() => {
    if (shuffledRef.current.length === 0 || indexRef.current >= shuffledRef.current.length) {
      shuffledRef.current = shuffleTips(TIPS)
      indexRef.current = 0
    }
    const next = shuffledRef.current[indexRef.current] ?? TIPS[0]
    indexRef.current += 1
    return next
  }, [])

  useEffect(() => {
    if (!active) return
    const id = window.setTimeout(() => setTip(nextTip()), 0)
    return () => window.clearTimeout(id)
  }, [active, nextTip])

  useEffect(() => {
    if (!active) return
    const id = window.setInterval(() => {
      setTip(nextTip())
    }, 10_000)
    return () => window.clearInterval(id)
  }, [active, nextTip])

  return tip
}

interface TipDisplayProps {
  show: boolean
}

export function TipDisplay({ show }: TipDisplayProps) {
  const tip = useTipRotation(show)
  if (!show || !tip) return null

  return (
    <div style={{ display: 'flex', flexDirection: 'row', alignItems: 'center', gap: 4 }}>
      <span style={{ fontSize: 11, color: 'var(--text-secondary)', flexShrink: 0 }}>Tips:</span>
      <AnimatePresence mode="wait">
        <motion.span
          key={tip}
          style={{ fontSize: 11, color: 'var(--text-secondary)' }}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.3 }}
        >
          {tip}
        </motion.span>
      </AnimatePresence>
    </div>
  )
}
