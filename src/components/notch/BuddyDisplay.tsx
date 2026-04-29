/* BuddyDisplay — Claude Buddy pet UI with ASCII art, stats bars, and heart particles */
import { useCallback, useEffect, useRef, useState } from 'react'
import { isTauri } from '../../services/tauriApi'

interface BuddyData {
  species: string
  name: string
  level: number
  xp: number
  xpMax: number
  happiness: number
  energy: number
  interactions: number
}

interface HeartParticle {
  id: number
  x: number
  y: number
}

// ASCII art for 18 species (3 rows each)
const SPECIES_ART: Record<string, string[]> = {
  cat:      [' /\\_/\\ ', '( ^.^ )', ' > ~ < '],
  dog:      ['  u u  ', ' (o_o) ', '  ^_^  '],
  fox:      [' /\\  /\\', '( @  @)', ' \\  / '],
  bunny:    [' (\\/) ', ' (^.^)', '(")_(")'],
  bear:     [' (>  <)', '(/   \\)', '|  ω  |'],
  penguin:  [' (> <) ', ' /|^|\\', ' ( v ) '],
  panda:    ['(●  ●)', '( >ω<)', '|  Uu |'],
  dragon:   [' <^> ', '(x.x)', '/|W|\\'],
  unicorn:  [' /\\  ', '(QQ) ', '>||< '],
  owl:      [' (O O) ', ' (> <) ', ' __|__ '],
  capybara: ['~~~~~', '(u u)', '|___| '],
  shiba:    [' /\\ /\\', '(=;=) ', ' W W '],
  axolotl:  [' ~oOo~', '( ◕ )', '|ε| |ε|'],
  frog:     ['  o   o  ', ' /  ∪  \\', '|  ---  |'],
  hamster:  [' (\\/) ', '(●.●)', '(//)  '],
  sloth:    [' .-.  ', '(=0=)', '(‾\\_)'],
  octopus:  [' (◉ ◉) ', ' ( ~~~ )', '|_|_|_| '],
  axe:      [' |  | ', '(>  <)', ' \\__/ '],
}

const FALLBACK_ART = [' (^ ^) ', '  |_|  ', ' / \\ ']

const SPECIES_COLORS: Record<string, string> = {
  cat: '#FF9500', dog: '#A0522D', fox: '#FF6B35', bunny: '#FFB6C1',
  bear: '#8B4513', penguin: '#4A4A8A', panda: '#333', dragon: '#7C3AED',
  unicorn: '#C084FC', owl: '#8B7355', capybara: '#BDB76B', shiba: '#E8822A',
  axolotl: '#FF69B4', frog: '#4CAF50', hamster: '#D2691E', sloth: '#9E8F7E',
  octopus: '#6A5ACD', axe: '#607D8B',
}

async function loadBuddyData(): Promise<BuddyData | null> {
  if (!isTauri()) {
    // Dev-mode stub
    return { species: 'cat', name: 'Whiskers', level: 3, xp: 240, xpMax: 500, happiness: 78, energy: 60, interactions: 42 }
  }
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const raw = await invoke<string>('read_buddy_data')
    return JSON.parse(raw) as BuddyData
  } catch {
    return null
  }
}

function StatBar({ label, value, color }: { label: string; value: number; color: string }) {
  return (
    <div className="buddy-stat">
      <span className="buddy-stat__label">{label}</span>
      <div className="buddy-stat__track">
        <div className="buddy-stat__fill" style={{ width: `${Math.min(100, value)}%`, background: color }} />
      </div>
      <span className="buddy-stat__val">{value}</span>
    </div>
  )
}

export function BuddyDisplay() {
  const [buddy, setBuddy] = useState<BuddyData | null>(null)
  const [hearts, setHearts] = useState<HeartParticle[]>([])
  const heartId = useRef(0)
  const containerRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    loadBuddyData().then(setBuddy)
  }, [])

  const spawnHearts = useCallback((e: React.MouseEvent) => {
    if (!containerRef.current) return
    const rect = containerRef.current.getBoundingClientRect()
    const x = ((e.clientX - rect.left) / rect.width) * 100
    const y = ((e.clientY - rect.top) / rect.height) * 100
    const batch: HeartParticle[] = Array.from({ length: 4 }, () => ({
      id: ++heartId.current,
      x: x + (Math.random() - 0.5) * 20,
      y: y + (Math.random() - 0.5) * 20,
    }))
    setHearts(h => [...h, ...batch])
    setTimeout(() => setHearts(h => h.filter(p => !batch.find(b => b.id === p.id))), 900)

    if (buddy) {
      setBuddy(b => b ? { ...b, happiness: Math.min(100, b.happiness + 2), interactions: b.interactions + 1 } : b)
    }
  }, [buddy])

  if (!buddy) {
    return (
      <div className="buddy-display buddy-display--empty">
        <span className="buddy-display__empty-text">No buddy found</span>
        <span className="buddy-display__empty-hint">Add a buddy in ~/.claude.json</span>
      </div>
    )
  }

  const art = SPECIES_ART[buddy.species] ?? FALLBACK_ART
  const color = SPECIES_COLORS[buddy.species] ?? '#007AFF'
  const xpPct = Math.min(100, (buddy.xp / buddy.xpMax) * 100)

  return (
    <div className="buddy-display" ref={containerRef}>
      <div className="buddy-ascii" onClick={spawnHearts} style={{ color }} aria-label={`Pet ${buddy.name}`}>
        {art.map((line, i) => (
          <div key={i} className="buddy-ascii__line">{line}</div>
        ))}
        {hearts.map(p => (
          <span
            key={p.id}
            className="buddy-heart"
            style={{ left: `${p.x}%`, top: `${p.y}%` }}
          >♥</span>
        ))}
      </div>

      <div className="buddy-info">
        <div className="buddy-info__name-row">
          <span className="buddy-info__name">{buddy.name}</span>
          <span className="buddy-info__species">{buddy.species}</span>
          <span className="buddy-info__level">Lv.{buddy.level}</span>
        </div>

        <div className="buddy-info__xp-row">
          <div className="buddy-stat__track">
            <div className="buddy-stat__fill buddy-stat__fill--xp" style={{ width: `${xpPct}%` }} />
          </div>
          <span className="buddy-stat__val">{buddy.xp}/{buddy.xpMax} XP</span>
        </div>

        <StatBar label="💖" value={buddy.happiness} color="#FF6B9D" />
        <StatBar label="⚡" value={buddy.energy} color="#FFD700" />
      </div>

      <div className="buddy-footer">
        <span className="buddy-footer__interactions">{buddy.interactions} pets</span>
        <span className="buddy-footer__hint">Click to pet!</span>
      </div>

      <style>{`
        .buddy-display { position: relative; display: flex; flex-direction: column; gap: 8px; padding: 12px; }
        .buddy-display--empty { align-items: center; padding: 16px; gap: 4px; }
        .buddy-display__empty-text { font-size: 12px; color: var(--vi-text-secondary); }
        .buddy-display__empty-hint { font-size: 10px; color: var(--vi-text-tertiary); font-family: var(--font-mono); }
        .buddy-ascii { position: relative; font-family: var(--font-mono); font-size: 13px; line-height: 1.5; cursor: pointer; user-select: none; text-align: center; padding: 4px; border-radius: 8px; transition: background 0.15s ease; }
        .buddy-ascii:hover { background: rgba(255,255,255,0.04); }
        .buddy-ascii__line { white-space: pre; }
        .buddy-heart { position: absolute; font-size: 14px; color: #FF6B9D; pointer-events: none; animation: buddy-float 0.85s ease-out forwards; transform: translateX(-50%) translateY(-50%); }
        @keyframes buddy-float { 0% { opacity: 1; transform: translateX(-50%) translateY(-50%) scale(1); } 100% { opacity: 0; transform: translateX(-50%) translateY(calc(-50% - 32px)) scale(1.4); } }
        .buddy-info { display: flex; flex-direction: column; gap: 4px; }
        .buddy-info__name-row { display: flex; align-items: baseline; gap: 6px; }
        .buddy-info__name { font-size: 13px; font-weight: 600; color: var(--vi-text-primary); }
        .buddy-info__species { font-size: 10px; color: var(--vi-text-tertiary); text-transform: capitalize; }
        .buddy-info__level { font-size: 11px; font-weight: 600; color: var(--vi-accent-blue); margin-left: auto; }
        .buddy-info__xp-row { display: flex; align-items: center; gap: 6px; }
        .buddy-info__xp-row .buddy-stat__track { flex: 1; }
        .buddy-info__xp-row .buddy-stat__val { font-size: 10px; white-space: nowrap; }
        .buddy-stat { display: flex; align-items: center; gap: 6px; }
        .buddy-stat__label { font-size: 11px; width: 16px; flex-shrink: 0; }
        .buddy-stat__track { flex: 1; height: 4px; background: rgba(255,255,255,0.08); border-radius: 2px; overflow: hidden; }
        .buddy-stat__fill { height: 100%; border-radius: 2px; transition: width 0.4s ease; }
        .buddy-stat__fill--xp { background: linear-gradient(90deg, #007AFF, #5AC8FA); }
        .buddy-stat__val { font-size: 10px; color: var(--vi-text-tertiary); width: 20px; text-align: right; flex-shrink: 0; }
        .buddy-footer { display: flex; justify-content: space-between; margin-top: 2px; }
        .buddy-footer__interactions { font-size: 10px; color: var(--vi-text-tertiary); }
        .buddy-footer__hint { font-size: 10px; color: var(--vi-text-tertiary); opacity: 0.5; }
      `}</style>
    </div>
  )
}
