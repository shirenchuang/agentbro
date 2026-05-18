# Vibe Island UI Alignment Design

## Goal

Align AgentBro's notch interaction model with the installed Vibe Island app while preserving AgentBro's deeper capabilities. Vibe's visible model is list-first: the island opens into a compact session list, and permission, question, plan, subagent, task, compacting, error, setup, and terminal-routed states appear as inline session context before the user drills into detail.

## Evidence

The installed Vibe Island app is a native Swift/Mach-O app with no readable frontend source or source maps. The recoverable state model comes from binary strings, localized resources, hook bridge events, and runtime observation. Relevant recovered surfaces include `SessionsListView`, `SessionCardView`, `PermissionApprovalView`, `QuestionApprovalView`, `ExitPlanModeApprovalView`, `TaskListView`, `TerminalApprovalHintView`, `StatusWarningCardView`, and `RestartSessionsBannerView`.

## Approach

Use AgentBro's existing state model and UI components, but change the default hover/list presentation to match Vibe's interaction hierarchy:

- Keep the existing overlay/detail/chat views as deep capability.
- Make the list card the primary surface for all important states.
- Add compact inline rows for terminal-routed approval, setup/trust/restart hints, subagents, tasks, compacting, and completion.
- Keep permission, question, and plan quick actions inline.
- Add dev-lab scenarios for states that Vibe exposes but AgentBro did not previously surface explicitly.

## State Coverage

- Empty/idle: compact empty island and list empty state.
- Session list: dense rows with status glyph, title, preview, agent/terminal/time badges.
- Running tool: short action labels and target.
- Permission: inline tool target, diff summary, deny/allow/always/auto actions; detail card remains.
- Question: inline single, multi-select, and multi-question answering; detail card remains.
- Plan: inline manual/accept-edits/bypass/feedback actions; detail card remains.
- Subagents: compressed child-agent summary in session card, expandable history remains.
- Tasks: compact task rows with done/in-progress/open counts.
- Compacting: explicit compacting and compact-complete rows.
- Error/attention: unified needs-attention row.
- Terminal-routed approval/question: inline "continue in terminal" row with jump action.
- Restart/trust/setup banners: list-level notice rows.
- Completion/response: non-blocking reveal remains available without removing the list-first path.

## Implementation Notes

Most work is in `HoverList` and its CSS. Backend changes should be avoided unless a state cannot be represented with existing `SessionState` fields. Lab-only synthetic data can use existing fields such as `statusLineText`, `description`, `pendingPermission`, `pendingQuestion`, `planTitle`, `subagents`, and `tasks`.

## Verification

- Add or update React tests for list rendering of Vibe-only states.
- Run focused Vitest suites for hover list, subagents, and notch panel.
- Run full `pnpm test:run` if the focused tests pass.
- Use the browser dev lab at `http://127.0.0.1:1423/` for visual QA.
