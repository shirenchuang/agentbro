import autoclawIcon from '../../assets/autoclaw.png'
import codebuddyIcon from '../../assets/codebuddy.png'
import codexIcon from '../../assets/codex.png'
import cursorIcon from '../../assets/cursor.png'
import easyclawIcon from '../../assets/easyclaw.png'
import factoryDroidIcon from '../../assets/factory-droid.png'
import hermesIcon from '../../assets/hermes.png'
import kiroIcon from '../../assets/kiro.png'
import openclawIcon from '../../assets/openclaw.png'
import qclawIcon from '../../assets/qclaw.png'
import qoderIcon from '../../assets/qoder.png'
import traeIcon from '../../assets/trae.png'
import windsurfIcon from '../../assets/windsurf.png'
import workbuddyIcon from '../../assets/workbuddy.png'

interface PlatformIconProps {
  agentId: string
  displayName?: string
  size?: number
  className?: string
}

const imageIcons: Record<string, string> = {
  autoclaw: autoclawIcon,
  codebuddy: codebuddyIcon,
  codebuddycn: codebuddyIcon,
  codex: codexIcon,
  cursor: cursorIcon,
  'cursor-cli': cursorIcon,
  droid: factoryDroidIcon,
  'factory-droid': factoryDroidIcon,
  easyclaw: easyclawIcon,
  'easyclaw-v2': easyclawIcon,
  hermes: hermesIcon,
  kiro: kiroIcon,
  openclaw: openclawIcon,
  qclaw: qclawIcon,
  qoder: qoderIcon,
  'qoder-cli': qoderIcon,
  trae: traeIcon,
  traecli: traeIcon,
  traecn: traeIcon,
  'trae-cn': traeIcon,
  windsurf: windsurfIcon,
  workbuddy: workbuddyIcon,
}

const glyphIcons: Record<string, string> = {
  'claude-code': 'CC',
  gemini: 'G',
  'gemini-cli': 'G',
  copilot: 'GH',
  opencode: 'OC',
  qwen: 'QW',
  kimi: 'KM',
  antigravity: 'AG',
  stepfun: 'SF',
  pi: 'PI',
  junie: 'JN',
  augment: 'AU',
  kilocode: 'KC',
  ob1: 'O1',
  amp: 'AMP',
  aider: 'AI',
}

export function PlatformIcon({ agentId, displayName, size = 28, className = '' }: PlatformIconProps) {
  const src = imageIcons[agentId]
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

  const label = glyphIcons[agentId] ?? displayName?.slice(0, 2).toUpperCase() ?? agentId.slice(0, 2).toUpperCase()
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
