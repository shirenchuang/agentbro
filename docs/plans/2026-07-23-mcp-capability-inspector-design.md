# MCP Capability Inspector Design

Date: 2026-07-23

## Goal

Upgrade AgentBro's explicit MCP connection test into a read-only capability inspector for:

- Claude Code
- Codex
- Gemini CLI
- Cursor
- Kimi Code
- ZCode

The inspector explains whether a configured server can connect, what capabilities it exposes, where an inspection failed, and what the user can try next. It never invokes tools or reads resource and prompt content.

## Scope

The first release includes:

- An inspector drawer opened from an MCP server row
- Connection and protocol negotiation details
- Read-only discovery of tools, resources, and prompts
- Tool input-schema summaries and declared risk annotations
- Step-level timing, sanitized diagnostics, and actionable suggestions
- Partial results when only one discovery request fails
- Explicit re-inspection and safe cancellation

The first release does not include:

- Calling MCP tools
- Reading resource contents
- Rendering prompt contents
- Sampling or elicitation
- Background health checks
- Persistent inspection history
- Cross-Agent MCP synchronization
- AgentBro-managed OAuth

## User Experience

The existing "Test connection" action becomes "Inspect." Selecting it opens an opaque right-side drawer approximately 720 pixels wide and immediately begins a read-only inspection.

The drawer header shows:

- MCP server name
- Agent name and transport
- Overall status
- Total duration
- Negotiated protocol version
- Re-inspect action

The body uses five tabs.

### Overview

Overview displays:

- Connected, partial, failed, or cancelled status
- Server name and version
- Transport
- Protocol version
- Total duration
- Tool, resource, and prompt counts
- Inspection timestamp
- Warnings and recommended actions

### Tools

Tools displays:

- Name and title
- Description
- Input-schema property summary
- Read-only, destructive, idempotent, and open-world annotations
- A warning when annotations are missing

Annotations are treated as untrusted metadata. AgentBro uses them to explain likely risk, not to enforce safety guarantees.

### Resources

Resources displays:

- URI
- Name and title
- Description
- MIME type
- Size when supplied

The inspector may call `resources/list` but never `resources/read`.

### Prompts

Prompts displays:

- Name and title
- Description
- Argument names
- Required state

The inspector may call `prompts/list` but never `prompts/get`.

### Logs

Logs is a structured sequence rather than raw protocol traffic. Each step contains:

- Phase
- Status
- Duration
- Sanitized message

Phases are connect or start, initialize, initialized notification, tools discovery, resources discovery, prompts discovery, and shutdown.

Raw environment variables, authorization headers, JSON-RPC payloads, and resource or prompt contents are not shown.

## List Integration

The MCP list remains compact.

After an inspection, the row shows a small session-only summary such as:

- `Healthy · 3 tools`
- `Partial · Resources failed`
- `Authentication required`

Closing and reopening the Agent Management page does not preserve the report. Opening the inspector starts a fresh inspection so the information does not appear current when it is stale.

## Protocol Flow

The backend resolves the raw server configuration and opens one connection using the configured transport.

1. Start a stdio process or connect to an HTTP/SSE endpoint.
2. Send `initialize`.
3. Record the negotiated protocol version, server identity, and declared capabilities.
4. Send `notifications/initialized`.
5. Call `tools/list` only when Tools are declared.
6. Call `resources/list` only when Resources are declared.
7. Call `prompts/list` only when Prompts are declared.
8. Follow pagination cursors up to bounded item and page limits.
9. Close the HTTP session or terminate the stdio process group.

The inspector must not send:

- `tools/call`
- `resources/read`
- `prompts/get`
- `sampling/createMessage`
- Elicitation requests
- Any request not required for initialization or capability discovery

## Data Model

The backend returns a normalized `McpInspectionReport`.

```text
McpInspectionReport
  inspectionId
  status
  category
  summary
  startedAt
  durationMs
  protocolVersion
  serverName
  serverVersion
  transport
  capabilities
  tools[]
  resources[]
  prompts[]
  steps[]
  warnings[]
  suggestions[]
```

Status is one of:

- `connected`
- `partial`
- `failed`
- `cancelled`

Each discovery item retains only the metadata required by the UI. Unknown fields are not forwarded.

## Partial Failure Semantics

Initialization failure makes the overall inspection fail.

After initialization:

- One failed discovery request produces `partial`.
- Unsupported capabilities are not failures.
- A malformed item is skipped and reported as a warning.
- A pagination failure retains the pages already received.
- Shutdown failure is a warning unless it indicates that a process could not be terminated.

This separates server health from discovery completeness.

## Timeouts and Limits

- Total inspection timeout: 20 seconds
- Per-step timeout: 5 seconds
- Maximum pages per capability: 10
- Maximum items per capability: 500
- Maximum sanitized diagnostic message: 1,600 characters
- Maximum captured stdio stderr: 8 KiB

When a limit is reached, the report is partial and explains which limit was applied.

## Cancellation

The frontend creates an inspection ID before invoking the backend.

The backend registers a cancellation signal for that ID. Closing the drawer or selecting a different Agent invokes a cancellation command. Cancellation drops pending network work, closes the HTTP session when possible, and terminates a spawned stdio process group.

Cancellation is idempotent. A late result for a closed or replaced drawer is ignored by the frontend.

## Security

- Sensitive environment and header values are never returned to the frontend.
- Diagnostic messages pass through the existing secret redactor.
- Header values and environment values are not included in structured steps.
- HTTP authorization discovery is not followed in this release.
- HTTP 401 and 403 responses report authentication requirements without exposing response bodies.
- Plain HTTP is allowed only as currently supported and is highlighted for non-loopback endpoints.
- Tool annotations are displayed as untrusted hints.
- Disabled MCP servers require an explicit notice because inspection temporarily starts or connects to them.

## Architecture

### Rust

Extend the dedicated MCP management module with:

- Normalized inspection DTOs
- A shared protocol-session abstraction for stdio, Streamable HTTP, and legacy SSE
- Bounded list discovery and pagination
- Structured step collection
- Cancellation registration and cleanup
- Compatibility mapping from the existing connection-test result

The existing connection-test command can remain temporarily for compatibility, but the MCP management UI uses the inspector command.

### Frontend

Add a dedicated `McpInspectorDrawer` component responsible for:

- Starting and cancelling inspections
- Loading, failure, partial, and success states
- Tab navigation
- Capability list rendering
- Structured diagnostic steps
- Re-inspection

`McpManagementTab` owns the selected server and session-only result summaries.

### Tauri API

Add commands for:

- `inspect_mcp_server_cmd`
- `cancel_mcp_inspection_cmd`

The frontend wrapper creates opaque inspection IDs and guards against late responses.

## Error Guidance

Known categories map to concise suggestions:

- Command not found: verify the command path or install the package runtime.
- Startup failed: inspect arguments, working directory, permissions, and sanitized stderr.
- Timeout: verify that the server writes MCP messages to stdout and does not wait for interactive input.
- Authentication required: authenticate through the target Agent or configure the required header.
- Network error: verify URL, proxy, DNS, TLS, and local firewall.
- Protocol incompatible: update the MCP server or target Agent.
- Invalid configuration: return to Edit and correct the highlighted configuration.

Unknown failures preserve a sanitized message and recommend re-inspection or checking the server's own logs.

## Testing

### Rust

Tests cover:

- Tools, resources, and prompts discovery
- Missing capabilities
- Pagination
- Partial discovery failure
- Item and page limits
- Timeout
- Cancellation and process cleanup
- HTTP authentication failures
- Protocol fallback
- Secret redaction
- Tool annotation normalization

### Frontend

Tests cover:

- Opening the inspector and starting an inspection
- Loading and cancellation
- Overview rendering
- Tools, resources, and prompts tabs
- Empty unsupported capability states
- Partial failure warnings
- Risk annotation labels
- Re-inspection
- Ignoring stale results
- Session-only row summaries

All copy is added to English, Chinese, Japanese, Korean, and Turkish locale files.

### Required Checks

- `pnpm lint`
- `pnpm test:run`
- `pnpm build`
- `cargo check --manifest-path src-tauri/Cargo.toml`

