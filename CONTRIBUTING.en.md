# Contributing to AgentBro

Thanks for your interest in contributing to AgentBro 🙌 Bug fixes, feature ideas, new Agent adapters, translations — all welcome.

> This document is for **all contributors**, including those working with AI assistants. AI agents should also read [`AGENTS.md`](AGENTS.md) (or [`.claude/CLAUDE.md`](.claude/CLAUDE.md) for Claude Code) first to pick up the project map.
> 中文版: [`CONTRIBUTING.md`](CONTRIBUTING.md).

---

## Table of contents

1. [Code of Conduct](#code-of-conduct)
2. [Before opening an issue](#before-opening-an-issue)
3. [Local development](#local-development)
4. [Branching & commit conventions](#branching--commit-conventions)
5. [Pull request flow](#pull-request-flow)
6. [Common contribution recipes](#common-contribution-recipes)
7. [Working with AI assistants](#working-with-ai-assistants)
8. [Brand & trademark](#brand--trademark)
9. [Community](#community)

---

## Code of Conduct

This project adopts the [Contributor Covenant v2.1](CODE_OF_CONDUCT.md). The short version: **be respectful to people, be honest about technical reality**.

---

## Before opening an issue

1. **Search first.** Check the [issue tracker](https://github.com/shirenchuang/agentbro/issues?q=is%3Aissue) for duplicates.
2. **Reproduce on the latest release.** Older versions often have the bug already fixed.
3. **Run Hook Doctor.** Settings → Island → Integration → Run Hook Doctor. Most Hook / permission issues surface here.
4. **Use the right template.** Bug, Feature, and Agent-request templates exist. Blank issues are disabled.

---

## Local development

### Prerequisites

- macOS (macOS is the only supported platform today)
- Node.js 20+
- pnpm 9+
- Rust stable + Cargo
- Tauri CLI: `cargo install tauri-cli` or `pnpm dlx @tauri-apps/cli`

### Getting started

```bash
git clone https://github.com/shirenchuang/agentbro.git
cd agentbro
pnpm install
pnpm tauri:dev   # full native app (recommended)
# or
pnpm dev         # browser UI only — http://localhost:1423
```

`pnpm tauri:dev` first runs `cargo build` for the `agentbro-bridge` binary, then starts Vite + the native window. First build is slow (5-10 min for Rust deps); incremental builds are quick.

### Run before every commit

```bash
pnpm lint
pnpm test:run
pnpm build
cargo check --manifest-path src-tauri/Cargo.toml
```

Claude Code users can run `/check` to chain all four.

---

## Branching & commit conventions

### Branches

- `main` — release branch, only maintainers merge release commits here
- `dev` — **integration branch; all PRs target `dev`**
- Feature branches: `feat/<short-name>`, `fix/<short-name>`, `docs/<short-name>`

### Commit messages

[Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add Foo Agent hook adapter
fix(notch): right-side icon overflows in compact mode
docs: correct theme table in README
refactor(agents): extract shared hook event parser
test: edge case for codex parse_event
chore: bump vitest to 4.2
```

One commit, one logical change. PRs that include "while I was here, I cleaned up X" will be asked to split.

### Version numbers

**Do NOT bump versions in PRs.** Maintainers update versions at release time, and they must stay in sync across four files:

- `package.json`
- `src-tauri/tauri.conf.json`
- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`

`pnpm release:check` enforces this; CI will fail on drift.

---

## Pull request flow

1. Create or claim an Issue that matches the scope of the change.
2. Fork → branch off `dev` → make your changes → run the four checks above.
3. Open the PR against `dev`. Title should clearly state what changed (English or Chinese).
4. Fill in the PR template and keep `Closes #<issue-number>` in the body; `PR policy / Issue link` validates it.
5. CI runs `ci.yml` (lint + test + cargo check/clippy/fmt + cargo test) and `build.yml` (macOS dual-arch build). Both must be green.
6. Review cadence: first response usually within 48 business hours. If a week passes with no reply, feel free to nudge maintainers in the PR.
7. Merge is squash. Repository members may enable auto-merge in advance; the PR merges and deletes its task branch after every required check passes. `Close linked Issues` closes Issues referenced in the body after the PR merges into `dev`.

---

## Common contribution recipes

### Bug fix

- Reproduce in an issue → add a failing test (vitest in `src/test/` or `#[cfg(test)] mod tests` inside the affected Rust file) → fix until it passes.

### New Agent adapter

Step-by-step guide: [`.claude/commands/add-agent.md`](.claude/commands/add-agent.md) (human-readable too — you don't need Claude Code to follow it). Short version:

1. `src-tauri/src/agents/<name>.rs`: implement the `AgentAdapter` trait (model after `kimi.rs`).
2. `src-tauri/src/agents/mod.rs`: register in `all_adapters()` and `impl_default_adapter!`.
3. `src-tauri/src/agents/profiles.rs`: add `<name>_profile()` and wire it into `profile_for_agent()`.
4. Add tests for `parse_event`.
5. Update icon mapping in `src/components/notch/AgentIcon.tsx`, display names in `src/i18n/locales/*.json`, and the README support table.

### New theme

Add the theme in `src/themes/` → update the theme table in both READMEs → add translations to `src/i18n/locales/*.json`.

### New translation

`src/i18n/locales/{en,zh,ja,ko,tr}.json` — **all five files must add the key together**. Missing keys fall back to English, which gives an inconsistent experience.

### Docs

Edit `README.md` / `README.en.md` / `docs/*.md` directly. Don't add standalone README files; improving the existing ones is preferred.

---

## Working with AI assistants

We encourage AI-assisted contributions — AgentBro is itself a tool for AI coding agents, so the philosophy is consistent.

- **Use the project-level config.** Claude Code auto-loads [`.claude/CLAUDE.md`](.claude/CLAUDE.md). Codex / Cursor / Aider / Copilot / Gemini CLI etc. read [`AGENTS.md`](AGENTS.md). Have your assistant read these *before* it starts editing — it saves a lot of misguided exploration.
- **Create an Issue before code changes.** The assistant must create or link an Issue before editing, use a dedicated task branch, and close the loop with `Closes #<number>` in the PR. Pure questions and read-only analysis do not need an Issue.
- **Disclose AI authorship honestly.** If a PR is mostly AI-generated, add a line like "Co-authored with <Agent name>" to the description. We don't discriminate — we just expect honesty.
- **AI-generated code still has to pass the checks.** `pnpm lint`, `pnpm test:run`, `cargo check` must be green before you open the PR. PRs where the assistant skipped tests / ignored errors / didn't verify will be sent back.
- **Don't let the AI touch brand assets, signing config, or the release pipeline.** These have trademark and security implications. See the [restricted areas list](.claude/CLAUDE.md#6-禁区不要动).
- **For non-trivial changes** (architecture changes, new dependencies, edits spanning 5+ files), align with maintainers in an issue or the PR description *before* the AI starts implementing. Don't drop a 200-line AI-generated diff out of nowhere.

---

## Brand & trademark

AgentBro **code** is licensed under [Apache License 2.0](LICENSE). But the **name "AgentBro", the logo, the app icon, and the official website assets** are NOT covered by the code license.

If you fork:
- Personal use, experiments, upstream PRs — fine.
- Distributing your own build — **you must rename it**. Don't let users think your version is the official AgentBro.

Details in [`TRADEMARKS.md`](TRADEMARKS.md) and [`NOTICE`](NOTICE).

---

## Community

- WeChat group: scan the QR in [README](README.md#加入交流群), note **AgentBro 交流群**.
- Releases: https://github.com/shirenchuang/agentbro/releases
- Website: https://www.agentbro.net

For larger ideas, open an issue first as a discussion thread before you implement — it's much friendlier than getting a finished PR sent back for redesign.
