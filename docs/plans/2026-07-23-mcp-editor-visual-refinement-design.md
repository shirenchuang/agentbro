# MCP Editor Visual Refinement

Date: 2026-07-23

## Goal

Make the MCP editor feel like a focused native configuration surface rather than a translucent form floating over the Agent list.

The work is visual and interaction-focused. It does not change MCP persistence, validation, or connection testing.

## Direction

Keep AgentBro's drawer interaction and blue accent, while adopting the strongest hierarchy cues from Codex's MCP settings:

- Opaque surfaces
- One clear configuration container
- Quiet borders instead of heavy shadows
- Full-width add-row actions
- A stable footer for save and cancel

The Codex page structure is not copied. AgentBro keeps the faster in-context drawer workflow.

## Visual System

### Color

- Canvas: `#FFFFFF`
- Subtle panel: `#F7F8FA`
- Border: `#E5E7EB`
- Primary text: `#17191F`
- Secondary text: `#6F7785`
- AgentBro accent: `#0A84FF`

All editor surfaces are opaque. The page behind the drawer may retain a restrained dim overlay, but no underlying content should show through the drawer itself.

### Type

- Form and action text: the existing macOS system UI stack
- Commands and paths: the existing monospace utility stack
- Labels use compact semibold text; helper content uses quieter secondary text

### Layout

The editor uses a solid header, a scrollable content area, and a sticky footer.

```text
┌────────────────────────────────────┐
│ Edit MCP                           │
│ Codex                           ×  │
├────────────────────────────────────┤
│ Name                               │
│ [ computer-use                   ] │
│                                    │
│ Transport                          │
│ [ Local command | HTTP | SSE ]     │
│                                    │
│ ┌ Configuration ────────────────┐  │
│ │ Command                       │  │
│ │ [ ...                       ] │  │
│ │                              │  │
│ │ Arguments                    │  │
│ │ [ mcp                    🗑 ] │  │
│ │ [ + Add argument           ] │  │
│ │                              │  │
│ │ Working directory            │  │
│ │ [ .                         ] │  │
│ │                              │  │
│ │ Environment variables        │  │
│ │ [ + Add environment var    ] │  │
│ └──────────────────────────────┘  │
│                                    │
│ Sensitive values remain hidden     │
├────────────────────────────────────┤
│                    Cancel  Save     │
└────────────────────────────────────┘
```

## Signature Element

Replace the native transport select with a three-option transport rail. The control communicates MCP's three connection modes directly and removes platform-dependent select styling.

This is the one visually distinctive choice. The rest of the editor remains quiet and disciplined.

## Interaction

- Transport options are keyboard-accessible radio-style buttons.
- Unsupported transports are omitted as they are today.
- Add argument and add environment/header actions span the configuration card width.
- Delete actions use quiet icon buttons and become red only on hover or focus.
- Save and cancel remain visible in a sticky opaque footer.
- Existing busy, error, validation, and secret-preservation behavior remains unchanged.

## Responsive Behavior

- The drawer remains at its current balanced width on desktop.
- On narrow windows, the transport rail remains a single row while key/value pairs stack.
- The body scrolls independently and the footer remains visible.
- Keyboard focus states and reduced-motion behavior remain supported.

## Verification

- Update component tests to select transports through the new control.
- Verify secret placeholders and save payloads are unchanged.
- Run lint, frontend tests, production build, and Rust checks required by the repository.
