# AGENTS.md

> Project guidance for AI coding agents (Codex / Cursor / Aider / GitHub Copilot / Gemini CLI / …).
> Claude Code users should also read [`.claude/CLAUDE.md`](.claude/CLAUDE.md) for deeper detail.

## What this is

**AgentBro** is a macOS-only Tauri app (Rust backend + React 19 / TypeScript frontend) that surfaces events from AI coding agents (Claude Code, Codex, Gemini CLI, Cursor, Copilot, etc.) into a floating "Dynamic Island" overlay.

## Local commands

```bash
pnpm install                                              # one-time
pnpm dev                                                  # browser UI only at http://localhost:1423
pnpm tauri:dev                                            # full native app
pnpm lint                                                 # ESLint
pnpm test:run                                             # vitest (one-shot)
pnpm build                                                # tsc + vite build
cargo check --manifest-path src-tauri/Cargo.toml          # Rust quick check
cargo test  --manifest-path src-tauri/Cargo.toml          # Rust tests
cargo fmt   --manifest-path src-tauri/Cargo.toml          # Rust format
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

Before any PR, all of these must be green: `pnpm lint`, `pnpm test:run`, `pnpm build`, `cargo check`.

## Mandatory GitHub delivery workflow

Use this workflow for every task that changes repository files, including bug fixes, features, refactors, tests, documentation, and configuration.

Pure questions, explanations, code review, read-only diagnosis, and research do not require an Issue or PR unless they turn into a repository change.

### Before editing

1. Confirm GitHub access with `gh auth status` and inspect the current branch and worktree.
2. Search open Issues for the same scope. Reuse an existing Issue only when it describes the requested work; otherwise create a focused Issue with context, goal, acceptance criteria, and constraints.
3. Start from the latest `origin/dev` on a dedicated branch named `<agent>/issue-<number>-<slug>` (for example, `codex/issue-61-agent-github-workflow`). Never implement on `dev`, `main`, or an unrelated task branch.
4. If another task owns the current branch or worktree, preserve it and use a separate git worktree.
5. Do not edit files until the Issue exists and its number is known.

### Finishing the task

1. Run the required local checks and fix failures before publishing.
2. Review the diff for unrelated edits and secrets, then create a Conventional Commit.
3. Push the task branch and open a PR against `dev`. The PR body must contain `Closes #<issue-number>` so the repository automation can close the Issue after the PR merges.
4. Enable squash auto-merge with branch deletion:

   ```bash
   gh pr merge --auto --squash --delete-branch
   ```

5. Monitor required checks. If a check fails, diagnose it, update the same branch, and leave auto-merge enabled. Never bypass checks, force-push shared branches, or merge a failing PR.
6. After the PR merges, verify that the linked Issue was closed by the `Close linked Issues` workflow. If it remains open, close it with `gh issue close <number> --reason completed` and report the fallback.
7. The task is complete only when the PR is merged and its Issue is closed (or when a concrete external blocker is reported). Report the Issue, PR, checks, and merge result.

Do not create duplicate or empty Issues just to increase activity. The Issue and PR must represent real, reviewable work.

## Where to make changes

| Goal | Files |
| --- | --- |
| Add a new Agent adapter | `src-tauri/src/agents/<name>.rs` + register in `src-tauri/src/agents/mod.rs` (`all_adapters()`, `impl_default_adapter!`) + add profile in `src-tauri/src/agents/profiles.rs`. Simplest reference: `kimi.rs`. |
| Add a Tauri IPC command | `#[tauri::command]` in `src-tauri/src/commands/`, register in `src-tauri/src/lib.rs` `invoke_handler`, wrap in `src/services/tauriApi.ts`, listen via `src/hooks/useTauri.ts` if event-driven |
| Add a frontend component | `src/components/{notch|settings|shared|overlay}/`, plain `.css` + BEM, theme via `var(--*)` CSS variables, state via existing Zustand stores in `src/stores/` |
| Add a translation | `src/i18n/locales/{en,zh,ja,ko,tr}.json` — **all five must be updated together** |
| Add a theme | `src/themes/` + README theme table + i18n names |

Detailed extension recipes: see [`.claude/CLAUDE.md`](.claude/CLAUDE.md) and [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Branching & commits

- Branch off `dev`. **PRs target `dev`**, not `main`.
- [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`.
- Do NOT bump version numbers in PRs. Versions are kept in sync by maintainers across four files (`package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `Cargo.lock`); `pnpm release:check` validates this.

## Do NOT touch

- Brand assets: `public/agentbro-*.{png,jpg}`, `docs/brand/`, `src-tauri/icons/`
- Legal / trademark: `LICENSE`, `NOTICE`, `TRADEMARKS.md`
- Signing / release: `src-tauri/Entitlements.plist`, any `*.key`/`*.p12`/`*.pem`/`*.mobileprovision`, `.github/workflows/release.yml`, `homebrew/`
- Generated: `src-tauri/target/`, `dist/`, `output/`, `node_modules/`

**Never commit secrets** (`.env`, signing keys, API tokens). `.gitignore` covers the common patterns; double-check before staging.

If you fork and redistribute, you **must rename** the product — see [`TRADEMARKS.md`](TRADEMARKS.md).

## Code style

- Default to **no comments**. Only write one when the WHY is non-obvious (hidden constraint, prior bug, surprising behavior). Don't explain WHAT.
- Don't add features, refactor, or introduce abstractions beyond what the task requires.
- Frontend: hooks > class components; no `any`; reuse Zustand stores in `src/stores/`.
- Rust: errors as `Result<T, String>` at the Tauri boundary; `unwrap()` only in tests.
- No new dependencies without discussing in the issue/PR first — extra Tauri crates inflate the binary noticeably.

## Platform support

macOS only for now. Windows is on the roadmap but the codebase is not yet abstracted for it — don't `#[cfg(windows)]`-pepper the code preemptively.
