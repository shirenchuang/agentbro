# AgentBro Release Pipeline Design

Date: 2026-05-21

## Decision

AgentBro will use the website as the primary download entry. GitHub Releases will store immutable release artifacts, the in-app updater will use Tauri updater metadata served from `releases.agentbro.cn`, and Homebrew Cask will be a developer-focused secondary install channel.

## Current Findings

- The app is a Tauri 2 desktop app with React, Rust, and `@tauri-apps/plugin-updater`.
- `src-tauri/tauri.conf.json` already points the updater at `https://releases.agentbro.cn/{{target}}/{{arch}}/{{current_version}}`.
- `src/hooks/useUpdater.ts` already checks for updates and passes a stable or beta channel header.
- Release automation exists in `.github/workflows/release.yml`.
- Homebrew Cask exists under `homebrew/Casks/agent-island.rb`.
- Naming is inconsistent: product config uses `AgentBro/agentbro`, while `build.sh`, release assets, workflow artifacts, and Homebrew still use `Agent Island/agent-island`.
- Versioning is inconsistent: `package.json` is `0.0.0`, while `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json` are `0.1.0`.
- The Tauri updater `pubkey` is empty, so signed automatic updates are not production-ready.

## Options Considered

### Option A: Website-first, GitHub-backed releases

The website owns the user journey and download buttons. GitHub Release stores the DMG, updater bundle, signature files, checksums, and release notes. `releases.agentbro.cn` serves updater metadata that points to GitHub or CDN assets.

Pros:
- Best fit for ordinary users.
- Keeps GitHub as a durable public archive.
- Allows the website to present stable download links.
- Allows a dynamic update API for stable and beta channels.

Cons:
- Requires a small release metadata service or static manifest hosting.
- More moving pieces than direct GitHub-only release.

### Option B: GitHub Release as the primary channel

Every release is published directly on GitHub. Website buttons point to the latest GitHub Release asset. Tauri updater can use `releases/latest/download/latest.json`.

Pros:
- Fastest to ship.
- Minimal infrastructure.
- Common for early open-source projects.

Cons:
- Less polished for non-developer users.
- Harder to control channel logic, staged rollout, and download analytics.
- GitHub UI becomes part of the install flow.

### Option C: Homebrew-first developer distribution

Publish releases mainly through a Homebrew tap, with GitHub Release only backing the cask download.

Pros:
- Great for developer users.
- Easy upgrades through `brew upgrade`.

Cons:
- Poor primary experience for ordinary macOS users.
- Does not replace in-app updates.

Recommended: Option A, with Option B as the fallback if `releases.agentbro.cn` is not ready for the first release.

## Release Architecture

### Public Channels

- Website: primary download page and stable links.
- GitHub Release: immutable version archive and binary asset storage.
- `releases.agentbro.cn`: updater metadata API or static manifest host.
- Homebrew tap: developer install and upgrade path.

### Release Artifacts

Each stable release should produce:

- `AgentBro_<version>_universal.dmg`
- Tauri updater archive for macOS, such as `AgentBro.app.tar.gz`
- Tauri updater signature, such as `AgentBro.app.tar.gz.sig`
- `latest.json` or equivalent channel manifest
- `checksums.txt`
- Generated release notes

The exact updater archive name should follow Tauri's generated artifact naming unless there is a strong reason to rename it.

## Versioning

Use SemVer tags:

```text
v0.1.0
v0.1.1
v0.2.0
v1.0.0
```

Before a tag is pushed, the release process must keep these files in sync:

- `package.json`
- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`
- `src-tauri/tauri.conf.json`

Recommended workflow:

```text
pnpm release:prepare 0.1.1
git commit -m "chore: release v0.1.1"
git tag v0.1.1
git push origin main --tags
```

The implementation can start with a small version-sync script rather than adopting a full release framework.

## CI Pipeline

### Trigger

Stable releases are triggered by tags matching:

```text
v*.*.*
```

Beta releases can later use:

```text
v*.*.*-beta.*
```

### Preflight

CI should fail before building if:

- Git tag version does not match Tauri version.
- `package.json`, Cargo, and Tauri versions differ.
- Tauri updater public key is missing.
- Required macOS signing and updater signing secrets are missing for stable release.
- Product names or binary paths still reference stale `Agent Island/agent-island` values.

### Build

Use Tauri's bundling flow as the source of truth for app packaging and updater artifacts. Avoid hand-building the app bundle unless Tauri cannot produce the required universal macOS artifact.

The build should:

- Install Node, pnpm, Rust, and macOS targets.
- Install frontend dependencies with a frozen lockfile.
- Run lint, tests, frontend build, and Rust checks.
- Build the macOS app.
- Sign and notarize the app for stable releases.
- Generate updater artifacts and signatures.
- Upload release assets to GitHub Release.

### Publish

After GitHub Release is created:

- Upload DMG, updater archive, updater signature, checksums, and `latest.json`.
- Update the website download manifest or redirect target.
- Update the Homebrew tap cask with version, URL, and SHA256.
- Optionally publish a short release announcement.

## Updater Design

AgentBro should keep using Tauri updater.

The app checks:

```text
https://releases.agentbro.cn/{{target}}/{{arch}}/{{current_version}}
```

The service responds:

- `204 No Content` when the client is current.
- `200 OK` with Tauri update JSON when an update is available.
- `400` or `404` for unsupported target or arch.

The update JSON must include the required Tauri fields:

- `version`
- `platforms.[target].url`
- `platforms.[target].signature`

Stable and beta channels can be selected by the existing `X-Update-Channel` header. The backend can map that to `stable.json` or `beta.json`.

If the update service is not ready for v0.1.0, use GitHub's static `latest.json` endpoint as a temporary fallback:

```text
https://github.com/<owner>/<repo>/releases/latest/download/latest.json
```

## Website Download Design

The website download button should not hardcode a versioned asset URL in page markup. It should call or link to a stable route:

```text
https://releases.agentbro.cn/download/mac/universal
```

That route redirects to the latest stable DMG. This lets the release pipeline update the target without redeploying the website.

The download page should show:

- Latest stable version.
- Release date.
- SHA256 checksum.
- macOS support range.
- Manual GitHub Release link.
- Homebrew command for developer users.

## Homebrew Design

Maintain a tap such as:

```text
brew tap agentbro/agentbro
brew install --cask agentbro
```

The cask should use the AgentBro product name, bundle id, and app path consistently:

- token: `agentbro`
- app: `AgentBro.app`
- bundle id: `com.agentbro.desktop`
- URL: latest GitHub Release DMG asset

The release workflow updates the tap after the GitHub Release asset exists and SHA256 is computed.

## Security

Stable releases require:

- Apple Developer ID signing certificate.
- Apple notarization credentials.
- Tauri updater private key in CI secrets.
- Tauri updater public key committed in `tauri.conf.json`.
- SHA256 checksums attached to every release.

Never store private signing keys in the repository.

## Error Handling

Release workflow should fail loudly when:

- Version files are out of sync.
- A release tag already exists.
- Signing or notarization fails.
- Updater signatures are missing.
- Homebrew tap update fails.
- Website manifest update fails.

Runtime updater behavior should:

- Continue silently when update check fails on startup.
- Show a clear error only for manual update checks.
- Avoid installing prerelease builds unless the beta channel setting is enabled.
- Never install unsigned or mismatched updater artifacts.

## Testing

### Local

- Version sync script dry run.
- `pnpm test:run`
- `pnpm lint`
- `pnpm build`
- `cargo check` in `src-tauri`
- Local Tauri build without publishing.

### CI

- Pull requests run build checks without signing.
- Tag builds run full signing, notarization, and release packaging.
- Release job validates expected files before publishing.

### Manual Release QA

Before announcing a stable release:

- Install the DMG on a clean macOS machine.
- Confirm Gatekeeper accepts the app.
- Confirm app launches and core hooks still work.
- Confirm website download link resolves to the new DMG.
- Confirm Homebrew cask installs the same version.
- Confirm an older app version can update to the new version through the updater.

## Implementation Phases

### Phase 0: Normalize identity

- Rename release scripts, workflow asset names, Homebrew cask, and binary references from `Agent Island/agent-island` to `AgentBro/agentbro`.
- Confirm app name, bundle id, app support directory, and zap paths.

### Phase 1: Version and release preflight

- Add a version sync script.
- Add CI checks that compare tag, Tauri, Cargo, and package versions.
- Add release checklist documentation.

### Phase 2: Tauri updater artifacts

- Generate updater signing keys.
- Configure updater `pubkey`.
- Make CI produce updater archive, signature, and `latest.json`.
- Test update from one local older build to a newer build.

### Phase 3: Website and release metadata

- Add stable download redirect or static manifest.
- Wire website download button to the stable route.
- Publish `stable.json` and optional `beta.json`.

### Phase 4: Homebrew automation

- Rename and update cask.
- Push cask updates to the tap after releases.
- Add a `livecheck` stanza where appropriate.

## Open Questions

- What is the canonical GitHub repository owner and repo name for public releases?
- Will `releases.agentbro.cn` be hosted as a dynamic endpoint, Cloudflare Worker, or static files on CDN?
- What minimum macOS version should be declared in release notes and Homebrew Cask?
- Do we want beta updates in v0.1.0, or should beta channel be enabled after stable auto-update works?
