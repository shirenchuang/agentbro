# Vibe Island UI State Reconstruction

This is a black-box/state-model reconstruction from the installed macOS app, not a source-code extraction.

## Evidence Sources

- Installed app: `/Applications/Vibe Island.app`, bundle id `app.vibeisland.macos`, version `1.0.33`.
- App type: native Swift/Mach-O binary, not Electron/Web. No readable frontend source, CSS, JS bundle, or source maps were present in `Contents/Resources`.
- Runtime entry points:
  - Unix socket: `~/.vibe-island/run/vibe-island.sock`
  - Local TCP listener: `127.0.0.1:17891`
  - Bridge CLI: `~/.vibe-island/bin/vibe-island-bridge --source <claude|codex|gemini|...> [--event <eventName>]`
- Static resources:
  - `Contents/Resources/*/Localizable.strings`
  - Swift class/view names recovered from binary strings.
  - Hook event names recovered from binary strings and hook binaries.

## Recovered View Surface

The installed app exposes these relevant SwiftUI/AppKit view and model names:

- `NotchPanel`
- `NotchContentView`
- `NotchViewModel`
- `SessionsListView`
- `SessionCardView`
- `StatusWarningCardView`
- `PermissionApprovalView`
- `ApprovalDetailView`
- `ApprovalButton`
- `TerminalApprovalHintView`
- `QuestionApprovalView`
- `SingleQuestionView`
- `MultiQuestionView`
- `QuestionIndicatorView`
- `CopilotQuestionReadOnlyView`
- `ExitPlanModeApprovalView`
- `CompactOptionButton`
- `TaskListView`
- `TaskRowView`
- `RestartSessionsBannerView`
- `ShowAllSessionsButton`
- `NotchPreviewCard`

This suggests Vibe Island's UI is primarily a notch shell plus a session-list surface, with specialized approval/question/plan/task subviews.

## Recovered Hook Events

Events visible in the binary/hook layer:

- `SessionStart`
- `UserPromptSubmit`
- `PreToolUse`
- `PostToolUse`
- `PostToolUseFailure`
- `PermissionRequest`
- `AskUserQuestion`
- `ExitPlanMode`
- `PreCompact`
- `Stop`
- `StopFailure`
- `SessionEnd`
- `Notification`

## Recovered Session Statuses

Status strings and internal tokens indicate these state groups:

- Empty/awaiting session: `session.emptyFirstInstall`, `session.waitingForSession`
- Ready/idle: `session.ready`, `status.thinking`, `status.processing`
- Running tool: `status.runningTool`, tool labels such as reading/editing/writing/running/searching/fetching/tasking
- Waiting approval: `status.waitingForApproval`, `PermissionApprovalView`
- Waiting question: `status.question`, `QuestionApprovalView`, `SingleQuestionView`, `MultiQuestionView`
- Plan approval: `ExitPlanModeApprovalView`
- Compacting: `status.compacting`, `session.compacting`, `session.compactComplete`
- Done: `session.doneTag`
- Error/attention: `session.statusWarning.title`, `session.mayNeedAttention`, `StopFailure`, `PostToolUseFailure`
- Interrupted: `session.interrupted`
- Subagents/child agents: `child_agents.running_%d`, `subagent`
- Task list: `tasks.title`, `tasks.stats`, `TaskListView`, `TaskRowView`
- Restart/setup banner: `RestartSessionsBannerView`, `session.restartBannerTitle`
- Terminal-routed approval: `TerminalApprovalHintView`, `approval.operateInTerminal`, `approval.goToTerminal`

## Recovered Approval States

Approval UI supports:

- Deny: `approval.deny`
- Allow once: `approval.allow`
- Always allow: `approval.always`
- Allow all: `approval.allowAll`
- Bypass/auto mode: `approval.bypass`, `approval.autoMode`
- Go to terminal: `approval.goToTerminal`
- Terminal-only fallback: `approval.operateInTerminal`
- Detail section:
  - `ApprovalDetailView`
  - `approval.detail.newFile`
  - `approval.detail.moreLines`
  - `approval.detailsUnavailable`

## Recovered Question States

Question UI supports:

- Waiting banner: `question.waiting`
- Single question: `SingleQuestionView`
- Multi-question/multi-select: `MultiQuestionView`, `question.multiSelect`
- Submit all answers: `question.submitAll`
- Confirm selection with count: `question.confirmSelection`
- Answer validation: `question.answerAllWarning`
- Terminal-only fallback: `question.answerInTerminal`
- Read-only Copilot variant: `CopilotQuestionReadOnlyView`

## Recovered Plan Approval States

Plan approval is represented by `ExitPlanModeApprovalView` and supports:

- Manual approve: `exitPlan.manualApprove`
- Auto-accept edits: `exitPlan.acceptEdits`
- Bypass permissions: `exitPlan.bypass`
- Feedback text field: `exitPlan.feedbackPlaceholder`
- Requested permissions list: `exitPlan.requestedPermissions`

## Comparison To AgentBro Current Model

AgentBro already has equivalent logical coverage:

- `SessionPhase`: `ready`, `idle`, `processing`, `waiting_approval`, `waiting_input`, `compacting`, `done`, `error`, `interrupted`
- `OverlayType`: `permission`, `question`, `plan`, `completion`, `response`
- Dev lab scenarios:
  - `empty-idle`
  - `multi-session-mixed`
  - `response-overlay`
  - `overlay-queue`
  - `long-content`
  - `status-badges`
  - `permission-request`
  - `ask-user-question`
  - `plan-approval`
  - `pre-tool-use`
  - `post-tool-use`
  - `stop`
  - `stop-failure`
  - real recorded variants for edit/read/write/bash/question/plan/subagent/compact

The gap is less about missing states and more about presentation:

- Vibe favors a compact `SessionsListView` first.
- AgentBro currently often elevates permission/question/plan into larger overlay cards.
- Vibe has explicit terminal-routed approval fallback states.
- Vibe has visible restart/setup and trust banners.
- Vibe appears to present task/subagent state as compact list context rather than large detail-first panels.

## Recommended Alignment Targets

1. Add a Vibe-aligned dense list mode for `HoverList`.
2. Keep approval/question/plan discoverable from the list first, then expand into detail.
3. Add explicit terminal-routed approval/question hint states.
4. Add restart/setup/trust warning banner scenarios to `ClaudeHookUiLab`.
5. Add `TaskListView`-style compact task rows in the hover/list surface.
6. Add a cropped screenshot workflow for visual QA; do not use full-screen captures as evidence.

## Limitations

- This document does not contain Vibe Island source code.
- The installed app is a native compiled binary. Without the original project or debug/source maps, full implementation details are not recoverable as source.
- Static symbol/string analysis can recover state names, view names, event names, and text surfaces, but not exact layout code.
- Exact visual metrics still require cropped runtime screenshots or direct window image capture.

## Related Notes

- [Vibe Island Usage Capability Reconstruction](./vibe-island-usage-reconstruction.md) covers the usage/rate-limit HUD, provider routing, Codex JSONL ingestion, Claude statusline bridge, caching, and AgentBro implementation notes.
