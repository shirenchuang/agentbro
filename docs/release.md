# AgentBro Release Runbook

## One-time Setup

Generate a Tauri updater key pair on a trusted machine:

```bash
cargo tauri signer generate -w ~/.tauri/agentbro.key
```

Copy the public key into `src-tauri/tauri.conf.json` at `plugins.updater.pubkey`.

Store these GitHub Actions secrets:

- `TAURI_SIGNING_PRIVATE_KEY`: contents of `~/.tauri/agentbro.key`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: password used when generating the key, if any

For unsigned preview releases, only the Tauri signing secret is required. For stable releases, also store:

- `CERTIFICATE_P12`: base64-encoded Apple Developer ID Application certificate
- `CERTIFICATE_PASSWORD`: certificate password
- `CODESIGN_IDENTITY`: Developer ID Application identity
- `APPLE_ID`: Apple ID for notarization
- `APPLE_PASSWORD`: app-specific password
- `APPLE_TEAM_ID`: Apple team id
- `HOMEBREW_TAP_TOKEN`: token that can push to the Homebrew tap repository

Do not commit the private key or Apple certificate.

## Preflight

Run:

```bash
pnpm release:check
pnpm test:run
pnpm lint
pnpm build
```

`pnpm release:check` validates:

- `package.json`, Cargo, Cargo.lock, and Tauri versions match.
- Product identity is `AgentBro` / `agentbro` / `com.agentbro.desktop`.
- Release files do not contain stale `Agent Island` names.
- Tauri updater artifacts are enabled.
- Preview CI releases have updater signing secrets.
- Stable CI releases have updater, code-signing, notarization, and Homebrew secrets.

## Unsigned Preview Release

Use this while you do not have an Apple Developer account:

```bash
pnpm release:check
git commit -am "chore: release v0.1.0-preview.1"
git tag v0.1.0-preview.1
git push origin main --tags
```

Preview releases:

- Are marked as GitHub prereleases.
- Build unsigned and unnotarized macOS DMGs.
- Still include Tauri updater signatures.
- Do not update the Homebrew tap.
- May require users to right-click the app and choose Open on first launch.

## Stable Release

Prepare a version commit:

```bash
pnpm release:check
git commit -am "chore: release v0.1.1"
git tag v0.1.1
git push origin main --tags
```

The `Release` workflow builds a universal macOS release and uploads:

- `AgentBro_<version>_universal.dmg`
- `AgentBro.app.tar.gz`
- `AgentBro.app.tar.gz.sig`
- `latest.json`
- `checksums.txt`

The workflow also updates the Homebrew cask in the tap repository.

## Manual QA

Before announcing a release:

- Install the DMG on a clean macOS machine.
- Confirm Gatekeeper accepts the app.
- Confirm `~/.agentbro/bin/agentbro-bridge` is installed after hook setup.
- Confirm the website download route points to the new DMG.
- Confirm Homebrew installs the same version.
- Confirm an older app version updates through the Tauri updater.
