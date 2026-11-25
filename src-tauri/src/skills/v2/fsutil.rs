//! Filesystem helpers for Skill Manager v2 — stable directory hashing,
//! SKILL.md frontmatter parsing, recursive copy, symlink creation with
//! fallback, and file-tree building.

#![allow(clippy::needless_question_mark)]

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Ignore these directory/file names when hashing or copying skill contents.
pub fn is_ignored_entry(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".DS_Store"
            | "node_modules"
            | "target"
            | "__pycache__"
            | ".idea"
            | ".venv"
            | "venv"
            | "output"
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

/// Primary center library for Skill Manager v2.
pub fn default_center_path() -> PathBuf {
    home().join(".agentbro").join("skills")
}

/// Legacy roots that may contain skills from older AgentBro builds. v2 keeps
/// them discoverable for migration/diagnosis, but new writes always go to
/// `default_center_path()`.
pub fn all_center_dirs() -> Vec<PathBuf> {
    vec![
        home().join(".agentbro").join("skills"),
        home().join(".agents").join("skills"),
    ]
}

pub fn default_sqlite_path() -> PathBuf {
    agentbro_home()
        .join("skill-manager")
        .join("skill-manager.db")
}

pub fn default_snapshot_path() -> PathBuf {
    default_center_path().join("agentbro-skills.snapshot.json")
}

pub fn settings_path() -> PathBuf {
    agentbro_home().join("skill-manager").join("settings.json")
}

pub fn expand_tilde(p: &str) -> PathBuf {
    if p == "~" {
        return home();
    }
    let relative = p.strip_prefix("~/");
    #[cfg(windows)]
    let relative = relative.or_else(|| p.strip_prefix("~\\"));
    relative.map_or_else(|| PathBuf::from(p), |rest| home().join(rest))
}

pub fn normalized_path(path: &Path) -> PathBuf {
    path.canonicalize()
        .unwrap_or_else(|_| resolve_non_existing_path(path))
}

pub fn is_path_within(base: &Path, target: &Path) -> bool {
    let base_resolved = normalized_path(base);
    let target_resolved = target
        .canonicalize()
        .unwrap_or_else(|_| resolve_non_existing_path(target));
    target_resolved.starts_with(base_resolved)
}

fn resolve_non_existing_path(path: &Path) -> PathBuf {
    for ancestor in path.ancestors() {
        if let Ok(real) = ancestor.canonicalize() {
            let mut out = real;
            if let Ok(rest) = path.strip_prefix(ancestor) {
                out.push(rest);
            }
            return normalize_path(&out);
        }
    }
    normalize_path(path)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn ensure_destination_not_inside_source(src: &Path, dst: &Path) -> Result<(), String> {
    let Ok(src_resolved) = src.canonicalize() else {
        return Ok(());
    };
    let dst_resolved = dst.canonicalize().ok().or_else(|| {
        let parent = dst.parent()?.canonicalize().ok()?;
        let name = dst.file_name()?;
        Some(parent.join(name))
    });
    if let Some(dst_resolved) = dst_resolved {
        if dst_resolved.starts_with(&src_resolved) {
            return Err(format!(
                "Destination {} is inside source {}; refusing recursive copy",
                dst.display(),
                src.display()
            ));
        }
    }
    Ok(())
}

/// Open a path or URL in the OS-default app. Best-effort.
pub fn open_path(target: &str) -> Result<(), String> {
    let target = target.trim();
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg(expand_tilde(target));
        c
    };
    #[cfg(target_os = "windows")]
    {
        return open_path_windows(target);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(expand_tilde(target));
        c
    };
    #[cfg(not(target_os = "windows"))]
    cmd.spawn().map(|_| ()).map_err(|e| format!("open: {}", e))
}

/// Reveal a path in the file manager without resolving symlinks to their destination.
pub fn reveal_path(target: &str) -> Result<(), String> {
    let target = expand_tilde(target);
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&target)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("reveal: {}", e))
    }
    #[cfg(target_os = "windows")]
    {
        windows_shell::reveal(&target)
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        open_path(&target.display().to_string())
    }
}

#[cfg(target_os = "windows")]
fn open_path_windows(target: &str) -> Result<(), String> {
    if target.is_empty() {
        return Err("open: target path is empty".to_string());
    }

    if is_url(target) {
        return std::process::Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", target])
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("open: {}", e));
    }

    windows_shell::open(&expand_tilde(target))
}

#[cfg(target_os = "windows")]
mod windows_shell {
    use crate::platform::windows_shell::{ordinary_path, ShellApartment};
    use std::collections::HashSet;
    use std::io;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};
    use windows::core::{Interface, BSTR};
    use windows::Win32::System::Com::{CoCreateInstance, IDispatch, CLSCTX_LOCAL_SERVER};
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Shell::{
        Folder, Folder2, IShellFolderViewDual, IShellWindows, IWebBrowserApp, ShellWindows,
        SVSI_DESELECTOTHERS, SVSI_ENSUREVISIBLE, SVSI_FOCUSED, SVSI_SELECT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    pub(super) fn open(path: &Path) -> Result<(), String> {
        let target =
            prepare_path(path).map_err(|error| format!("open {}: {error}", path.display()))?;
        let apartment = ShellApartment::initialize()?;
        apartment
            .open(target.as_os_str())
            .map_err(|error| format!("open {}: {error}", path.display()))
    }

    pub(super) fn reveal(path: &Path) -> Result<(), String> {
        let target =
            prepare_path(path).map_err(|error| format!("reveal {}: {error}", path.display()))?;
        let apartment = ShellApartment::initialize()?;
        let Some(parent) = target.parent() else {
            return apartment
                .open(target.as_os_str())
                .map_err(|error| format!("reveal {}: {error}", path.display()));
        };
        let filename = BSTR::from_wide(
            &target
                .file_name()
                .ok_or_else(|| format!("Could not identify {}", path.display()))?
                .encode_wide()
                .collect::<Vec<_>>(),
        );
        let shell: IShellWindows =
            unsafe { CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER) }
                .map_err(|error| format!("connect to File Explorer: {error}"))?;
        let existing_windows: HashSet<_> = explorer_windows(&shell)
            .filter_map(|window| unsafe { window.HWND() }.ok().map(|id| id.0))
            .collect();
        apartment
            .open(parent.as_os_str())
            .map_err(|error| format!("open {}: {error}", parent.display()))?;

        let start = Instant::now();
        loop {
            let foreground = unsafe { GetForegroundWindow() } as isize;
            for window in explorer_windows(&shell) {
                let Ok(id) = (unsafe { window.HWND() }) else {
                    continue;
                };
                if id.0 != foreground
                    && existing_windows.contains(&id.0)
                    && start.elapsed() < Duration::from_secs(1)
                {
                    continue;
                }
                if select_in_explorer(&window, parent, &filename, &target)
                    .is_ok_and(|selected| selected)
                {
                    return Ok(());
                }
            }
            if start.elapsed() >= Duration::from_secs(5) {
                return Err(format!("File Explorer did not select {}", path.display()));
            }
            std::thread::sleep(Duration::from_millis(75));
        }
    }

    fn explorer_windows(shell: &IShellWindows) -> impl Iterator<Item = IWebBrowserApp> + '_ {
        let count = unsafe { shell.Count() }.unwrap_or(0);
        (0..count).filter_map(|index| {
            let window = unsafe { shell.Item(&VARIANT::from(index)) }.ok()?;
            window.cast().ok()
        })
    }

    fn select_in_explorer(
        window: &IWebBrowserApp,
        parent: &Path,
        filename: &BSTR,
        target: &Path,
    ) -> windows::core::Result<bool> {
        let view: IShellFolderViewDual = unsafe { window.Document()? }.cast()?;
        let folder: Folder = unsafe { view.Folder()? };
        let location = unsafe { folder.cast::<Folder2>()?.Self_()?.Path()? };
        if !matches_shell_path(&location, parent) {
            return Ok(false);
        }

        let item: IDispatch = unsafe { folder.ParseName(filename)? }.cast()?;
        let item = VARIANT::from(item);
        let flags = SVSI_SELECT.0 | SVSI_DESELECTOTHERS.0 | SVSI_ENSUREVISIBLE.0 | SVSI_FOCUSED.0;
        unsafe { view.SelectItem(&item, flags)? };
        let selected = unsafe { view.SelectedItems()? };
        if unsafe { selected.Count()? } != 1 {
            return Ok(false);
        }
        let selected_path = unsafe { selected.Item(&VARIANT::from(0i32))?.Path()? };
        Ok(matches_shell_path(&selected_path, target))
    }

    fn matches_shell_path(location: &BSTR, target: &Path) -> bool {
        let location = PathBuf::from(std::ffi::OsString::from_wide(location));
        location == target
            || (location.to_string_lossy().to_lowercase()
                == target.to_string_lossy().to_lowercase()
                && location.canonicalize().is_ok_and(|resolved| {
                    target
                        .canonicalize()
                        .is_ok_and(|expected| expected == resolved)
                }))
    }

    fn prepare_path(path: &Path) -> io::Result<PathBuf> {
        let path = std::path::absolute(path)?;
        std::fs::symlink_metadata(&path)?;
        let ordinary = ordinary_path(&path).filter(|candidate| {
            candidate
                .canonicalize()
                .is_ok_and(|target| path.canonicalize().is_ok_and(|original| target == original))
        });
        Ok(ordinary.unwrap_or(path))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn shell_paths_preserve_drive_and_unc_locations() {
            assert_eq!(
                ordinary_path(Path::new(r"\\?\C:\Users\Some User\项目, fixture")),
                Some(PathBuf::from(r"C:\Users\Some User\项目, fixture"))
            );
            assert_eq!(
                ordinary_path(Path::new(r"\\?\UNC\server\share\文件")),
                Some(PathBuf::from(r"\\server\share\文件"))
            );
            assert!(ordinary_path(Path::new(r"\\?\Volume{example}\stay")).is_none());
        }

        #[test]
        fn shell_path_returns_an_error_for_a_missing_target() {
            let path = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
            let error = prepare_path(&path).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::NotFound);
        }
    }
}

#[cfg(target_os = "windows")]
fn is_url(target: &str) -> bool {
    target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
        || target.starts_with("agentbro:")
        || target.starts_with("ccswitch:")
}

/// Stable hash over a directory's files: relative paths + contents, sorted,
/// ignoring noise entries. Returns hex digest.
pub fn hash_dir(dir: &Path) -> String {
    hash_dir_with_root(dir, true)
}

pub fn hash_dir_contents(dir: &Path) -> String {
    hash_dir_with_root(dir, false)
}

fn hash_dir_with_root(dir: &Path, include_root: bool) -> String {
    let mut entries: Vec<(PathBuf, PathBuf)> = Vec::new();
    collect_files(dir, dir, &mut entries);
    entries.sort_by(|a, b| a.1.cmp(&b.1));

    let mut hasher = Sha256::new();
    if include_root {
        hasher.update(
            dir.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
                .as_bytes(),
        );
    }
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
        } else if (ch == ' ' || ch == '.' || ch == '/' || ch == '\\')
            && !prev_dash
            && !out.is_empty()
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
        self.map
            .get("description")
            .map(String::as_str)
            .unwrap_or("")
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
    crate::skills::frontmatter::parse_content(content)
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

pub(crate) fn replace_skill_directory<T>(
    destination: &Path,
    prepare: impl FnOnce(&Path) -> Result<(), String>,
    commit: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let parent = destination
        .parent()
        .ok_or_else(|| format!("Skill path '{}' has no parent.", destination.display()))?;
    fs::create_dir_all(parent).map_err(|error| format!("Create {}: {error}", parent.display()))?;
    let staging = parent.join(format!(".agentbro-replace-{}.tmp", uuid::Uuid::new_v4()));
    fs::create_dir(&staging)
        .map_err(|error| format!("Prepare Skill replacement '{}': {error}", staging.display()))?;
    let pending = staging.join("incoming");
    let backup = staging.join("previous");
    let mut saved_previous = false;
    let mut activated = false;

    let result = (|| {
        prepare(&pending)?;
        if !is_skill_dir(&pending) {
            return Err(format!(
                "Replacement Skill '{}' is missing SKILL.md.",
                destination.display()
            ));
        }
        match fs::rename(destination, &backup) {
            Ok(()) => saved_previous = true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Back up existing Skill '{}': {error}",
                    destination.display()
                ));
            }
        }
        fs::rename(&pending, destination)
            .map_err(|error| format!("Activate Skill '{}': {error}", destination.display()))?;
        activated = true;
        commit()
    })();

    if let Err(error) = &result {
        let restore = (|| {
            if activated {
                fs::rename(destination, &pending)?;
            }
            if saved_previous {
                match destination.symlink_metadata() {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                    Ok(_) => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::AlreadyExists,
                            "The replacement path is occupied",
                        ))
                    }
                }
                fs::rename(&backup, destination)?;
            }
            Ok::<(), std::io::Error>(())
        })();
        if let Err(restore_error) = restore {
            return Err(format!(
                "{error} Restoration failed: {restore_error}. Skill recovery files are at '{}'.",
                staging.display()
            ));
        }
    }
    if let Err(error) = remove_path(&staging) {
        log::warn!("Could not clean up Skill replacement directory: {error}");
    }
    result
}

/// Recursive copy of a directory, skipping ignored entries. Used for `copy`
/// distribution and importing into the center library.
pub fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.is_dir() {
        return Err(format!("Source is not a directory: {}", src.display()));
    }
    ensure_destination_not_inside_source(src, dst)?;
    fs::create_dir_all(dst).map_err(|e| format!("create dir {}: {}", dst.display(), e))?;
    let rd = fs::read_dir(src).map_err(|e| format!("read dir {}: {}", src.display(), e))?;
    for entry in rd {
        let entry = entry.map_err(|error| format!("read dir {}: {error}", src.display()))?;
        let name = entry.file_name();
        if is_ignored_entry(&name.to_string_lossy()) {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        let ft = entry
            .file_type()
            .map_err(|error| format!("inspect {}: {error}", from.display()))?;
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
        let _ = target;
        Ok(false)
    }
    #[cfg(not(any(unix, windows)))]
    {
        Ok(false)
    }
}

/// Remove a file or symlink or directory tree at the given path.
pub fn remove_path(path: &Path) -> Result<(), String> {
    let file_type = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata.file_type(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("inspect {}: {error}", path.display())),
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTypeExt;

        if file_type.is_symlink_dir() {
            return fs::remove_dir(path)
                .map_err(|e| format!("remove directory link {}: {e}", path.display()));
        }
    }
    if file_type.is_dir() {
        fs::remove_dir_all(path).map_err(|e| format!("remove dir {}: {}", path.display(), e))
    } else {
        fs::remove_file(path).map_err(|e| format!("remove {}: {e}", path.display()))
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
            let resolved = resolve_symlink_chain(resolved);
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

pub fn resolved_symlink_target(path: &Path) -> Option<PathBuf> {
    match inspect_path(path) {
        PathKind::Symlink(target) => Some(target),
        _ => None,
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
pub fn build_file_tree(
    root: &Path,
    max_depth: u32,
) -> Option<crate::skills::v2::models::FileTreeNode> {
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
        if fs::symlink_metadata(path).is_err() {
            return None;
        }
        let is_dir = path.is_dir();
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

fn resolve_symlink_chain(mut path: PathBuf) -> PathBuf {
    for _ in 0..8 {
        if !path.is_symlink() {
            break;
        }
        let Ok(target) = fs::read_link(&path) else {
            break;
        };
        path = if target.is_absolute() {
            target
        } else {
            path.parent().unwrap_or(Path::new("")).join(target)
        };
    }
    path
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "windows")]
    #[test]
    fn windows_open_path_recognizes_urls_without_treating_paths_as_urls() {
        assert!(super::is_url("https://github.com/shirenchuang/agentbro"));
        assert!(super::is_url("mailto:hello@example.com"));
        assert!(super::is_url("agentbro://settings"));
        assert!(!super::is_url(r"C:\Users\admin\Documents\github\agentbro"));
        assert!(!super::is_url(r"\\server\share\agentbro"));
    }
}
