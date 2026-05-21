# Vibe Island Usage Capability Reconstruction

This is a black-box reconstruction from the installed macOS app and local runtime traces. It is not a source-code extraction.

## Executive Summary

Vibe Island's "usage" feature is primarily a subscription rate-limit HUD, not a detailed per-request cost dashboard.

The visible product behavior is the compact header shown in the expanded island, for example:

- `5h <used>% <reset/remaining> | 7d <used>% <reset/remaining>`
- optional speaker/settings controls in the same header area
- settings for showing usage limits, preferred provider, and used-vs-remaining display mode

The implementation appears to normalize provider-specific quota data into two common windows:

- `five_hour`
- `seven_day`

Each window stores a used percentage and reset timestamp. The app then renders the two windows as compact colored usage chips.

## Evidence

Observed environment:

- Installed app: `/Applications/Vibe Island.app`
- Bundle id: `app.vibeisland.macos`
- Version: `1.0.33`
- `Info.plist` contains `VibeIslandUsageEnabled = true`
- Runtime socket: `~/.vibe-island/run/vibe-island.sock`
- User cache: `~/.vibe-island/cache/usage-persist-openai.json`
- Claude statusline bridge: `~/.vibe-island/bin/vibe-island-statusline`
- Hook bridge launcher: `~/.vibe-island/bin/vibe-island-bridge`

UI screenshot evidence:

- [output/vibe-island-samples/02-expanded-list-after-simulated-events.png](/Users/shirenchuang/code/aidelivery/vibeIsland/agentbro/output/vibe-island-samples/02-expanded-list-after-simulated-events.png) shows the expanded panel header with `5h ... | 7d ...` usage indicators.

Localized strings expose the product surface:

- `settings.showUsage` / `settings.showUsageHelp`: "Show Usage Limits" / "Display subscription usage limits in the notch panel header"
- `settings.usageProvider`: preferred provider
- `settings.usageProvider.auto`: auto, follow session
- `settings.usageValueMode.used`: used
- `settings.usageValueMode.remaining`: remaining
- `usage.activate`: show usage limits
- `usage.waitingForProvider`: waiting for provider activity
- `usage.unavailable`: unavailable

Local cache schema:

```json
{
  "provider": "openai",
  "fetched_at": 0,
  "metadata": {
    "codex.planType": "...",
    "codex.limitId": "...",
    "codex.origin": "..."
  },
  "five_hour": {
    "used_percentage": 0,
    "resets_at": 0
  },
  "seven_day": {
    "used_percentage": 0,
    "resets_at": 0
  }
}
```

The current cache contains only quota percentages and reset timestamps. It does not contain raw token totals, API keys, or request logs.

## Data Sources

### 1. Codex Rate Limits

Codex stores `token_count` events in JSONL rollout files under `~/.codex/sessions/.../rollout-*.jsonl`.

Those events include:

```json
{
  "type": "event_msg",
  "payload": {
    "type": "token_count",
    "info": {
      "total_token_usage": {
        "input_tokens": 0,
        "cached_input_tokens": 0,
        "output_tokens": 0,
        "reasoning_output_tokens": 0,
        "total_tokens": 0
      },
      "last_token_usage": {},
      "model_context_window": 0
    },
    "rate_limits": {
      "limit_id": "codex",
      "primary": {
        "used_percent": 0,
        "window_minutes": 300,
        "resets_at": 0
      },
      "secondary": {
        "used_percent": 0,
        "window_minutes": 10080,
        "resets_at": 0
      },
      "plan_type": "..."
    }
  }
}
```

Vibe logs confirm this path:

- `Codex JSONL rate_limits observed`
- `Codex rate limits notification received`
- `Codex rate limits accepted`
- `Codex rate limits stored`
- `origin=jsonl-token-count`

Vibe also attempts a live Codex probe:

- `Codex usage probe starting`
- `Codex app-server starting for usage`
- `Codex app-server launch failed`
- `Codex usage probe skipped: cached data`

So the likely Codex order is:

1. Use event-pushed `token_count.rate_limits` from rollout JSONL when available.
2. Cache the normalized result immediately.
3. Optionally try a live Codex app-server/account probe when a manual or scheduled refresh runs.
4. Fall back to cached data when live probing fails or would be too frequent.

### 2. Claude Code Usage

Claude Code exposes usage through its status line input. Vibe installs a reversible bridge by setting:

```json
{
  "statusLine": {
    "command": "/Users/.../.vibe-island/bin/vibe-island-statusline"
  }
}
```

The generated `vibe-island-statusline` script reads stdin once, extracts `.rate_limits`, and writes it into:

```text
~/.vibe-island/cache/rl.json
```

The bridge deliberately keeps the user's visible statusline output unchanged. Vibe's own localization calls this out repeatedly:

- it adds a small bridge
- it preserves existing visible output
- it can wrap Claude HUD
- it can remove only the marked Vibe block later
- it creates timestamped backups before changes

The hook bridge binary also contains an Anthropic OAuth usage endpoint string:

```text
https://api.anthropic.com/api/oauth/usage
```

This suggests two Claude paths:

1. Passive path: Claude Code statusline stdin -> `.rate_limits` -> local `rl.json` -> app store.
2. Active refresh path: bridge reads Claude credentials/OAuth and calls Anthropic's usage endpoint, protected by a local lock and cooldown.

The passive statusline bridge is the safer and more visible product mechanism.

### 3. Other Providers

The bridge is installed for many agent sources:

- `claude`
- `codex`
- `gemini`
- `cursor`
- `droid`
- `qoder`
- `qwen`
- `kimi`
- `deepseek`
- `copilot`
- `codebuddy`
- `kiro`

Logs and strings show provider-selection state such as:

- `selectedProvider`
- `displayProvider`
- `focusedSource`
- `focusedMappedProvider`
- `availableProviders`
- `usageAnchorSessionId`
- `sessionSources`
- `transientProvider`

This implies Vibe has a provider router:

1. Track active/focused agent sessions.
2. Map agent source to a billing provider, for example Codex -> OpenAI and Claude -> Anthropic.
3. In auto mode, display the provider attached to the focused or anchor session.
4. In manual mode, display the preferred provider if data exists.
5. If the chosen provider has no data, show "waiting for provider" or "unavailable".

Some providers appear to be recognized before full quota readers are implemented.

## Runtime Architecture

The usage pipeline is likely:

```text
Agent CLI / statusline / JSONL
        |
        v
Vibe hook bridge or statusline bridge
        |
        v
Local normalized cache
  - usage-persist-openai.json
  - rl.json
  - refresh lock
        |
        v
Vibe Island native app store
        |
        v
Provider selector
  - auto follows active session
  - manual preferred provider
        |
        v
Compact island header
  - 5h window
  - 7d window
  - used or remaining mode
```

The native app does not need to continuously inspect terminal text. It relies on structured events/files that the agent CLIs already emit.

## Scheduling And Caching

Logs show the app avoids aggressive polling:

- `usage refresh marked dirty`
- `usage refresh scheduled`
- `usage refresh task fired`
- `usage fetch begin`
- `usage refresh schedule skipped`
- `usage refresh skip: another refresh in progress`
- `usage refresh skip: cooldown still hot`
- `Codex usage probe skipped: cached data`

There is also a local lock file:

```text
~/.vibe-island/cache/.rl.refresh.lock
```

The refresh model is probably:

- session start or provider change marks usage dirty
- short refresh delay when session activity changes
- longer periodic refresh when nothing changes
- lock/cooldown prevents multiple probes
- push-style Codex JSONL observations can update the store without waiting for a scheduled fetch

## What It Is Not

The observed feature is not primarily:

- a full API billing ledger
- a provider invoice explorer
- a per-request cost dashboard
- a generic token tracker across all agents

Codex JSONL contains token totals, and Vibe may read them, but the visible usage capability is centered on subscription/rate-limit windows.

## Implementation Notes For AgentBro

To reproduce this capability cleanly, use a normalized model:

```ts
type UsageSnapshot = {
  provider: 'openai' | 'anthropic' | string
  providerLabel: string
  source: 'codex-jsonl' | 'codex-app-server' | 'claude-statusline' | 'anthropic-oauth' | string
  fetchedAt: number
  metadata?: Record<string, string>
  windows: Array<{
    id: 'five_hour' | 'seven_day' | string
    title: string
    usedPercent: number
    remainingPercent?: number
    remainingLabel?: string
    resetsAt?: string
    windowMinutes?: number
  }>
}
```

Recommended ingestion adapters:

1. Codex JSONL adapter: scan recent rollout files for `event_msg.payload.type == "token_count"` and parse `payload.rate_limits`.
2. Codex live adapter: call Codex app-server/account rate-limit API when available, with a short timeout.
3. Claude statusline adapter: install a reversible bridge that extracts `.rate_limits` from statusline stdin.
4. Claude live adapter: optional OAuth usage endpoint probe, gated by credentials, lock, and cooldown.

Recommended UI behavior:

1. Put `5h` and `7d` chips in the island header, not in a large stats panel.
2. Let users switch used vs remaining mode.
3. Keep provider selection as auto/manual.
4. Show "waiting for provider activity" when no snapshot exists.
5. Keep raw credential and OAuth data out of frontend state.
6. Persist only normalized snapshots and metadata.

## AgentBro Gap Check

AgentBro already has adjacent pieces:

- `RateLimitInfo` and `UsageRateWindow` in `src-tauri/src/hooks/session_store.rs`
- `RateLimitBar` in `src/components/notch/RateLimitBar.tsx`
- Codex JSONL and live app-server probing in `src-tauri/src/commands/mod.rs`
- usage-provider status UI in `src/components/settings/sections/IslandSection.tsx`

Important differences from Vibe:

- Vibe's Claude passive bridge writes `~/.vibe-island/cache/rl.json`; AgentBro should use its own app-owned equivalent rather than `/tmp` if we want the same durability and privacy properties.
- Vibe treats usage as a global/provider-level HUD that follows focused sessions; AgentBro currently also attaches rate limits to session state, so we should keep a global latest/provider snapshot for header display.
- Vibe's settings explicitly explain the statusline bridge, backup/restore behavior, and compatibility with existing statusline tools; AgentBro should match that trust model if we install a bridge.
