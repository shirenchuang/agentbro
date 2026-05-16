import autoclawIcon from '../../assets/autoclaw.png'
import antigravityCliIcon from '../../assets/cli-icons/antigravity.png'
import claudeCliIcon from '../../assets/cli-icons/claude.png'
import codebuddyCliIcon from '../../assets/cli-icons/codebuddy.png'
import codexCliIcon from '../../assets/cli-icons/codex.png'
import copilotCliIcon from '../../assets/cli-icons/copilot.png'
import cursorCliIcon from '../../assets/cli-icons/cursor.png'
import factoryCliIcon from '../../assets/cli-icons/factory.png'
import geminiCliIcon from '../../assets/cli-icons/gemini.png'
import hermesCliIcon from '../../assets/cli-icons/hermes.png'
import kimiCliIcon from '../../assets/cli-icons/kimi.png'
import opencodeCliIcon from '../../assets/cli-icons/opencode.png'
import piCliIcon from '../../assets/cli-icons/pi.png'
import qoderCliIcon from '../../assets/cli-icons/qoder.png'
import qwenCliIcon from '../../assets/cli-icons/qwen.png'
import stepfunCliIcon from '../../assets/cli-icons/stepfun.png'
import traeCliIcon from '../../assets/cli-icons/trae.png'
import workbuddyCliIcon from '../../assets/cli-icons/workbuddy.png'
import easyclawIcon from '../../assets/easyclaw.png'
import kiroIcon from '../../assets/kiro.png'
import openclawIcon from '../../assets/openclaw.png'
import qclawIcon from '../../assets/qclaw.png'
import windsurfIcon from '../../assets/windsurf.png'

interface PlatformIconProps {
  agentId: string
  displayName?: string
  size?: number
  className?: string
}

const imageIcons: Record<string, string> = {
  antcc: claudeCliIcon,
  'ant-cc': claudeCliIcon,
  antigravity: antigravityCliIcon,
  autoclaw: autoclawIcon,
  claude: claudeCliIcon,
  'claude-code': claudeCliIcon,
  codebuddy: codebuddyCliIcon,
  'code-buddy': codebuddyCliIcon,
  codebuddycn: codebuddyCliIcon,
  codex: codexCliIcon,
  'openai-codex': codexCliIcon,
  copilot: copilotCliIcon,
  'github-copilot': copilotCliIcon,
  cursor: cursorCliIcon,
  'cursor-cli': cursorCliIcon,
  droid: factoryCliIcon,
  factory: factoryCliIcon,
  'factory-droid': factoryCliIcon,
  easyclaw: easyclawIcon,
  'easyclaw-v2': easyclawIcon,
  gemini: geminiCliIcon,
  'gemini-cli': geminiCliIcon,
  'google-gemini': geminiCliIcon,
  hermes: hermesCliIcon,
  kiro: kiroIcon,
  kimi: kimiCliIcon,
  opencode: opencodeCliIcon,
  'open-code': opencodeCliIcon,
  openclaw: openclawIcon,
  pi: piCliIcon,
  qclaw: qclawIcon,
  qoder: qoderCliIcon,
  'qoder-cli': qoderCliIcon,
  qwen: qwenCliIcon,
  stepfun: stepfunCliIcon,
  trae: traeCliIcon,
  traecli: traeCliIcon,
  'trae-cli': traeCliIcon,
  traecn: traeCliIcon,
  'trae-cn': traeCliIcon,
  windsurf: windsurfIcon,
  workbuddy: workbuddyCliIcon,
  'work-buddy': workbuddyCliIcon,
}

const glyphIcons: Record<string, string> = {
  junie: 'JN',
  augment: 'AU',
  kilocode: 'KC',
  ob1: 'O1',
  amp: 'AMP',
  aider: 'AI',
}

export function PlatformIcon({ agentId, displayName, size = 28, className = '' }: PlatformIconProps) {
  const normalizedAgentId = agentId.trim().toLowerCase().replace(/[\s_]+/g, '-')
  const src = imageIcons[normalizedAgentId]
  if (src) {
    return (
      <img
        src={src}
        width={size}
        height={size}
        alt=""
        aria-hidden="true"
        className={`platform-icon platform-icon--image ${className}`}
      />
    )
  }

  const label = glyphIcons[normalizedAgentId] ?? displayName?.slice(0, 2).toUpperCase() ?? normalizedAgentId.slice(0, 2).toUpperCase()
  return (
    <span
      className={`platform-icon platform-icon--glyph ${className}`}
      aria-hidden="true"
      style={{ width: size, height: size, fontSize: size > 24 ? 11 : 9 }}
    >
      {label}
    </span>
  )
}
