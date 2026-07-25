---
name: agentbro-release
description: Use when releasing AgentBro from this repository: merging dev/main, bumping versions, updating release notes, tagging, pushing, monitoring GitHub Actions, Homebrew cask publication, or fixing a bad release.
---

# AgentBro Release

Use this repository-scoped skill for AgentBro release work. Keep unrelated local changes out of commits.

## Safety Rules

- Start with `git status --short --branch`. If unrelated files are dirty, do not stage them.
- Never reuse or force-move an existing release tag. If a published release is wrong, make the next patch version.
- Do not edit signing keys, certificates, entitlements, or release secrets.
- Do not bump versions in feature PRs. Only bump for an actual release.
- Do not leave `dev` behind the latest stable release. Every stable release must be merged back through a PR to `dev`.
- If `gh` is not authenticated, use the public GitHub API with `curl` for read-only checks.

## Version Selection

Use semantic versioning when choosing the next release number:

- Bug fixes and small reliability fixes bump the third number: `X.Y.Z` -> `X.Y.(Z+1)`.
- Minor/user-facing feature releases bump the middle number and reset patch: `X.Y.Z` -> `X.(Y+1).0`.
- Major breaking releases bump the first number and reset the rest: `X.Y.Z` -> `(X+1).0.0`.

When the maintainer explicitly asks for a version, use that version after confirming the tag does not already exist locally or remotely.

## Version Files

All four must match:

- `package.json`
- `src-tauri/tauri.conf.json`
- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`

After editing the first three, update the lockfile with:

```bash
cargo update --manifest-path src-tauri/Cargo.toml -p agentbro
```

Run:

```bash
pnpm release:check
```

## Standard Release Flow

1. Fetch current remote state:

```bash
git fetch origin main dev --tags
```

2. If releasing from `dev`, merge it to `main` only after `dev` is pushed:

```bash
git checkout main
git pull --ff-only origin main
git merge --no-ff origin/dev -m "Merge branch 'dev' into main"
```

3. If `main` changed while working, pull it before creating the release commit. Stash only your own release edits if needed:

```bash
git stash push -u -m "release-work"
git pull --ff-only origin main
git stash pop
```

4. Update `.github/release-notes.md` for the actual release. The GitHub Release body and updater `latest.json` notes both come from this file.

   **Before writing, derive the changelog from the actual diff** — do not hand-wave a short summary. See "Drafting Release Notes From the Diff" below. The notes must reflect everything shipped since the previous release.

   Release notes must be bilingual:

   - English first, under `## English`.
   - Chinese second, under `## 中文`.
   - Keep the same facts in both sections. Do not ship an English-only or Chinese-only release body.
   - The app update dialog selects the Chinese section only when the user's language is Chinese; all other UI languages show the English section.
   - If this release includes first-time contributors, include a `### New Contributors` block in English and a matching `### 新贡献者` block in Chinese. Mention GitHub usernames and PR numbers.

5. Bump to the next version using the "Version Selection" rules above and run validation:

```bash
pnpm release:check
pnpm test:run
pnpm lint
pnpm build
cargo check --manifest-path src-tauri/Cargo.toml
```

Known acceptable warnings today:

- Vitest/jsdom may print `HTMLCanvasElement's getContext()` not implemented while tests pass.
- ESLint may report existing React hook dependency warnings.
- Vite may warn about chunk size or ineffective dynamic imports.
- `cargo check` may warn about `apps_claude_gemini` dead code.

6. Commit release-related changes separately when useful:

```bash
git add <changed-files>
git commit -m "fix: <release fix>"
git add package.json src-tauri/Cargo.lock src-tauri/Cargo.toml src-tauri/tauri.conf.json
git commit -m "chore: release vX.Y.Z"
```

7. Confirm the tag does not exist:

```bash
git tag --list vX.Y.Z
git ls-remote --tags origin refs/tags/vX.Y.Z
```

8. Tag and push:

```bash
git tag vX.Y.Z
git push origin main vX.Y.Z
```

9. After the stable GitHub Release is published, verify that the
   `Sync release back to dev` workflow opens a PR from
   `chore/sync-vX.Y.Z-to-dev` into `dev`:

```bash
gh pr list \
  --base dev \
  --head chore/sync-vX.Y.Z-to-dev \
  --state open
```

Review and merge that PR before accepting more feature PRs into `dev`. The PR
must carry the release commit and all four version files back together; do not
manually change only `package.json`.

If the workflow is unavailable, create the equivalent sync PR manually:

```bash
git checkout dev
git pull --ff-only origin dev
git checkout -b chore/sync-vX.Y.Z-to-dev
git merge --no-ff vX.Y.Z -m "chore: sync vX.Y.Z back to dev"
git push -u origin chore/sync-vX.Y.Z-to-dev
gh pr create \
  --base dev \
  --head chore/sync-vX.Y.Z-to-dev \
  --title "chore: sync vX.Y.Z back to dev"
```

10. After the sync PR merges, confirm `dev` is not behind the latest stable
    tag:

```bash
git checkout dev
git pull --ff-only origin dev
pnpm release:check
```

## Drafting Release Notes From the Diff

Past releases shipped thin, generic notes ("continues polishing the first
release") that did not tell users what actually changed. Always derive the
changelog from the real diff between the last released tag and the release
commit. Never write the notes from memory of this session alone — you may have
missed commits made by others, and features that landed in an earlier tag but
were never announced still deserve a mention if users have not heard of them.

1. Find the previous released tag and collect every change since it:

```bash
PREV=$(git tag --sort=-creatordate | grep -E '^v[0-9]' | head -1)   # last tag before this release
echo "Comparing $PREV..HEAD"
git log "$PREV"..HEAD --no-merges --format='%h %s'        # every non-merge commit
git diff "$PREV"..HEAD --stat                              # files touched + churn
```

2. Group the commits into user-facing buckets — **Features**, **Fixes**,
   **Performance**, **Docs/Internal**. Internal-only churn (refactors, test-only
   changes, generated schema files) can be collapsed into one line or omitted;
   anything a user can see or feel must be called out.

3. For each user-facing change, write what the user gets, not the code path.
   "Codex subagent list anchors on the active spawn wave" → "Completed
   subagents from earlier turns no longer linger in the hover list."

4. Cross-check against what was actually announced. Compare the previous
   release body to the features present in the code — a capability that shipped
   in code but was never described in any release note is still news:

```bash
gh release view "$PREV" --json body -q .body          # what users were last told
```

   If the diff or the comparison surfaces a notable feature that prior notes
   never mentioned (e.g. Pet Market shipped silently in an earlier tag),
   include it in this release's highlights.

5. Check first-time contributors before writing the final notes:

```bash
gh api repos/shirenchuang/agentbro/releases/generate-notes \
  -f tag_name=vX.Y.Z \
  -f target_commitish=main \
  -f previous_tag_name="$PREV" \
  --jq .body
```

Use this as a contributor-discovery aid, not as the final release body. If a
new contributor appears, add them manually to both language sections, e.g.
`Welcome @username for their first contribution in #123.`

6. Lead with a one-line summary of the release's theme, then a **Highlights**
   list ordered by user impact (biggest feature first, minor fixes last). Keep
   the existing Install / Downloads / Notes sections.

7. Mirror every fact into the `## 中文` section. Same buckets, same ordering.

This step is mandatory, not optional polish. A release note that does not
account for the full `$PREV..HEAD` diff is incomplete.

## Release Notes Gotcha

The release workflow must set the GitHub Release body explicitly. Do not rely only on `body_path`; previous releases uploaded assets but showed an empty GitHub Release body.

The workflow should:

- generate `dist/release-notes.md` from `.github/release-notes.md`
- export the generated notes to `$GITHUB_ENV`
- pass `body: ${{ env.RELEASE_NOTES }}` to `softprops/action-gh-release`

After the workflow starts, verify the Release run:

```bash
curl -fsSL "https://api.github.com/repos/shirenchuang/agentbro/actions/runs?per_page=10" \
  | node -e "let s='';process.stdin.on('data',d=>s+=d);process.stdin.on('end',()=>{const j=JSON.parse(s); for (const r of j.workflow_runs.slice(0,10)) console.log([r.id,r.name,r.event,r.head_branch,r.head_sha?.slice(0,7),r.status,r.conclusion,r.html_url].join(' | '));})"
```

After the Release job completes, verify the published body:

```bash
curl -fsSL https://api.github.com/repos/shirenchuang/agentbro/releases/tags/vX.Y.Z \
  | node -e "let s='';process.stdin.on('data',d=>s+=d);process.stdin.on('end',()=>{const r=JSON.parse(s); console.log({tag:r.tag_name, bodyLength:r.body?.length ?? 0, url:r.html_url});})"
```

Also verify updater notes:

```bash
curl -fsSL https://github.com/shirenchuang/agentbro/releases/download/vX.Y.Z/latest.json \
  | node -e "let s='';process.stdin.on('data',d=>s+=d);process.stdin.on('end',()=>{const j=JSON.parse(s); console.log({version:j.version, notesLength:j.notes?.length ?? 0});})"
```

For bilingual release notes, also verify the expected section markers are present:

```bash
curl -fsSL https://github.com/shirenchuang/agentbro/releases/download/vX.Y.Z/latest.json \
  | node -e "let s='';process.stdin.on('data',d=>s+=d);process.stdin.on('end',()=>{const n=JSON.parse(s).notes||''; console.log({hasEnglish:n.includes('## English'), hasChinese:n.includes('## 中文')});})"
```

## Homebrew

The release workflow updates the Homebrew cask only when `HOMEBREW_TAP_TOKEN` is configured. User install command:

```bash
brew tap shirenchuang/tap && brew install --cask agentbro
```

If Homebrew update is skipped, the GitHub DMG release can still be valid.
