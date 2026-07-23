# MCP Management Design

Date: 2026-07-23

## Goal

Upgrade AgentBro's global MCP inventory from a read-only view into safe, complete management for:

- Claude Code
- Codex
- Gemini CLI
- Cursor
- Kimi Code
- ZCode

The first release supports create, edit, rename, enable, disable, delete, static validation, and an explicit real connection test for stdio, Streamable HTTP, and legacy SSE servers.

Project-scoped MCP configuration remains read-only.

## Current State

The Agent Management and Project Management pages scan MCP configuration and render server status and config paths. The frontend does not expose MCP mutations.

The Rust backend already contains older upsert, remove, toggle, and static validation commands. Those commands are not connected to the current UI and assume a mostly JSON-shaped configuration. They cannot safely manage Codex TOML or preserve the full set of transport, authentication, enablement, and Agent-specific fields.

## Architecture

Add a dedicated MCP management module instead of extending the Skill installer.

The module exposes a normalized model to the frontend and delegates persistence to an adapter selected by Agent ID.

### Normalized model

Each server record contains:

- Agent ID
- Server name
- Transport: `stdio`, `http`, or `sse`
- Command, arguments, environment, and working directory for stdio
- URL and headers for HTTP/SSE
- Enabled state
- Config path and source kind
- Editable state and capability flags
- Static validation state
- Last connection-test summary
- A revision token derived from the source content

The model preserves Agent-specific and unknown fields in the backend. The frontend only sends fields that it owns plus the revision token.

### Agent adapters

The first release uses the following global configuration sources:

| Agent | Configuration |
| --- | --- |
| Claude Code | `~/.claude.json` user-scoped MCP entries |
| Codex | `~/.codex/config.toml` |
| Gemini CLI | `~/.gemini/settings.json` |
| Cursor | `~/.cursor/mcp.json` |
| Kimi Code | `$KIMI_CODE_HOME/mcp.json` |
| ZCode | `~/.zcode/cli/config.json` under `mcp.servers` |

Adapters implement:

- List and normalize servers
- Read one server with sensitive fields redacted
- Validate a proposed server
- Create or update one server
- Rename one server atomically
- Enable or disable one server
- Delete one server
- Resolve the raw server configuration for a backend-only connection test

JSON adapters mutate only the selected object path. The Codex adapter performs a targeted edit of the selected `mcp_servers` table so comments, ordering, unknown keys, and unrelated TOML remain intact.

### Enable and disable semantics

Use an Agent's native persisted enablement mechanism where one is reliable:

- Codex: per-server `enabled`
- Gemini CLI: MCP allow/exclude configuration
- Kimi Code: per-server `enabled`
- ZCode: per-server `enable`

For Agents without a reliable config-level switch, AgentBro stores the complete disabled entry in a user-only local disabled store and removes it from the active config. Re-enabling restores the entry exactly. The UI identifies this state as "disabled by AgentBro."

Plugin-provided MCP servers remain read-only and direct users to Plugin management.

## Safe Writes

Every mutation follows this sequence:

1. Re-read the source file.
2. Compare its revision token with the token supplied by the UI.
3. Reject the mutation if another program changed the file.
4. Create a timestamped backup with user-only permissions.
5. Apply only the selected MCP mutation.
6. Write a sibling temporary file.
7. Set the intended permissions and atomically replace the source.
8. Re-read and parse the source.
9. Restore the backup if parsing or verification fails.

Backups and disabled entries live under AgentBro's private data directory and use mode `0600`.

## User Experience

The existing MCP tab becomes a management workspace.

### Summary and list

The tab header shows:

- Total servers
- Enabled servers
- Invalid servers
- Add MCP action
- Rescan action

Each row shows:

- Name
- Transport
- Command summary or URL
- Enabled switch
- Static validation state
- Last connection-test result and time
- Test Connection action
- Edit action
- Delete action

Unsupported Agents and plugin-provided entries show a read-only state.

### Add and edit drawer

The drawer changes fields based on transport.

For stdio:

- Name
- Command
- Argument list
- Working directory
- Environment key/value rows

For HTTP/SSE:

- Name
- URL
- Header key/value rows

Arguments are edited as individual values rather than as a shell command string.

Changing a name during edit is an atomic rename. Saving runs static validation. A failed real connection test never blocks saving.

### Destructive and failed operations

Deletion requires confirmation showing the Agent, server name, and config path.

When a write fails, the drawer remains open and retains the user's input. An enable/disable failure restores the previous switch state. A revision conflict asks the user to reload instead of overwriting external changes.

## Validation and Connection Testing

### Static validation

Static validation checks:

- Valid, non-conflicting server name
- Exactly one transport-specific connection definition
- Executable availability for stdio
- Valid URL for HTTP/SSE
- Plain HTTP warnings for non-local endpoints
- Valid environment and header keys
- Agent capability compatibility

### Real MCP handshake

Connection testing is explicit and never invokes an MCP tool.

The backend:

1. Opens the configured transport.
2. Sends `initialize`.
3. Negotiates the protocol version and capabilities.
4. Sends `notifications/initialized`.
5. Sends `tools/list` only when the server declares the Tools capability.
6. Returns latency, server identity, negotiated protocol version, and tool count.
7. Closes the connection and terminates any spawned child process.

The default timeout is 15 seconds. The tester supports current MCP protocol behavior with a bounded fallback for common older versions. Streamable HTTP is attempted before legacy HTTP+SSE fallback.

Result categories are:

- Connected
- Command not found
- Startup failed
- Timed out
- Protocol incompatible
- Authentication required
- Network error

For HTTP `401` or `403`, AgentBro reports that authentication is required and directs the user to the relevant Agent. AgentBro does not implement or store OAuth credentials in this release.

## Sensitive Data

Values for `Authorization` and keys containing terms such as `TOKEN`, `KEY`, `SECRET`, or `PASSWORD` are not returned to the frontend.

The frontend receives a configured placeholder. If the user leaves it unchanged, the backend preserves the original value. The user can replace or remove it without revealing it.

Sensitive values must not appear in:

- Logs
- Error strings
- Connection-test output
- SQLite
- Test snapshots

Captured stderr is bounded and redacted before it reaches the UI.

## Data Flow

1. Selecting an Agent loads the normalized global MCP list from the backend.
2. Opening add or edit loads an editable, redacted draft and its revision token.
3. Saving submits the draft, original name, and revision token.
4. The adapter performs a safe mutation and returns the refreshed list.
5. The UI replaces the list from the backend response.
6. Connection testing resolves the raw config only inside the backend and stores a non-sensitive result summary.

## Testing

### Rust

Adapter fixtures cover all six formats:

- List
- Create
- Edit
- Rename
- Enable and disable
- Delete
- Unknown-field preservation
- Codex comment and unrelated-table preservation
- Revision conflict
- Failed-write rollback
- Disabled-store persistence
- Permission enforcement

Connection tests use local fake stdio, HTTP, and SSE servers and cover:

- Successful initialization
- Tool discovery
- No Tools capability
- Timeout
- Early process exit
- Protocol mismatch
- HTTP authentication required
- Child-process cleanup
- Secret redaction

### Frontend

Vitest coverage includes:

- Empty and read-only states
- Transport-specific form fields
- Argument and key/value editing
- Secret placeholders
- Static validation errors
- Rename behavior
- Toggle rollback
- Connection-test states
- Revision-conflict messaging
- Delete confirmation

All new UI copy is added to the five locale files.

### Required checks

- `pnpm lint`
- `pnpm test:run`
- `pnpm build`
- `cargo check --manifest-path src-tauri/Cargo.toml`

## Out of Scope

- Project-level MCP mutations
- MCP marketplace and one-click install
- Cross-Agent MCP copying or synchronization
- AgentBro-managed OAuth
- Editing plugin-provided MCP servers
- Calling MCP tools during connection testing

## References

- [Claude Code MCP](https://code.claude.com/docs/en/mcp)
- [Codex MCP](https://developers.openai.com/codex/mcp/)
- [Gemini CLI MCP](https://google-gemini.github.io/gemini-cli/docs/tools/mcp-server.html)
- [Cursor MCP](https://docs.cursor.com/context/model-context-protocol)
- [Kimi Code MCP](https://www.kimi.com/code/docs/en/kimi-code-cli/customization/mcp.html)
- [ZCode MCP](https://zcode.z.ai/cn/docs/mcp-services)
- [MCP lifecycle](https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle)
- [MCP transports](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports)
- [MCP tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)
