use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub fn command_exists(binary: &str) -> bool {
    find_binary(binary).is_some()
}

pub fn find_binary(binary: &str) -> Option<PathBuf> {
    if let Some(path) = which(binary) {
        return Some(path);
    }

    candidate_dirs()
        .into_iter()
        .map(|dir| dir.join(binary))
        .find(|path| path.is_file())
}

fn which(binary: &str) -> Option<PathBuf> {
    std::process::Command::new("which")
        .arg(binary)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

fn user_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "/bin/zsh".to_string())
}

fn is_fish(shell: &str) -> bool {
    shell.ends_with("/fish")
}

fn shell_var_fallback_cmd(var: &str) -> String {
    let shell = user_shell();
    if is_fish(&shell) {
        format!("printf '%s' \"${}\"", var)
    } else {
        format!("printf '%s' \"${{{}:-}}\"", var)
    }
}

pub fn login_shell_var(var: &str) -> Option<String> {
    let shell = user_shell();
    let cmd = shell_var_fallback_cmd(var);
    let output = std::process::Command::new(&shell)
        .args(["-lc", &cmd])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let val = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if val.is_empty() {
        None
    } else {
        Some(val)
    }
}

fn login_shell_path() -> Option<Vec<PathBuf>> {
    let shell = user_shell();
    let cmd = if is_fish(&shell) {
        "printf '%s' (string join ':' $PATH)"
    } else {
        "printf '%s' \"$PATH\""
    };
    let output = std::process::Command::new(shell)
        .args(["-lc", cmd])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path_str.is_empty() {
        return None;
    }
    Some(std::env::split_paths(&path_str).collect())
}

fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = BTreeSet::new();

    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }

    if let Some(shell_paths) = login_shell_path() {
        dirs.extend(shell_paths);
    }

    dirs.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
        PathBuf::from("/usr/sbin"),
        PathBuf::from("/sbin"),
    ]);

    if let Some(home) = dirs::home_dir() {
        dirs.extend([
            home.join(".local").join("bin"),
            home.join(".npm-global").join("bin"),
            home.join(".bun").join("bin"),
            home.join(".cargo").join("bin"),
        ]);
        dirs.extend(nvm_node_bins(&home));
        dirs.extend(volta_bin(&home));
        dirs.extend(mise_shims(&home));
    }

    dirs.into_iter().collect()
}

fn nvm_node_bins(home: &Path) -> Vec<PathBuf> {
    let versions = home.join(".nvm").join("versions").join("node");
    let Ok(entries) = std::fs::read_dir(versions) else {
        return Vec::new();
    };

    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("bin"))
        .filter(|path| path.is_dir())
        .collect()
}

fn volta_bin(home: &Path) -> Option<PathBuf> {
    let path = home.join(".volta").join("bin");
    path.is_dir().then_some(path)
}

fn mise_shims(home: &Path) -> Option<PathBuf> {
    let path = home.join(".local").join("share").join("mise").join("shims");
    path.is_dir().then_some(path)
}
