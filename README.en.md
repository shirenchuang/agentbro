<div align="center">
  <img src="public/agentbro-logo.png" alt="AgentBro Logo" width="148" />

  <h1>AgentBro</h1>

  <p><strong>Your desktop control center for AI coding agents</strong></p>

  <p>
    Spend less time watching terminals and switching windows.<br />
    Handle agent sessions, approvals, and questions from a floating workspace, then manage hooks, skills, MCP servers, plugins, API providers, and remote hosts in the same app.
  </p>

  <p>
    <a href="https://www.agentbro.net">Website</a>
    ·
    <a href="https://github.com/shirenchuang/agentbro/releases">Download</a>
    ·
    <a href="docs/privacy-policy.md">Privacy</a>
    ·
    <a href="README.md">中文</a>
  </p>

  <p>
    <img alt="License" src="https://img.shields.io/badge/license-Apache--2.0-111820" />
    <img alt="Platform" src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows-f5b84b" />
    <img alt="Release" src="https://img.shields.io/github/v/release/shirenchuang/agentbro?color=0c6b63" />
    <img alt="Built with Tauri" src="https://img.shields.io/badge/Tauri-React%20%2B%20Rust-0c6b63" />
  </p>

  <p>
    <strong>Available for macOS and Windows, with integrations for Claude Code, Codex, Gemini CLI, Cursor, Copilot, Kimi, OpenCode, ZCode, and more.</strong>
  </p>
</div>

<img src="docs/assets/screenshots/island-expanded.png" alt="AgentBro expanded Dynamic Island" width="100%" />

## Download and get started

| Platform | Recommended install | Other package |
| --- | --- | --- |
| macOS | `brew tap shirenchuang/tap && brew install --cask agentbro` | [Universal DMG](https://github.com/shirenchuang/agentbro/releases/latest/download/AgentBro_latest_universal.dmg) · [China mirror](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg) |
| Windows x64 | [Download the EXE installer](https://github.com/shirenchuang/agentbro/releases/latest/download/AgentBro_latest_x64-setup.exe) | [MSI](https://github.com/shirenchuang/agentbro/releases/latest/download/AgentBro_latest_x64.msi) |

After installing, open **Island -> Integration**, run **Hook Doctor**, and install the hook for the agent you use. The Windows build is an early MVP: the floating workspace, hook transport, path detection, and Agent Management work, but unsigned installers may trigger a SmartScreen warning.

## What does AgentBro solve?

AI coding agents can work for long stretches, but people still end up watching the terminal: waiting for permissions, answering questions, checking whether a task is stuck, and maintaining separate hooks, skills, MCP servers, and plugins for every tool. AgentBro puts those jobs behind one desktop entry point.

| When you are... | AgentBro can... |
| --- | --- |
| Running several agent sessions | Collect status, tool calls, subagents, token usage, and completion notices in the island. |
| Waiting on an approval or question | Approve a permission, answer a question, confirm a plan, or send a quick reply without finding the original terminal. |
| Maintaining several agent environments | Scan versions, paths, and hooks, then manage skills, MCP servers, plugins, and config files together. |
| Switching models or API providers | Manage, test, and switch provider configs for Claude, Codex, Gemini, OpenCode, and Hermes. |
| Running agents on a server | Bring remote sessions and hook events back over SSH and diagnose the connection in the same app. |

Agent session events and local configuration do not need a cloud relay. The hook server uses a per-user local Unix socket on macOS and a local TCP endpoint on Windows. Update checks, marketplace downloads, SSH, and webhooks contact their respective services only when you use those features.

## Demo videos

### Interaction demo

https://github.com/user-attachments/assets/df857822-ea0a-4745-a0b9-80f265f30dc6

### Theme demo

https://github.com/user-attachments/assets/374d6e53-c126-41be-a593-4e5f63485602

## Core capabilities

### The island brings attention requests to your desktop

- Sessions have compact, hover, expanded, and detail views. Quiet Assistant mode keeps the island hidden until something needs you.
- Handle permission requests, questions, plan approvals, completions, and errors in the floating window. Supported agents also accept quick replies.
- Tool calls, file diffs, subagents, task summaries, context pressure, tokens, and rate limits update with the session.
- Global shortcuts, sounds, quiet hours, multi-display placement, and terminal-focus suppression keep the window useful without making it noisy.
- Important events can also be forwarded to DingTalk or Feishu webhooks.

<table>
  <tr>
    <td width="50%">
      <img src="docs/assets/screenshots/island-permission.png" alt="Handle a permission request in the AgentBro island" width="100%" />
      <sub>Handle approvals, questions, and plan confirmations without returning to the terminal.</sub>
    </td>
    <td width="50%">
      <img src="docs/assets/screenshots/island-detail.png" alt="AgentBro island session details" width="100%" />
      <sub>Inspect tasks, tool calls, tokens, and session details.</sub>
    </td>
  </tr>
</table>

### Agent Monitor shows what an agent is doing

Agent Monitor collects active and historical sessions. Inspect phases, tool timelines, approvals, questions, conversations, and raw hook events by project. If you manually enable Claude Code network monitoring, the local inspector can also show system prompts, messages, tools, responses, token usage, and KV cache statistics grouped by model and project. Network monitoring is off by default.

## Pet market

Beyond the island, AgentBro can switch the floating window into a **pet status panel**: a desktop pet follows your active agent, and its vitals react in real time to context pressure and token usage — so you can tell at a glance whether a session is relaxed or under strain.

The **Pet Market** lets you browse community-contributed pets and install them with one click, all driven by the [`abpets`](https://www.npmjs.com/package/abpets) CLI (Node.js v18+). Open it from **Island -> Pet Market** in settings, or preview every pet on the web:

👉 **[www.agentbro.net/pets](https://www.agentbro.net/pets)**

Want to author your own pet? Use the [`shirenchuang/agentbro-pet`](https://github.com/shirenchuang/agentbro-pet) skill to turn a character concept, brand cue, or reference image into an AgentBro-ready `pet.json` + `spritesheet.webp` package with pluggable image-generation backends. Install it with `npx skills add https://github.com/shirenchuang/agentbro-pet.git` or clone it directly; Codex, Claude Code, Cursor, Gemini CLI, and any other agent that can run scripts and generate images can use the workflow.

<img src="https://github.com/user-attachments/assets/53a17db6-54c4-40f1-95b6-89a7f1977f00" alt="AgentBro pet mode" width="100%" />

<img src="https://github.com/user-attachments/assets/efd1acc8-67bb-460f-b7c9-3faa490611f5" alt="AgentBro Pet Market" width="100%" />

The island includes Midnight, AgentBro Classic, Frosted Glass, Apple, Smoke, Ocean Mist, Warm Paper, and Soft Lavender themes. It can also follow the system light or dark appearance.

## Agent Management puts every agent capability in one place

If you use Claude Code, Codex, Gemini CLI, Cursor, Kimi, Doubao, Qoder, OpenCode, and other tools side by side, **Agent Management** brings their installs, integrations, and local configuration into one workspace.

- Discover CLIs and desktop apps, with installed and available versions, executables, config directories, and official download pages. Supported CLIs can be installed, updated, or removed in place.
- Install and repair hooks per agent, inspect bridge commands and config paths, and control approval, notification, lifecycle, and activity events separately.
- Scan skills scattered across agent folders, adopt them into a center library, and distribute them to agents or projects by symlink or copy. Batch jobs, conflict decisions, and diagnostics stay visible.
- Group common skills into reusable packs, apply them to several agents, and safely revoke them later.
- Manage stdio, HTTP, and SSE MCP servers. Inspect tools, resources, prompts, and connection logs, with arguments and a risk confirmation before tool calls.
- Browse plugins for Codex, Claude Code, WorkBuddy, ZCode, Kimi, and other supported agents. Search, enable or disable supported plugins, inspect manifests and packaged capabilities, and preview files.
- Edit supported JSON or text config files with validation. The Projects view imports repositories and checks project-level instructions, skills, MCP servers, and plugins.

<table>
  <tr>
    <td width="50%">
      <img src="docs/assets/screenshots/agent-management-skill-library.png" alt="AgentBro Skill Library" width="100%" />
      <sub>Skill Library: review center-library skills, distribution state, and diagnostics.</sub>
    </td>
    <td width="50%">
      <img src="docs/assets/screenshots/agent-management-install-skills.png" alt="AgentBro Install Skills" width="100%" />
      <sub>Install Skills: import from the market, another agent, a local folder, or Git.</sub>
    </td>
  </tr>
  <tr>
    <td width="50%">
      <img src="docs/assets/screenshots/agent-management-skill-packs.png" alt="AgentBro Skill Packs" width="100%" />
      <sub>Skill Packs: apply grouped skills to agents while keeping revocable claims.</sub>
    </td>
    <td width="50%">
      <img src="docs/assets/screenshots/agent-management-agent-detail.png" alt="AgentBro Agent Management Detail" width="100%" />
      <sub>Agent Management: inspect skills, MCP servers, plugins, hooks, and paths per agent.</sub>
    </td>
  </tr>
</table>

## Agent Switch

Agent Switch manages API provider configuration for Claude, Codex, Gemini, OpenCode, and Hermes. Add or duplicate providers, switch the active config, test connectivity and latency, or preview and import existing providers, MCP servers, prompts, and skills from CC Switch.

## SSH Remote

Remote development does not need a separate monitoring setup. AgentBro can import hosts from `~/.ssh/config`, receive remote hook events through an SSH tunnel, show remote sessions locally, install or repair hooks, and run connection diagnostics. Sessions keep their host identity, so local work and tasks from several servers remain easy to distinguish.

## Supported agents

AgentBro supports agents at two levels. Runtime hook adapters send session events into the island, while Agent Management scans a wider set of CLIs, desktop apps, skills, MCP servers, plugins, and paths. Event coverage and interaction depth vary because each agent exposes different hooks.

| Scope | Agents |
| --- | --- |
| Island / hook integration | Claude Code, Codex, Gemini CLI, Cursor / Cursor CLI, GitHub Copilot, Cline, Qoder / Qoder CLI, CodeBuddy / CodeBuddy CN, Qwen, Kimi, DeepSeek, OpenCode, Factory Droid, StepFun, AntiGravity, WorkBuddy, Hermes, Pi, Kiro, ZCode |
| Agent Management scan | Everything above, plus Doubao, the `.agents` shared folder, Junie, Windsurf, Augment, KiloCode, OB1, Amp, Aider, OpenClaw / QClaw / EasyClaw / AutoClaw, and custom agents |
| Project-level scan | Currently focused on common Claude Code and Codex project config: project-level skills, MCP servers, plugins, and instruction files |

Doubao support on macOS detects `/Applications/Doubao.app`, manages `~/Doubao/skills`, and continues to cover Doubao's compatible `~/.agents/skills` through the central library. Doubao does not currently expose a public hook, so island activity is a best-effort inference from local processes and read-only metadata for two task-state directories; conversation contents are never read, and page synchronization can cause a brief false positive.

## Roadmap

AgentBro will remain local-first. The next priorities include:

- Remote sync: sync settings, hooks, themes, prompts, skills, and remote host configuration across devices.
- Skills community: discover, install, share, and update Skill Packs for different agents.
- Windows: add code signing, automatic updates, and deeper interaction with more agents.
- Pet ecosystem: ship more community pets, grow the Pet Market, and open up authoring and sharing of custom pets.
- Team collaboration: shared configuration, team Skill Packs, access control, and clearer collaboration views.

## Join the community

If you use AgentBro or want to discuss the Windows experience, deeper agent integrations, Agent Monitor, Agent Switch, or the skills community, scan the QR code to add the maintainer on WeChat (mention **AgentBro community**), or join the **AgentBro Open Source Community** group chat directly.

<div align="center">
  <table>
    <tr>
      <td align="center">
        <img src="public/agentbro-wechat-qr.jpg" alt="AgentBro WeChat community QR code" width="260" /><br />
        <sub>Add on WeChat — mention <b>AgentBro community</b></sub>
      </td>
      <td align="center">
        <img src="public/agentbro-group-qr.png" alt="AgentBro Open Source Community group QR code" width="260" /><br />
        <sub>Group chat: <b>AgentBro Open Source Community</b> (QR refreshed every 7 days)</sub>
      </td>
    </tr>
  </table>
</div>

## Platform support

Official releases now include downloadable artifacts for both macOS and Windows:

| Platform | Current status | Distribution |
| --- | --- | --- |
| macOS | Primary development and signed release platform, with the broadest feature coverage | Universal Apple Silicon / Intel DMG, Homebrew Cask, in-app updates |
| Windows x64 | Early MVP with the floating window, TCP hooks, Windows path detection, Agent Management, skills, and installers working | NSIS `.exe`, MSI |
| Linux | No official build | Not in the current release plan |

Windows still needs code signing, a smoother SmartScreen experience, automatic updates, and deeper interaction with some agents. A small number of features, including free-text replies to Codex Desktop, are unavailable because of Windows client API limitations. Session monitoring and basic hook interactions still work.

## Local development

### Prerequisites

- macOS or Windows
- Node.js 20+ and pnpm
- Rust toolchain + Cargo
- Tauri CLI: `cargo tauri --version`
- Xcode Command Line Tools on macOS; Microsoft C++ Build Tools and WebView2 on Windows

### Start the project

```bash
git clone https://github.com/shirenchuang/agentbro.git
cd agentbro
pnpm install
pnpm tauri:dev
```

`pnpm tauri:dev` starts the Vite dev server on `http://localhost:1423` and opens the native AgentBro windows.

### Browser-only UI development

```bash
pnpm dev
```

Open:

- Island UI: `http://localhost:1423`
- Settings UI: `http://localhost:1423/#settings`

The browser development view includes the Claude Hook UI Lab for testing static island states such as permission requests, plan approval, questions, completion, compact mode, list mode, and detail mode.

### Common commands

```bash
pnpm test:run                                      # Run tests once
pnpm test                                          # Run tests in watch mode
pnpm lint                                          # ESLint
pnpm build                                         # Type-check and build frontend
cargo check --manifest-path src-tauri/Cargo.toml   # Check the Rust backend
pnpm tauri:build                                   # Build the macOS app / DMG
pnpm tauri:build:windows                           # Build Windows NSIS / MSI installers
./build.sh                                         # Build the universal macOS DMG
```

## Use with an agent

1. Open AgentBro settings.
2. If you only want the island integration, go to **Island -> Integration** and run **Hook Doctor**.
3. Click **Install All Hooks**, or install the hook for the agent you use.
4. If you want unified agent, skills, MCP, and plugin management, open **Agent Management**, then choose the **Agent Management** page.
5. Select an agent to install or update it, then use the **Hooks**, **Skills**, **MCP**, or **Plugins** pages as needed.
6. Restart the corresponding CLI session, then start Claude Code, Codex, Gemini CLI, or another supported agent.

AgentBro will then show session state, tool activity, approvals, questions, plans, and completions in the island.

## Contributing

Issues and pull requests are welcome!

- Contributing guide: [CONTRIBUTING.en.md](CONTRIBUTING.en.md)
- Code of conduct: [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
- AI agent collaboration guide: [AGENTS.md](AGENTS.md)
- Claude Code project config: [.claude/CLAUDE.md](.claude/CLAUDE.md)
- Community discussions: [GitHub Discussions](https://github.com/shirenchuang/agentbro/discussions)
- Starter tasks: [`good first issue`](https://github.com/shirenchuang/agentbro/issues?q=is%3Aissue%20is%3Aopen%20label%3A%22good%20first%20issue%22) / [`help wanted`](https://github.com/shirenchuang/agentbro/issues?q=is%3Aissue%20is%3Aopen%20label%3A%22help%20wanted%22)

Please target the `dev` branch. Run `pnpm lint && pnpm test:run && pnpm build && cargo check --manifest-path src-tauri/Cargo.toml` before submitting.

## Release

Release notes and signing requirements live in [`docs/release.md`](docs/release.md).

- Website: [www.agentbro.net](https://www.agentbro.net)
- China mirror: `https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg`
- GitHub releases: `https://github.com/shirenchuang/agentbro/releases`

## License

AgentBro source code is licensed under the [Apache License 2.0](LICENSE).

The AgentBro name, logo, app icon, website design, and other brand assets are not licensed with the source code. Modified builds and redistributions should use a different name to avoid confusion with the official project and follow [NOTICE](NOTICE) and [TRADEMARKS.md](TRADEMARKS.md).
