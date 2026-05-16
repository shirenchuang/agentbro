# Skills Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add unified skill/MCP/plugin management to AgentBro — scan, install, uninstall, toggle, pack, and sync across agents and devices.

**Architecture:** Rust backend provides scanning, registry, installation, and sync via Tauri IPC commands. React frontend adds a "技能管理" section in Settings with three tabs (Skills, Packs, Sync) and a slide-in detail panel. Data model: Scanner (local truth) + Registry metadata overlay (sync truth).

**Tech Stack:** Rust, Tauri 2, serde_json, tokio, zip crate | React 19, TypeScript, Zustand, Framer Motion, i18next

---

## File Map

### New Rust Files

| File | Responsibility |
|------|---------------|
| `src-tauri/src/skills/mod.rs` | Module entry, re-exports, shared types (`ScannedSkill`, `SkillPack`, `SyncConfig`, `InstallMode`, etc.) |
| `src-tauri/src/skills/agent_paths.rs` | Path constants for each agent's skill dir, MCP config, settings file |
| `src-tauri/src/skills/scanner.rs` | Walk agent skill dirs, parse frontmatter, return `Vec<ScannedSkill>` |
| `src-tauri/src/skills/registry.rs` | Read/write `~/.agentbro/metadata.json` — sources, packs, sync config |
| `src-tauri/src/skills/installer.rs` | Install (copy/symlink), uninstall, toggle skill on/off per agent |
| `src-tauri/src/skills/sync.rs` | Git push/pull, export/import zip, agent-to-agent sync, conflict detection |

### New TypeScript Files

| File | Responsibility |
|------|---------------|
| `src/stores/skillStore.ts` | Zustand store — skills list, packs, sync state, filter state, batch selection |
| `src/services/skillApi.ts` | Typed Tauri IPC wrappers for all skill commands |
| `src/components/settings/sections/SkillsSection.tsx` | Settings section entry — tab switcher, container |
| `src/components/settings/sections/SkillsSection.css` | Styles for the entire skills section |
| `src/components/skills/SkillListView.tsx` | Skills tab — search, filter, list, batch bar |
| `src/components/skills/SkillCard.tsx` | Single skill row — icon, name, agent tags, toggle, actions |
| `src/components/skills/PackListView.tsx` | Packs tab — pack cards, create button |
| `src/components/skills/PackCard.tsx` | Single pack card — skills chips, agent tags, actions |
| `src/components/skills/SyncView.tsx` | Sync tab — GitHub, export/import, agent-to-agent, scanner status |
| `src/components/skills/SkillDetailSlider.tsx` | Right slide-in panel — agents, info, file browser, actions |
| `src/components/skills/FileTreeViewer.tsx` | Tree + code preview component |
| `src/components/skills/InstallDialog.tsx` | Install modal — 4 source forms |
| `src/components/skills/PackDialog.tsx` | Create/edit pack modal |
| `src/components/skills/ConfirmDialog.tsx` | Reusable confirm dialog (uninstall, sync, conflict) |

### Modified Files

| File | Change |
|------|--------|
| `src-tauri/src/lib.rs` | Register skills module, add IPC commands to Tauri builder |
| `src-tauri/Cargo.toml` | Add `zip` crate dependency |
| `src/components/settings/sections/` | Import SkillsSection in settings routing |
| `src/i18n/locales/zh.json` | Add skills section translations |
| `src/i18n/locales/en.json` | Add skills section translations |

---

## Task 1: Rust Types & Module Scaffolding

**Files:**
- Create: `src-tauri/src/skills/mod.rs`
- Create: `src-tauri/src/skills/agent_paths.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod skills;`)

- [ ] **Step 1: Create `src-tauri/src/skills/mod.rs` with shared types**

```rust
pub mod agent_paths;
pub mod scanner;
pub mod registry;
pub mod installer;
pub mod sync;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillType {
    Skill,
    Mcp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallMode {
    Direct,
    Symlink,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillSource {
    Island,
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillState {
    pub agent: String,
    pub install_path: String,
    pub install_mode: InstallMode,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub skill_type: SkillType,
    pub icon: Option<String>,
    pub source: SkillSource,
    pub origin_url: Option<String>,
    pub has_update: bool,
    pub file_path: String,
    pub file_size: u64,
    pub modified_at: u64,
    pub agents: Vec<AgentSkillState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPack {
    pub id: String,
    pub name: String,
    pub description: String,
    pub skills: Vec<String>,
    pub target_agents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConfig {
    pub method: String,
    pub github_repo: Option<String>,
    pub github_token: Option<String>,
    pub last_sync_at: Option<String>,
    pub auto_sync: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetConfig {
    pub agent: String,
    pub install_mode: InstallMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeNode {
    pub name: String,
    pub node_type: String, // "file" or "dir"
    pub path: String,
    pub children: Option<Vec<FileTreeNode>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub success: bool,
    pub message: String,
    pub conflicts: Vec<SyncConflict>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConflict {
    pub skill_id: String,
    pub local_modified: String,
    pub remote_modified: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPreview {
    pub to_copy: u32,
    pub to_skip: u32,
    pub to_update: u32,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictResolution {
    pub skill_id: String,
    pub action: String, // "keep_local", "use_remote", "keep_both"
}
```

- [ ] **Step 2: Create `src-tauri/src/skills/agent_paths.rs`**

```rust
use std::path::PathBuf;

pub struct SkillPaths {
    pub skill_dirs: Vec<PathBuf>,
    pub mcp_config: Option<PathBuf>,
    pub settings_file: Option<PathBuf>,
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| std::env::temp_dir())
}

pub fn paths_for_agent(agent: &str) -> SkillPaths {
    let h = home();
    match agent {
        "claude-code" => SkillPaths {
            skill_dirs: vec![h.join(".claude").join("skills")],
            mcp_config: Some(h.join(".claude").join("settings.json")),
            settings_file: Some(h.join(".claude").join("settings.json")),
        },
        "codex" => SkillPaths {
            skill_dirs: vec![h.join(".codex").join("agents")],
            mcp_config: Some(h.join(".codex").join("config.json")),
            settings_file: Some(h.join(".codex").join("config.json")),
        },
        "gemini-cli" => SkillPaths {
            skill_dirs: vec![h.join(".gemini").join("skills")],
            mcp_config: Some(h.join(".gemini").join("settings.json")),
            settings_file: Some(h.join(".gemini").join("settings.json")),
        },
        "cursor" | "cursor-cli" => SkillPaths {
            skill_dirs: vec![h.join(".cursor").join("rules")],
            mcp_config: Some(h.join(".cursor").join("mcp.json")),
            settings_file: Some(h.join(".cursor").join("mcp.json")),
        },
        "hermes" => SkillPaths {
            skill_dirs: vec![h.join(".hermes").join("skills")],
            mcp_config: None,
            settings_file: None,
        },
        _ => SkillPaths {
            skill_dirs: vec![],
            mcp_config: None,
            settings_file: None,
        },
    }
}

pub fn agentbro_skills_dir() -> PathBuf {
    home().join(".agentbro").join("skills")
}

pub fn agentbro_metadata_path() -> PathBuf {
    home().join(".agentbro").join("metadata.json")
}
```

- [ ] **Step 3: Add module declaration to `src-tauri/src/lib.rs`**

Add `mod skills;` near the top alongside other module declarations.

- [ ] **Step 4: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: compiles with no errors (scanner/registry/installer/sync modules not yet created — add empty files)

- [ ] **Step 5: Create empty stub files for remaining modules**

Create these with just a comment so `mod.rs` compiles:
- `src-tauri/src/skills/scanner.rs` — `// Skill scanner — to be implemented`
- `src-tauri/src/skills/registry.rs` — `// Metadata registry — to be implemented`
- `src-tauri/src/skills/installer.rs` — `// Skill installer — to be implemented`
- `src-tauri/src/skills/sync.rs` — `// Sync engine — to be implemented`

- [ ] **Step 6: Verify compilation again**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/skills/ src-tauri/src/lib.rs
git commit -m "feat(skills): add types and module scaffolding for skill management"
```

---

## Task 2: Scanner — Walk Agent Skill Directories

**Files:**
- Modify: `src-tauri/src/skills/scanner.rs`

- [ ] **Step 1: Implement scanner**

```rust
use std::path::{Path, PathBuf};
use std::fs;
use super::{ScannedSkill, SkillType, SkillSource, AgentSkillState, InstallMode};
use super::agent_paths;

pub fn scan_agent(agent: &str) -> Vec<ScannedSkill> {
    let paths = agent_paths::paths_for_agent(agent);
    let mut results = Vec::new();

    for skill_dir in &paths.skill_dirs {
        if !skill_dir.is_dir() {
            continue;
        }
        scan_directory(skill_dir, agent, &mut results);
    }

    results
}

pub fn scan_all() -> std::collections::HashMap<String, Vec<ScannedSkill>> {
    let agents = ["claude-code", "codex", "gemini-cli", "cursor", "hermes"];
    let mut map = std::collections::HashMap::new();
    for agent in &agents {
        let skills = scan_agent(agent);
        if !skills.is_empty() {
            map.insert(agent.to_string(), skills);
        }
    }
    map
}

fn scan_directory(dir: &Path, agent: &str, results: &mut Vec<ScannedSkill>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skill is a directory — look for index.md or main file
            let index = find_index_file(&path);
            if let Some(index_path) = index {
                let (name, desc) = parse_frontmatter(&index_path);
                let skill_name = name.unwrap_or_else(||
                    path.file_name().unwrap_or_default().to_string_lossy().to_string()
                );
                let meta = fs::metadata(&path).ok();
                let is_symlink = entry.path().symlink_metadata()
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(false);

                results.push(ScannedSkill {
                    id: skill_name.clone(),
                    name: skill_name,
                    description: desc.unwrap_or_default(),
                    skill_type: SkillType::Skill,
                    icon: None,
                    source: SkillSource::Local,
                    origin_url: None,
                    has_update: false,
                    file_path: path.display().to_string(),
                    file_size: dir_size(&path),
                    modified_at: meta.and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                    agents: vec![AgentSkillState {
                        agent: agent.to_string(),
                        install_path: path.display().to_string(),
                        install_mode: if is_symlink { InstallMode::Symlink } else { InstallMode::Direct },
                        enabled: true,
                    }],
                });
            }
        } else if path.extension().map(|e| e == "md").unwrap_or(false) {
            // Single-file skill
            let (name, desc) = parse_frontmatter(&path);
            let skill_name = name.unwrap_or_else(||
                path.file_stem().unwrap_or_default().to_string_lossy().to_string()
            );
            let meta = fs::metadata(&path).ok();

            results.push(ScannedSkill {
                id: skill_name.clone(),
                name: skill_name,
                description: desc.unwrap_or_default(),
                skill_type: SkillType::Skill,
                icon: None,
                source: SkillSource::Local,
                origin_url: None,
                has_update: false,
                file_path: path.display().to_string(),
                file_size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                modified_at: meta.and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                agents: vec![AgentSkillState {
                    agent: agent.to_string(),
                    install_path: path.display().to_string(),
                    install_mode: InstallMode::Direct,
                    enabled: true,
                }],
            });
        }
    }
}

fn find_index_file(dir: &Path) -> Option<PathBuf> {
    for name in &["index.md", "README.md", "main.md"] {
        let p = dir.join(name);
        if p.exists() { return Some(p); }
    }
    // Fallback: first .md file
    fs::read_dir(dir).ok()?.flatten()
        .find(|e| e.path().extension().map(|x| x == "md").unwrap_or(false))
        .map(|e| e.path())
}

fn parse_frontmatter(path: &Path) -> (Option<String>, Option<String>) {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (None, None),
    };

    if !content.starts_with("---") {
        return (None, None);
    }

    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        return (None, None);
    }

    let frontmatter = parts[1];
    let mut name = None;
    let mut desc = None;

    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("name:") {
            name = Some(val.trim().trim_matches('"').trim_matches('\'').to_string());
        } else if let Some(val) = line.strip_prefix("description:") {
            desc = Some(val.trim().trim_matches('"').trim_matches('\'').to_string());
        }
    }

    (name, desc)
}

fn dir_size(path: &Path) -> u64 {
    fs::read_dir(path).ok()
        .map(|entries| entries.flatten()
            .map(|e| {
                let p = e.path();
                if p.is_file() {
                    fs::metadata(&p).map(|m| m.len()).unwrap_or(0)
                } else if p.is_dir() {
                    dir_size(&p)
                } else { 0 }
            })
            .sum())
        .unwrap_or(0)
}

pub fn read_file_tree(skill_path: &str) -> super::FileTreeNode {
    let path = PathBuf::from(skill_path);
    build_tree(&path)
}

fn build_tree(path: &Path) -> super::FileTreeNode {
    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

    if path.is_file() {
        return super::FileTreeNode {
            name,
            node_type: "file".to_string(),
            path: path.display().to_string(),
            children: None,
        };
    }

    let children: Vec<super::FileTreeNode> = fs::read_dir(path)
        .ok()
        .map(|entries| {
            let mut nodes: Vec<_> = entries.flatten()
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .map(|e| build_tree(&e.path()))
                .collect();
            nodes.sort_by(|a, b| {
                let a_dir = a.node_type == "dir";
                let b_dir = b.node_type == "dir";
                b_dir.cmp(&a_dir).then(a.name.cmp(&b.name))
            });
            nodes
        })
        .unwrap_or_default();

    super::FileTreeNode {
        name,
        node_type: "dir".to_string(),
        path: path.display().to_string(),
        children: Some(children),
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/skills/scanner.rs
git commit -m "feat(skills): implement scanner — walk agent dirs, parse frontmatter"
```

---

## Task 3: Registry — Metadata Storage

**Files:**
- Modify: `src-tauri/src/skills/registry.rs`

- [ ] **Step 1: Implement registry**

```rust
use std::fs;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use super::{SkillPack, SyncConfig};
use super::agent_paths;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SkillSourceEntry {
    pub origin: String,
    pub installed_via: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    #[serde(default)]
    pub sources: HashMap<String, SkillSourceEntry>,
    #[serde(default)]
    pub packs: Vec<SkillPack>,
    #[serde(default)]
    pub sync: Option<SyncConfig>,
}

pub fn load() -> Metadata {
    let path = agent_paths::agentbro_metadata_path();
    fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

pub fn save(metadata: &Metadata) -> Result<(), String> {
    let path = agent_paths::agentbro_metadata_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(metadata).map_err(|e| e.to_string())?;
    fs::write(&path, content).map_err(|e| e.to_string())
}

pub fn add_source(skill_id: &str, origin: &str) -> Result<(), String> {
    let mut meta = load();
    meta.sources.insert(skill_id.to_string(), SkillSourceEntry {
        origin: origin.to_string(),
        installed_via: "island".to_string(),
    });
    save(&meta)
}

pub fn remove_source(skill_id: &str) -> Result<(), String> {
    let mut meta = load();
    meta.sources.remove(skill_id);
    save(&meta)
}

pub fn list_packs() -> Vec<SkillPack> {
    load().packs
}

pub fn create_pack(pack: SkillPack) -> Result<(), String> {
    let mut meta = load();
    meta.packs.push(pack);
    save(&meta)
}

pub fn update_pack(pack: SkillPack) -> Result<(), String> {
    let mut meta = load();
    if let Some(existing) = meta.packs.iter_mut().find(|p| p.id == pack.id) {
        *existing = pack;
    }
    save(&meta)
}

pub fn delete_pack(id: &str) -> Result<(), String> {
    let mut meta = load();
    meta.packs.retain(|p| p.id != id);
    save(&meta)
}

pub fn get_sync_config() -> Option<SyncConfig> {
    load().sync
}

pub fn set_sync_config(config: SyncConfig) -> Result<(), String> {
    let mut meta = load();
    meta.sync = Some(config);
    save(&meta)
}
```

- [ ] **Step 2: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/skills/registry.rs
git commit -m "feat(skills): implement registry — metadata.json read/write"
```

---

## Task 4: Installer — Install, Uninstall, Toggle

**Files:**
- Modify: `src-tauri/src/skills/installer.rs`

- [ ] **Step 1: Implement installer**

```rust
use std::fs;
use std::path::{Path, PathBuf};
use super::{InstallMode, TargetConfig};
use super::agent_paths;

pub fn install_skill(
    source_path: &str,
    targets: &[TargetConfig],
    mode: &InstallMode,
) -> Result<(), String> {
    let src = PathBuf::from(source_path);
    if !src.exists() {
        return Err(format!("Source not found: {}", source_path));
    }

    let skill_name = src.file_name()
        .ok_or("Invalid source path")?
        .to_string_lossy()
        .to_string();

    // For symlink mode, copy to central store first
    let central_path = if matches!(mode, InstallMode::Symlink) {
        let central_dir = agent_paths::agentbro_skills_dir();
        fs::create_dir_all(&central_dir).map_err(|e| e.to_string())?;
        let dest = central_dir.join(&skill_name);
        copy_recursive(&src, &dest)?;
        Some(dest)
    } else {
        None
    };

    for target in targets {
        let paths = agent_paths::paths_for_agent(&target.agent);
        let target_dir = paths.skill_dirs.first()
            .ok_or_else(|| format!("No skill directory for agent: {}", target.agent))?;

        fs::create_dir_all(target_dir).map_err(|e| e.to_string())?;
        let dest = target_dir.join(&skill_name);

        match mode {
            InstallMode::Direct => {
                copy_recursive(&src, &dest)?;
            }
            InstallMode::Symlink => {
                if let Some(ref central) = central_path {
                    if dest.exists() || dest.symlink_metadata().is_ok() {
                        fs::remove_file(&dest).or_else(|_| fs::remove_dir_all(&dest))
                            .map_err(|e| e.to_string())?;
                    }
                    #[cfg(unix)]
                    std::os::unix::fs::symlink(central, &dest)
                        .map_err(|e| e.to_string())?;
                    #[cfg(not(unix))]
                    copy_recursive(central, &dest)?;
                }
            }
        }
    }

    Ok(())
}

pub fn uninstall_skill(skill_path: &str) -> Result<(), String> {
    let path = PathBuf::from(skill_path);
    if !path.exists() && path.symlink_metadata().is_err() {
        return Ok(());
    }

    if path.is_dir() {
        fs::remove_dir_all(&path).map_err(|e| e.to_string())
    } else {
        fs::remove_file(&path).map_err(|e| e.to_string())
    }
}

pub fn toggle_skill(skill_id: &str, agent: &str, enabled: bool) -> Result<(), String> {
    let paths = agent_paths::paths_for_agent(agent);
    let settings_path = match paths.settings_file {
        Some(p) => p,
        None => return Err(format!("Agent {} has no settings file", agent)),
    };

    if !settings_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;
    let mut settings: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| e.to_string())?;

    // Each agent stores disabled skills differently.
    // We use a generic "disabledSkills" array in the config.
    let disabled = settings
        .as_object_mut()
        .ok_or("Settings is not an object")?
        .entry("disabledSkills")
        .or_insert_with(|| serde_json::json!([]));

    if let Some(arr) = disabled.as_array_mut() {
        let skill_val = serde_json::Value::String(skill_id.to_string());
        if enabled {
            arr.retain(|v| v != &skill_val);
        } else if !arr.contains(&skill_val) {
            arr.push(skill_val);
        }
    }

    let content = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    fs::write(&settings_path, content).map_err(|e| e.to_string())
}

fn copy_recursive(src: &Path, dest: &Path) -> Result<(), String> {
    if src.is_file() {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(src, dest).map_err(|e| e.to_string())?;
        return Ok(());
    }

    fs::create_dir_all(dest).map_err(|e| e.to_string())?;

    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if src_path.is_dir() {
            copy_recursive(&src_path, &dest_path)?;
        } else {
            fs::copy(&src_path, &dest_path).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}
```

- [ ] **Step 2: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/skills/installer.rs
git commit -m "feat(skills): implement installer — copy, symlink, uninstall, toggle"
```

---

## Task 5: Sync Engine

**Files:**
- Modify: `src-tauri/src/skills/sync.rs`

- [ ] **Step 1: Implement sync engine**

```rust
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use super::{SyncResult, SyncConflict, SyncPreview, ConflictResolution};
use super::{registry, scanner, installer, agent_paths};

pub fn push_to_github() -> Result<SyncResult, String> {
    let meta = registry::load();
    let repo = meta.sync.as_ref()
        .and_then(|s| s.github_repo.as_ref())
        .ok_or("No GitHub repo configured")?;
    let token = meta.sync.as_ref()
        .and_then(|s| s.github_token.as_ref())
        .ok_or("No GitHub token configured")?;

    let tmp_dir = std::env::temp_dir().join("agentbro-sync");
    let _ = fs::remove_dir_all(&tmp_dir);

    // Clone or init
    let repo_url = format!("https://{}@github.com/{}.git", token, repo);
    let clone_result = Command::new("git")
        .args(["clone", "--depth", "1", &repo_url, &tmp_dir.display().to_string()])
        .output()
        .map_err(|e| e.to_string())?;

    if !clone_result.status.success() {
        // Repo might not exist — init
        fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
        Command::new("git").args(["init"]).current_dir(&tmp_dir).output().map_err(|e| e.to_string())?;
        Command::new("git").args(["remote", "add", "origin", &repo_url]).current_dir(&tmp_dir).output().map_err(|e| e.to_string())?;
    }

    // Write metadata
    let meta_content = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    fs::write(tmp_dir.join("metadata.json"), meta_content).map_err(|e| e.to_string())?;

    // Copy skills from agentbro central store
    let skills_src = agent_paths::agentbro_skills_dir();
    if skills_src.is_dir() {
        let skills_dest = tmp_dir.join("skills");
        installer::copy_recursive_pub(&skills_src, &skills_dest)?;
    }

    // Git add, commit, push
    Command::new("git").args(["add", "."]).current_dir(&tmp_dir).output().map_err(|e| e.to_string())?;
    Command::new("git").args(["commit", "-m", "AgentBro sync"]).current_dir(&tmp_dir).output().map_err(|e| e.to_string())?;
    let push = Command::new("git").args(["push", "-u", "origin", "main"]).current_dir(&tmp_dir).output().map_err(|e| e.to_string())?;

    let _ = fs::remove_dir_all(&tmp_dir);

    if push.status.success() {
        // Update last sync time
        let mut meta = registry::load();
        if let Some(ref mut sync) = meta.sync {
            sync.last_sync_at = Some(chrono::Utc::now().to_rfc3339());
        }
        let _ = registry::save(&meta);

        Ok(SyncResult { success: true, message: "推送成功".to_string(), conflicts: vec![] })
    } else {
        let stderr = String::from_utf8_lossy(&push.stderr).to_string();
        Ok(SyncResult { success: false, message: format!("推送失败: {}", stderr), conflicts: vec![] })
    }
}

pub fn pull_from_github() -> Result<SyncResult, String> {
    let meta = registry::load();
    let repo = meta.sync.as_ref()
        .and_then(|s| s.github_repo.as_ref())
        .ok_or("No GitHub repo configured")?;
    let token = meta.sync.as_ref()
        .and_then(|s| s.github_token.as_ref())
        .ok_or("No GitHub token configured")?;

    let tmp_dir = std::env::temp_dir().join("agentbro-sync-pull");
    let _ = fs::remove_dir_all(&tmp_dir);

    let repo_url = format!("https://{}@github.com/{}.git", token, repo);
    let clone = Command::new("git")
        .args(["clone", "--depth", "1", &repo_url, &tmp_dir.display().to_string()])
        .output()
        .map_err(|e| e.to_string())?;

    if !clone.status.success() {
        return Ok(SyncResult { success: false, message: "拉取失败：无法克隆仓库".to_string(), conflicts: vec![] });
    }

    // Read remote metadata
    let remote_meta_path = tmp_dir.join("metadata.json");
    if remote_meta_path.exists() {
        let content = fs::read_to_string(&remote_meta_path).map_err(|e| e.to_string())?;
        let remote_meta: registry::Metadata = serde_json::from_str(&content).map_err(|e| e.to_string())?;

        // Merge packs
        let mut local_meta = registry::load();
        for pack in remote_meta.packs {
            if !local_meta.packs.iter().any(|p| p.id == pack.id) {
                local_meta.packs.push(pack);
            }
        }
        for (k, v) in remote_meta.sources {
            local_meta.sources.entry(k).or_insert(v);
        }
        if let Some(ref mut sync) = local_meta.sync {
            sync.last_sync_at = Some(chrono::Utc::now().to_rfc3339());
        }
        registry::save(&local_meta)?;
    }

    // Copy skills to central store
    let remote_skills = tmp_dir.join("skills");
    if remote_skills.is_dir() {
        let local_skills = agent_paths::agentbro_skills_dir();
        fs::create_dir_all(&local_skills).map_err(|e| e.to_string())?;
        installer::copy_recursive_pub(&remote_skills, &local_skills)?;
    }

    let _ = fs::remove_dir_all(&tmp_dir);

    Ok(SyncResult { success: true, message: "拉取成功".to_string(), conflicts: vec![] })
}

pub fn sync_agent_to_agent(from: &str, to: &str) -> Result<SyncPreview, String> {
    let from_skills = scanner::scan_agent(from);
    let to_skills = scanner::scan_agent(to);
    let to_ids: std::collections::HashSet<String> = to_skills.iter().map(|s| s.id.clone()).collect();

    let mut to_copy = 0u32;
    let mut to_skip = 0u32;
    let mut details = Vec::new();

    for skill in &from_skills {
        if to_ids.contains(&skill.id) {
            to_skip += 1;
            details.push(format!("跳过: {} (已存在)", skill.id));
        } else {
            to_copy += 1;
            details.push(format!("复制: {}", skill.id));
        }
    }

    Ok(SyncPreview { to_copy, to_skip, to_update: 0, details })
}

pub fn execute_agent_sync(from: &str, to: &str) -> Result<(), String> {
    let from_skills = scanner::scan_agent(from);
    let to_skills = scanner::scan_agent(to);
    let to_ids: std::collections::HashSet<String> = to_skills.iter().map(|s| s.id.clone()).collect();

    let targets = vec![super::TargetConfig {
        agent: to.to_string(),
        install_mode: super::InstallMode::Direct,
    }];

    for skill in &from_skills {
        if !to_ids.contains(&skill.id) {
            installer::install_skill(&skill.file_path, &targets, &super::InstallMode::Direct)?;
        }
    }

    Ok(())
}

pub fn export_backup(path: &str) -> Result<(), String> {
    // Simple: copy metadata.json + skills dir into a zip
    let dest = PathBuf::from(path);
    let meta = registry::load();
    let meta_json = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;

    let file = fs::File::create(&dest).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();

    zip.start_file("metadata.json", options).map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut zip, meta_json.as_bytes()).map_err(|e| e.to_string())?;

    // Add skills
    let skills_dir = agent_paths::agentbro_skills_dir();
    if skills_dir.is_dir() {
        add_dir_to_zip(&mut zip, &skills_dir, "skills", options)?;
    }

    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn import_backup(path: &str) -> Result<(), String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let extract_dir = std::env::temp_dir().join("agentbro-import");
    let _ = fs::remove_dir_all(&extract_dir);
    archive.extract(&extract_dir).map_err(|e| e.to_string())?;

    // Import metadata
    let meta_path = extract_dir.join("metadata.json");
    if meta_path.exists() {
        let content = fs::read_to_string(&meta_path).map_err(|e| e.to_string())?;
        let imported: registry::Metadata = serde_json::from_str(&content).map_err(|e| e.to_string())?;
        let mut local = registry::load();
        for (k, v) in imported.sources {
            local.sources.entry(k).or_insert(v);
        }
        for pack in imported.packs {
            if !local.packs.iter().any(|p| p.id == pack.id) {
                local.packs.push(pack);
            }
        }
        registry::save(&local)?;
    }

    // Import skills
    let skills_dir = extract_dir.join("skills");
    if skills_dir.is_dir() {
        let dest = agent_paths::agentbro_skills_dir();
        fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        installer::copy_recursive_pub(&skills_dir, &dest)?;
    }

    let _ = fs::remove_dir_all(&extract_dir);
    Ok(())
}

fn add_dir_to_zip<W: std::io::Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    dir: &std::path::Path,
    prefix: &str,
    options: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = format!("{}/{}", prefix, entry.file_name().to_string_lossy());
        if path.is_file() {
            zip.start_file(&name, options).map_err(|e| e.to_string())?;
            let data = fs::read(&path).map_err(|e| e.to_string())?;
            std::io::Write::write_all(zip, &data).map_err(|e| e.to_string())?;
        } else if path.is_dir() {
            add_dir_to_zip(zip, &path, &name, options)?;
        }
    }
    Ok(())
}
```

- [ ] **Step 2: Make `copy_recursive` public in installer.rs**

Add this public wrapper at the end of `installer.rs`:

```rust
pub fn copy_recursive_pub(src: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    copy_recursive(src, dest)
}
```

- [ ] **Step 3: Add `zip` and `chrono` to `Cargo.toml`**

Add under `[dependencies]`:
```toml
zip = "2"
```

(`chrono` should already be a dependency — verify.)

- [ ] **Step 4: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/skills/sync.rs src-tauri/src/skills/installer.rs src-tauri/Cargo.toml
git commit -m "feat(skills): implement sync engine — git push/pull, export/import, agent-to-agent"
```

---

## Task 6: Tauri IPC Commands

**Files:**
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs` (register commands in Tauri builder)

- [ ] **Step 1: Add skill commands to `commands/mod.rs`**

Append at the end of the file:

```rust
// ── Skill Management Commands ───────────────────────────────────

#[tauri::command]
pub async fn scan_all_skills() -> Result<std::collections::HashMap<String, Vec<crate::skills::ScannedSkill>>, String> {
    Ok(crate::skills::scanner::scan_all())
}

#[tauri::command]
pub async fn scan_agent_skills(agent: String) -> Result<Vec<crate::skills::ScannedSkill>, String> {
    Ok(crate::skills::scanner::scan_agent(&agent))
}

#[tauri::command]
pub async fn install_skill_cmd(
    source: String,
    targets: Vec<crate::skills::TargetConfig>,
    mode: crate::skills::InstallMode,
) -> Result<(), String> {
    crate::skills::installer::install_skill(&source, &targets, &mode)?;
    crate::skills::registry::add_source(
        &std::path::PathBuf::from(&source)
            .file_name().unwrap_or_default()
            .to_string_lossy(),
        &source,
    )?;
    Ok(())
}

#[tauri::command]
pub async fn uninstall_skill_cmd(skill_path: String) -> Result<(), String> {
    crate::skills::installer::uninstall_skill(&skill_path)
}

#[tauri::command]
pub async fn toggle_skill_cmd(skill_id: String, agent: String, enabled: bool) -> Result<(), String> {
    crate::skills::installer::toggle_skill(&skill_id, &agent, enabled)
}

#[tauri::command]
pub async fn read_skill_files(skill_path: String) -> Result<crate::skills::FileTreeNode, String> {
    Ok(crate::skills::scanner::read_file_tree(&skill_path))
}

#[tauri::command]
pub async fn read_skill_file_content(file_path: String) -> Result<String, String> {
    std::fs::read_to_string(&file_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_packs_cmd() -> Result<Vec<crate::skills::SkillPack>, String> {
    Ok(crate::skills::registry::list_packs())
}

#[tauri::command]
pub async fn create_pack_cmd(pack: crate::skills::SkillPack) -> Result<(), String> {
    crate::skills::registry::create_pack(pack)
}

#[tauri::command]
pub async fn update_pack_cmd(pack: crate::skills::SkillPack) -> Result<(), String> {
    crate::skills::registry::update_pack(pack)
}

#[tauri::command]
pub async fn delete_pack_cmd(id: String) -> Result<(), String> {
    crate::skills::registry::delete_pack(&id)
}

#[tauri::command]
pub async fn configure_sync_cmd(config: crate::skills::SyncConfig) -> Result<(), String> {
    crate::skills::registry::set_sync_config(config)
}

#[tauri::command]
pub async fn push_sync_cmd() -> Result<crate::skills::SyncResult, String> {
    crate::skills::sync::push_to_github()
}

#[tauri::command]
pub async fn pull_sync_cmd() -> Result<crate::skills::SyncResult, String> {
    crate::skills::sync::pull_from_github()
}

#[tauri::command]
pub async fn sync_agent_to_agent_cmd(from: String, to: String) -> Result<crate::skills::SyncPreview, String> {
    crate::skills::sync::sync_agent_to_agent(&from, &to)
}

#[tauri::command]
pub async fn execute_agent_sync_cmd(from: String, to: String) -> Result<(), String> {
    crate::skills::sync::execute_agent_sync(&from, &to)
}

#[tauri::command]
pub async fn export_backup_cmd(path: String) -> Result<(), String> {
    crate::skills::sync::export_backup(&path)
}

#[tauri::command]
pub async fn import_backup_cmd(path: String) -> Result<(), String> {
    crate::skills::sync::import_backup(&path)
}

#[tauri::command]
pub async fn get_registry_metadata() -> Result<crate::skills::registry::Metadata, String> {
    Ok(crate::skills::registry::load())
}
```

- [ ] **Step 2: Register commands in Tauri builder in `lib.rs`**

Find the `.invoke_handler(tauri::generate_handler![...])` call and add all new commands to the array.

- [ ] **Step 3: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat(skills): register all skill management IPC commands"
```

---

## Task 7: Frontend — Zustand Store & Tauri API

**Files:**
- Create: `src/stores/skillStore.ts`
- Create: `src/services/skillApi.ts`

- [ ] **Step 1: Create `src/services/skillApi.ts`**

```typescript
import { invoke } from '@tauri-apps/api/core'

export interface AgentSkillState {
  agent: string
  installPath: string
  installMode: 'direct' | 'symlink'
  enabled: boolean
}

export interface ScannedSkill {
  id: string
  name: string
  description: string
  skillType: 'skill' | 'mcp'
  icon: string | null
  source: 'island' | 'local'
  originUrl: string | null
  hasUpdate: boolean
  filePath: string
  fileSize: number
  modifiedAt: number
  agents: AgentSkillState[]
}

export interface SkillPack {
  id: string
  name: string
  description: string
  skills: string[]
  targetAgents: string[]
}

export interface SyncConfig {
  method: string
  githubRepo: string | null
  githubToken: string | null
  lastSyncAt: string | null
  autoSync: boolean
}

export interface FileTreeNode {
  name: string
  nodeType: 'file' | 'dir'
  path: string
  children: FileTreeNode[] | null
}

export interface SyncResult {
  success: boolean
  message: string
  conflicts: { skillId: string; localModified: string; remoteModified: string }[]
}

export interface SyncPreview {
  toCopy: number
  toSkip: number
  toUpdate: number
  details: string[]
}

export interface TargetConfig {
  agent: string
  installMode: 'direct' | 'symlink'
}

const isTauri = '__TAURI__' in window

export const skillApi = {
  scanAll: () => isTauri
    ? invoke<Record<string, ScannedSkill[]>>('scan_all_skills')
    : Promise.resolve({}),

  scanAgent: (agent: string) => isTauri
    ? invoke<ScannedSkill[]>('scan_agent_skills', { agent })
    : Promise.resolve([]),

  install: (source: string, targets: TargetConfig[], mode: 'direct' | 'symlink') => isTauri
    ? invoke('install_skill_cmd', { source, targets, mode })
    : Promise.resolve(),

  uninstall: (skillPath: string) => isTauri
    ? invoke('uninstall_skill_cmd', { skillPath })
    : Promise.resolve(),

  toggle: (skillId: string, agent: string, enabled: boolean) => isTauri
    ? invoke('toggle_skill_cmd', { skillId, agent, enabled })
    : Promise.resolve(),

  readFileTree: (skillPath: string) => isTauri
    ? invoke<FileTreeNode>('read_skill_files', { skillPath })
    : Promise.resolve({ name: '', nodeType: 'dir' as const, path: '', children: [] }),

  readFileContent: (filePath: string) => isTauri
    ? invoke<string>('read_skill_file_content', { filePath })
    : Promise.resolve(''),

  listPacks: () => isTauri
    ? invoke<SkillPack[]>('list_packs_cmd')
    : Promise.resolve([]),

  createPack: (pack: SkillPack) => isTauri
    ? invoke('create_pack_cmd', { pack })
    : Promise.resolve(),

  updatePack: (pack: SkillPack) => isTauri
    ? invoke('update_pack_cmd', { pack })
    : Promise.resolve(),

  deletePack: (id: string) => isTauri
    ? invoke('delete_pack_cmd', { id })
    : Promise.resolve(),

  configureSyncConfig: (config: SyncConfig) => isTauri
    ? invoke('configure_sync_cmd', { config })
    : Promise.resolve(),

  pushSync: () => isTauri
    ? invoke<SyncResult>('push_sync_cmd')
    : Promise.resolve({ success: true, message: '', conflicts: [] }),

  pullSync: () => isTauri
    ? invoke<SyncResult>('pull_sync_cmd')
    : Promise.resolve({ success: true, message: '', conflicts: [] }),

  syncAgentPreview: (from: string, to: string) => isTauri
    ? invoke<SyncPreview>('sync_agent_to_agent_cmd', { from, to })
    : Promise.resolve({ toCopy: 0, toSkip: 0, toUpdate: 0, details: [] }),

  executeAgentSync: (from: string, to: string) => isTauri
    ? invoke('execute_agent_sync_cmd', { from, to })
    : Promise.resolve(),

  exportBackup: (path: string) => isTauri
    ? invoke('export_backup_cmd', { path })
    : Promise.resolve(),

  importBackup: (path: string) => isTauri
    ? invoke('import_backup_cmd', { path })
    : Promise.resolve(),

  getMetadata: () => isTauri
    ? invoke<{ sources: Record<string, { origin: string }>; packs: SkillPack[]; sync: SyncConfig | null }>('get_registry_metadata')
    : Promise.resolve({ sources: {}, packs: [], sync: null }),
}
```

- [ ] **Step 2: Create `src/stores/skillStore.ts`**

```typescript
import { create } from 'zustand'
import type { ScannedSkill, SkillPack, SyncConfig, FileTreeNode } from '../services/skillApi'
import { skillApi } from '../services/skillApi'

interface SkillState {
  skills: ScannedSkill[]
  packs: SkillPack[]
  syncConfig: SyncConfig | null
  loading: boolean
  scanning: boolean
  activeTab: 'skills' | 'packs' | 'sync'
  selectedSkillId: string | null
  detailOpen: boolean
  fileTree: FileTreeNode | null
  fileContent: string
  selectedFilePath: string
  searchQuery: string
  typeFilter: 'all' | 'skill' | 'mcp'
  agentFilter: string
  batchMode: boolean
  batchSelected: Set<string>
}

interface SkillActions {
  loadAll: () => Promise<void>
  setTab: (tab: SkillState['activeTab']) => void
  selectSkill: (id: string) => void
  closeDetail: () => void
  loadFileTree: (skillPath: string) => Promise<void>
  loadFileContent: (filePath: string) => Promise<void>
  setSearchQuery: (q: string) => void
  setTypeFilter: (f: SkillState['typeFilter']) => void
  setAgentFilter: (a: string) => void
  toggleBatchMode: () => void
  toggleBatchItem: (id: string) => void
  clearBatch: () => void
}

export const useSkillStore = create<SkillState & SkillActions>()((set, get) => ({
  skills: [],
  packs: [],
  syncConfig: null,
  loading: false,
  scanning: false,
  activeTab: 'skills',
  selectedSkillId: null,
  detailOpen: false,
  fileTree: null,
  fileContent: '',
  selectedFilePath: '',
  searchQuery: '',
  typeFilter: 'all',
  agentFilter: 'all',
  batchMode: false,
  batchSelected: new Set(),

  loadAll: async () => {
    set({ scanning: true })
    try {
      const [scanResult, meta] = await Promise.all([
        skillApi.scanAll(),
        skillApi.getMetadata(),
      ])

      // Merge scanned skills from all agents, dedup by id
      const merged = new Map<string, ScannedSkill>()
      for (const [, agentSkills] of Object.entries(scanResult)) {
        for (const skill of agentSkills) {
          if (merged.has(skill.id)) {
            const existing = merged.get(skill.id)!
            existing.agents.push(...skill.agents)
          } else {
            const source = meta.sources[skill.id] ? 'island' as const : skill.source
            merged.set(skill.id, { ...skill, source, originUrl: meta.sources[skill.id]?.origin ?? null })
          }
        }
      }

      set({
        skills: Array.from(merged.values()),
        packs: meta.packs,
        syncConfig: meta.sync,
        scanning: false,
      })
    } catch (e) {
      console.error('Failed to load skills:', e)
      set({ scanning: false })
    }
  },

  setTab: (tab) => set({ activeTab: tab }),

  selectSkill: (id) => {
    set({ selectedSkillId: id, detailOpen: true, fileTree: null, fileContent: '', selectedFilePath: '' })
    const skill = get().skills.find(s => s.id === id)
    if (skill) {
      get().loadFileTree(skill.filePath)
    }
  },

  closeDetail: () => set({ detailOpen: false, selectedSkillId: null }),

  loadFileTree: async (skillPath) => {
    try {
      const tree = await skillApi.readFileTree(skillPath)
      set({ fileTree: tree })
    } catch { /* ignore */ }
  },

  loadFileContent: async (filePath) => {
    try {
      const content = await skillApi.readFileContent(filePath)
      set({ fileContent: content, selectedFilePath: filePath })
    } catch { /* ignore */ }
  },

  setSearchQuery: (q) => set({ searchQuery: q }),
  setTypeFilter: (f) => set({ typeFilter: f }),
  setAgentFilter: (a) => set({ agentFilter: a }),
  toggleBatchMode: () => set(s => ({ batchMode: !s.batchMode, batchSelected: new Set() })),
  toggleBatchItem: (id) => set(s => {
    const next = new Set(s.batchSelected)
    if (next.has(id)) next.delete(id); else next.add(id)
    return { batchSelected: next }
  }),
  clearBatch: () => set({ batchSelected: new Set() }),
}))
```

- [ ] **Step 3: Commit**

```bash
git add src/services/skillApi.ts src/stores/skillStore.ts
git commit -m "feat(skills): add frontend store and Tauri API wrappers"
```

---

## Task 8: Frontend — SkillsSection Settings Entry

**Files:**
- Create: `src/components/settings/sections/SkillsSection.tsx`
- Create: `src/components/settings/sections/SkillsSection.css`

This task creates the top-level settings section that contains the three tabs and wires up the store. The child components (SkillListView, PackListView, SyncView, etc.) will be created in subsequent tasks.

- [ ] **Step 1: Create `SkillsSection.tsx`** — the container component with tab switching. Use placeholder `<div>` elements for child views, to be replaced in Tasks 9-12.

- [ ] **Step 2: Create `SkillsSection.css`** — styles following existing section patterns, referencing the mockup's dark theme.

- [ ] **Step 3: Register in settings routing** — add the section to the settings sidebar and view renderer (follow the pattern of `HookSection`).

- [ ] **Step 4: Add i18n keys** — add `skills.*` keys to `zh.json` and `en.json`.

- [ ] **Step 5: Verify** — run `pnpm dev`, navigate to Settings, confirm "技能管理" tab appears.

- [ ] **Step 6: Commit**

```bash
git add src/components/settings/sections/SkillsSection.tsx src/components/settings/sections/SkillsSection.css src/i18n/
git commit -m "feat(skills): add SkillsSection settings entry with tab layout"
```

---

## Task 9: Frontend — SkillListView + SkillCard

**Files:**
- Create: `src/components/skills/SkillListView.tsx`
- Create: `src/components/skills/SkillCard.tsx`

- [ ] **Step 1: Create `SkillCard.tsx`** — renders one skill row: icon, name, description, agent tags (color-coded), toggle switch, hover actions (view, update indicator, uninstall). Receives skill data and callbacks as props.

- [ ] **Step 2: Create `SkillListView.tsx`** — search bar, type filter chips (All/Skills/MCP), agent filter pills, batch bar, section headers ("通过 AgentBro 安装" / "本地发现"), skill card list, empty states. Reads from `useSkillStore`.

- [ ] **Step 3: Wire into SkillsSection** — replace the placeholder in the skills tab.

- [ ] **Step 4: Verify** — run `pnpm dev`, confirm skill list renders with filters working.

- [ ] **Step 5: Commit**

```bash
git add src/components/skills/
git commit -m "feat(skills): implement SkillListView and SkillCard components"
```

---

## Task 10: Frontend — SkillDetailSlider + FileTreeViewer

**Files:**
- Create: `src/components/skills/SkillDetailSlider.tsx`
- Create: `src/components/skills/FileTreeViewer.tsx`

- [ ] **Step 1: Create `FileTreeViewer.tsx`** — recursive tree component (expandable/collapsible dirs, file click to preview, mini search filter). Code preview panel with line numbers and basic syntax highlighting (frontmatter, headings, YAML keys).

- [ ] **Step 2: Create `SkillDetailSlider.tsx`** — slide-in panel from right (Framer Motion `AnimatePresence`). Sections: agent install status with per-agent toggles, basic info, pack membership with "add to pack" action, file browser, footer actions (open file, add to pack, uninstall with confirmation).

- [ ] **Step 3: Wire into SkillsSection** — render the slider when `detailOpen` is true.

- [ ] **Step 4: Verify** — click a skill card, confirm slide-in panel appears with file tree.

- [ ] **Step 5: Commit**

```bash
git add src/components/skills/SkillDetailSlider.tsx src/components/skills/FileTreeViewer.tsx
git commit -m "feat(skills): implement detail slide-in panel with file tree viewer"
```

---

## Task 11: Frontend — PackListView + PackCard + PackDialog

**Files:**
- Create: `src/components/skills/PackListView.tsx`
- Create: `src/components/skills/PackCard.tsx`
- Create: `src/components/skills/PackDialog.tsx`

- [ ] **Step 1: Create `PackCard.tsx`** — displays pack name, description, skill chips, agent tags, install status. Action buttons: edit, enable all, disable all, push to agents, delete.

- [ ] **Step 2: Create `PackDialog.tsx`** — modal for creating/editing a pack. Fields: name, description, skill picker (checkboxes from current skills), target agents (checkboxes).

- [ ] **Step 3: Create `PackListView.tsx`** — search, pack card list, "create new" dashed card.

- [ ] **Step 4: Wire into SkillsSection packs tab.**

- [ ] **Step 5: Verify** — switch to Packs tab, create a new pack, confirm it appears.

- [ ] **Step 6: Commit**

```bash
git add src/components/skills/PackListView.tsx src/components/skills/PackCard.tsx src/components/skills/PackDialog.tsx
git commit -m "feat(skills): implement pack management UI"
```

---

## Task 12: Frontend — SyncView + Dialogs

**Files:**
- Create: `src/components/skills/SyncView.tsx`
- Create: `src/components/skills/InstallDialog.tsx`
- Create: `src/components/skills/ConfirmDialog.tsx`

- [ ] **Step 1: Create `ConfirmDialog.tsx`** — reusable modal: icon, message, sub-message, cancel/confirm buttons. Used for uninstall confirm, agent sync confirm, conflict resolution.

- [ ] **Step 2: Create `InstallDialog.tsx`** — modal with source type radio (URL/GitHub/Local/skills.sh). Each source has its own form. Shared: target agents checkboxes, install mode radio. Install button calls `skillApi.install()`.

- [ ] **Step 3: Create `SyncView.tsx`** — GitHub sync section (repo input, token config, push/pull with progress), export/import section, agent-to-agent sync (agent dropdowns, preview, confirm), scanner status section with rescan button.

- [ ] **Step 4: Wire into SkillsSection sync tab and topbar buttons.**

- [ ] **Step 5: Verify** — test all three tabs end to end. Install dialog opens, sync UI shows scanner status.

- [ ] **Step 6: Commit**

```bash
git add src/components/skills/SyncView.tsx src/components/skills/InstallDialog.tsx src/components/skills/ConfirmDialog.tsx
git commit -m "feat(skills): implement sync view, install dialog, and confirm dialog"
```

---

## Task 13: Integration Testing & Polish

**Files:**
- Modify: various files for fixes discovered during testing

- [ ] **Step 1: Run full build**

Run: `cd src-tauri && cargo check` and `pnpm build`
Expected: both PASS

- [ ] **Step 2: Test with `pnpm tauri:dev`**

Verify:
1. Skills tab loads and scans installed agents
2. Click a skill → detail panel slides in from right
3. File tree loads and clicking files shows content
4. Toggle enables/disables per agent
5. Create a new pack
6. Agent-to-agent sync preview works
7. Install dialog source switching works

- [ ] **Step 3: Fix any issues found**

- [ ] **Step 4: Commit fixes**

```bash
git add -A
git commit -m "fix(skills): integration fixes from end-to-end testing"
```
