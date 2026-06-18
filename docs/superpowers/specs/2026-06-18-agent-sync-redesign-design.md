# Agent Sync Redesign Design

Status: Ready for review
Date: 2026-06-18
Scope: `src/components/skills-v2/InstallPage.tsx`, `src/components/skills-v2/InstallView.tsx`, and related Skill Manager v2 styles/tests

## Context

The current `Agent 同步` tab inside `安装 Skills` exposes too many controls at once: status tabs, search, Agent filtering, list/card switching, statistics, explanatory copy, batch actions, and a large grid that includes managed skills. Users see many possible actions but no clear first step.

The product intent for this tab should be narrower: help users synchronize skills that already exist in their local Agent directories into AgentBro's center library. The page should guide users toward organizing unmanaged skills, not behave like another full skill browser.

## Design Goal

Make `Agent 同步` feel like a local sync task, with one obvious path:

1. Scan local Agents.
2. Review the unmanaged/conflict skills that need attention.
3. Organize importable skills into the center library.
4. Handle conflicts separately.

The default page should answer three questions:

- What did AgentBro find?
- What should I do next?
- Which skills still need my decision?

## Non-Goals

- Do not redesign official source, local import, or Git install tabs.
- Do not change the backend adopt/link/copy semantics.
- Do not remove list/card view support.
- Do not expose all managed skills by default.
- Do not add new dependencies.

## Proposed Layout

### 1. Task Header

Replace the current generic market-like toolbar with a task-focused header.

Content:

- Title: `本机 Agent 同步`
- One-line purpose: `把散落在各 Agent 里的 Skills 收进中心库`
- Secondary note: `优先处理未管理和冲突项；已管理 Skills 默认隐藏。`
- Secondary action: `重新扫描`

The scan action remains visible, but it is not the primary call to action.

### 2. Sync Summary Panel

Add a prominent summary panel directly below the header. This is the page's visual anchor.

Content:

- Main summary, for example: `发现 18 个可接管 Skill，2 个同名冲突`
- Recommended next step: `建议先一键整理可接管项，再逐个处理冲突。`
- Small status chips:
  - Agent count
  - Managed count, explicitly marked as hidden
  - Recommended mode, such as soft link
- Primary action: `一键整理 N 个`
- Secondary action: `查看整理方式`

Behavior:

- If there are importable skills, the primary action opens the existing one-click organize dialog.
- If there are no importable skills but conflicts exist, the panel directs users to conflict handling.
- If everything is healthy, the panel shows a done state and keeps scan available.

### 3. Agent Summary Strip

Show each installed Agent as a compact summary card. These cards are status summaries, not full management surfaces.

Each card shows:

- Agent display name and icon
- Importable count
- Conflict count when present
- Managed count as subdued context
- Health tone:
  - Needs attention
  - Conflict
  - Healthy

Clicking an Agent filters the pending inbox to that Agent. It should not navigate away or open a complex detail panel by default.

### 4. Pending Inbox

Rename the main list area to `待处理收纳箱`.

Default content:

- Only skills that need user attention:
  - Importable unmanaged skills
  - Unmanaged non-importable skills
  - Conflicts
- Managed skills are hidden by default.

Controls:

- Search pending skills
- Agent filter through the Agent summary cards and a compact select for narrow screens
- View toggle: list and card
- Batch actions:
  - Select visible importable skills
  - Clear selection
  - Adopt selected

The inbox is the only large repeated-content area on the default page.

### 5. List And Card Views

Both list and card views remain supported.

Default view:

- Prefer list view for the pending inbox because it is denser and better for cleanup workflows.
- Preserve the user's existing view preference during the session when possible.

List view requirements:

- One row per pending skill.
- Show checkbox, Agent icon/name, skill name, status, mode if applicable, and truncated path.
- Row-level action:
  - `接管到中心库` for importable skills
  - `处理冲突` for conflicts

Card view requirements:

- Use the same pending dataset as list view.
- Keep cards compact and cleanup-oriented.
- Show checkbox, Agent icon/name, skill name, status, mode if applicable, path, and primary row action.

Both views must:

- Use the same selection state.
- Use the same search/filter results.
- Open the existing skill detail slide-over on item click.
- Never default to showing the full managed inventory.

### 6. Advanced View

Move the current broad controls into a lower-priority `高级查看` area.

Advanced controls include:

- Full status tabs: all, importable, unmanaged, managed, conflict
- Show managed skills
- Card/list view toggle if not already present near the inbox
- Full Agent select
- Path/status troubleshooting filters

The advanced area can be a disclosure panel. It should be closed by default.

When `显示已管理` is enabled, the page may show managed skills, but the summary panel should still keep the user's attention on pending work.

## Empty And Loading States

Loading:

- Show `正在扫描本机 Agent Skills...`
- Keep the page shell stable so controls do not jump.

No Agents:

- Show a clear empty state explaining that no installed Agent skill directories were found.
- Primary action: `重新扫描`

Nothing to organize:

- Summary panel shows a healthy state.
- Pending inbox shows: `没有需要接管的 Skill。`
- Managed count remains available as subdued context.

Only conflicts:

- Summary panel says conflicts need review.
- Primary action changes to `处理冲突`.
- One-click organize is not offered unless importable skills exist.

## Data Flow

The redesign can reuse the existing data and commands:

- `skillApiV2.listAgentSkillInventory()`
- `skillApiV2.refresh()`
- `skillApiV2.previewAdopt(agentId, itemId)`
- `skillApiV2.executeAdopt(agentId, itemId, mode, conflictResolution?)`

Derived datasets:

- `pendingRows`: non-managed rows and conflict rows.
- `importableRows`: pending rows where `canImport` is true.
- `conflictRows`: pending rows where `status === 'conflict'`.
- `managedRows`: managed rows, only shown when advanced managed view is enabled.

The current `rows` concept should become a view result derived from:

- Active Agent filter
- Search query
- Pending-only default
- Advanced status filter
- Managed visibility setting

## Component Shape

Keep the implementation scoped inside the existing Agent sync panel unless extraction clearly reduces complexity.

Suggested internal sections:

- `AgentSyncTaskHeader`
- `AgentSyncSummaryPanel`
- `AgentSyncAgentStrip`
- `AgentSyncPendingInbox`
- `AgentSyncAdvancedControls`

These may be private functions in `InstallView.tsx` at first. Extract to separate files only if the file becomes harder to reason about during implementation.

## Accessibility

- The primary sync action must have a stable text label with the count.
- The advanced disclosure must use a real button with `aria-expanded`.
- List/card toggle must expose the selected view with `aria-pressed` or equivalent.
- Agent cards must be keyboard reachable when they act as filters.
- Loading, import progress, notice, and error states should keep `role="status"` or existing live-region behavior.

## Visual Direction

Keep the page quiet and operational. This is a maintenance workflow, not a marketplace.

Design choices:

- Fewer top-level controls.
- Stronger hierarchy around the recommended action.
- Compact Agent status cards.
- Dense list rows for cleanup.
- Cards remain available but should not dominate the default experience.
- Avoid large decorative elements and marketing copy.

## Acceptance Criteria

- The default `Agent 同步` view shows a task header, summary panel, Agent summary strip, and pending inbox.
- The default repeated list does not include managed skills.
- Users can still switch between list and card views.
- List and card views operate on the same filtered pending dataset.
- Users can still select visible importable skills and adopt selected skills.
- Users can still open the one-click organize dialog.
- Users can still handle conflicts through the existing adopt preview flow.
- Users can still rescan Agents.
- Managed skills can be reached through advanced viewing, but are not shown by default.
- Existing detail slide-over behavior remains available from both list and card items.
- Tests cover default pending-only behavior, managed visibility through advanced controls, and list/card parity.

## Implementation Notes

- Start by separating derived datasets before changing JSX layout.
- Keep existing adopt and one-click flows intact.
- Prefer renaming UI labels and reorganizing controls over changing backend behavior.
- Update tests in `src/test/skillManagerV2View.test.tsx` to reflect the new default hierarchy.
- No i18n expansion is required unless the current feature strings are already localized for this view; if any translation key is introduced, all five locale files must be updated together.
