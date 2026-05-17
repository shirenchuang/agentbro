# Agent Monitor Network V2 Continuation Plan

Date: 2026-05-17
Repo: `/Users/shirenchuang/code/aidelivery/vibeIsland/agentbro`

## Background

User feedback on the first Agent Monitor version:

- The previous monitor was mostly a session/hook status panel.
- Many metrics were inferred from hook events, transcript parsing, statusline data, or `sessionStore`, so accuracy was limited.
- It did not monitor native network requests.
- It could not show per-request `system prompt`, `messages`, `tools`, request metadata, raw request/response, usage, streaming chunks, or true HTTP failures.

Decision made:

- Keep `Agent监控` as a top-level settings capability.
- Add Claude Code-first native network request monitoring through a local proxy.
- Do not use system-wide MITM in this version because CA install and privacy risk are too high.
- Network monitoring must default to off.
- Monitoring must be manually enabled each time and should not persist as an always-on setting.
- When the monitor page unmounts, the frontend disables the backend proxy.

## Current Implementation State

The current branch now contains a Claude/Anthropic request proxy MVP.

Key backend files:

- `src-tauri/src/network_monitor.rs`
  - New in-memory network monitor runtime.
  - Starts a local HTTP proxy on `127.0.0.1:<random-port>` only when enabled.
  - Default upstream is `https://api.anthropic.com`.
  - Captures request body, redacted headers, response headers, response body preview, status, duration, model, message count, tool count, system preview, usage, stream event count.
  - Keeps data in memory only.
  - Limits:
    - `MAX_REQUEST_BODY_BYTES = 16 MB`
    - `MAX_CAPTURED_RESPONSE_BYTES = 2 MB`
    - `MAX_CAPTURED_REQUESTS = 200`
  - Redacts `authorization`, `x-api-key`, cookies, token/key/secret-like headers.

- `src-tauri/src/commands/monitor.rs`
  - Existing monitor commands remain.
  - Added IPC:
    - `get_network_monitor_status`
    - `set_network_monitor_enabled`
    - `get_network_monitor_requests`
    - `get_network_monitor_request_detail`

- `src-tauri/src/commands/mod.rs`
  - `AppState` now includes `network_monitor`.
  - `launch_agent_session` injects `ANTHROPIC_BASE_URL=<proxy>` for Claude Code launches when monitoring is enabled.

- `src-tauri/src/lib.rs`
  - Registers `network_monitor` module.
  - Initializes `Arc<NetworkMonitor>`.
  - Registers the new IPC commands.

- `src-tauri/Cargo.toml`
  - Added dependencies:
    - `axum = "0.7"`
    - `reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "stream", "json"] }`
    - `futures-util = "0.3"`
    - `async-stream = "0.3"`

Key frontend files:

- `src/components/settings/sections/AgentMonitorSection.tsx`
  - Added top-level network control card.
  - Toggle defaults off.
  - On mount: reads current network monitor status.
  - On unmount: calls `setNetworkMonitorEnabled(false)`.
  - Adds `网络请求` detail tab.
  - Shows proxy command when enabled:
    - `ANTHROPIC_BASE_URL=http://127.0.0.1:<port> claude`
  - Shows request list and detail:
    - model/status/duration/tokens
    - system prompt preview
    - raw request JSON
    - raw response text
    - request/response headers

- `src/components/settings/sections/AgentMonitorSection.css`
  - Added styling for the network monitor control, request list, and request details.

- `src/services/tauriApi.ts`
  - Added TypeScript types and wrappers for network monitor IPC.

- `src/services/monitorApi.ts`
  - Re-exports network monitor APIs and types.

- `src/test/agentMonitorSection.test.tsx`
  - Added mocks and test coverage for the manual-off-by-default network monitor toggle.

Related prior monitor work already present:

- `src/components/settings/SettingsApp.tsx`
- `src/components/settings/SettingsSidebar.tsx`
- locale files under `src/i18n/locales/`
- `src-tauri/src/hooks/server.rs`
- `src-tauri/src/commands/monitor.rs`
- `src/services/monitorApi.ts`

## Important Current Behavior

For AgentBro-launched Claude Code sessions:

- If the user enables network monitor first, `launch_agent_session` launches Claude with:
  - `ANTHROPIC_BASE_URL=<proxy-url>`
- This should route Claude API requests through AgentBro's proxy.

For externally launched Claude Code sessions:

- The user must launch manually with the env command shown in the UI:
  - `ANTHROPIC_BASE_URL=http://127.0.0.1:<port> claude`

The monitor does not yet automatically modify existing external terminals or existing Claude processes.

## Validation Already Run

These passed after the current implementation:

```bash
pnpm exec tsc -b --pretty false
pnpm test:run src/test/agentMonitorSection.test.tsx src/test/settingsIslandMenu.test.tsx
pnpm build
cargo check --manifest-path src-tauri/Cargo.toml
git diff --check
```

Notes:

- `pnpm build` still shows the existing Vite large chunk warning.
- During dependency experiments, `reqwest 0.13` with `rustls` was tried and rejected because it pulled `aws-lc`/`cmake`. The current code uses `reqwest 0.12` with `rustls-tls`.

## Known Gaps

This is still an MVP. Do not treat it as complete production-quality request analysis yet.

1. Real Claude Code end-to-end proxy validation is still needed.
   - Need to run the app, enable monitor, start Claude with the shown `ANTHROPIC_BASE_URL`, trigger a real request, and confirm the request appears in the UI.
   - Verify `/v1/messages` streaming behavior, status codes, response usage, and raw response capture.

2. Request detail UI is still raw-heavy.
   - It currently shows system preview plus raw request/response.
   - It should be split into focused tabs:
     - `System Prompt`
     - `Messages`
     - `Tools`
     - `Response`
     - `Headers`
     - `Raw JSON`

3. Session correlation is weak.
   - AgentBro-launched Claude can be associated indirectly through launch context, but the proxy currently only reads optional headers like `x-agentbro-session-id`.
   - External terminal requests likely have no session id.
   - Need a correlation strategy using cwd, pid, terminal env, time window, or injected env/header.

4. Upstream configuration is minimal.
   - UI has an upstream URL input.
   - No profile management yet.
   - No validation beyond backend URL parsing.

5. No clear "Claude not routed through proxy" diagnosis.
   - If monitor is enabled but no requests arrive, UI should explain likely causes and show quick commands.

6. No "clear requests" action.
   - Data is memory-only and cleared when proxy starts, but the UI should expose clear/reset.

7. No persisted privacy options.
   - Current behavior is intentionally memory-only.
   - Future options should include:
     - capture full body on/off
     - max retained requests
     - max response bytes
     - clear on page close

8. Usage extraction is basic.
   - It tries normal JSON `usage` and SSE `data:` lines.
   - Needs real Claude streaming validation and probably provider-specific parsing.

9. The proxy currently targets Anthropic-compatible traffic only.
   - Do not expand to Codex/Gemini until Claude Code is solid.

## Recommended Next Work

### Step 1: Real E2E Test With Claude Code

Run the desktop app:

```bash
pnpm tauri dev
```

Open settings -> `Agent监控`.

Enable `原生网络请求监控`.

Copy the displayed command, for example:

```bash
ANTHROPIC_BASE_URL=http://127.0.0.1:<port> claude
```

Trigger a simple Claude request.

Expected:

- Network request count increments.
- `网络请求` tab shows a request row.
- Request detail contains:
  - model
  - system prompt preview
  - messages array
  - tools array if present
  - HTTP status
  - duration
  - usage if response includes it

Fix any proxy incompatibility first before improving UI.

### Step 2: Add a Better Network Detail Layout

Refactor `NetworkRequestsTab` in:

```text
src/components/settings/sections/AgentMonitorSection.tsx
```

Add local sub-tabs for selected request:

- `System`
- `Messages`
- `Tools`
- `Response`
- `Headers`
- `Raw`

Use structured renderers instead of one huge JSON block:

- `system`:
  - If string, render as markdown-like text in `pre`.
  - If array, render each block separately.
- `messages`:
  - Show role, content preview, tool use/tool result indicators.
- `tools`:
  - Show tool name, description, input schema.
- `response`:
  - For SSE, parse `event:` and `data:` blocks.
  - Show final usage separately.

### Step 3: Add Request Actions

Add backend IPC and UI actions:

- `clear_network_monitor_requests`
- `copy proxy command`
- `copy request JSON`
- `copy system prompt`

Keep clear in-memory only.

### Step 4: Improve Session Correlation

Preferred approach:

- When AgentBro launches Claude Code, generate a launch id and inject:
  - `AGENTBRO_SESSION_LAUNCH_ID`
  - `AGENTBRO_PROJECT`
  - `ANTHROPIC_BASE_URL`
- Make the proxy read a custom header if possible.
- If Claude does not forward env into request headers, fallback to:
  - cwd/project from launch request
  - recent session start timestamp
  - active Claude session with matching cwd

UI should display correlation confidence:

- `exact`
- `cwd/time`
- `unknown`

### Step 5: Better Empty/Error States

When monitor is enabled but no traffic arrives:

- Show:
  - proxy URL
  - exact env command
  - "Only new Claude processes launched with this env are monitored."
  - "Existing Claude processes will not be intercepted."

When upstream errors:

- Show HTTP status and upstream error body.
- Avoid hiding errors behind generic `Proxy Error`.

### Step 6: Tests

Add focused tests:

Frontend:

- Toggle is off by default.
- Enabling shows proxy command.
- Network tab empty state when enabled with no requests.
- Request list renders model/status/token summary.
- Request detail renders system/messages/tools.

Backend:

- Unit tests for:
  - header redaction
  - upstream URL joining
  - system prompt extraction
  - usage extraction from JSON
  - usage extraction from SSE

Integration, if feasible:

- Start proxy with a local fake upstream.
- Send a mock Anthropic request.
- Verify captured summary/detail.
- Verify response streams back to client unchanged.

## Files To Read First On The New Machine

Read in this order:

1. `docs/plans/2026-05-17-agent-monitor-network-v2-continuation.md`
2. `src-tauri/src/network_monitor.rs`
3. `src-tauri/src/commands/monitor.rs`
4. `src/components/settings/sections/AgentMonitorSection.tsx`
5. `src/services/tauriApi.ts`
6. `src/test/agentMonitorSection.test.tsx`

## Commands To Re-run After Checkout

```bash
pnpm install
pnpm exec tsc -b --pretty false
pnpm test:run src/test/agentMonitorSection.test.tsx src/test/settingsIslandMenu.test.tsx
cargo check --manifest-path src-tauri/Cargo.toml
```

Then run the real app:

```bash
pnpm tauri dev
```

## Caution

The working tree at the time this plan was written had unrelated dirty files from prior work. Do not revert unrelated changes blindly.

Use `git status --short` and inspect diffs before continuing.

The network monitor captures prompts and request bodies. Keep the default off, keep data local/in-memory unless the user explicitly asks otherwise, and preserve header redaction.
