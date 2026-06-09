import { shortcutDisplayParts } from '../../utils/keyboardShortcuts'
import { isApplePlatform } from '../../utils/platform'

export type TipSurface = 'island' | 'pet'

export function formatShortcut(shortcut: string): string {
  const parts = shortcutDisplayParts(shortcut)
  return isApplePlatform() ? parts.join('') : parts.join('+')
}

export function buildTips(config: {
  globalShortcut: string
  shortcutApprove: string
  shortcutApproveEnabled: boolean
  shortcutDeny: string
  shortcutDenyEnabled: boolean
  shortcutSkip: string
  shortcutSkipEnabled: boolean
}, surface: TipSurface = 'island'): string[] {
  const tips = surface === 'pet'
    ? [
        `${formatShortcut(config.globalShortcut)} 可切换 AgentBro 显示`,
        '点击宠物可查看当前会话列表',
        'ESC 可以收起宠物弹窗',
        'Agents 设置里可以一键安装或修复 hooks',
        'Follow Focus 只显示当前窗口相关会话',
        'Webhook 可以把完成通知发到群里',
        '宠物可在 Island 设置里切换或自动跟随 Agent',
        '右键宠物可以快速打开设置',
        '拖拽宠物可以调整它在屏幕上的位置',
        '开启宠物活力后，可看到上下文压力和体力状态',
        '会话详情里可以直接给 Agent 继续发送消息',
        '任务完成通知可以用 ESC 轻轻收起',
      ]
    : [
        `${formatShortcut(config.globalShortcut)} 切换灵动岛显示`,
        'ESC 收起当前展开面板',
        '悬停灵动岛查看会话详情',
        '点击设置图标可调整主题、声音和快捷键',
        '在 Agents 设置里安装或修复各 Agent hooks',
        '开启 Follow Focus 后只看当前窗口相关会话',
        '空闲提示可以在 Island 设置里关闭',
        '设置里可在灵动岛和宠物模式之间切换',
        '通知声音、静音时段和快捷键都可以单独配置',
        '开启用量展示后可在灵动岛查看 5h/7d 额度',
        '会话详情里可以继续追问，不用切回终端',
        'Webhook 可把完成、失败和审批事件同步到群里',
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

export function shuffleTips(tips: string[]): string[] {
  const shuffled = [...tips]
  for (let i = shuffled.length - 1; i > 0; i -= 1) {
    const j = Math.floor(Math.random() * (i + 1))
    const tmp = shuffled[i]
    shuffled[i] = shuffled[j]
    shuffled[j] = tmp
  }
  return shuffled
}
