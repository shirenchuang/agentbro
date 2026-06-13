//! Filesystem helpers for Skill Manager v2 — stable directory hashing,
//! SKILL.md frontmatter parsing, recursive copy, symlink creation with
//! fallback, and file-tree building.

#![allow(clippy::needless_question_mark)]

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// Ignore these directory/file names when hashing or copying skill contents.
pub fn is_ignored_entry(name: &str) -> bool {
    matches!(
        name,
        ".git" | ".DS_Store" | "node_modules" | "target" | "__pycache__" | ".idea"
    ) || name.ends_with(".tmp")
        || name.ends_with(".swp")
}

pub fn home() -> PathBuf {
    // Prefer $HOME directly so tests that set HOME get their temp dir even when
    // dirs::home_dir() resolves through a cached/OS path.
    if let Some(h) = std::env::var_os("HOME") {
        if !h.is_empty() {
            return PathBuf::from(h);
        }
    }
    dirs::home_dir().unwrap_or_else(std::env::temp_dir)
}

pub fn agentbro_home() -> PathBuf {
    home().join(".agentbro")
}

pub fn default_center_path() -> PathBuf {
    agentbro_home().join("skills")
}

pub fn default_sqlite_path() -> PathBuf {
    agentbro_home().join("skill-manager.db")
}

pub fn default_snapshot_path() -> PathBuf {
    default_center_path().join("agentbro-skills.snapshot.json")
}

pub fn settings_path() -> PathBuf {
    agentbro_home().join("skill-manager-settings.json")
}

pub fn expand_tilde(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        home().join(rest)
    } else {
        PathBuf::from(p)
    }
}

/// Open a path or URL in the OS-default app. Best-effort.
pub fn open_path(target: &str) -> Result<(), String> {
    let target = expand_tilde(target);
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg(&target);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", "", &target.display().to_string()]);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(&target);
        c
    };
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("open: {}", e))
}

/// Stable hash over a directory's files: relative paths + contents, sorted,
/// ignoring noise entries. Returns hex digest.
pub fn hash_dir(dir: &Path) -> String {
    let mut entries: Vec<(PathBuf, PathBuf)> = Vec::new();
    collect_files(dir, dir, &mut entries);
    entries.sort_by(|a, b| a.1.cmp(&b.1));

    let mut hasher = Sha256::new();
    hasher.update(dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default().as_bytes());
    for (abs, rel) in &entries {
        hasher.update(rel.to_string_lossy().as_bytes());
        hasher.update(b"\0");
        match fs::read(abs) {
            Ok(bytes) => hasher.update(&bytes),
            Err(_) => hasher.update(b"<missing>"),
        }
        hasher.update(b"\0");
    }
    hex_encode(&hasher.finalize())
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, PathBuf)>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if is_ignored_entry(&name) {
            continue;
        }
        let path = entry.path();
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_dir() {
            // Don't follow symlinks into other trees.
            if path.is_symlink() {
                continue;
            }
            collect_files(root, &path, out);
        } else if ft.is_file() {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push((path.clone(), rel.to_path_buf()));
            }
        }
    }
}

pub fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Sanitize a directory/file segment into a safe skill id.
pub fn sanitize_id(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in raw.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
            prev_dash = ch == '-';
        } else if (ch == ' ' || ch == '.' || ch == '/' || ch == '\\') && !prev_dash && !out.is_empty()
        {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "skill".to_string()
    } else {
        trimmed
    }
}

#[derive(Debug, Clone, Default)]
pub struct Frontmatter {
    pub map: std::collections::BTreeMap<String, String>,
}

impl Frontmatter {
    pub fn name(&self) -> Option<&str> {
        self.map.get("name").map(String::as_str)
    }
    pub fn description(&self) -> &str {
        self.map.get("description").map(String::as_str).unwrap_or("")
    }
}

/// A directory is a valid skill if it contains a SKILL.md file.
pub fn is_skill_dir(dir: &Path) -> bool {
    dir.join("SKILL.md").is_file()
}

pub fn read_frontmatter(dir: &Path) -> Frontmatter {
    let path = dir.join("SKILL.md");
    let Ok(content) = fs::read_to_string(&path) else {
        return Frontmatter::default();
    };
    Frontmatter {
        map: parse_frontmatter_text(&content),
    }
}

pub fn parse_frontmatter_text(content: &str) -> std::collections::BTreeMap<String, String> {
    let mut map = std::collections::BTreeMap::new();
    if !content.starts_with("---") {
        return map;
    }
    let Some(frontmatter) = content.split("---").nth(1) else {
        return map;
    };
    for line in frontmatter.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if !key.trim().is_empty() && !value.is_empty() {
            map.insert(key.trim().to_string(), value.to_string());
        }
    }
    map
}

/// Resolve a skill id from a directory: prefer frontmatter `name`, else
/// sanitized directory name.
pub fn infer_skill_id(dir: &Path) -> String {
    let fm = read_frontmatter(dir);
    if let Some(name) = fm.name().filter(|n| !n.trim().is_empty()) {
        return sanitize_id(name);
    }
    sanitize_id(
        dir.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
            .as_str(),
    )
}

/// Recursive copy of a directory, skipping ignored entries. Used for `copy`
/// distribution and importing into the center library.
pub fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.is_dir() {
        return Err(format!("Source is not a directory: {}", src.display()));
    }
    fs::create_dir_all(dst).map_err(|e| format!("create dir {}: {}", dst.display(), e))?;
    let rd = fs::read_dir(src).map_err(|e| format!("read dir {}: {}", src.display(), e))?;
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if is_ignored_entry(&name) {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_dir() {
            if from.is_symlink() {
                continue;
            }
            copy_dir_recursive(&from, &to)?;
        } else if ft.is_file() {
            fs::copy(&from, &to).map_err(|e| format!("copy {}: {}", from.display(), e))?;
        }
    }
    Ok(())
}

/// Attempt a symlink. On unix, relative symlink within the agent dir is not
/// safe across machines, so use an absolute target. Returns Ok(true) if link
/// created, Ok(false) if it could not be created (caller decides fallback).
pub fn try_symlink(target: &Path, link: &Path) -> Result<bool, String> {
    if link.exists() || link.is_symlink() {
        return Ok(false);
    }
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {}", parent.display(), e))?;
    }
    #[cfg(unix)]
    {
        let abs_target = if target.is_absolute() {
            target.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default().join(target)
        };
        match std::os::unix::fs::symlink(&abs_target, link) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
    #[cfg(windows)]
    {
        // Windows directory symlink requires privileges; treat as unavailable.
        Ok(false)
    }
    #[cfg(not(any(unix, windows)))]
    {
        Ok(false)
    }
}

/// Remove a file or symlink or directory tree at the given path.
pub fn remove_path(path: &Path) -> Result<(), String> {
    if path.is_symlink() {
        fs::remove_file(path).map_err(|e| format!("remove link {}: {}", path.display(), e))
    } else if path.is_dir() {
        fs::remove_dir_all(path).map_err(|e| format!("remove dir {}: {}", path.display(), e))
    } else if path.exists() {
        fs::remove_file(path).map_err(|e| format!("remove file {}: {}", path.display(), e))
    } else {
        Ok(())
    }
}

/// What does a target path currently point to?
pub fn inspect_path(path: &Path) -> PathKind {
    if !path.exists() && !path.is_symlink() {
        return PathKind::Missing;
    }
    if path.is_symlink() {
        if let Ok(target) = fs::read_link(path) {
            let resolved = if target.is_absolute() {
                target
            } else {
                path.parent().unwrap_or(Path::new("")).join(target)
            };
            if resolved.exists() {
                return PathKind::Symlink(resolved);
            }
            return PathKind::BrokenSymlink;
        }
        return PathKind::BrokenSymlink;
    }
    if path.is_dir() {
        PathKind::Dir
    } else {
        PathKind::File
    }
}

#[derive(Debug, Clone)]
pub enum PathKind {
    Missing,
    File,
    Dir,
    Symlink(PathBuf),
    BrokenSymlink,
}

/// Build a limited file tree for the detail panel (depth-bounded).
pub fn build_file_tree(root: &Path, max_depth: u32) -> Option<crate::skills::v2::models::FileTreeNode> {
    fn build(
        path: &Path,
        _root: &Path,
        depth: u32,
        max_depth: u32,
    ) -> Option<crate::skills::v2::models::FileTreeNode> {
        if depth > max_depth {
            return None;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let Ok(meta) = fs::symlink_metadata(path) else {
            return None;
        };
        let is_dir = meta.is_dir() && !meta.file_type().is_symlink();
        if is_dir {
            let mut children = Vec::new();
            if let Ok(rd) = fs::read_dir(path) {
                let mut entries: Vec<_> = rd.flatten().collect();
                entries.sort_by_key(|e| e.file_name());
                for entry in entries {
                    let ename = entry.file_name().to_string_lossy().to_string();
                    if crate::skills::v2::fsutil::is_ignored_entry(&ename) {
                        continue;
                    }
                    if let Some(child) = build(&entry.path(), _root, depth + 1, max_depth) {
                        children.push(child);
                    }
                }
            }
            Some(crate::skills::v2::models::FileTreeNode {
                name,
                node_type: "dir".to_string(),
                path: path.display().to_string(),
                children: Some(children),
            })
        } else {
            Some(crate::skills::v2::models::FileTreeNode {
                name,
                node_type: "file".to_string(),
                path: path.display().to_string(),
                children: None,
            })
        }
    }
    build(root, root, 0, max_depth)
}
