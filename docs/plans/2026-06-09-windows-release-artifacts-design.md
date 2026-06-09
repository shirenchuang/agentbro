# Windows Release Artifacts Design

## Goal

Publish Windows MVP installers from the existing tag-driven release flow without destabilizing the signed macOS release path.

## Chosen Approach

Keep `build-universal` as the job that creates the GitHub Release and macOS updater artifacts. Add a separate `build-windows` job that depends on `build-universal`, runs on `windows-latest`, builds NSIS/MSI installers with `pnpm tauri:build:windows`, and attaches the resulting files to the same tag release.

This avoids concurrent release creation races, keeps Homebrew and OSS behavior scoped to macOS, and makes Windows packaging failure visible in the release workflow conclusion.

## Artifacts

Stable tags upload:

- The Tauri-emitted NSIS `.exe`
- The Tauri-emitted MSI `.msi`
- `AgentBro_latest_x64-setup.exe`
- `AgentBro_latest_x64.msi`
- `checksums-windows.txt`

Prerelease tags upload only the versioned Tauri-emitted installer names and `checksums-windows.txt`.

## Current Limits

Windows installers are unsigned MVP artifacts. They may trigger SmartScreen warnings and are not wired into the Tauri updater manifest yet. Windows OSS mirrors can be added later if the website needs mainland China Windows download links.

## Validation

The non-release `Build` workflow also builds Windows installers and uploads them as Actions artifacts. Local validation covers YAML shape and release readiness; full Windows packaging validation runs on GitHub Actions.
