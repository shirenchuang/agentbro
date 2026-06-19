# Local Symlink Import Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a local import mode that stores a center-library skill as a symlink to a local development source directory.

**Architecture:** Extend the existing Skill Manager v2 add-center-skill DTO with an import mode. The backend keeps current copy behavior as default and creates a symlink in the center library only when the caller requests link mode. The local import UI exposes the mode with clear guidance so users know link mode is for actively edited local skills and downstream agents must also use link distribution for live updates.

**Tech Stack:** Rust Tauri commands and service layer, React 19/TypeScript UI, Vitest and Rust unit tests.

---

### Task 1: Backend Link Import Behavior

**Files:**
- Modify: `src-tauri/src/skills/v2/models.rs`
- Modify: `src-tauri/src/skills/v2/service.rs`
- Test: `src-tauri/src/skills/v2/tests.rs`

- [ ] **Step 1: Write the failing test**

Add a Rust test named `add_center_skill_can_link_to_local_source` that imports a local skill with `import_mode: Some("link")`, asserts `~/.agentbro/skills/<skill>` is a symlink to the source directory, edits the source `SKILL.md`, and asserts the center path sees the edited content.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml add_center_skill_can_link_to_local_source`

Expected: FAIL because `AddCenterSkillInput` has no import mode and center import currently copies.

- [ ] **Step 3: Implement minimal backend support**

Add `import_mode: Option<String>` to `AddCenterSkillInput`. Keep `None` and `"copy"` as current copy behavior. For `"link"`, refuse archive/remote imports, replace the center destination with a symlink to the candidate source directory, and record the source metadata using the linked center path.

- [ ] **Step 4: Run backend tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml add_center_skill_can_link_to_local_source`

Expected: PASS.

### Task 2: Local Import UI Copy

**Files:**
- Modify: `src/services/skillApiV2.ts`
- Modify: `src/components/skills-v2/InstallView.tsx`
- Modify: `src/components/skills-v2/SkillManagerV2.css`
- Test: `src/test/skillManagerV2View.test.tsx`

- [ ] **Step 1: Write the failing UI test**

Add a Vitest case for `LocalPanel` that selects link import mode, previews, imports, and asserts `previewAddCenterSkill` and `executeAddCenterSkill` receive `importMode: "link"`.

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm test:run src/test/skillManagerV2View.test.tsx -- --runInBand`

Expected: FAIL because the local panel has no link mode control.

- [ ] **Step 3: Implement UI and wording**

Add an `importMode` field to the TypeScript input type and local panel state. Show copy and link mode choices for folder imports. Explain that link mode keeps the local folder as the source of truth, center and linked agents update immediately, copied agents need re-sync, and moving/deleting the source breaks the link.

- [ ] **Step 4: Run UI test**

Run: `pnpm test:run src/test/skillManagerV2View.test.tsx -- --runInBand`

Expected: PASS.

### Task 3: Verification

**Files:**
- No new files.

- [ ] Run focused Rust and frontend tests.
- [ ] Run `pnpm lint` and `cargo check --manifest-path src-tauri/Cargo.toml`.
- [ ] Summarize behavior and any skipped full-suite checks.
