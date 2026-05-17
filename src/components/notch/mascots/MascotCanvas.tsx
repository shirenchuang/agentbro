import type { CSSProperties } from 'react'
import type { AgentType } from '../../../types/agent'
import antigravityIcon from '../../../assets/cli-icons/antigravity.png'
import claudeIcon from '../../../assets/cli-icons/claude.png'
import clineIcon from '../../../assets/cli-icons/cline.png'
import codebuddyIcon from '../../../assets/cli-icons/codebuddy.png'
import codexIcon from '../../../assets/cli-icons/codex.png'
import copilotIcon from '../../../assets/cli-icons/copilot.png'
import cursorIcon from '../../../assets/cli-icons/cursor.png'
import factoryIcon from '../../../assets/cli-icons/factory.png'
import geminiIcon from '../../../assets/cli-icons/gemini.png'
import hermesIcon from '../../../assets/cli-icons/hermes.png'
import kimiIcon from '../../../assets/cli-icons/kimi.png'
import opencodeIcon from '../../../assets/cli-icons/opencode.png'
import piIcon from '../../../assets/cli-icons/pi.png'
import qoderIcon from '../../../assets/cli-icons/qoder.png'
import qwenIcon from '../../../assets/cli-icons/qwen.png'
import stepfunIcon from '../../../assets/cli-icons/stepfun.png'
import traeIcon from '../../../assets/cli-icons/trae.png'
import workbuddyIcon from '../../../assets/cli-icons/workbuddy.png'
import kiroIcon from '../../../assets/kiro.png'
import './MascotCanvas.css'

export type MascotAnimState = 'idle' | 'processing' | 'running' | 'alert'

interface MascotCanvasProps {
  toolType: AgentType | string
  animState: MascotAnimState
  size?: number
}

type MascotAsset = {
  src: string
  fit?: 'contain' | 'cover'
  inset?: number
}

const MASCOT_ASSETS: Record<string, MascotAsset> = {
  'claude-code': { src: claudeIcon, inset: 3 },
  'claude': { src: claudeIcon, inset: 3 },
  'codex': { src: codexIcon, inset: 3 },
  'gemini-cli': { src: geminiIcon, inset: 3 },
  'gemini': { src: geminiIcon, inset: 3 },
  'cursor': { src: cursorIcon, inset: 3 },
  'cursor-cli': { src: cursorIcon, inset: 3 },
  'qoder': { src: qoderIcon, inset: 3 },
  'qoder-cli': { src: qoderIcon, inset: 3 },
  'codebuddy': { src: codebuddyIcon, inset: 3 },
  'codebuddycn': { src: codebuddyIcon, inset: 3 },
  'codybuddycn': { src: codebuddyIcon, inset: 3 },
  'opencode': { src: opencodeIcon, inset: 3 },
  'droid': { src: factoryIcon, inset: 3 },
  'factory': { src: factoryIcon, inset: 3 },
  'cline': { src: clineIcon, inset: 3 },
  'copilot': { src: copilotIcon, inset: 3 },
  'trae': { src: traeIcon, inset: 3 },
  'traecli': { src: traeIcon, inset: 3 },
  'traecn': { src: traeIcon, inset: 3 },
  'qwen': { src: qwenIcon, inset: 3 },
  'kimi': { src: kimiIcon, inset: 3 },
  'stepfun': { src: stepfunIcon, inset: 3 },
  'antigravity': { src: antigravityIcon, inset: 3 },
  'workbuddy': { src: workbuddyIcon, inset: 3 },
  'hermes': { src: hermesIcon, inset: 3 },
  'pi': { src: piIcon, inset: 3 },
  'kiro': { src: kiroIcon, inset: 3 },
}

function normalizeToolType(toolType: string) {
  return toolType.trim().toLowerCase()
}

function getMascotAsset(toolType: string): MascotAsset {
  const normalized = normalizeToolType(toolType)
  return MASCOT_ASSETS[normalized] ?? MASCOT_ASSETS['claude-code']
}

export function MascotCanvas({ toolType, animState, size = 32 }: MascotCanvasProps) {
  const asset = getMascotAsset(toolType)
  const inset = asset.inset ?? 0

  return (
    <span
      className="mascot-image"
      data-mascot-state={animState}
      data-mascot-source={normalizeToolType(toolType)}
      style={{ '--mascot-size': `${size}px`, '--mascot-inset': `${inset}px` } as CSSProperties}
      aria-hidden="true"
    >
      <img
        src={asset.src}
        alt=""
        draggable={false}
        style={{ objectFit: asset.fit ?? 'contain' }}
      />
    </span>
  )
}
