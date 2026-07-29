# Antigravity 2.0 Implementation Plan

Issue: <https://github.com/shirenchuang/agentbro/issues/74>

## 1. Add the official named-hook configuration

Files:

- `src-tauri/src/agents/profiles.rs`

Changes:

- Add the five documented Antigravity events with matcher metadata.
- Add a dedicated installation kind for a top-level named hook definition.
- Render, remove, and inspect the `agentbro` definition without changing other
  definitions.
- Include `--event` in each command because payloads do not identify the event.
- Add schema, idempotency, preservation, corruption, and health tests.

## 2. Update the bridge contract

Files:

- `src-tauri/src/bridge/main.rs`

Changes:

- Read `conversationId`, `workspacePaths`, and nested `toolCall` fields.
- Preserve Antigravity termination and transcript metadata in normalized events.
- Add Antigravity-specific permission output.
- Fall back to `ask` when the Dynamic Island cannot provide a decision.
- Print valid JSON for non-permission events.
- Add tests for documented payloads and all output paths.

## 3. Replace the legacy adapter behavior

Files:

- `src-tauri/src/agents/antigravity.rs`

Changes:

- Point the adapter at `~/.gemini/config/hooks.json`.
- Parse all five official events.
- Map permission, tool, processing, completion, and error states.
- Distinguish desktop and CLI transcript roots without splitting the AgentBro
  agent identity.
- Remove only managed legacy hook entries during migration and uninstall.
- Add payload fixture tests.

## 4. Correct runtime detection and program metadata

Files:

- `src-tauri/src/agents/programs.rs`
- `src/services/agentApi.ts`

Changes:

- Detect `agy` instead of obsolete `ag` or `antigravity` candidates.
- Treat either the desktop app or CLI binary as installed.
- Use the official download endpoint and customization directory.
- Avoid presenting an unavailable desktop-open action for CLI-only installs.
- Update browser-mode seed data for parity.

## 5. Correct Agent Management capability paths

Files:

- `src-tauri/src/skills/agent_paths.rs`
- `src-tauri/src/skills/scanner.rs`
- `src-tauri/src/skills/plugin_management.rs` if read-only behavior requires an
  explicit capability guard
- relevant skill-v2 tests

Changes:

- Scan global skills from `~/.gemini/config/skills`.
- Read and edit MCP configuration at `~/.gemini/config/mcp_config.json`.
- Discover plugins from `~/.gemini/config/plugins`.
- Expose `~/.gemini/config/hooks.json` as the managed configuration.
- Keep Antigravity plugins read-only when no documented enable switch exists.

## 6. Validate and deliver

- Run focused Rust tests while developing.
- Review the complete diff and secret scan.
- Run `pnpm lint`, `pnpm test:run`, `pnpm build`, and
  `cargo check --manifest-path src-tauri/Cargo.toml` in order.
- Commit with a Conventional Commit.
- Push the branch and open a PR against `dev` containing `Closes #74`.
- Enable squash auto-merge with branch deletion.
- Monitor checks through merge and verify Issue #74 closes.
