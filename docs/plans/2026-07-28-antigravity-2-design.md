# Antigravity 2.0 Desktop and CLI Integration

## Context

AgentBro already exposes an Antigravity entry, but the implementation targets the
legacy `~/.antigravity/settings.json` layout and generic snake_case hook payloads.
Current Antigravity 2.0 and the `agy` CLI share a customization model rooted at
`~/.gemini/config`:

- Hooks: `~/.gemini/config/hooks.json`
- Skills: `~/.gemini/config/skills`
- MCP: `~/.gemini/config/mcp_config.json`
- Plugins: `~/.gemini/config/plugins`

The current hook contract uses camelCase fields such as `conversationId`,
`workspacePaths`, `transcriptPath`, and `toolCall`. The supported lifecycle
events are `PreToolUse`, `PostToolUse`, `PreInvocation`, `PostInvocation`, and
`Stop`.

References:

- <https://www.antigravity.google/docs/hooks>
- <https://www.antigravity.google/docs/skills>
- <https://www.antigravity.google/docs/mcp>
- <https://www.antigravity.google/docs/plugins>
- <https://www.antigravity.google/docs/cli-getting-started>

## Scope

One AgentBro adapter will represent both the Antigravity 2.0 desktop app and the
`agy` CLI. AgentBro considers Antigravity installed when either
`/Applications/Antigravity.app` exists or the `agy` executable can be resolved.
This avoids duplicate Agent Management entries for runtimes that intentionally
share customization files and conversation infrastructure.

The change covers:

- Dynamic Island lifecycle and permission events
- Hook installation, health checks, event selection, migration, and removal
- Desktop and CLI installation detection
- Skills, MCP, Plugins, hook, and configuration paths in Agent Management
- Regression tests based on the documented payloads and configuration schema

It does not add Antigravity project creation, scheduled-task control, or direct
conversation orchestration.

## Hook Configuration

AgentBro owns one top-level named hook definition in
`~/.gemini/config/hooks.json`:

```json
{
  "agentbro": {
    "PreToolUse": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "agentbro-bridge --source antigravity --event PreToolUse",
            "timeout": 21600
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "agentbro-bridge --source antigravity --event PostToolUse"
          }
        ]
      }
    ],
    "PreInvocation": [
      {
        "type": "command",
        "command": "agentbro-bridge --source antigravity --event PreInvocation"
      }
    ],
    "PostInvocation": [
      {
        "type": "command",
        "command": "agentbro-bridge --source antigravity --event PostInvocation"
      }
    ],
    "Stop": [
      {
        "type": "command",
        "command": "agentbro-bridge --source antigravity --event Stop"
      }
    ]
  }
}
```

Installation replaces only the AgentBro-owned top-level definition and preserves
all other named hooks. Reinstalling is idempotent. Uninstall removes only that
definition. During installation or removal, AgentBro also removes its own
managed commands from the legacy `~/.antigravity/settings.json` structure while
leaving unrelated legacy hooks untouched.

Hook health checks validate the enabled event set, bridge path, source argument,
event argument, and the different matcher/non-matcher shapes required by the
official schema.

## Event Flow

The bridge receives the event name through `--event` because Antigravity payloads
do not include a generic event discriminator. It normalizes:

- `conversationId` to the AgentBro session ID
- the first `workspacePaths` item to the session working directory
- `toolCall.name` and `toolCall.args` to tool name and input
- `transcriptPath`, `artifactDirectoryPath`, `stepIdx`, `error`,
  `terminationReason`, and `fullyIdle` to raw session metadata

Desktop conversations use transcript paths under `~/.gemini/antigravity`;
CLI conversations use `~/.gemini/antigravity-cli`. Both retain the
`antigravity` AgentBro agent type.

Events map to AgentBro as follows:

| Antigravity event | AgentBro behavior |
| --- | --- |
| `PreToolUse` | Show a Dynamic Island permission request with tool name and arguments |
| `PostToolUse` | Mark the latest tool successful or failed |
| `PreInvocation` | Mark the conversation as processing |
| `PostInvocation` | Keep processing while the response/next tool step is resolved |
| `Stop` | Complete the task, or surface an error termination |

## Permission Safety and Fallbacks

`PreToolUse` is the permission gate. When AgentBro is reachable, the bridge waits
for the Dynamic Island decision and returns the official Antigravity shape:

- Allow: `{"decision":"allow"}`
- Deny: `{"decision":"deny","reason":"..."}`

AgentBro does not rewrite Antigravity security presets or persist broader
permissions. If AgentBro is unavailable, communication fails, or no valid
decision is returned, the bridge emits `{"decision":"ask"}` so Antigravity falls
back to its native approval UI.

Non-gating hooks always emit valid JSON. `PostToolUse`, `PreInvocation`, and
`PostInvocation` return `{}`. `Stop` returns a non-continue decision, allowing
the execution loop to terminate normally. Hook failures must never silently
grant a permission.

## Agent Management

The existing Antigravity row remains the single management surface:

- Installed when the desktop app or `agy` is found
- Shows the resolved `agy` binary when present
- Opens the desktop app only when it exists
- Uses the official Antigravity download page otherwise
- Manages global skills from `~/.gemini/config/skills`
- Reads and edits MCP configuration at `~/.gemini/config/mcp_config.json`
- Discovers plugins under `~/.gemini/config/plugins`
- Manages Dynamic Island hooks at `~/.gemini/config/hooks.json`

Plugins are auto-discovered by Antigravity. AgentBro will inventory and inspect
them without inventing an undocumented enable/disable setting.

## Error Handling

- Corrupt `hooks.json` is reported as settings corruption and is not overwritten.
- Missing customization directories are created only during an explicit install.
- Legacy migration is best-effort after the new hook configuration is written;
  failure is returned rather than presented as a successful migration.
- Hook parsing uses stable fallbacks for missing workspace, tool, and error data.
- Removing hooks is safe when either the current or legacy file is absent.

## Testing

Rust tests will cover:

- Rendering the official named-hook schema for all five events
- Preserving unrelated named hooks during install and uninstall
- Detecting incomplete, outdated, and corrupt managed hook definitions
- Cleaning only legacy AgentBro-managed entries
- Parsing documented camelCase payload fixtures
- Mapping allow, deny, unavailable, and malformed permission responses
- Returning valid JSON for every hook event
- Detecting `agy` and the desktop application
- Resolving Antigravity Skills, MCP, Plugins, and configuration paths

The required repository checks remain:

1. `pnpm lint`
2. `pnpm test:run`
3. `pnpm build`
4. `cargo check --manifest-path src-tauri/Cargo.toml`

