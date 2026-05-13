# AgentBro

Your AI agents' best bro. One native app to monitor, manage, and supercharge every coding agent you use.

AgentBro is a lightweight Rust-powered desktop companion that sits alongside your AI coding tools — Claude Code, Codex, Gemini CLI, Cursor, Copilot, and 15+ more. It watches sessions in real time, surfaces permission requests and questions without leaving your editor, and gives you a single control plane for all your agents.

## Why AgentBro

- **One app, every agent.** No more juggling terminals. AgentBro connects to 18+ AI coding tools through a unified hook system.
- **Native & lightweight.** Built with Rust and Tauri. Low memory, instant startup, no Electron bloat.
- **Stay in flow.** Approve permissions, answer questions, and read plans from a macOS Dynamic Island-style panel — without switching windows.
- **Full control.** Manage hooks, themes, shortcuts, remote SSH sessions, webhooks, and more from one settings panel.

## Features

### Agent Monitoring
- Real-time session tracking across all connected AI agents
- Compact, hover, and expanded views for active sessions
- Full conversation history from agent JSONL files

### Smart Interaction
- Overlay cards for permission requests, plans, questions, and completions
- One-click approvals and inline text responses
- Smart suppression when the terminal is already focused

### Multi-Agent Management
- Unified hook installation and recovery for all supported agents
- Session grouping by project
- Agent switching (cc-switch) support

### Desktop Integration
- macOS Dynamic Island-style always-on-top panel
- System tray with quick actions
- Sound and haptic feedback for events
- Global keyboard shortcuts
- Theme system with custom color schemes

### Remote & Team
- SSH tunnel support for remote agent sessions
- Webhook notifications to DingTalk, Feishu, Slack, and more
- CI/CD event forwarding

## Supported Agents

AgentBro works with these AI coding tools out of the box:

| Agent | Status |
|-------|--------|
| Claude Code | Full integration |
| Codex | Full integration |
| Gemini CLI | Full integration |
| Cursor / Cursor CLI | Full integration |
| GitHub Copilot | Full integration |
| Trae / Trae CN | Full integration |
| Qoder / Qoder CLI | Supported |
| CodeBuddy / CodeBuddy CN | Supported |
| Qwen | Supported |
| Kimi | Supported |
| OpenCode | Supported |
| Droid / Factory | Supported |
| StepFun | Supported |
| AntiGravity | Supported |
| WorkBuddy | Supported |
| Hermes | Supported |
| Pi | Supported |
| Kiro | Supported |

## Tech Stack

- **Frontend:** React 19, TypeScript, Vite, Framer Motion, Zustand, i18next
- **Desktop shell:** Tauri 2
- **Backend:** Rust, Tokio, notify, rodio
- **Tests:** Vitest + jsdom

## Getting Started

### Prerequisites

- macOS (full desktop experience)
- Node.js + pnpm
- Rust toolchain + Cargo
- Tauri CLI: `cargo tauri --version`

### Install

```bash
pnpm install
```

### Run

```bash
# Full desktop app
pnpm tauri:dev

# Frontend-only browser dev (with mock sessions)
pnpm dev
```

The app starts a Vite dev server on `http://localhost:1423`, then launches the native AgentBro window.

Browser mode: open `http://localhost:1423` for the notch view, `http://localhost:1423/#settings` for settings. `Cmd+,` toggles between views.

## How It Works

1. A supported AI agent runs a hook script.
2. The hook sends a JSON event to AgentBro via `/tmp/agentbro.sock` or `127.0.0.1:17892`.
3. The Rust backend parses the event through agent adapters and updates the session store.
4. Tauri emits events to the React frontend.
5. The Dynamic Island panel renders status, overlays, and chat views.
6. For blocking requests, the UI sends responses back through the hook server to the agent.

## Commands

```bash
pnpm test:run      # Run tests once
pnpm test          # Run tests in watch mode
pnpm build         # Type-check and build frontend
pnpm lint          # ESLint
cargo check        # Check Rust backend
pnpm tauri:build   # Build the app
./build.sh         # Build universal macOS DMG
```

## Project Layout

```text
.
├── src/                         # React frontend
│   ├── components/notch/        # Dynamic Island panel UI
│   ├── components/overlay/      # Permission/question/plan cards
│   ├── components/settings/     # Settings app
│   ├── stores/                  # Zustand state management
│   ├── services/                # Tauri IPC wrappers
│   ├── hooks/                   # React hooks for Tauri events
│   ├── i18n/                    # Internationalization (en/zh/ja/ko/tr)
│   └── themes/                  # Built-in theme bundles
├── src-tauri/                   # Rust backend
│   ├── src/lib.rs               # App bootstrap and service init
│   ├── src/hooks/               # Hook server, file watcher, recovery
│   ├── src/agents/              # Agent adapters and hook management
│   ├── src/commands/            # Tauri IPC commands
│   ├── src/platform/            # macOS display/notification helpers
│   ├── src/terminal/            # Terminal focus and jump helpers
│   ├── src/remote/              # SSH remote support
│   ├── src/sound/               # Sound engine
│   └── src/theme/               # Theme scanning/import
└── public/                      # Static assets
```

## License

MIT

## Links

- Website: [agentbro.cn](https://agentbro.cn)
