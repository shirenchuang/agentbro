# Agent Program Management Implementation Plan

**Goal:** Add a Settings > Agents page that manages installed AI agent programs, with direct CLI operations where safe and download/open fallbacks for desktop apps.

**Spec:** `docs/superpowers/specs/2026-05-12-agent-program-management-design.md`

## File Map

### Backend

| File | Change |
| --- | --- |
| `src-tauri/src/agents/traits.rs` | Add program-management metadata methods with defaults |
| `src-tauri/src/agents/mod.rs` | Add serializable program types, IPC commands, operation runner |
| `src-tauri/src/agents/*.rs` | Add package metadata and commands for supported adapters |
| `src-tauri/src/lib.rs` | Register new IPC commands |
| `src-tauri/capabilities/default.json` | Allow new agent commands |

### Frontend

| File | Change |
| --- | --- |
| `src/services/agentApi.ts` | Tauri wrappers and event listener |
| `src/stores/agentStore.ts` | Zustand store for agents, filters, selection, operation state |
| `src/components/settings/sections/AgentsSection.tsx` | New settings page shell |
| `src/components/settings/sections/AgentsSection.css` | New page styles |
| `src/components/settings/sections/AgentRow.tsx` | Agent list row and terminal output |
| `src/components/settings/sections/AgentDetailSlider.tsx` | Right-side detail panel |
| `src/components/settings/SettingsApp.tsx` | Add Agents route |
| `src/components/settings/SettingsSidebar.tsx` | Add sidebar item |
| `src/i18n/locales/*.json` | Add visible labels |

## Task 1: Backend Types and Adapter Defaults

- [ ] Add `AgentProgramKind`, `AgentProgramStatus`, `AgentProgramInfo`, and `AgentOutputEvent`.
- [ ] Extend `AgentAdapter` with safe default program metadata methods.
- [ ] Implement helper functions for `which`, command preview formatting, version lookup, config path strings, and hook detection.
- [ ] Preserve existing hook installation behavior.

## Task 2: Backend Commands

- [ ] Implement `agent_list` and `agent_refresh`.
- [ ] Implement a shared async command runner for install/update/uninstall.
- [ ] Stream stdout/stderr lines to the frontend through `agent-output`.
- [ ] Implement `agent_open_download` and `agent_open_app`.
- [ ] Register commands in `lib.rs` and capabilities.

## Task 3: Adapter Coverage

- [ ] Add CLI package metadata for common safe paths first: Claude Code, Codex, Gemini CLI, OpenCode, Qoder CLI, Copilot.
- [ ] Add app metadata/download URLs for desktop-app adapters.
- [ ] Leave unsupported operations as `None` so the UI exposes fallback actions.

## Task 4: Frontend Data Layer

- [ ] Create `agentApi.ts`.
- [ ] Create `agentStore.ts`.
- [ ] Subscribe to `agent-output` and append lines to per-agent operations.
- [ ] Refresh agent state after successful operations.

## Task 5: Frontend UI

- [ ] Add the Agents section to Settings routing and sidebar.
- [ ] Build header, search, filters, summary, and refresh action.
- [ ] Build `AgentRow` with badges, actions, inline confirm, and expandable terminal.
- [ ] Build `AgentDetailSlider` with CLI/App-specific fields and actions.
- [ ] Style with existing settings tokens.

## Task 6: Verification

- [ ] `pnpm tsc --noEmit`
- [ ] `pnpm build`
- [ ] `cargo check` in `src-tauri`
- [ ] Manual check in the app:
  - Agents page appears.
  - Search/filter works.
  - Row click opens detail slider.
  - Running operation shows terminal output.
  - Unsupported install paths show download/open fallback.
