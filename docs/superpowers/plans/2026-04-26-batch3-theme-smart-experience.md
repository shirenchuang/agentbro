# Batch 3: Theme Engine + Smart Experience

**Date:** 2026-04-26
**Spec:** `docs/superpowers/specs/2026-04-26-agent-island-completion-design.md` — Sections 4, 5, 6
**Depends on:** Batch 1 (priority system), Batch 2 (overlay cards)

---

## Goal

Three areas:
1. **Theme engine** — loadable themes with priority-driven pixel art and optional sprite characters
2. **Smart experience** — suppression, display level, peek, session mute, quiet hours
3. **Remaining features** — chat history improvements, task/subagent tracking, hook auto-recovery, macOS notifications

---

## Part A: Theme Engine (Spec Section 5)

### Task A1: Theme types and loading infrastructure

**Files to create:**
- `src/types/theme.ts` (new)

```typescript
export interface PixelPattern {
  activePixels: Array<{ row: number; col: number }>
  animation: 'wave' | 'pulse' | 'breath' | 'spin' | 'blink'
  fps?: number
}

export interface SpriteAnimation {
  row: number
  frames: number
  fps: number
}

export interface ThemeConfig {
  name: string
  version: string
  author: 'builtin' | 'user'
  pixelGrid: { cols: number; rows: number }
  priorityColors: Record<string, string>
  prioritySpeeds: Record<string, number>
  priorityPatterns: Record<string, PixelPattern>
  character?: {
    spriteSheet: string          // base64 data URL from Rust
    frameSize: { width: number; height: number }
    scale: number
    animations: Record<string, SpriteAnimation>
  }
  stateMapping?: Record<string, string>  // priority name → animation name
  sounds: {
    pack: 'synth' | '8bit' | 'system' | 'none'
    overrides?: Record<string, string>
  }
  statusLabels?: Record<string, string>
  alertColors?: { permission?: string; question?: string; plan?: string; feedback?: string }
  compactHeight?: number
  pixelCursor?: { enabled: boolean; color: string }
}
```

### Task A2: Theme store

**Files to create:**
- `src/stores/themeStore.ts` (new)

```typescript
interface ThemeStore {
  themes: ThemeConfig[]
  activeThemeName: string
  activeTheme: ThemeConfig  // derived
  setActiveTheme: (name: string) => void
  loadThemes: (themes: ThemeConfig[]) => void
}
```

- Default theme built-in (current pixel grid behavior codified as a ThemeConfig)
- Listen for `theme-bundle` Tauri event to load themes from Rust

### Task A3: Rust theme scanner

**Files to create:**
- `src-tauri/src/theme/mod.rs` (new)
- `src-tauri/src/theme/scanner.rs` (new)

**`scanner.rs`:**
- On startup, scan `~/.agent-island/themes/` directory
- For each subdirectory: read `theme.json`, validate, convert sprite sheet PNG to base64 data URL
- Emit `theme-bundle` Tauri event with `Vec<ThemeConfig>` payload
- Watch directory for changes (reuse `notify` crate pattern from file_watcher.rs)

**`mod.rs`:**
- `#[tauri::command] fn get_themes()` — returns loaded themes
- `#[tauri::command] fn set_active_theme(name: String)` — persists to config

Wire into `src-tauri/src/lib.rs` — add module + commands.

### Task A4: Update PixelIndicator to use theme

**Files to modify:**
- `src/components/notch/PixelIndicator.tsx`

Current: hardcoded 5×5 grid with phase-driven colors.
Change: read from `themeStore.activeTheme`:

```typescript
const theme = useThemeStore((s) => s.activeTheme)
const priorityKey = priorityName(priority)
const color = theme.priorityColors[priorityKey] ?? fallbackColor
const speed = theme.prioritySpeeds[priorityKey] ?? 1000
const pattern = theme.priorityPatterns[priorityKey]
const { cols, rows } = theme.pixelGrid
```

Render pixel grid using theme dimensions and patterns.

### Task A5: Sprite renderer component

**Files to create:**
- `src/components/notch/SpriteCanvas.tsx` (new)

Canvas-based sprite sheet renderer:

```typescript
interface SpriteCanvasProps {
  theme: ThemeConfig   // must have character defined
  priority: Priority
  size: number
}
```

- Load sprite sheet from base64 data URL into Image
- Map priority → animation name via `theme.stateMapping`
- Animate frames on canvas using requestAnimationFrame
- Fallback to PixelIndicator if no character in theme

### Task A6: Theme selector in settings

**Files to modify:**
- `src/components/settings/sections/DisplaySection.tsx` — add theme dropdown

Show list of loaded themes. Preview showing PixelIndicator or SpriteCanvas with current priority. Selecting a theme updates `themeStore` and persists via Rust command.

### Task A7: Built-in themes

**Files to create (in repo, copied to `~/.agent-island/themes/` on first run):**
- `themes/default/theme.json` — codifies current pixel behavior
- `themes/8bit-cat/theme.json` + `themes/8bit-cat/cat-sprite.png`
- `themes/minimal-dot/theme.json` — single dot, minimal animation

Rust startup: if `~/.agent-island/themes/` doesn't exist or is empty, copy built-in themes there.

---

## Part B: Smart Experience (Spec Section 4)

### Task B1: Display level controller (Rust)

**Files to modify:**
- `src-tauri/src/platform/mod.rs` — add `DisplayController`

Implement 3-level display:

```
dormant ──(cursor near notch / new session / alert)──→ compact
compact ──(hover / click / alert needs interaction)──→ visible
visible ──(mouse leave / ESC / dismiss)──→ compact
compact ──(no session 1s / ESC / all idle 5min)──→ dormant
```

- Use existing `NSTrackingArea` cursor detection (already in platform)
- Emit `display-level-changed` Tauri event
- Control `setIgnoreMouseEvents` based on level
- Track silence period (ESC from compact → dormant + 30s ignore cursor)

### Task B2: Frontend display level hook

**Files to create:**
- `src/hooks/useDisplayLevel.ts` (new)

Listen for `display-level-changed` event, update store:

```typescript
export function useDisplayLevel() {
  useEffect(() => {
    if (!isTauri()) return
    // listen for display-level-changed
    // update sessionStore.displayLevel
    // map to baseLayer changes:
    //   dormant → hide panel (setNotchOpacity(0))
    //   compact → baseLayer 'compact'
    //   visible → keep current base layer
  }, [])
}
```

Wire into `useTauriInit()`.

### Task B3: Peek mechanism

**Files to modify:**
- `src/stores/sessionStore.ts` — add peek state

```typescript
interface PeekConfig {
  duration: 3000
  cooldown: 30000
  skipCooldownForAlert: true
}
```

When display level is dormant and new event arrives:
- Non-alert event → check cooldown, if ok → briefly show compact pill for `duration` ms
- Alert event → force peek (ignore cooldown) → upgrade to visible

Track `lastPeekAt` timestamp for cooldown enforcement.

### Task B4: Smart suppression (frontend integration)

**Files to modify:**
- `src/hooks/useTauri.ts` — enhance `useSessionEvents`

Current: `session-update` event has `suppressed?: boolean` field, used to revert auto-expand.

Enhancement:
- When `suppressed` is true for a session, also suppress that session's non-blocking overlays (response/completion)
- Blocking overlays (permission/question/plan) → degrade: don't show card, but increment badge count + play sound
- Store suppressed session IDs in a Set

### Task B5: Session mute

**Files to modify:**
- `src/stores/sessionStore.ts` — add mute state
- `src/components/notch/HoverList.tsx` — add mute UI

```typescript
// In SessionStore:
mutedSessions: Record<string, number>  // sessionId → mute expiry timestamp
muteSession: (id: string, durationMs?: number) => void
unmuteSession: (id: string) => void
isSessionMuted: (id: string) => boolean
```

Default mute duration: 30 minutes. Auto-unmute via timer. Auto-cleanup on session end.

In HoverList: right-click context menu on session card → "Mute 30min" option. Muted sessions render semi-transparent with mute icon.

In overlay queue: skip overlays from muted sessions (check before showing `activeOverlay`).

### Task B6: Quiet hours

**Files to modify:**
- `src/stores/configStore.ts` — add quiet hours config
- `src/components/settings/sections/GeneralSection.tsx` — add quiet hours UI

```typescript
interface QuietHoursConfig {
  enabled: boolean
  start: string  // "22:00"
  end: string    // "08:00"
}
```

During quiet hours:
- No sounds (check before playing in sound module)
- No macOS notifications
- No peek
- Blocking overlays still appear normally

Add `isQuietHours()` utility that checks current time against config.

---

## Part C: Remaining Features (Spec Section 6)

### Task C1: Chat history improvements

**Files to modify:**
- `src/components/notch/ChatView.tsx`
- `src/components/notch/MessageBubble.tsx`

Implement message collapsing:
- Group consecutive thinking + tool_use messages into a collapsible summary: `[thinking × N] [tool calls × M]`
- Click to expand full tool call list
- Each tool call shows: icon + tool name + target + status badge
- Edit/Write tool calls show expandable diff preview (reuse DiffView)
- Final assistant text (`trailingContent`) always visible outside collapsed region

### Task C2: Task tracking from hook events

**Files to modify:**
- `src/stores/sessionStore.ts` — extract tasks from tool events
- `src/types/agent.ts` — add `task_update` event type

```typescript
// New event type:
| { type: 'task_update'; sessionId: string; taskId: string; subject: string; status: 'pending' | 'in_progress' | 'completed' }
```

In `updateSession` for `tool_use` events:
- If `toolName === 'TaskCreate'` → parse `toolInput` for subject, add to session's tasks
- If `toolName === 'TaskUpdate'` → parse for taskId + status, update existing task

Display in HoverList session cards using existing `TaskSummary` component (already exists).

### Task C3: Subagent tracking

**Files to modify:**
- `src/stores/sessionStore.ts` — track subagents from Agent tool calls

In `updateSession` for `tool_use` events:
- If `toolName === 'Agent'` → parse description, add to session's subagents list
- On session `task_complete` → mark all running subagents as completed

Display in HoverList and ChatView using existing `SubagentList` component.

### Task C4: Hook auto-recovery (Rust)

**Files to create/modify:**
- `src-tauri/src/hooks/recovery.rs` (new)
- `src-tauri/src/hooks/mod.rs` (modify)

Use `notify` crate to watch `~/.claude/settings.json`:
- On file change → verify hooks still present
- If hooks removed/modified → auto-restore from backup
- If repeated deletions (>3/min) → stop restoring, emit `hook-recovery-failed` event
- Frontend shows warning notification bar

### Task C5: macOS system notifications

**Files to modify:**
- `src-tauri/Cargo.toml` — add `tauri-plugin-notification`
- `src-tauri/src/lib.rs` — register plugin
- `src/services/tauriApi.ts` — add notification wrapper

When blocking event arrives and app is not foreground:
- Permission → "Permission: {toolName} needs approval"
- Question → "Question: {question text truncated}"
- Plan → "Plan: {planTitle}"
- Completion → "Task Complete: {summary}"

Click notification → bring island window to focus.

Respect quiet hours and mute state before sending.

---

## File Change Summary

| File | Action | Description |
|------|--------|-------------|
| **Theme** | | |
| `src/types/theme.ts` | Create | Theme type definitions |
| `src/stores/themeStore.ts` | Create | Theme state management |
| `src-tauri/src/theme/mod.rs` | Create | Theme commands |
| `src-tauri/src/theme/scanner.rs` | Create | Theme directory scanner |
| `src/components/notch/SpriteCanvas.tsx` | Create | Canvas sprite renderer |
| `src/components/notch/PixelIndicator.tsx` | Modify | Read from theme store |
| `src/components/settings/sections/DisplaySection.tsx` | Modify | Theme selector |
| `themes/default/theme.json` | Create | Built-in default theme |
| `themes/8bit-cat/theme.json` + sprite | Create | Built-in cat theme |
| `themes/minimal-dot/theme.json` | Create | Built-in minimal theme |
| **Smart Experience** | | |
| `src-tauri/src/platform/mod.rs` | Modify | DisplayController |
| `src/hooks/useDisplayLevel.ts` | Create | Display level hook |
| `src/stores/sessionStore.ts` | Modify | Peek, mute, suppression |
| `src/components/notch/HoverList.tsx` | Modify | Mute UI |
| `src/stores/configStore.ts` | Modify | Quiet hours config |
| `src/components/settings/sections/GeneralSection.tsx` | Modify | Quiet hours UI |
| **Remaining** | | |
| `src/components/notch/ChatView.tsx` | Modify | Message collapsing |
| `src/components/notch/MessageBubble.tsx` | Modify | Collapsible groups |
| `src-tauri/src/hooks/recovery.rs` | Create | Hook auto-recovery |
| `src-tauri/Cargo.toml` | Modify | Add notification plugin |
| `src/services/tauriApi.ts` | Modify | Notification wrapper |

---

## Implementation Order

```
Part A (Theme):     A1 → A2 → A3 → A4 → A5 → A6 → A7
Part B (Smart):     B1 → B2 → B3 → B4 → B5 → B6
Part C (Features):  C1, C2, C3 (parallel) → C4 → C5
```

Parts A, B, C are mostly independent and can be worked on in parallel by different engineers. Dependencies:
- A4 needs Batch 1's priority system
- B3/B4 need Batch 1's overlay queue
- C5 needs B6 (quiet hours check before sending notification)

---

## Verification Checklist

**Theme:**
- [ ] Default theme loads and renders same as before (no visual regression)
- [ ] Theme selector in settings shows available themes
- [ ] Switching theme updates PixelIndicator colors/patterns immediately
- [ ] Sprite theme renders canvas animation correctly
- [ ] Custom theme in `~/.agent-island/themes/` auto-detected

**Smart Experience:**
- [ ] Display level transitions work: dormant ↔ compact ↔ visible
- [ ] Peek shows pill briefly on new events, respects cooldown
- [ ] Smart suppression: permission request while in terminal → sound + badge only
- [ ] Session mute: right-click → mute → no overlays for 30 min
- [ ] Quiet hours: no sounds/notifications during configured period
- [ ] ESC from compact → dormant with 30s silence

**Remaining:**
- [ ] Chat history: consecutive tools collapse, click expands
- [ ] Task tracking: TaskCreate/TaskUpdate tool calls update session tasks
- [ ] Subagent tracking: Agent tool calls appear in session card
- [ ] Hook auto-recovery: delete hooks → auto-restore within seconds
- [ ] macOS notification on permission request when app not focused
- [ ] Notification click brings island to foreground
