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
- Monitoring is enabled explicitly by the user, but the proxy lifecycle must not be tied to a React page unmount.
- Claude Code hooks must be preserved. Network capture must not break AgentBro's hook-driven island/session monitor.

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
    - `get_claude_wrapper_status`
    - `install_claude_wrapper`
    - `remove_claude_wrapper`
  - Installs a PATH shim at `~/.agentbro/bin/claude`.
  - The shim detects the real upstream from env/project/global Claude settings, routes it through AgentBro, then launches real Claude with process-level `ANTHROPIC_BASE_URL`.
  - The shim must not pass `--settings`; doing so can override or bypass Claude Code hooks.

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
  - Added Agent Monitor sub-views: overview, capture, stats, sessions, access.
  - Toggle defaults off.
  - On mount: reads current network monitor status.
  - Does not disable the proxy on page unmount.
  - Adds `请求抓包` workbench and per-session `网络请求` detail tab.
  - Shows proxy command when enabled for debugging/manual fallback:
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

- The preferred path is the AgentBro Claude command shim.
- After the user installs the shim and opens a new shell, typing `claude` normally should route new Claude processes through AgentBro.
- The shim injects only process-level `ANTHROPIC_BASE_URL=<agentbro-route>`.
- It must preserve the user's Claude settings and hooks; it must not pass generated `--settings`.
- The manual env command remains useful for debugging or non-standard shells:
  - `ANTHROPIC_BASE_URL=http://127.0.0.1:<port> claude`

The monitor does not modify existing external terminals or existing Claude processes.

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

## AgentBro Native Integration Direction

Do not embed cc-viewer as a separate app or server inside AgentBro. Treat cc-viewer as the reference implementation for Claude Code request interpretation, then rebuild the durable pieces in AgentBro's native monitor domain.

Target product shape:

- `Agent监控` remains the top-level entry.
- Network capture stays local, manual, memory-first, and off by default.
- Claude command wrapping must preserve Claude Code hooks and AgentBro island behavior.
- Captured HTTP requests are normalized into AgentBro trace records instead of staying as raw proxy rows.
- AgentBro sessions, hook events, terminal launch metadata, and network requests should converge into one timeline.
- Token usage should become a first-class analytics surface by project, session, model, request type, and time range.

Wrapper invariant:

- The wrapper may read Claude settings to discover the real provider upstream.
- The wrapper may inject `ANTHROPIC_BASE_URL=<AgentBro route>` into the child process environment.
- The wrapper must not generate or pass `--settings` to Claude Code.
- The wrapper must not write project or global Claude settings.
- Existing Claude Code hooks are part of AgentBro's monitoring contract and must be preserved.
- If the AgentBro proxy is unavailable or stale, the wrapper must fall back to launching the real Claude command unchanged.

Recommended layering:

1. `Capture`
   - Local provider proxy for Claude/Anthropic-compatible traffic.
   - Preserve raw request/response details with redaction and bounded retention.
   - Keep this provider-specific until Claude Code is reliable.

2. `Interpret`
   - Classify requests into `MainAgent`, `SubAgent`, `Count`, `Preflight`, `Synthetic`, `Plan`, or `Unknown`.
   - Extract structured `system`, `messages`, `tools`, `response`, streaming chunks, and usage.
   - Adopt cc-viewer-style heuristics, but keep the code native to AgentBro.

3. `Correlate`
   - Link requests to AgentBro sessions using injected launch metadata when available.
   - Fall back to cwd/project/time-window matching with explicit confidence labels.
   - Display unknown correlation honestly instead of implying exactness.

4. `Analyze`
   - Roll up token/cost metrics by session, project, model, request type, and date.
   - Track cache read/create tokens separately.
   - Later add waste diagnostics: low cache hit rate, repeated large prompts, retry storms, and expensive subagents.

5. `Present`
   - Replace raw-only request detail with focused tabs:
     - `System`
     - `Messages`
     - `Tools`
     - `Response`
     - `Headers`
     - `Raw`
   - Keep raw JSON available for debugging, but make the normal path explain what the agent actually did.

First implementation slice:

- Add request classification and normalized usage summary to `NetworkRequestSummary`.
- Add structured request detail tabs in `AgentMonitorSection`.
- Keep storage in-memory and maintain all current privacy constraints.
- Add frontend coverage that renders a classified request and its structured tabs.

### Step 1: Real E2E Test With Claude Code

Run the desktop app:

```bash
pnpm tauri dev
```

Open settings -> `Agent监控`.

Enable `原生网络请求监控`.

Install `Claude 命令无感接入`, then open a new terminal and run:

```bash
claude
```

The manual command remains a debugging fallback:

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
- Overview/access copy makes it clear that AgentBro preserves Claude hooks/settings.

Backend:

- Unit tests for:
  - Claude wrapper script never contains generated `--settings`
  - Claude wrapper launches via process-level `ANTHROPIC_BASE_URL`
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
