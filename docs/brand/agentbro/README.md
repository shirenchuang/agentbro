# AgentBro Brand Resources

This folder contains the first production-ready brand resource pass for the AgentBro redesign.

## Positioning

- Chinese slogan: **让 Agent 更好用**
- English slogan: **Make Agents Easier to Use**
- Visual idea: a friendly AB monogram where the letters meet as a handshake, expressing management, optimization, and better user experience for Agents.

## Recommended System

- Primary palette: charcoal ink, warm amber, and paper neutrals.
- Approved raster mark: `assets/raster/agentbro-mark-reference.png`
- Approved Chinese lockup: `assets/raster/agentbro-lockup-zh-reference.png`
- Approved English lockup: `assets/raster/agentbro-lockup-en-reference.png`
- Approved app icon concept: `assets/raster/agentbro-app-icon-reference.png`
- Vector drafts: `assets/logo/*.svg` and `assets/app/agentbro-app-icon.svg`
- Tray icons:
  - `assets/raster/tray/agentbro-tray-default.png`
  - `assets/raster/tray/agentbro-tray-running.png`
  - `assets/raster/tray/agentbro-tray-attention.png`
  - `assets/raster/tray/agentbro-tray-error.png`
  - `assets/raster/tray/agentbro-tray-paused.png`
- Tray showcase: `assets/raster/tray/agentbro-tray-showcase.png`
- Six color system board: `assets/raster/variants/agentbro-color-systems-board.png`
- Tray variant board: `assets/raster/variants/agentbro-tray-variants-board.png`
- AgentBro Classic detailed direction: `assets/raster/ink-amber/agentbro-ink-amber-detail-board.png`
- AgentBro Classic detailed assets: `assets/raster/ink-amber/`
- Dark-background mark: `assets/raster/ink-amber/agentbro-ink-amber-mark-dark.png`
- CSS tokens: `assets/tokens/agentbro-brand.css`
- PNG previews/exports: `exports/png/`
- AI concept reference board: `references/color-exploration-board.png`

## Color Tokens

| Token | Hex | Use |
| --- | --- | --- |
| Ink | `#111820` | Main logo stroke, text, tray default |
| Paper | `#F8FBF8` | Light background |
| Teal | `#0C6B63` | Primary brand accent |
| Bright Teal | `#19D3C5` | Highlight, handshake bridge |
| Amber | `#F5B84B` | Warm collaboration accent and attention |
| Running | `#18B981` | Tray running status |
| Error | `#EF4444` | Tray error status |

## Usage Notes

- Keep the tray icon mostly monochrome. Use the small status dot for runtime state.
- Use the Chinese lockup for Chinese-facing product pages and onboarding.
- Use the English lockup for international docs, release notes, and marketing surfaces.
- Keep clear space around the mark equal to at least 20% of the mark width.
- Avoid placing the full lockup below 220 px wide; use the mark or tray icon instead.
- On dark backgrounds, use the dark-background mark so the A switches from ink to paper.

## Current Scope

These are standalone brand assets only. They are not wired into the application, favicon, Tauri icons, or tray implementation yet.

## Preview

Open `previews/brand-board.html` in a browser to review the current set on one board.

The approved assets currently use a raster master extracted from `references/color-exploration-board.png` so the proportions match the concept image. The SVG files are editable drafts, not the source of visual truth yet.
