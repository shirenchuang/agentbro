# Vibe Island Experience Gap Alignment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Align agent-island experience with Vibe Island by implementing: 1) task complete auto-collapse, 2) session state persistence, 3) toolTarget tracking.

**Architecture:** Add timer-based auto-collapse logic in sessionStore, implement JSON file persistence for sessions, and enhance tool tracking to include target information.

**Tech Stack:** TypeScript (Zustand), Tauri (Rust backend), JSON file storage

---

## File Structure

```
src/
├── stores/sessionStore.ts        # Modify: Add auto-collapse timer, persistence
├── stores/configStore.ts         # Modify: Add timing configuration
├── services/tauriApi.ts          # Modify: Add persistence methods
├── hooks/useTauri.ts             # Modify: Add lastToolTarget mapping
└── types/agent.ts                # Modify: Add toolTarget type

src-tauri/src/
├── commands/persistence.rs       # Create: File I/O commands
├── agents/claude_code.rs         # Modify: Extract tool_target in adapter
└── agents/mod.rs                 # Modify: Add tool_target to ToolUse
```

---

## Task 1: Task Complete Auto-Collapse

**Files:**
- Modify: `src/stores/sessionStore.ts`
- Modify: `src/stores/configStore.ts`
- Test: Manual testing with Claude Code

- [ ] **Step 1: Add taskCompleteDwellSeconds config to configStore**

Add to `src/stores/configStore.ts`:
```typescript
taskCompleteDwellSeconds: number // default: 3 seconds
```

- [ ] **Step 2: Modify sessionStore to handle task_complete with auto-collapse**

In `src/stores/sessionStore.ts`, find the `task_complete` case and add:
```typescript
case 'task_complete': {
  const session = sessions[event.sessionId]
  if (session) {
    const msg: ChatMessage = { role: 'assistant', content: event.summary, timestamp: Date.now() }
    sessions[event.sessionId] = {
      ...session,
      phase: 'done',
      description: event.summary,
      pendingPermission: undefined,
      pendingQuestion: undefined,
      chatHistory: [...session.chatHistory, msg],
      taskCompletedAt: Date.now(),
    }
  }
  // FIX: Store session ID and verify it's still active before collapsing
  const completedSessionId = event.sessionId
  const dwellSeconds = useConfigStore.getState().taskCompleteDwellSeconds || 3
  setTimeout(() => {
    const store = useSessionStore.getState()
    if (store.activeSessionId === completedSessionId) {
      store.setPanelState('collapsed')
    }
  }, dwellSeconds * 1000)
  break
}
```

**Note:** Import `useConfigStore` at the top of sessionStore.ts.

- [ ] **Step 3: Add taskCompletedAt to SessionState type**

In `src/types/agent.ts`, add to SessionState interface:
```typescript
taskCompletedAt?: number // timestamp when task completed
```

- [ ] **Step 4: Test auto-collapse behavior**

Run: Start Claude Code session, complete a task, verify panel auto-collapses after configured seconds

- [ ] **Step 5: Commit**

```bash
git add src/stores/sessionStore.ts src/types/agent.ts src/stores/configStore.ts
git commit -m "feat: add task complete auto-collapse timer"
```

---

## Task 2: Session State Persistence

**Files:**
- Modify: `src/stores/sessionStore.ts`
- Modify: `src/services/tauriApi.ts`
- Create: `src-tauri/src/commands/persistence.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: Verify sessions survive app restart

- [ ] **Step 1: Add persistence methods to tauriApi**

In `src/services/tauriApi.ts`, add:
```typescript
async function saveSessions(sessions: SessionState[]): Promise<void> {
  await invoke('save_sessions', { sessionsJson: JSON.stringify(sessions) })
}

async function loadSessions(): Promise<SessionState[]> {
  const data = await invoke<string>('load_sessions')
  return data ? JSON.parse(data) : []
}
```

- [ ] **Step 2: Create Rust persistence commands**

Create `src-tauri/src/commands/persistence.rs`:
```rust
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const SESSIONS_FILE: &str = "sessions.json";
const APP_SUPPORT_DIR: &str = "agent-island";

fn get_sessions_path() -> Option<PathBuf> {
    dirs::data_dir()
        .or_else(dirs::data_local_dir)  // fallback
        .map(|p| p.join(APP_SUPPORT_DIR).join(SESSIONS_FILE))
}

#[tauri::command]
pub async fn save_sessions(sessions_json: String) -> Result<(), String> {
    let path = get_sessions_path().ok_or("Cannot get data directory")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, sessions_json).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn load_sessions() -> Result<String, String> {
    let path = get_sessions_path().ok_or("Cannot get data directory")?;
    if !path.exists() {
        return Ok("[]".to_string());
    }
    fs::read_to_string(&path).map_err(|e| e.to_string())
}
```

- [ ] **Step 3: Register commands in lib.rs**

Add to `src-tauri/src/lib.rs`:
```rust
mod commands;
use commands::persistence::{save_sessions, load_sessions};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![save_sessions, load_sessions])
        // ... rest of setup
}
```

- [ ] **Step 4: Integrate persistence with sessionStore**

In `src/stores/sessionStore.ts`, add load on init and save on change:
```typescript
// In the create<SessionStore>((set, get) => ({ ... })) initialization:
const loadPersistedSessions = async () => {
  try {
    const saved = await tauriApi.loadSessions()
    if (saved.length > 0) {
      const sessionMap: Record<string, SessionState> = {}
      saved.forEach((s: SessionState) => { sessionMap[s.id] = s })
      set({ sessions: sessionMap, sessionList: saved })
    }
  } catch (e) {
    console.warn('Failed to load sessions:', e)
  }
}
loadPersistedSessions()

// Add debounced save after each update
let saveTimeout: ReturnType<typeof setTimeout> | null = null
const saveSessionsDebounced = () => {
  if (saveTimeout) clearTimeout(saveTimeout)
  saveTimeout = setTimeout(async () => {
    try {
      await tauriApi.saveSessions(Object.values(get().sessions))
    } catch (e) {
      console.warn('Failed to save sessions:', e)
    }
  }, 1000)
}
// Call saveSessionsDebounced() at the end of each updateSession case
```

- [ ] **Step 5: Test persistence**

Run:
1. Start agent-island with active sessions
2. Kill and restart the app
3. Verify sessions are restored

- [ ] **Step 6: Commit**

```bash
git add src/stores/sessionStore.ts src/services/tauriApi.ts src-tauri/src/commands/
git commit -m "feat: add session state persistence"
```

---

## Task 3: Tool Target Tracking

**Files:**
- Modify: `src/types/agent.ts`
- Modify: `src/stores/sessionStore.ts`
- Modify: `src-tauri/src/agents/mod.rs`
- Modify: `src-tauri/src/agents/claude_code.rs`
- Modify: `src/hooks/useTauri.ts`
- Test: Verify toolTarget displays correctly

- [ ] **Step 1: Add toolTarget to SessionState type**

In `src/types/agent.ts`, add to SessionState:
```typescript
lastToolTarget?: string // The target of the current tool (e.g., file path)
```

- [ ] **Step 2: Add tool_target to AgentEvent ToolUse variant**

In `src-tauri/src/agents/mod.rs`, modify ToolUse:
```rust
ToolUse {
    session_id: String,
    tool_name: String,
    tool_input: String,
    tool_target: Option<String>, // NEW
    status: String,
},
```

- [ ] **Step 3: Extract tool_target in the adapter (NOT bridge)**

In `src-tauri/src/agents/claude_code.rs`, find where ToolUse events are parsed and add extraction:

```rust
fn extract_tool_target(tool_name: &str, tool_input: &serde_json::Value) -> Option<String> {
    match tool_name {
        "Read" | "Edit" | "Write" | "Glob" | "Grep" => {
            tool_input.get("file_path")
                .or_else(|| tool_input.get("path"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        }
        "Bash" | "shell" => {
            tool_input.get("command")
                .and_then(|v| v.as_str())
                .map(|s| {
                    let cmd = s.trim();
                    if cmd.len() > 50 {
                        format!("{}...", &cmd[..47])
                    } else {
                        cmd.to_string()
                    }
                })
        }
        _ => None
    }
}
```

Then in the parse_event function where ToolUse is created:
```rust
ToolUse {
    session_id: session_id.clone(),
    tool_name: tool_name.clone(),
    tool_input: tool_input.to_string(),
    tool_target: extract_tool_target(&tool_name, &tool_input), // NEW
    status: "running".to_string(),
}
```

- [ ] **Step 4: Update sessionStore to store toolTarget**

In `src/stores/sessionStore.ts`, modify ToolUse handling:
```typescript
case 'tool_use': {
  const session = sessions[event.sessionId]
  if (session) {
    sessions[event.sessionId] = {
      ...session,
      lastToolName: event.toolName,
      lastToolStatus: event.status,
      lastToolTarget: event.toolTarget, // NEW
      phase: 'processing',
    }
  }
  break
}
```

- [ ] **Step 5: Add lastToolTarget mapping in useTauri.ts transformSession**

In `src/hooks/useTauri.ts`, find transformSession and add:
```typescript
lastToolName: bs.lastToolName ?? undefined,
lastToolStatus: (bs.lastToolStatus as ToolStatus) ?? undefined,
lastToolTarget: bs.lastToolTarget ?? undefined, // NEW
```

- [ ] **Step 6: Test toolTarget display**

Run:
1. Start a session that uses Read/Edit tools
2. Verify toolTarget shows file path
3. Test Bash command shows truncated command

- [ ] **Step 7: Commit**

```bash
git add src/types/agent.ts src/stores/sessionStore.ts src-tauri/src/agents/mod.rs src-tauri/src/agents/claude_code.rs src/hooks/useTauri.ts
git commit -m "feat: add toolTarget tracking for better UX"
```

---

## Task 4: Configuration UI for Timing (Optional)

**Files:**
- Modify: `src/components/settings/sections/DisplaySection.tsx`
- Test: Verify settings persist

- [ ] **Step 1: Add timing configuration to DisplaySection**

In `src/components/settings/sections/DisplaySection.tsx`, add:
```typescript
<SettingRow
  label={t('notch.taskCompleteDwell')}
  description={t('notch.taskCompleteDwellDesc')}
>
  <Slider
    value={config.taskCompleteDwellSeconds}
    min={1}
    max={10}
    step={1}
    onChange={(v) => updateConfig({ taskCompleteDwellSeconds: v })}
  />
</SettingRow>
```

- [ ] **Step 2: Add translations**

In `src/i18n/locales/zh.json`:
```json
"taskCompleteDwell": "任务完成后保持展开",
"taskCompleteDwellDesc": "任务完成后，面板保持展开的秒数"
```

In `src/i18n/locales/en.json`:
```json
"taskCompleteDwell": "Keep panel expanded after task",
"taskCompleteDwellDesc": "Seconds to keep panel expanded after task completion"
```

- [ ] **Step 3: Commit**

```bash
git add src/components/settings/sections/DisplaySection.tsx src/i18n/locales/
git commit -m "feat: add timing configuration UI"
```

---

## Summary

This plan implements 3 key experience gaps:

| Gap | Implementation | Files Changed |
|-----|---------------|---------------|
| Task Complete Auto-Collapse | Timer-based collapse with session verification | sessionStore.ts, configStore.ts, types/agent.ts |
| Session State Persistence | JSON file storage in App Support | tauriApi.ts, persistence.rs, lib.rs |
| Tool Target Tracking | Extract in adapter, store in session | agents/mod.rs, agents/claude_code.rs, useTauri.ts |

**Estimated total tasks:** 20-25 steps across 4 tasks