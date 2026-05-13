// ProcessTree — Build and query the system process tree
// Used for: terminal discovery, tmux detection, TTY mapping

use super::registry;
use std::collections::HashMap;

/// Information about a single process
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: u32,
    pub command: String,
    pub tty: Option<String>,
}

/// Terminal environment variables extracted from a process's environment
#[derive(Debug, Clone, Default)]
pub struct TerminalEnv {
    pub term_program: Option<String>,
    pub iterm_session_id: Option<String>,
    pub tmux: Option<String>,
    pub kitty_window_id: Option<String>,
    pub cf_bundle_identifier: Option<String>,
    pub wezterm_pane: Option<String>,
}

/// Terminal type classification derived from process environment + process tree
#[derive(Debug, Clone, PartialEq)]
pub enum TerminalType {
    ITerm2 { session_id: Option<String> },
    Ghostty,
    TerminalApp,
    WezTerm,
    Kitty { window_id: Option<String> },
    Tmux { env: Option<String> },
    Other(String),
    Unknown,
}

impl TerminalType {
    pub fn app_name(&self) -> &str {
        match self {
            TerminalType::ITerm2 { .. } => "iTerm2",
            TerminalType::Ghostty => "Ghostty",
            TerminalType::TerminalApp => "Terminal",
            TerminalType::WezTerm => "WezTerm",
            TerminalType::Kitty { .. } => "kitty",
            TerminalType::Tmux { .. } => "tmux",
            TerminalType::Other(s) => s.as_str(),
            TerminalType::Unknown => "",
        }
    }
}

/// Build a snapshot of the process tree using `ps`
pub fn build_tree() -> HashMap<u32, ProcessInfo> {
    let mut tree = HashMap::new();

    let output = match std::process::Command::new("/bin/ps")
        .args(["-eo", "pid,ppid,tty,comm"])
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return tree,
    };

    for line in output.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }

        let pid = match parts[0].parse::<u32>() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let ppid = match parts[1].parse::<u32>() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let tty = if parts[2] == "??" {
            None
        } else {
            Some(parts[2].to_string())
        };
        let command = parts[3..].join(" ");

        tree.insert(
            pid,
            ProcessInfo {
                pid,
                ppid,
                command,
                tty,
            },
        );
    }

    tree
}

/// Read a single environment variable for a process using kern.procargs2 (macOS).
/// Returns None if the variable is not set or cannot be read.
pub fn read_env_var(pid: u32, var_name: &str) -> Option<String> {
    let env = read_proc_environ(pid)?;
    let prefix = format!("{}=", var_name);
    env.lines()
        .find(|line| line.starts_with(&prefix))
        .map(|line| line[prefix.len()..].to_string())
}

/// Read all terminal-relevant environment variables for a process.
pub fn read_terminal_env(pid: u32, tree: &HashMap<u32, ProcessInfo>) -> TerminalEnv {
    let mut env = TerminalEnv::default();

    // Walk ancestry — start from agent PID up to terminal parent
    let mut current = pid;
    for _ in 0..20 {
        if current <= 1 {
            break;
        }
        if let Some(text) = read_proc_environ(current) {
            merge_terminal_env(&text, &mut env);
        }

        match tree.get(&current) {
            Some(info) => {
                if registry::is_terminal(&info.command) {
                    break;
                }
                current = info.ppid;
            }
            None => break,
        }
    }

    env
}

/// Detect terminal type from environment variables and process ancestry.
pub fn detect_terminal_type(pid: u32, tree: &HashMap<u32, ProcessInfo>) -> TerminalType {
    let env = read_terminal_env(pid, tree);

    // Bundle identifier is most precise
    if let Some(ref bundle) = env.cf_bundle_identifier {
        let lower = bundle.to_lowercase();
        if lower.contains("iterm") {
            return TerminalType::ITerm2 {
                session_id: env.iterm_session_id,
            };
        }
        if lower.contains("ghostty") {
            return TerminalType::Ghostty;
        }
        if lower.contains("wezterm") {
            return TerminalType::WezTerm;
        }
        if lower.contains("kitty") {
            return TerminalType::Kitty {
                window_id: env.kitty_window_id,
            };
        }
        if lower.contains("apple.terminal") {
            return TerminalType::TerminalApp;
        }
    }

    // TERM_PROGRAM is set in most terminals
    if let Some(ref tp) = env.term_program {
        let lower = tp.to_lowercase();
        if lower.contains("iterm") {
            return TerminalType::ITerm2 {
                session_id: env.iterm_session_id,
            };
        }
        if lower == "ghostty" {
            return TerminalType::Ghostty;
        }
        if lower.contains("wezterm") {
            return TerminalType::WezTerm;
        }
        if lower.contains("kitty") {
            return TerminalType::Kitty {
                window_id: env.kitty_window_id,
            };
        }
        if lower.contains("apple_terminal") || lower == "terminal" {
            return TerminalType::TerminalApp;
        }
    }

    // TMUX env var
    if env.tmux.is_some() {
        return TerminalType::Tmux { env: env.tmux };
    }

    // Fall back to process tree command name matching
    if let Some(cmd) = find_terminal_app_name(pid, tree) {
        let lower = cmd.to_lowercase();
        if lower.contains("iterm") {
            return TerminalType::ITerm2 { session_id: None };
        }
        if lower == "ghostty" {
            return TerminalType::Ghostty;
        }
        if lower.contains("wezterm") {
            return TerminalType::WezTerm;
        }
        if lower.contains("kitty") {
            return TerminalType::Kitty { window_id: None };
        }
        if lower.contains("terminal") {
            return TerminalType::TerminalApp;
        }
        if lower.contains("tmux") {
            return TerminalType::Tmux { env: None };
        }
        return TerminalType::Other(cmd);
    }

    TerminalType::Unknown
}

/// Read process environment via kern.procargs2 (macOS sysctl) using python3.
/// Returns a newline-separated list of KEY=VALUE strings, or None on failure.
fn read_proc_environ(pid: u32) -> Option<String> {
    // Use python3 to read kern.procargs2 — reliable across macOS versions
    let script = format!(
        r"
import ctypes, struct, sys
lib = ctypes.CDLL(None)
CTL_KERN, KERN_PROCARGS2 = 1, 49
mib = (ctypes.c_int * 3)(CTL_KERN, KERN_PROCARGS2, {pid})
size = ctypes.c_size_t(256 * 1024)
buf = ctypes.create_string_buffer(size.value)
if lib.sysctl(mib, 3, buf, ctypes.byref(size), None, 0) != 0:
    sys.exit(1)
data = buf.raw[:size.value]
argc = struct.unpack_from('<I', data, 0)[0]
pos = 4
while pos < len(data) and data[pos] != 0: pos += 1
while pos < len(data) and data[pos] == 0: pos += 1
for _ in range(argc):
    while pos < len(data) and data[pos] != 0: pos += 1
    while pos < len(data) and data[pos] == 0: pos += 1
rest = data[pos:]
for part in rest.split(b'\x00'):
    try:
        s = part.decode('utf-8', errors='replace')
        if '=' in s: print(s)
    except: pass
",
        pid = pid
    );

    let output = std::process::Command::new("python3")
        .args(["-c", &script])
        .output()
        .ok()?;

    if output.status.success() && !output.stdout.is_empty() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        None
    }
}

fn merge_terminal_env(text: &str, env: &mut TerminalEnv) {
    for line in text.lines() {
        let line = line.trim();
        if env.term_program.is_none() {
            if let Some(v) = line.strip_prefix("TERM_PROGRAM=") {
                env.term_program = Some(v.to_string());
            }
        }
        if env.iterm_session_id.is_none() {
            if let Some(v) = line.strip_prefix("ITERM_SESSION_ID=") {
                env.iterm_session_id = Some(v.to_string());
            }
        }
        if env.tmux.is_none() {
            if let Some(v) = line.strip_prefix("TMUX=") {
                env.tmux = Some(v.to_string());
            }
        }
        if env.kitty_window_id.is_none() {
            if let Some(v) = line.strip_prefix("KITTY_WINDOW_ID=") {
                env.kitty_window_id = Some(v.to_string());
            }
        }
        if env.cf_bundle_identifier.is_none() {
            if let Some(v) = line.strip_prefix("__CFBundleIdentifier=") {
                env.cf_bundle_identifier = Some(v.to_string());
            }
        }
        if env.wezterm_pane.is_none() {
            if let Some(v) = line.strip_prefix("WEZTERM_PANE=") {
                env.wezterm_pane = Some(v.to_string());
            }
        }
    }
}

/// Check if a process has tmux in its parent chain
pub fn is_in_tmux(pid: u32, tree: &HashMap<u32, ProcessInfo>) -> bool {
    let mut current = pid;
    for _ in 0..20 {
        if current <= 1 {
            break;
        }
        match tree.get(&current) {
            Some(info) => {
                if info.command.to_lowercase().contains("tmux") {
                    return true;
                }
                current = info.ppid;
            }
            None => break,
        }
    }
    false
}

/// Walk up the process tree to find the terminal app PID
pub fn find_terminal_pid(pid: u32, tree: &HashMap<u32, ProcessInfo>) -> Option<u32> {
    let mut current = pid;
    for _ in 0..20 {
        if current <= 1 {
            break;
        }
        match tree.get(&current) {
            Some(info) => {
                if registry::is_terminal(&info.command) {
                    return Some(current);
                }
                current = info.ppid;
            }
            None => break,
        }
    }
    None
}

/// Get the terminal app name for a process by walking its parent chain
pub fn find_terminal_app_name(pid: u32, tree: &HashMap<u32, ProcessInfo>) -> Option<String> {
    let mut current = pid;
    for _ in 0..20 {
        if current <= 1 {
            break;
        }
        match tree.get(&current) {
            Some(info) => {
                if registry::is_terminal(&info.command) {
                    return Some(info.command.clone());
                }
                current = info.ppid;
            }
            None => break,
        }
    }
    None
}

/// Check if target_pid is a descendant of ancestor_pid
pub fn is_descendant(target_pid: u32, ancestor_pid: u32, tree: &HashMap<u32, ProcessInfo>) -> bool {
    let mut current = target_pid;
    for _ in 0..50 {
        if current == ancestor_pid {
            return true;
        }
        if current <= 1 {
            break;
        }
        match tree.get(&current) {
            Some(info) => current = info.ppid,
            None => break,
        }
    }
    false
}

/// Get the TTY device for a given process
pub fn get_tty(pid: u32, tree: &HashMap<u32, ProcessInfo>) -> Option<String> {
    tree.get(&pid).and_then(|info| info.tty.clone())
}
