# Agent Island Completion Design Spec

**Date:** 2026-04-26
**Status:** Approved
**Product Positioning:** Monitor + Light Interaction (monitors external CLI agents, supports approval/questions/summary viewing, does NOT run its own agent sessions)
**Tech Stack:** Tauri v2 (Rust backend + WebView frontend), React 19, Zustand 5, framer-motion, i18next

## Implementation Batches

- **Batch 1:** State machine refactor + Priority system
- **Batch 2:** Interaction cards (Plan/Question/Response/Completion + enhanced Permission)
- **Batch 3:** Theme engine + Smart experience (suppression, display level, notifications, quiet hours)

---

## 1. Layered State Machine + Overlay Stack

### Base Layer

```
compact <-> expanded <-> detail(chat_history)
```

- **compact**: Collapsed pill showing highest-priority session summary
- **expanded**: Session list sorted by priority, showing session cards
- **detail**: Full chat history for a single session

Transitions:
- compact -> expanded: hover or click
- expanded -> compact: mouse leave / ESC
- expanded -> detail: click a session
- detail -> expanded: back button / ESC

### Overlay Layer

Overlays are independent of the base layer, managed by a priority queue:

```typescript
type OverlayType = 'permission' | 'question' | 'plan' | 'completion' | 'response'

const OVERLAY_PRIORITY: Record<OverlayType, number> = {
  permission: 100,  // blocks agent, most urgent
  plan: 90,         // blocks agent
  question: 80,     // blocks agent
  completion: 20,   // non-blocking, informational
  response: 10,     // non-blocking, AI reply preview
}
```

Core behavior:
- Overlay appears on top of base layer; base state is **preserved** (not changed)
- Multiple overlays queue up; highest-priority is shown first
- Handling one overlay auto-advances to the next in queue
- Queue empty -> overlay layer disappears, base layer visible again
- Non-blocking overlays (completion/response) have configurable dwell timer, auto-dismiss

### Display Level (Rust-controlled)

```
dormant -> compact -> visible
```

- **dormant**: Fully hidden (no sessions / ESC hide / idle timeout)
- **compact**: Pill only (cursor near notch area / persistent mode)
- **visible**: Full interaction (has overlay / hover expanded)

Rust layer responsibilities:
- Cursor position detection via `NSTrackingArea` (more efficient than JS polling)
- Frontmost app detection (for Smart Suppression)
- Window resize and click-through management (`setIgnoreMouseEvents`)
- Global ESC hotkey capture

---

## 2. Priority System

### 7-Level Priority Model

```typescript
const PRIORITY = {
  dormant: 0,     // no sessions
  idle: 1,        // idle > threshold (10min), can hide
  done: 2,        // just completed, awaiting user's next action
  thinking: 3,    // API request in progress, no tool call
  working: 4,     // executing tool call
  compacting: 5,  // context compaction in progress
  attention: 6,   // needs user action (permission/question/plan)
} as const
```

### Priority Computation

```typescript
function computePriority(session: SessionState): Priority {
  if (session.phase === 'error') return PRIORITY.attention
  if (session.phase === 'waiting_approval' || session.phase === 'waiting_input')
    return PRIORITY.attention
  if (session.phase === 'compacting') return PRIORITY.compacting
  if (session.phase === 'done') return PRIORITY.done
  if (session.phase === 'processing') {
    return session.lastToolName ? PRIORITY.working : PRIORITY.thinking
  }
  if (session.phase === 'idle') {
    const idleMs = Date.now() - (session.idleSince ?? session.startedAt)
    return idleMs > 10 * 60 * 1000 ? PRIORITY.idle : PRIORITY.done
  }
  return PRIORITY.idle
}
```

**Design decision (differs from evolab):** evolab escalates unattended attention to error-level after 60s. We reject this: user not responding != error. Instead, compact pill animation intensity increases over time (faster pulse, brighter color) without changing semantic priority. `idleSince` is stored inside session state rather than a separate Map.

### Compact Pill Priority-Driven Rendering

```
[PixelIndicator] [status text] [alert count badge] [session count] [rate%]
```

- PixelIndicator color and animation speed driven by priority
- Alert badge only shown at attention level; count = pending permissions + questions + plans
- Rate % display is configurable

---

## 3. Overlay Interaction Cards

### 3.1 Permission Card (existing, needs enhancement)

```
+-- Session Context --------------------------+
| project-name . Claude Code                  |
| You: Please refactor the auth module        |
+-- Permission -------------------------------+
| Bash                                        |
| npm install express                         |
|                                             |
| [Always Allow]  [Allow]  [Deny]             |
+---------------------------------------------+
```

Enhancements over current ApprovalBar:
- **Session Context Header**: Which session (project name + last user message) -- very useful context from evolab
- **Always Allow option**: Leverage Claude Code's `permissionSuggestions` for one-click permanent permission
- **Keyboard shortcut hints**: Show when modifier held (Cmd+Y allow / Cmd+N deny)

### 3.2 Plan Approval Card (new)

```
+-- Session Context --------------------------+
| project-name . Claude Code                  |
+-- Plan -------------------------------------+
| Implementation Plan for Auth Refactor [tag] |
|                                             |
| ## Task 1: Extract middleware...            |
| ## Task 2: Add JWT validation...            |
| (markdown rendered, max-height scrollable)  |
|                                             |
| Requested permissions:                      |
|  . Bash: run tests                          |
|  . Bash: install dependencies               |
+-- Feedback Input ---------------------------+
| [Tell Claude what to change...            ] |
+-- Actions ----------------------------------+
| [Send Feedback / Manual] [Accept Edits] [Auto] |
+---------------------------------------------+
```

3 action buttons:
- **Manual Review** / **Send Feedback** (transforms when input has content)
- **Accept Edits** (auto-approve Edit/Write, others still need confirmation)
- **Auto-approve All** (red button warning, all auto-approved)

**Design decision (differs from evolab):** evolab couples Plan and Permission through same IPC channel (respondPermission) due to SDK constraints. agent-island communicates through hooks bridge, so plan and permission are independent message types -- cleaner interface.

### 3.3 Question Card (existing, needs major enhancement)

**Single question mode:**
```
+-- Session Context --------------------------+
+-- Claude's Question ------------------------+
|                                             |
| [Tag] Which database should we use?         |
|                                             |
|  (1) PostgreSQL                             |
|     Best for relational data                |
|  (2) MongoDB                                |
|     Best for document-oriented data         |
|  (3) SQLite                                 |
|     Best for embedded/local                 |
|                                             |
| [-- Type something... --]                   |
|                                             |
| [Multi-select: Confirm Selection (2)]       |
+---------------------------------------------+
```

**Multi-question mode:**
- Multiple questions stacked vertically, each with independent options
- Each question supports custom text input
- Unified submit button at bottom

Key capabilities:
- Single-select: clicking an option sends immediately
- Multi-select (`multiSelect`): check options then confirm
- Custom input: click to expand input field, Enter to send
- Multi-question: submit all answers together

### 3.4 Assistant Response Card (new)

```
+-- Response ---------------------------------+
| You: Please fix the login bug               |
| ----------------------------  [Complete]     |
|                                             |
| I've fixed auth/login.ts by...              |
| (markdown rendered, max-height configurable) |
|                                             |
|          Click to jump to terminal ->       |
+---------------------------------------------+
```

Behavior:
- Appears after AI completes response (non-blocking overlay)
- Configurable dwell timer (default 5s, auto-dismiss)
- Hover pauses timer
- Click jumps to corresponding terminal
- Configurable `notifyMode: 'every' | 'turnEnd'`

### 3.5 Completion Card (new)

```
+-- Task Complete ----------------------------+
|                                             |
| Completed auth module refactor, 5 files     |
|                                             |
| [auto-dismiss after dwell timer]            |
+---------------------------------------------+
```

Simple completion notification, lighter weight than response card. Dwell timer then dismiss.

---

## 4. Smart Experience

### 4.1 Smart Suppression

When the user is already looking at the agent's terminal, the island shouldn't pop up to disturb them.

**Implementation (Rust layer):**

```rust
// Check frontmost app every 2 seconds
fn get_frontmost_app() -> Option<String> {
    // macOS: NSWorkspace.shared.frontmostApplication
    // Uses objc2 crate directly, no osascript (more efficient than evolab's approach)
}
```

**Suppression rules:**
1. Frontmost app matches a session's terminal (e.g., Terminal.app / iTerm2) -> suppress that session's overlays
2. Do NOT suppress other sessions' overlays
3. Only suppress non-blocking overlays (response/completion); blocking overlays (permission/question/plan) **degrade** to sound-only + badge count instead of full card popup (the pending request remains in the overlay queue and will display when the user switches away from the terminal or hovers the island)
4. Config toggle: `smartSuppression: boolean`

**Design decision (differs from evolab):** evolab uses `osascript` shell commands to poll frontmost app (forks a process each time, high overhead). agent-island uses Rust `NSWorkspace` API directly (zero fork overhead). evolab fully suppresses blocking overlays, which could cause users to miss permission requests. We degrade instead of suppress.

### 4.2 Display Level Management

3-level display controlled by Rust `DisplayController`:

```
dormant --(cursor near notch / new session / alert arrives)--> compact
compact --(hover / click / alert needs interaction)--> visible
visible --(mouse leave / ESC / dismiss)--> compact
compact --(no session 1s / ESC / all idle 5min)--> dormant
```

**Rust layer:**
- `NSTrackingArea` detects cursor entering notch region -> emits `display_level_changed` event
- `setIgnoreMouseEvents(true)` when dormant, `false` for compact/visible
- Fullscreen app detection -> configurable whether to hide during fullscreen

**Interaction Mode config:**
- `persistent`: Default -- mouse leave returns to compact (pill always visible)
- `minimal`: Mouse leave returns to dormant (fully hidden, only peek on events)

### 4.3 Peek Mechanism

When island is dormant but a new event occurs, briefly show compact pill:

```typescript
interface PeekConfig {
  duration: 3000,               // peek display duration (ms)
  cooldown: 30000,              // minimum interval between peeks
  skipCooldownForAlert: true,   // alerts ignore cooldown
}
```

- New session -> peek
- Tool change -> peek (with cooldown)
- Permission/question/plan -> forced peek (no cooldown) + upgrade to visible

### 4.4 ESC Multi-Level Exit

```
State               -> ESC behavior
---------------------------------------------
overlay visible     -> close current overlay (non-blocking: dismiss; blocking: fold to compact but keep pending)
detail (chat)       -> return to expanded
expanded            -> collapse to compact
compact             -> dormant + 30s silence period (cursor proximity won't wake)
```

**Design decision:** evolab uses double-ESC to hide from expanded state. We use single-ESC progressive exit -- more intuitive.

### 4.5 Session Mute

In expanded session list, right-click/long-press a session to mute:

- Muted session's all overlays are suppressed (no cards, no sounds)
- Auto-unmute after 30 minutes
- Auto-cleanup mute state when session ends
- Muted sessions have visual indicator in list (semi-transparent + mute icon)

---

## 5. Theme Engine

### Architecture

```
~/.agent-island/themes/
  default/
    theme.json          # theme config
    sprite.png          # sprite sheet (optional)
  pixel-cat/
    theme.json
    cat-sprite.png
  cyberpunk/
    theme.json
    cyber-sprite.png
```

Two rendering layers:
- **PixelIndicator** (default): No sprite sheet needed, code-rendered pixel grid animation
- **Sprite renderer** (optional): Loads sprite sheet, frame-by-frame character animation

### theme.json Schema

```typescript
interface ThemeConfig {
  name: string
  version: string
  author: 'builtin' | 'user'

  // Pixel indicator config (always available)
  pixelGrid: {
    cols: number        // default 5
    rows: number        // default 5
  }
  priorityColors: Record<string, string>   // priority -> color
  prioritySpeeds: Record<string, number>   // priority -> animation speed (ms)
  priorityPatterns: Record<string, {       // priority -> pixel pattern
    activePixels: Array<{ row: number; col: number }>
    animation: 'wave' | 'pulse' | 'breath' | 'spin' | 'blink'
    fps?: number
  }>

  // Sprite character (optional)
  character?: {
    spriteSheet: string      // filename
    frameSize: { width: number; height: number }
    scale: number
    animations: Record<string, {
      row: number
      frames: number
      fps: number
    }>
  }
  stateMapping?: Record<string, string>  // priority -> animation name

  // Sounds
  sounds: {
    pack: 'synth' | '8bit' | 'system' | 'none'
    overrides?: Record<string, string>   // event -> custom audio file
  }

  // UI customization
  statusLabels?: Record<string, string>    // priority -> custom label text
  alertColors?: {
    permission?: string
    question?: string
    plan?: string
    feedback?: string
  }
  compactHeight?: number
  pixelCursor?: { enabled: boolean; color: string }
}
```

### Rendering Strategy

```
Has sprite sheet?
  Yes -> SpriteCanvas component (canvas 2D frame-by-frame rendering)
  No  -> PixelIndicator component (CSS grid + animation)
```

In compact pill:
- Default theme: PixelIndicator (existing pixel grid)
- Sprite theme: Small character on pill left side, state-driven animation (idle -> walk -> run -> alert)

Both renderers share the same priority input; seamless transition on theme switch.

### Theme Loading Flow

1. Rust layer scans `~/.agent-island/themes/` on startup
2. Reads each theme's `theme.json`, converts sprite sheet to base64 data URL
3. Sends `ThemeBundle` to frontend via Tauri event
4. Frontend store saves `activeTheme`, render components select renderer based on theme

### Design Decisions (vs evolab)

- evolab stores themes in Electron `APP_DATA_DIR`, read via main process IPC. agent-island uses `~/.agent-island/themes/` -- transparent, user can edit directly
- evolab's `compactCrop` and `spriteFilter` config is overly granular. Simplified to `character.scale` + `stateMapping` -- sufficient and less error-prone
- Built-in 2-3 themes (default pixel, 8bit cat, minimal dot) for out-of-box choice

---

## 6. Chat History + Task/Subagent Tracking + Other Features

### 6.1 Chat History View

Click a session in expanded list to enter detail layer with full conversation history:

```
+-- Header -----------------------------------+
| <- Back    project-name . session title      |
+-- Chat Messages ----------------------------+
| You: Please refactor the auth module         |
|                                             |
| [thinking x 3] [tool calls x 5]            |
|   Edit: src/auth/login.ts  ok              |
|   Bash: npm test  ok                        |
|   Read: src/auth/config.ts  ok              |
|                                             |
|   Final reply: I've completed the refactor...|
|                                             |
| You: Did the tests pass?                    |
| ...                                         |
+-- (scrollable, loaded from JSONL) ----------+
+---------------------------------------------+
```

**Message collapsing strategy:**
- Assistant intermediate thinking + tool calls collapse to single summary line: `[thinking x N] [tool calls x M]`
- Click to expand full tool call list
- Each tool call shows: tool name + target + status icon (ok/fail/running)
- Edit/Write tool calls can expand to show diff preview
- Final reply text (`trailingContent`) always shown outside collapsed region

**Data loading:**
- Enter detail -> load from JSONL file (via Rust `get_chat_history` command)
- Polling refresh (3s interval) while session is active
- Stop polling on leaving detail view

### 6.2 Task Tracking

Extract `TaskCreate` / `TaskUpdate` tool calls from hook events, show progress in session card:

```
+-- Session Card (in expanded list) ----------+
| project-name          Working    Edit       |
| You: Implement user auth system             |
|                                             |
| Tasks: done done active pending pending 2/5 |
|  done  Set up project structure             |
|  done  Create user model                    |
|  active  Implement login endpoint           |
|  pending  Add JWT validation                |
|  pending  Write tests                       |
|                                             |
| Agents: @researcher done  @implementer ...  |
+---------------------------------------------+
```

**Data source:**
- Hook bridge forwards `PreToolUse` / `PostToolUse` events
- Extract TaskCreate `subject`, TaskUpdate `taskId` + `status` from tool_input
- Stored in sessionStore, per-session task Map

**Display rules:**
- Task area only shown when tasks exist
- Hidden when all completed (cleanup after next user message)
- Progress in compact format: `done done active pending pending (2/5)`

### 6.3 Subagent Tracking

From `Agent` / `Task` tool calls:
- Detect Agent tool -> add to session's subagents list
- Mark all running subagents as completed at turn end
- Display as compact one-line list in session card: `@researcher done  @implementer running`

### 6.4 Hook Auto-Recovery

agent-island communicates with Claude Code through hooks. Other tools may break hooks config.

**Solution:**
- Rust layer uses `notify` crate to watch `~/.claude/settings.json` file changes
- Hooks deleted/modified -> auto-restore + frontend notification bar
- Repeated deletions (>3 times/min) -> stop restoring, show warning for manual handling
- Improvement over evolab: evolab uses JS-layer polling; agent-island uses Rust fs watcher (real-time, no polling overhead)

### 6.5 macOS System Notifications

When app is not in foreground and blocking event arrives, send macOS native notification:

- Permission Request -> "Permission: Bash needs approval"
- Question -> "Question: Which database?"
- Plan -> "Plan: Auth Refactor"
- Completion -> "Task Complete: modified 5 files"

Click notification -> bring up island window and focus. Implemented via Tauri's `tauri-plugin-notification`.

### 6.6 Quiet Hours

```typescript
interface QuietHoursConfig {
  enabled: boolean
  start: string    // "22:00"
  end: string      // "08:00"
}
```

- During quiet hours: no sounds, no macOS notifications, no peek
- Blocking overlays still appear normally (can't miss permissions)
- Configured in settings panel

---

## Key Design Decisions Summary

| Decision | Our Approach | evolab's Approach | Rationale |
|----------|-------------|-------------------|-----------|
| State model | Layered (base + overlay) | Flat 7-state | Fewer transition rules, no lost context during alerts |
| Unattended escalation | Animation intensity increase | Priority escalation to error | Not responding != error |
| Smart suppression for blocking events | Degrade to sound+badge | Full suppression | Can't risk user missing permissions |
| Frontmost app detection | Rust NSWorkspace API | osascript fork | Zero fork overhead |
| ESC behavior | Single-ESC progressive | Double-ESC from expanded | More intuitive |
| Theme storage | ~/.agent-island/themes/ | Electron APP_DATA_DIR | Transparent, user-editable |
| Plan/Permission IPC | Independent message types | Shared respondPermission | Cleaner separation via hooks bridge |
| Sprite config | scale + stateMapping | compactCrop + spriteFilter | Simpler, less error-prone |
| Hook recovery | Rust fs watcher (notify crate) | JS polling | Real-time, no polling overhead |
