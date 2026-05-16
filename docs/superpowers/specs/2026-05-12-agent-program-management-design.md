# Agent Program Management Design

## Overview

Add a top-level **Agents** page to Settings for managing AI agent programs themselves. This page is separate from Skills management: it installs, uninstalls, updates, detects, and opens download pages for agent CLIs and desktop apps.

The first version should cover all adapters already known by the app. CLI agents get direct commands where feasible. Desktop app agents stay in the same list, but use download/open actions instead of pretending every app has a safe package-manager install path.

## Confirmed Decisions

| Topic | Decision |
| --- | --- |
| Settings entry | New top-level **Agents** page next to General, Island, Skills, License, About |
| Main layout | Compact list view |
| Operation feedback | Expandable terminal output in the affected row |
| Details | Right slide-in detail panel, same pattern as Skills |
| Scope | Install, uninstall, update, refresh, open download page |
| Agent coverage | Include all existing adapters |
| CLI vs app handling | One unified list with `CLI` / `App` badges and type-specific actions |
| Hooks | Show AgentBro hook status/actions in the detail panel, but do not mix hooks with program install status |

## Product Model

Agents fall into two practical groups:

- **CLI tools**: Claude Code, Codex, Gemini CLI, OpenCode, Qoder CLI, Copilot, and similar command-line tools. These can expose `Install`, `Update`, and `Uninstall` actions backed by commands such as npm, brew, pipx, go, or gh extension commands.
- **Desktop apps**: Cursor, Trae, Kiro, Kimi, CodeBuddy, Qwen, and similar GUI apps. These can be detected through app paths, config paths, and binaries when present. If there is no safe package-manager path, the primary action is `Download` or `Open App`.

This keeps the page comprehensive without implying unsupported automation.

## UI Structure

The Agents page has three areas:

1. **Header**
   - Title and short count summary.
   - Search input.
   - Filter tabs: `All`, `Installed`, `Available`, `Updates`.
   - `Refresh` action.

2. **Agent list**
   - One row per agent.
   - Shows icon, display name, package/source information, type badge, status badge, version when known, and actions.
   - Clicking the row opens the detail panel.
   - Running rows expand an embedded terminal area with live output.

3. **Detail panel**
   - Slides from the right at roughly the same width as the Skills detail slider.
   - Shows richer metadata, install/download information, program status, and hook status/actions.

## Agent Row

Each row displays:

- Adapter icon and display name.
- Secondary metadata:
  - CLI: package manager, package name, installed version.
  - App: app path or download source.
- Type badge: `CLI` or `App`.
- Status badge:
  - `Installed`
  - `Update Available`
  - `Not Installed`
  - `Unavailable`
- Actions:
  - CLI installed: `Update`, `Uninstall`.
  - CLI not installed: `Install`.
  - App installed: `Open`, optionally `Uninstall` only when a safe implementation exists.
  - App not installed: `Download`.

Uninstall should use the existing inline-confirm pattern instead of a blocking dialog.

## Terminal Feedback

Install, update, and uninstall operations create an operation state per agent:

- `idle`
- `running`
- `success`
- `error`

While running, the row receives a subtle active border and expands a terminal output panel. Output is streamed from Rust through a Tauri event named `agent-output`.

On failure, the row keeps the terminal output visible, shows an error state, and exposes `Retry`.

The terminal panel is for transparency and debugging. The normal success path should still feel one-click.

## Detail Panel

For **CLI tools**, show:

- Large icon, name, status, installed version, latest version when known.
- Package manager and package name.
- Binary path.
- Config directory.
- Install, update, and uninstall command previews.
- AgentBro hooks status.
- Actions: install/update/uninstall and install/remove hooks.

For **desktop apps**, show:

- Large icon, name, status.
- App path.
- Config directory.
- Download URL.
- AgentBro hooks status.
- Actions: open app, open download page, install/remove hooks.

## Backend Design

Extend the existing agent system rather than building a parallel registry.

### Rust Types

Add shared types under `src-tauri/src/agents/`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentProgramKind {
    Cli,
    App,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentProgramStatus {
    Installed,
    NotInstalled,
    UpdateAvailable,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProgramInfo {
    pub id: String,
    pub display_name: String,
    pub icon: String,
    pub kind: AgentProgramKind,
    pub status: AgentProgramStatus,
    pub package_manager: Option<String>,
    pub package_name: Option<String>,
    pub installed_version: Option<String>,
    pub latest_version: Option<String>,
    pub binary_path: Option<String>,
    pub config_dir: Option<String>,
    pub app_path: Option<String>,
    pub download_url: Option<String>,
    pub install_command: Option<String>,
    pub update_command: Option<String>,
    pub uninstall_command: Option<String>,
    pub hooks_installed: bool,
}
```

### Adapter API

Extend `AgentAdapter` with metadata methods that have safe defaults:

```rust
fn program_kind(&self) -> AgentProgramKind;
fn package_manager(&self) -> Option<&'static str> { None }
fn package_name(&self) -> Option<&'static str> { None }
fn download_url(&self) -> Option<&'static str> { None }
fn install_command(&self) -> Option<Vec<String>> { None }
fn update_command(&self) -> Option<Vec<String>> { None }
fn uninstall_command(&self) -> Option<Vec<String>> { None }
fn app_path(&self) -> Option<String> { None }
fn config_dir(&self) -> Option<String> { None }
```

Adapters opt into program actions one by one. Missing commands make the UI show `Download` or disabled actions instead of inventing unsafe behavior.

### IPC Commands

Add Tauri commands:

- `agent_list` — return all `AgentProgramInfo` entries.
- `agent_refresh` — re-detect and return all entries.
- `agent_install(agent_id)` — execute adapter install command and stream output.
- `agent_update(agent_id)` — execute adapter update command and stream output.
- `agent_uninstall(agent_id)` — execute adapter uninstall command and stream output.
- `agent_open_download(agent_id)` — open the download page.
- `agent_open_app(agent_id)` — open the app path when known.

Commands should return `Result<(), String>` for operations and send structured output:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentOutputEvent {
    pub agent_id: String,
    pub operation: String,
    pub stream: String,
    pub line: String,
    pub done: bool,
    pub success: Option<bool>,
}
```

## Frontend Design

New files:

- `src/services/agentApi.ts` — typed wrappers around Tauri commands and event subscription.
- `src/stores/agentStore.ts` — Zustand store for list, filters, selected agent, and operation states.
- `src/components/settings/sections/AgentsSection.tsx` — page shell and list.
- `src/components/settings/sections/AgentsSection.css` — styling using settings tokens.
- `src/components/settings/sections/AgentRow.tsx` — one row plus expandable terminal.
- `src/components/settings/sections/AgentDetailSlider.tsx` — right-side details.

The UI should reuse existing settings visual language and avoid a separate dashboard style. It should feel like a quiet utility page: dense enough for repeated use, with clear status and predictable controls.

## Error Handling

- If no install command exists, show `Download`.
- If a command is missing on the machine, show a readable error in terminal output.
- If a command exits non-zero, keep output expanded and show retry.
- If version lookup fails, keep the installed status and omit latest version.
- If app detection is uncertain, show `Unavailable` rather than pretending it is installable.

## First Implementation Slice

The first slice should favor a useful vertical path over perfect coverage:

1. Add the Agents settings page and list all adapters.
2. Show reliable detection for installed/not installed.
3. Implement CLI operations for agents with clear package-manager commands.
4. Stream terminal output into rows.
5. Add the detail slider.
6. Add app download/open fallbacks.

Per-agent command coverage can expand safely after the page and operation pipeline are in place.

## Verification

- `pnpm tsc --noEmit`
- `pnpm build`
- `cargo check` from `src-tauri`
- Manual UI check:
  - Agents page appears in Settings.
  - Search and filters work.
  - Clicking a row opens details.
  - A command operation streams output and reaches success/error.
  - Agents without install commands show download/open behavior instead of broken buttons.
