# Batch 2: Interaction Cards (Overlay Content)

**Date:** 2026-04-26
**Spec:** `docs/superpowers/specs/2026-04-26-agent-island-completion-design.md` — Section 3
**Depends on:** Batch 1 (overlay queue must exist)

---

## Goal

Build the 5 overlay card components that render inside the overlay layer established in Batch 1. Each card type corresponds to an `OverlayType`: permission, plan, question, completion, response.

---

## Current State Analysis

| Card | Exists? | Current Location | Gap |
|------|---------|-----------------|-----|
| Permission | Yes — `ApprovalBar.tsx` | Inline in HoverList + ChatView | Missing session context header, keyboard hint, "Always Allow" leveraging `permissionSuggestions` |
| Plan | Partial — `PlanCard.tsx` | Inline in HoverList session card | Display-only, no approval buttons, no feedback input |
| Question | Partial — inside `ApprovalBar.tsx` | Combined with permission in one component | No multi-question support, no multi-select, no option descriptions |
| Completion | Yes — `CompletionCard.tsx` | Standalone but triggered from HoverList | Needs integration with overlay queue dwell timer |
| Response | Yes — `ResponseCard.tsx` | Inline in HoverList session card | Needs click-to-jump, dwell timer, hover-pause |

---

## Task Breakdown

### Task 1: Create `OverlayCard` container component

**Files to create:**
- `src/components/overlay/OverlayCard.tsx` (new)
- `src/components/overlay/OverlayCard.css` (new)
- `src/components/overlay/SessionContextHeader.tsx` (new)

**`OverlayCard.tsx`** — generic wrapper for all overlay cards:

```tsx
interface OverlayCardProps {
  sessionId: string
  children: React.ReactNode
  onDismiss?: () => void
}
```

- Renders a glass-morphism card with rounded corners, blur backdrop
- Positioned absolute within NotchPanel's overlay layer
- AnimatePresence enter/exit animations (slide up + fade)
- Renders `SessionContextHeader` at top showing: project name, agent type, last user message

**`SessionContextHeader.tsx`**:

```tsx
interface SessionContextHeaderProps {
  session: SessionState
}
```

- Compact one-line header: `[PixelIndicator] project-name · Agent Type`
- Second line (truncated): `You: {lastUserMessage}`
- Shared by all 5 card types for consistent context

**Verification:** Import into NotchPanel's overlay render section, confirm it renders with correct session context.

---

### Task 2: Rebuild Permission Card as overlay

**Files to create/modify:**
- `src/components/overlay/PermissionCard.tsx` (new)
- `src/components/overlay/PermissionCard.css` (new)
- `src/components/notch/ApprovalBar.tsx` (modify — remove permission logic, keep question + text input)

**`PermissionCard.tsx`** — matches spec Section 3.1:

```tsx
interface PermissionCardProps {
  overlay: OverlayItem  //  { toolName, toolInput, diff, options }
  session: SessionState
  onAllow: () => void
  onAllowAlways: () => void
  onDeny: () => void
}
```

Layout:
```
+-- SessionContextHeader -----+
+-- Tool Card ----------------+
| ⚠ toolName                  |
| toolInput (code block)       |
| [DiffView if diff present]  |
+-- Buttons ------------------+
| [Deny]  [Allow]  [Always]   |
+-- Keyboard Hints -----------+
| ⌘Y Allow  ⌘N Deny           |
+-----------------------------+
```

Enhancements over current `ApprovalBar`:
1. Session context header (which session is this for?)
2. DiffView integration — if `overlay.data.diff` exists, render inline diff preview (reuse existing `DiffView.tsx`)
3. Keyboard shortcut hints displayed at bottom (always visible, not just on modifier hold — simpler than spec, revisit if too noisy)
4. "Always Allow" button with tooltip explaining it adds permanent permission

**Wire up in NotchPanel:**
```tsx
if (activeOverlay?.type === 'permission') {
  return <PermissionCard overlay={activeOverlay} session={session} ... />
}
```

**Verification:** Dev server — trigger a permission request (mock data in App.tsx has one). Card should appear as overlay with context header, tool info, diff, and 3 buttons. Allow/Deny should dismiss overlay and advance queue.

---

### Task 3: Build Plan Approval Card

**Files to create:**
- `src/components/overlay/PlanCard.tsx` (new — different from existing `src/components/notch/PlanCard.tsx` which is display-only)
- `src/components/overlay/PlanCard.css` (new)

**`PlanCard.tsx`** — matches spec Section 3.2:

```tsx
interface PlanCardProps {
  overlay: OverlayItem  // data: { planTitle, planContent, requestedPermissions }
  session: SessionState
  onSendFeedback: (feedback: string) => void
  onAcceptEdits: () => void
  onAutoApprove: () => void
}
```

Layout:
```
+-- SessionContextHeader ----------+
+-- Plan Content ------------------+
| planTitle                  [tag] |
|                                  |
| (markdown rendered via           |
|  react-markdown + remark-gfm,   |
|  max-height scrollable)          |
|                                  |
| Requested permissions:           |
|  · Bash: run tests               |
|  · Bash: install dependencies    |
+-- Feedback Input ----------------+
| [Tell Claude what to change...  ]|
+-- Actions -----------------------+
| [Feedback] [Accept Edits] [Auto] |
+---------------------------------+
```

Key behaviors:
- Markdown content rendered with `react-markdown` + `remark-gfm` (both already in package.json)
- Scrollable content area with max-height (configurable, default 300px)
- Feedback input: when text is entered, first button changes from "Manual Review" to "Send Feedback"
- "Accept Edits" → respond with `acceptEdits` permission mode
- "Auto" → respond with `bypassPermissions` mode (red-tinted button as warning)

**Data source:** Plan overlay is pushed when session receives `planContent` + `planTitle` via hook events. Need to add a new `AgentEvent` type:

```typescript
// Add to AgentEvent union in src/types/agent.ts:
| { type: 'plan_request'; sessionId: string; planTitle: string; planContent: string; requestedPermissions?: string[] }
```

Wire the event in `sessionStore.updateSession()` to push an overlay of type `'plan'`.

**Verification:** Add mock plan data to App.tsx. Verify plan card renders markdown, scrolls, feedback input works, all 3 buttons respond correctly.

---

### Task 4: Rebuild Question Card as overlay

**Files to create:**
- `src/components/overlay/QuestionCard.tsx` (new)
- `src/components/overlay/QuestionCard.css` (new)

**`QuestionCard.tsx`** — matches spec Section 3.3:

```tsx
interface QuestionCardProps {
  overlay: OverlayItem  // data: AskQuestion (enhanced)
  session: SessionState
  onAnswer: (answers: Record<string, string>) => void
}
```

**Enhanced `AskQuestion` type** (modify `src/types/agent.ts`):

```typescript
// Replace existing AskQuestion:
export interface AskQuestionOption {
  label: string
  description?: string
}

export interface AskQuestionItem {
  question: string
  header?: string
  options: AskQuestionOption[]
  multiSelect?: boolean
}

export interface AskQuestion {
  questions: AskQuestionItem[]  // supports multi-question
}
```

**Single question mode:**
- Radio-style options with label + description
- Click option → send immediately (single-select)
- Custom text input expandable at bottom

**Multi-select mode** (`multiSelect: true`):
- Checkbox-style options
- "Confirm Selection (N)" button at bottom

**Multi-question mode** (questions.length > 1):
- Stack questions vertically
- Each question independent (single or multi-select)
- Unified "Submit All" button at bottom

**Keyboard shortcuts:**
- `⌘1`/`⌘2`/`⌘3` select first 3 options (existing behavior, just move into this component)

**Verification:** Update mock data to include multi-option questions. Verify single-select sends immediately, multi-select shows confirm button, custom text input works.

---

### Task 5: Upgrade Response Card as overlay

**Files to modify:**
- `src/components/overlay/ResponseCard.tsx` (new overlay version)
- `src/components/overlay/ResponseCard.css` (new)

**`ResponseCard.tsx`** — matches spec Section 3.4:

```tsx
interface ResponseCardProps {
  overlay: OverlayItem  // data: { userMessage, responseText, sessionId }
  session: SessionState
  onJumpToTerminal: () => void
  onDismiss: () => void
}
```

Layout:
```
+-- SessionContextHeader -----------+
| You: {lastUserMessage}            |
| --------------------------  [Done]|
|                                   |
| {responseText}                    |
| (markdown rendered, configurable  |
|  max-height)                      |
|                                   |
|       Click to jump to terminal → |
+----------------------------------+
```

Key behaviors:
- Auto-dismiss after dwell timer (configurable, default 5s via `configStore.taskCompleteDwellSeconds`)
- Hover pauses dwell timer
- Click "jump to terminal" or the card body → `jumpToTerminal(sessionId)` + dismiss
- Non-blocking: lower overlay priority, won't interrupt permission/question/plan

**Dwell timer implementation:**
```typescript
const [remaining, setRemaining] = useState(dwellMs)
const paused = useRef(false)

useEffect(() => {
  const interval = setInterval(() => {
    if (!paused.current) {
      setRemaining(r => {
        if (r <= 100) { onDismiss(); return 0 }
        return r - 100
      })
    }
  }, 100)
  return () => clearInterval(interval)
}, [dwellMs, onDismiss])
```

Visual: thin progress bar at bottom showing remaining time.

**Verification:** Complete a mock session → response card appears, auto-dismisses after dwell. Hover pauses countdown. Click jumps to terminal.

---

### Task 6: Upgrade Completion Card as overlay

**Files to modify:**
- `src/components/overlay/CompletionCard.tsx` (new overlay version)
- `src/components/overlay/CompletionCard.css` (new)

Simpler than ResponseCard — just a notification banner:

```tsx
interface CompletionCardProps {
  overlay: OverlayItem  // data: { summary }
  session: SessionState
  onDismiss: () => void
}
```

Layout:
```
+-- Task Complete ---------+
| ✓ {summary}              |
| [progress bar countdown] |
+-------------------------+
```

Same dwell timer pattern as ResponseCard. Lighter weight — no markdown, no jump button.

**Verification:** Session completion → card appears, auto-dismisses.

---

### Task 7: Wire overlay rendering in NotchPanel

**Files to modify:**
- `src/components/notch/NotchPanel.tsx`

Add overlay render switch:

```tsx
function OverlayRenderer({ overlay }: { overlay: OverlayItem }) {
  const session = useSessionStore((s) => s.sessions[overlay.sessionId])
  if (!session) return null

  switch (overlay.type) {
    case 'permission':
      return <PermissionCard overlay={overlay} session={session} ... />
    case 'plan':
      return <PlanCard overlay={overlay} session={session} ... />
    case 'question':
      return <QuestionCard overlay={overlay} session={session} ... />
    case 'response':
      return <ResponseCard overlay={overlay} session={session} ... />
    case 'completion':
      return <CompletionCard overlay={overlay} session={session} ... />
  }
}
```

Render in NotchPanel after base layer content:

```tsx
{activeOverlay && (
  <motion.div className="notch-panel__overlay" ...>
    <OverlayRenderer overlay={activeOverlay} />
  </motion.div>
)}
```

When overlay is active, expand panel height to accommodate card.

**Verification:** Full flow — permission request shows permission card, dismiss it, question appears, answer it, response card shows, auto-dismisses.

---

### Task 8: Clean up old inline cards from HoverList

**Files to modify:**
- `src/components/notch/HoverList.tsx` — remove inline ApprovalBar, PlanCard, ResponseCard, CompletionCard renders
- `src/components/notch/ApprovalBar.tsx` — simplify to only handle text input (for ChatView's message input)

After Batch 2, HoverList session cards only show:
- Row 1: pixel + project + title + badges + duration + jump
- Row 2: last user message
- Row 3: last tool call
- Row 4: task summary

All overlay content is handled by the overlay layer.

**Verification:** HoverList is cleaner. No duplicate card rendering. All interactive cards come from overlay layer.

---

## File Change Summary

| File | Action | Description |
|------|--------|-------------|
| `src/components/overlay/OverlayCard.tsx` | **Create** | Generic overlay wrapper with animation |
| `src/components/overlay/OverlayCard.css` | **Create** | Glass-morphism overlay styles |
| `src/components/overlay/SessionContextHeader.tsx` | **Create** | Shared session context header |
| `src/components/overlay/PermissionCard.tsx` | **Create** | Full permission card with diff, shortcuts |
| `src/components/overlay/PermissionCard.css` | **Create** | Permission card styles |
| `src/components/overlay/PlanCard.tsx` | **Create** | Plan approval with markdown, feedback |
| `src/components/overlay/PlanCard.css` | **Create** | Plan card styles |
| `src/components/overlay/QuestionCard.tsx` | **Create** | Single/multi question with options |
| `src/components/overlay/QuestionCard.css` | **Create** | Question card styles |
| `src/components/overlay/ResponseCard.tsx` | **Create** | Response preview with dwell timer |
| `src/components/overlay/ResponseCard.css` | **Create** | Response card styles |
| `src/components/overlay/CompletionCard.tsx` | **Create** | Completion notification with countdown |
| `src/components/overlay/CompletionCard.css` | **Create** | Completion card styles |
| `src/types/agent.ts` | **Modify** | Add `plan_request` event, enhance `AskQuestion` |
| `src/stores/sessionStore.ts` | **Modify** | Handle `plan_request` event → push overlay |
| `src/components/notch/NotchPanel.tsx` | **Modify** | Add `OverlayRenderer` switch |
| `src/components/notch/HoverList.tsx` | **Modify** | Remove inline card rendering |
| `src/components/notch/ApprovalBar.tsx` | **Modify** | Simplify to text input only |

---

## Implementation Order

```
Task 1 (OverlayCard + SessionContextHeader)
    ↓
Task 2 (PermissionCard) ──→ Task 7 (wire in NotchPanel) ──→ Task 8 (cleanup HoverList)
Task 3 (PlanCard)      ──↗
Task 4 (QuestionCard)  ──↗
Task 5 (ResponseCard)  ──↗
Task 6 (CompletionCard)──↗
```

Tasks 2-6 are independent of each other. Task 7 integrates them. Task 8 cleans up.

---

## Verification Checklist

- [ ] `npm run build` passes
- [ ] Permission card: renders with session context, tool info, diff, 3 buttons work
- [ ] Plan card: markdown renders, scrollable, feedback input toggles button label, all 3 modes work
- [ ] Question card: single-select sends immediately, multi-select shows confirm, custom input works
- [ ] Response card: dwell timer countdown, hover pauses, click jumps to terminal
- [ ] Completion card: appears on session done, auto-dismisses
- [ ] Overlay queue: multiple overlays → highest priority shown, dismissing advances
- [ ] HoverList: no duplicate cards, clean session info only
- [ ] No regressions: chat view, settings, keyboard shortcuts all work
