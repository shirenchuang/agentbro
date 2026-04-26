# Batch 1: State Machine Refactor + Priority System

**Date:** 2026-04-26
**Spec:** `docs/superpowers/specs/2026-04-26-agent-island-completion-design.md`
**Depends on:** Nothing (foundation batch)
**Blocks:** Batch 2, Batch 3

---

## Goal

Replace the ad-hoc 3-state panel (`collapsed | hover | expanded`) and inline priority sorting with the spec's layered state machine (base layer + overlay layer) and 7-level priority model. This is the foundation all other batches build on.

---

## Current State Analysis

### What exists

| Area | Current | File |
|------|---------|------|
| Panel states | `collapsed \| hover \| expanded` — flat, no separation of base vs overlay | `src/types/agent.ts:17` (`PanelState`) |
| Priority | Ad-hoc sort in `getLeadSession()` using hardcoded phase→number map | `src/components/notch/CollapsedBar.tsx:17-29` |
| Overlay cards | Inline in `HoverList.tsx` — approval, plan, response, completion all rendered inside session cards | `src/components/notch/HoverList.tsx:176-229` |
| Session phases | 8 phases: `idle \| processing \| waiting_approval \| waiting_input \| compacting \| done \| error \| interrupted` | `src/types/agent.ts:7-15` |
| Auto-expand | Permission request → force `panelState = 'expanded'` + set `activeSessionId` | `src/stores/sessionStore.ts:156-159` |
| PixelIndicator | Phase-driven color/animation | `src/components/notch/PixelIndicator.tsx` |

### What's missing

- Layered state machine (base layer independent of overlay layer)
- Overlay priority queue (multiple overlays queued, highest shown first)
- 7-level priority model with `computePriority()` function
- Priority-driven compact pill rendering (alert badge, animation intensity)
- `idleSince` tracking in session state

---

## Task Breakdown

### Task 1: Add priority types and `computePriority()`

**Files to create/modify:**
- `src/types/priority.ts` (new)
- `src/types/agent.ts` (modify)

**`src/types/priority.ts`** — new file:

```typescript
export const PRIORITY = {
  dormant: 0,
  idle: 1,
  done: 2,
  thinking: 3,
  working: 4,
  compacting: 5,
  attention: 6,
} as const

export type Priority = typeof PRIORITY[keyof typeof PRIORITY]
export type PriorityName = keyof typeof PRIORITY

export function computePriority(session: {
  phase: string
  lastToolName?: string
  idleSince?: number
  startedAt: number
}): Priority {
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

export function priorityName(p: Priority): PriorityName {
  const entries = Object.entries(PRIORITY) as [PriorityName, Priority][]
  return entries.find(([, v]) => v === p)?.[0] ?? 'idle'
}
```

**`src/types/agent.ts`** — add `idleSince` to `SessionState`:

```typescript
// Add to SessionState interface (after line 70, startedAt):
idleSince?: number
```

**Verification:** `npm run build` should pass with no type errors.

---

### Task 2: Refactor `PanelState` to layered base + overlay model

**Files to modify:**
- `src/types/agent.ts` — replace `PanelState`, add overlay types
- `src/stores/sessionStore.ts` — add overlay queue state and actions

**`src/types/agent.ts`** — replace PanelState and add overlay types:

```typescript
// Replace line 17:
// OLD: export type PanelState = 'collapsed' | 'hover' | 'expanded'
// NEW:
export type BaseLayer = 'compact' | 'expanded' | 'detail'
export type DisplayLevel = 'dormant' | 'compact' | 'visible'

export type OverlayType = 'permission' | 'question' | 'plan' | 'completion' | 'response'

export const OVERLAY_PRIORITY: Record<OverlayType, number> = {
  permission: 100,
  plan: 90,
  question: 80,
  completion: 20,
  response: 10,
}

export interface OverlayItem {
  id: string
  sessionId: string
  type: OverlayType
  data: unknown
  createdAt: number
}

// Keep old PanelState as alias during migration
export type PanelState = 'collapsed' | 'hover' | 'expanded'
```

**`src/stores/sessionStore.ts`** — add overlay queue alongside existing state:

Add new state fields to `SessionStore` interface:

```typescript
// New fields (add after rateLimits):
baseLayer: BaseLayer
overlayQueue: OverlayItem[]
activeOverlay: OverlayItem | null

// New actions (add after setRateLimits):
setBaseLayer: (layer: BaseLayer) => void
pushOverlay: (item: OverlayItem) => void
dismissOverlay: (id: string) => void
clearSessionOverlays: (sessionId: string) => void
```

Implement `pushOverlay`:
- Insert into `overlayQueue` sorted by `OVERLAY_PRIORITY[item.type]` descending
- Set `activeOverlay` to highest priority item

Implement `dismissOverlay`:
- Remove from queue by id
- Auto-advance: set `activeOverlay` to next highest, or `null` if queue empty

Implement `clearSessionOverlays`:
- Remove all overlays for a given sessionId (called on session_end)

**Migration strategy:** Keep `panelState` and `setPanelState` working during this batch. Map:
- `collapsed` → `baseLayer: 'compact'`
- `hover` → `baseLayer: 'expanded'`
- `expanded` → `baseLayer: 'detail'`

Add a computed getter `panelState` that derives from `baseLayer` so existing components don't break:

```typescript
// Compatibility getter
get panelState(): PanelState {
  switch (this.baseLayer) {
    case 'compact': return 'collapsed'
    case 'expanded': return 'hover'
    case 'detail': return 'expanded'
  }
}
```

**Verification:** All existing UI should work unchanged. `npm run build` passes. Manually verify collapsed → hover → expanded transitions still work in dev server.

---

### Task 3: Wire overlay queue to event handlers

**Files to modify:**
- `src/stores/sessionStore.ts` — update `updateSession` cases

**Changes to `updateSession`:**

`permission_request` case:
- Instead of directly setting `phase: 'waiting_approval'` and forcing `panelState = 'expanded'`, push an overlay:
  ```typescript
  const overlayId = `perm-${event.sessionId}-${Date.now()}`
  // still set session phase
  sessions[event.sessionId] = { ...session, phase: 'waiting_approval', ... }
  // push overlay instead of forcing panel state
  // (will be done via pushOverlay after set())
  ```
- After `set()`, call `pushOverlay({ id: overlayId, sessionId, type: 'permission', data: { toolName, toolInput, diff, options }, createdAt: Date.now() })`

`ask_question` case:
- Same pattern: set phase + push overlay of type `'question'`

`task_complete` case:
- Push overlay of type `'completion'` with dwell timer
- Start auto-dismiss timer: `setTimeout(() => dismissOverlay(id), dwellSeconds * 1000)`

**For plan events** (currently detected via `session.planContent`):
- When session gets `planContent` set, push overlay of type `'plan'`

**For response events** (currently `session.responseText`):
- When session completes with `responseText`, push overlay of type `'response'`

`session_end` case:
- Call `clearSessionOverlays(event.sessionId)` to clean up

**Verification:** Open dev server, verify that permission requests still show approval UI. Check that completing a session shows completion card then auto-dismisses.

---

### Task 4: Migrate `NotchPanel` to use base layer + overlay

**Files to modify:**
- `src/components/notch/NotchPanel.tsx`

**Changes:**

1. Replace `panelState` reads with `baseLayer` reads:
   ```typescript
   const baseLayer = useSessionStore((s) => s.baseLayer)
   const activeOverlay = useSessionStore((s) => s.activeOverlay)
   const setBaseLayer = useSessionStore((s) => s.setBaseLayer)
   ```

2. Update mouse handlers:
   - `handleMouseEnter`: `compact` → `expanded`
   - `handleMouseLeave`: `expanded` → `compact` (with dwell delay)

3. Update `AnimatePresence` children:
   - `baseLayer === 'expanded'` → show `HoverList`
   - `baseLayer === 'detail'` → show `ChatView`
   - When `activeOverlay` is non-null → render overlay card on top (absolute positioned above base layer content)

4. Update sizing logic:
   - `compact`: pill width
   - `expanded`: session list width
   - `detail`: full chat width
   - When overlay active: expand height to fit overlay card

5. Keep the compatibility `setPanelState` calls working via the mapping layer from Task 2.

**Verification:** All 3 base states work. Overlay appears on top when permission request comes in. Dismissing overlay reveals base layer underneath.

---

### Task 5: Migrate `CollapsedBar` to priority-driven rendering

**Files to modify:**
- `src/components/notch/CollapsedBar.tsx`
- `src/components/notch/PixelIndicator.tsx`

**CollapsedBar changes:**

1. Replace `getLeadSession()` with priority-based lead:
   ```typescript
   import { computePriority, PRIORITY, priorityName } from '../../types/priority'
   
   function getLeadSession(sessions: SessionState[]): SessionState | undefined {
     return [...sessions].sort((a, b) => computePriority(b) - computePriority(a))[0]
   }
   ```

2. Add alert badge count:
   ```typescript
   const alertCount = sessions.filter(s => computePriority(s) === PRIORITY.attention).length
   ```
   Render as badge next to session count when `alertCount > 0`.

3. Pass priority to PixelIndicator instead of phase:
   ```typescript
   <PixelIndicator priority={computePriority(lead)} size={14} />
   ```

**PixelIndicator changes:**

1. Change prop from `phase: SessionPhase` to `priority: Priority`
2. Map priority to color and animation speed:
   ```typescript
   const PRIORITY_COLORS: Record<Priority, string> = {
     0: '#666',    // dormant — gray
     1: '#888',    // idle — light gray
     2: '#4CAF50', // done — green
     3: '#2196F3', // thinking — blue
     4: '#FF9800', // working — orange
     5: '#9C27B0', // compacting — purple
     6: '#FF5252', // attention — red
   }
   const PRIORITY_SPEEDS: Record<Priority, number> = {
     0: 0, 1: 2000, 2: 1500, 3: 800, 4: 600, 5: 500, 6: 300,
   }
   ```
3. For attention priority, increase animation intensity over time (faster pulse)

**Verification:** Compact pill shows correct color/animation for each session phase. Alert badge shows count of sessions needing attention.

---

### Task 6: Sort sessions by priority in `HoverList`

**Files to modify:**
- `src/components/notch/HoverList.tsx`

**Changes:**

1. Sort sessions by priority descending before rendering:
   ```typescript
   import { computePriority } from '../../types/priority'
   
   const sortedSessions = useMemo(
     () => [...sessions].sort((a, b) => computePriority(b) - computePriority(a)),
     [sessions]
   )
   ```

2. Use `sortedSessions` in the render loop instead of `sessions`.

3. Remove inline overlay card rendering (approval, plan, response, completion) from individual session cards — these are now handled by the overlay layer in NotchPanel. Keep only the session card info (project, title, tool, tasks).

**Verification:** Sessions in expanded list are sorted by priority. Highest priority session at top.

---

### Task 7: Add `idleSince` tracking

**Files to modify:**
- `src/stores/sessionStore.ts`

**Changes:**

In `updateSession`, when a session transitions to `idle` phase, record `idleSince`:

```typescript
// In session_end case: already handled (session removed)

// Add to processing case: clear idleSince when session becomes active
sessions[event.sessionId] = { ...session, phase: 'processing', idleSince: undefined, ... }

// Add to task_complete case: set idleSince
sessions[event.sessionId] = { ...session, phase: 'done', idleSince: Date.now(), ... }
```

When `computePriority` reads `idleSince`, sessions idle >10 min get `PRIORITY.idle` (can be hidden), while recently-done sessions stay at `PRIORITY.done`.

**Verification:** A session that finishes and sits for >10 minutes should drop to idle priority in the sort order.

---

### Task 8: Update keyboard shortcuts for layered model

**Files to modify:**
- `src/components/notch/NotchPanel.tsx` — ESC handler

**Implement progressive ESC:**

```typescript
// In keyboard handler:
if (e.key === 'Escape') {
  const overlay = useSessionStore.getState().activeOverlay
  const base = useSessionStore.getState().baseLayer
  
  if (overlay) {
    // Close current overlay (non-blocking: dismiss; blocking: keep pending but hide)
    if (['completion', 'response'].includes(overlay.type)) {
      useSessionStore.getState().dismissOverlay(overlay.id)
    } else {
      // Blocking overlay: fold to compact but keep in queue
      setBaseLayer('compact')
    }
  } else if (base === 'detail') {
    setBaseLayer('expanded')
  } else if (base === 'expanded') {
    setBaseLayer('compact')
  }
  // compact + ESC → dormant will be in Batch 3 (display level)
  return
}
```

**Verification:** ESC from overlay → closes/hides overlay. ESC from detail → expanded. ESC from expanded → compact.

---

## File Change Summary

| File | Action | Description |
|------|--------|-------------|
| `src/types/priority.ts` | **Create** | Priority constants, `computePriority()`, `priorityName()` |
| `src/types/agent.ts` | **Modify** | Add `BaseLayer`, `DisplayLevel`, `OverlayType`, `OverlayItem`, `idleSince`; keep `PanelState` as compat alias |
| `src/stores/sessionStore.ts` | **Modify** | Add overlay queue state/actions, wire events to push overlays, add `baseLayer` |
| `src/components/notch/NotchPanel.tsx` | **Modify** | Use `baseLayer` + `activeOverlay`, render overlay layer, progressive ESC |
| `src/components/notch/CollapsedBar.tsx` | **Modify** | Priority-based lead session, alert badge, pass priority to PixelIndicator |
| `src/components/notch/PixelIndicator.tsx` | **Modify** | Accept `priority` prop instead of `phase`, priority-driven colors/speeds |
| `src/components/notch/HoverList.tsx` | **Modify** | Sort by priority, remove inline overlay cards |

---

## Implementation Order

```
Task 1 (types) → Task 2 (store refactor) → Task 3 (event wiring) → Task 7 (idleSince)
                                                     ↓
                                          Task 4 (NotchPanel) → Task 8 (ESC)
                                                     ↓
                                    Task 5 (CollapsedBar + PixelIndicator)
                                                     ↓
                                          Task 6 (HoverList sort)
```

Tasks 1-3 + 7 are store/type changes (no visible UI change).
Tasks 4-6 + 8 are UI migration (visible changes, verify in browser).

---

## Verification Checklist

- [ ] `npm run build` passes with zero errors
- [ ] Dev server: compact pill shows correct priority color and alert badge
- [ ] Dev server: hover → session list sorted by priority (attention sessions first)
- [ ] Dev server: permission request → overlay appears on top of base layer
- [ ] Dev server: dismiss overlay → base layer visible again, queue advances
- [ ] Dev server: ESC progressive exit works (overlay → detail → expanded → compact)
- [ ] Dev server: multiple sessions with different phases render correct priority ordering
- [ ] No regressions: existing approval flow (allow/deny/always) still works
- [ ] No regressions: chat view still accessible via session click
- [ ] No regressions: settings, sound, jump-to-terminal still work
