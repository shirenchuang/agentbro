# Pet Surface Design

## Goal

Integrate Evolab's pet display mode into AgentBro without breaking the existing island mode. The first pass must preserve the current session, permission, question, plan, completion, chat-history, and terminal-jump flows while presenting them through a pet-style surface.

## Approach

Use an incremental `islandSurfaceMode` config flag with values `island` and `pet`. The default remains `island`. When `pet` is selected, `NotchPanel` renders a dedicated `PetSurface` branch instead of the notch shell. The pet branch reuses existing stores and commands instead of adding a parallel state system.

## Components

- `PetSurface`: main pet surface with mascot/sprite, badges, session fan, action toast, completion/response toast, and quick reply.
- Existing stores: `useSessionStore`, `useConfigStore`, `useThemeStore`.
- Existing commands: `respondPermission`, `respondQuestion`, `respondPlan`, `sendMessage`, `jumpToTerminal`, `getChatHistory`, `resizeNotch`, notch drag commands.
- Settings: add display mode and pet scale controls in Island settings.

## Data Flow

Backend config persists `islandSurfaceMode` and `islandPetScale`. Frontend config mirrors those values and applies them during Tauri config load. Settings updates write to the local store and backend config. `PetSurface` derives visible sessions using the same follow-focus filtering used by island mode.

## Verification

Run front-end tests, TypeScript build, Rust tests, `git diff --check`, release DMG build, and release hook smoke. Pet-specific unit tests cover mode selection and scale persistence where practical.
