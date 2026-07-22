<div align="center">
  <img src="public/agentbro-logo.png" alt="AgentBro Logo" width="148" />

  <h1>AgentBro</h1>

  <p><strong>Make Agents Easier to Use</strong></p>

  <p>
    A native macOS Dynamic Island for AI coding agents.<br />
    Bring permissions, questions, plans, quick replies, remote sessions, tool activity, completions, agent installs, hooks, and skills management into one lightweight desktop workspace.
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
    <img alt="Platform" src="https://img.shields.io/badge/platform-macOS-f5b84b" />
    <img alt="Built with Tauri" src="https://img.shields.io/badge/Tauri-React%20%2B%20Rust-0c6b63" />
  </p>

  <p>
    <strong>A local control center for AI coding agents: Claude Code, Codex, Gemini CLI, Cursor, Copilot, Kimi, Qoder, OpenCode, and more.</strong>
  </p>
</div>

<img src="docs/assets/screenshots/island-expanded.png" alt="AgentBro expanded Dynamic Island" width="100%" />

## Quick Start

```bash
brew tap shirenchuang/tap && brew install --cask agentbro
```

- Download: [`GitHub Releases`](https://github.com/shirenchuang/agentbro/releases) or the [latest China mirror DMG](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg).
- Connect an agent: open **Island -> Integration**, run **Hook Doctor**, then install the hook for the agent you use.
- Contribute: start with [`good first issue`](https://github.com/shirenchuang/agentbro/issues?q=is%3Aissue%20is%3Aopen%20label%3A%22good%20first%20issue%22) or [`help wanted`](https://github.com/shirenchuang/agentbro/issues?q=is%3Aissue%20is%3Aopen%20label%3A%22help%20wanted%22).

If AgentBro saves you from bouncing between terminals, editors, and approval prompts, a star helps more AI coding agent users discover it.

## What Is AgentBro?

AgentBro is a native macOS app that floats above your editor and terminal. It watches active sessions from AI coding agents such as Claude Code, Codex, and Gemini CLI, then collects the flow-breaking moments into a small Dynamic Island. You can approve permissions, answer questions, send quick replies, and forward agent events from remote SSH machines back to your local desktop.

Beyond the runtime island, AgentBro now includes an **Agent Management** workspace. It scans local CLIs and desktop agents, then gives you one local-first control panel for install state, versions, hooks, skills, MCP servers, plugins, and paths.

## Logo Meaning

The center of the AgentBro logo is shaped like a handshake. It represents the collaboration between humans and AI agents: not replacement, not remote control, but a bro-like companion that helps, nudges, and catches the moments that need attention. The outer `A` / `B` structure comes from the AgentBro initials and also resembles two connected agent nodes.

## Demo Videos

### Interaction Demo

https://github.com/user-attachments/assets/df857822-ea0a-4745-a0b9-80f265f30dc6

### Theme Demo

https://github.com/user-attachments/assets/374d6e53-c126-41be-a593-4e5f63485602

## Supported Themes

| Theme | ID | Style |
| --- | --- | --- |
| Midnight | `midnight` | Default dark theme for long coding sessions and low-light environments. |
| AgentBro Classic | `ink-amber` | Warm brand theme with ink and amber contrast. |
| Frosted Glass | `frosted-glass` | Light glass-style theme for bright desktops. |
| Apple | `apple` | Clean macOS-style theme with a native, low-distraction feel. |
| Smoke | `smoke` | Neutral light theme for calmer continuous monitoring. |
| Ocean Mist | `ocean-mist` | Cool light theme with blue accents for state and actions. |
| Warm Paper | `warm-paper` | Warm paper-like theme for softer desktop setups. |
| Soft Lavender | `soft-lavender` | Gentle lavender theme with a lighter, lower-contrast feel. |
| System | `system` | Follows the system light / dark appearance automatically. |

## Main Features

| Feature | Description |
| --- | --- |
| Dynamic Island | Compact, hover, expanded, and detail views for active agent sessions. |
| Instant actions | Handle permission requests, questions, plan approvals, completions, and response cards in the island. |
| Quick replies | Type a message directly in the island without switching back to the terminal. |
| Task awareness | Show tool activity, subagent progress, task summaries, and token/rate-limit data where supported. |
| Agent management | Scan local AI coding tools and review install state, versions, paths, hooks, skills, MCP servers, and plugins. |
| Skill center | Adopt skills scattered across agent folders, then distribute them to agents or projects by symlink or copy. |
| Pet mode | Switch the island into a pet status panel whose vitals react to context pressure and token usage. |
| Pet Market | Browse community pets and install them with one click, powered by the abpets CLI. See [www.agentbro.net/pets](https://www.agentbro.net/pets). |
| Hook integration | One-click hook installation, per-agent hook state and event toggles, plus Hook Doctor diagnostics. |
| Desktop controls | Global shortcuts, sounds, notifications, themes, display placement, and terminal-focus suppression. |
| Local-first | The hook server runs locally through `/tmp/agentbro.sock` or `127.0.0.1:17892`. |
| SSH Remote | Forward agent events from remote SSH machines back to your local island for remote development. |
| Webhook notifications | Send notifications to DingTalk / Feishu webhooks. |

## Pet Market

Beyond the island, AgentBro can switch the floating window into a **pet status panel**: a desktop pet follows your active agent, and its vitals react in real time to context pressure and token usage — so you can tell at a glance whether a session is relaxed or under strain.

The **Pet Market** lets you browse community-contributed pets and install them with one click, all driven by the [`abpets`](https://www.npmjs.com/package/abpets) CLI (Node.js v18+). Open it from **Island -> Pet Market** in settings, or preview every pet on the web:

👉 **[www.agentbro.net/pets](https://www.agentbro.net/pets)**

Want to author your own pet? Use the [`shirenchuang/agentbro-pet`](https://github.com/shirenchuang/agentbro-pet) skill to turn a character concept, brand cue, or reference image into an AgentBro-ready `pet.json` + `spritesheet.webp` package with pluggable image-generation backends. Install it with `npx skills add https://github.com/shirenchuang/agentbro-pet.git` or clone it directly; Codex, Claude Code, Cursor, Gemini CLI, and any other agent that can run scripts and generate images can use the workflow.

<img src="https://github.com/user-attachments/assets/53a17db6-54c4-40f1-95b6-89a7f1977f00" alt="AgentBro pet mode" width="100%" />

<img src="https://github.com/user-attachments/assets/efd1acc8-67bb-460f-b7c9-3faa490611f5" alt="AgentBro Pet Market" width="100%" />

## Agent Management

If you use Claude Code, Codex, Gemini CLI, Cursor, Kimi, Doubao, Qoder, OpenCode, and other tools side by side, AgentBro brings their installs, integrations, capability packs, and local config into one workspace. Open **Agent Management** in settings to access the Skill Library, Skill Install, Skill Packs, Projects, Agent Management, Diagnostics, and Settings pages.

- Installs and versions: detect whether each CLI or desktop app is available, show current/latest versions, executable paths, config directories, and official install pages; supported CLIs can be installed or updated directly, while desktop apps open their download page.
- Hook integration: install or remove hooks per agent, inspect bridge commands and config paths, and toggle event groups such as approvals, notifications, lifecycle, and activity.
- Skill center: scan each agent's skills folder, adopt unmanaged skills into the center library, then distribute them to selected agents by symlink or copy.
- Skill packs: group skills into reusable packs, apply or revoke them from an agent detail page, and resolve conflicts by overwriting, skipping, or keeping the agent copy.
- MCP, plugins, and paths: review MCP servers, plugins, config files, skills folders, and health state in one place; the Projects page can also import repos and inspect project-level instructions and agent config.

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

## Supported Agents

AgentBro supports agents at two levels: runtime hook adapters send session events into the island, while Agent Management scans a wider set of CLIs/apps, skills, MCP servers, plugins, and paths.

| Scope | Agents |
| --- | --- |
| Dynamic Island / hook integration | Claude Code, Codex, Gemini CLI, Cursor / Cursor CLI, GitHub Copilot, Cline, Qoder / Qoder CLI, CodeBuddy / CodeBuddy CN, Qwen, Kimi, DeepSeek, OpenCode, Factory Droid, StepFun, AntiGravity, WorkBuddy, Hermes, Pi, Kiro |
| Agent Management scan | Everything above, plus Doubao, the `.agents` shared folder, Junie, Windsurf, Augment, KiloCode, OB1, Amp, Aider, OpenClaw / QClaw / EasyClaw / AutoClaw, and custom agents |
| Project-level scan | Currently focused on common Claude Code and Codex project config: project-level skills, MCP servers, plugins, and instruction files |

Doubao support on macOS detects `/Applications/Doubao.app`, manages `~/Doubao/skills`, and continues to cover Doubao's compatible `~/.agents/skills` through the central library. Doubao does not currently expose a public hook, so island activity is a best-effort inference from local processes and read-only metadata for two task-state directories; conversation contents are never read, and page synchronization can cause a brief false positive.

## Roadmap

AgentBro stays local-first. The current public release focuses on making the island, Agent Management, Skill center, hook integration, quick actions, and SSH Remote reliable. Future directions we want to explore include:

- Remote sync: sync settings, hooks, themes, prompts, skills, and remote host configuration across devices.
- Skills community: discover, install, share, and update Skill Packs for different agents.
- Pet ecosystem: ship more community pets, grow the Pet Market, and open up authoring and sharing of custom pets.
- Team collaboration: shared configuration, team Skill Packs, access control, and clearer collaboration views.

## Join The Community

If you use AgentBro or want to discuss upcoming Windows support, deeper agent integrations, Agent Monitor, Agent Switch, or the skills community, scan the QR code to add the maintainer on WeChat (mention **AgentBro community**), or join the **AgentBro Open Source Community** group chat directly.

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

## Platform Support

AgentBro is currently developed, tested, and released for **macOS** first.

Windows support is planned. The Tauri + React + Rust foundation is portable, but a good Windows release still needs dedicated work for floating window behavior, tray integration, shortcuts, terminal/editor focus detection, hook paths, installers, signing, and release automation.

Linux support is possible later, but it is not part of the first public release target.

## Installation

### Homebrew Cask

One-line install:

```bash
brew tap shirenchuang/tap && brew install --cask agentbro
```

Step-by-step install:

```bash
brew tap shirenchuang/tap
brew install --cask agentbro
```

### Download a Release

- 🌍 [GitHub Releases](https://github.com/shirenchuang/agentbro/releases) (all versions)
- 🇨🇳 Mainland China mirror (faster): [latest DMG](https://agentbro.oss-cn-hangzhou.aliyuncs.com/AgentBro_latest_universal.dmg)

## Local Development

### Prerequisites

- macOS
- Node.js
- pnpm
- Rust toolchain + Cargo
- Tauri CLI: `cargo tauri --version`

### Start The Project

```bash
git clone https://github.com/shirenchuang/agentbro.git
cd agentbro
pnpm install
pnpm tauri:dev
```

`pnpm tauri:dev` starts the Vite dev server on `http://localhost:1423` and opens the native AgentBro windows.

### Browser-Only UI Development

```bash
pnpm dev
```

Open:

- Island UI: `http://localhost:1423`
- Settings UI: `http://localhost:1423/#settings`

The browser development view includes the Claude Hook UI Lab for testing static island states such as permission requests, plan approval, questions, completion, compact mode, list mode, and detail mode.

### Common Commands

```bash
pnpm test:run      # Run tests once
pnpm test          # Run tests in watch mode
pnpm lint          # ESLint
pnpm build         # Type-check and build frontend
cargo check        # Check Rust backend
pnpm tauri:build   # Build the Tauri app
./build.sh         # Build universal macOS DMG
```

## Use With An Agent

1. Open AgentBro settings.
2. If you only want the island integration, go to **Island -> Integration** and run **Hook Doctor**.
3. Click **Install All Hooks**, or install the hook for the agent you use.
4. If you want unified agent, skills, MCP, and plugin management, open **Agent Management**, then choose the **Agent Management** page.
5. Select an agent to install, update, or open its install page; use the **Hooks** tab for hook setup and the **Skills** tab to scan, adopt, distribute, or remove skills.
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
