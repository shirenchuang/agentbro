import { AnimatePresence, motion } from 'framer-motion'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useConfigStore } from '../../stores/configStore'

function formatShortcut(shortcut: string): string {
  return shortcut
    .replace(/CommandOrControl/g, '⌘')
    .replace(/Command/g, '⌘')
    .replace(/Control/g, '⌃')
    .replace(/Alt|Option/g, '⌥')
    .replace(/Shift/g, '⇧')
    .replace(/\+/g, '')
}

function buildTips(config: {
  globalShortcut: string
  shortcutApprove: string
  shortcutApproveEnabled: boolean
  shortcutDeny: string
  shortcutDenyEnabled: boolean
  shortcutSkip: string
  shortcutSkipEnabled: boolean
}): string[] {
  const tips = [
    `${formatShortcut(config.globalShortcut)} 切换灵动岛显示`,
    'ESC 收起当前展开面板',
    '悬停灵动岛查看会话详情',
    '点击设置图标可调整主题、声音和快捷键',
    '在 Agents 设置里安装或修复各 Agent hooks',
    '开启 Follow Focus 后只看当前窗口相关会话',
    '空闲提示可以在 Island 设置里关闭',
  ]

  if (config.shortcutApproveEnabled) {
    tips.push(`${formatShortcut(config.shortcutApprove)} 批准当前权限请求`)
  }
  if (config.shortcutDenyEnabled) {
    tips.push(`${formatShortcut(config.shortcutDeny)} 拒绝当前权限请求`)
  }
  if (config.shortcutSkipEnabled) {
    tips.push(`${formatShortcut(config.shortcutSkip)} 跳过当前问题`)
  }

  return tips
}

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

function useTipRotation(active: boolean, tips: string[]): string | null {
  const [tip, setTip] = useState<string | null>(() => tips[0] ?? null)
  const shuffledRef = useRef<string[]>([])
  const indexRef = useRef(0)

  const nextTip = useCallback(() => {
    if (shuffledRef.current.length === 0 || indexRef.current >= shuffledRef.current.length) {
      shuffledRef.current = shuffleTips(tips)
      indexRef.current = 0
    }
    const next = shuffledRef.current[indexRef.current] ?? tips[0]
    indexRef.current += 1
    return next
  }, [tips])

  useEffect(() => {
    shuffledRef.current = []
    indexRef.current = 0
    setTip(tips[0] ?? null)
  }, [tips])

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
  const globalShortcut = useConfigStore((s) => s.globalShortcut)
  const shortcutApprove = useConfigStore((s) => s.shortcutApprove)
  const shortcutApproveEnabled = useConfigStore((s) => s.shortcutApproveEnabled)
  const shortcutDeny = useConfigStore((s) => s.shortcutDeny)
  const shortcutDenyEnabled = useConfigStore((s) => s.shortcutDenyEnabled)
  const shortcutSkip = useConfigStore((s) => s.shortcutSkip)
  const shortcutSkipEnabled = useConfigStore((s) => s.shortcutSkipEnabled)
  const tips = useMemo(() => buildTips({
    globalShortcut,
    shortcutApprove,
    shortcutApproveEnabled,
    shortcutDeny,
    shortcutDenyEnabled,
    shortcutSkip,
    shortcutSkipEnabled,
  }), [
    globalShortcut,
    shortcutApprove,
    shortcutApproveEnabled,
    shortcutDeny,
    shortcutDenyEnabled,
    shortcutSkip,
    shortcutSkipEnabled,
  ])
  const tip = useTipRotation(show, tips)
  if (!show || !tip) return null

  return (
    <div className="tip-display">
      <span className="tip-display__label">Tips:</span>
      <AnimatePresence mode="wait">
        <motion.span
          key={tip}
          className="tip-display__text"
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
