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
    pub waveterm_block_id: Option<String>,
    pub waveterm_tab_id: Option<String>,
    pub waveterm_jwt: Option<String>,
    pub zellij_pane_id: Option<String>,
    pub zellij_session_name: Option<String>,
    pub cmux_surface_id: Option<String>,
    pub cmux_workspace_id: Option<String>,
}

/// Terminal type classification derived from process environment + process tree
#[derive(Debug, Clone, PartialEq)]
pub enum TerminalType {
    ITerm2 {
        session_id: Option<String>,
    },
    Ghostty,
    TerminalApp,
    WezTerm,
    Wave {
        block_id: Option<String>,
    },
    Zellij {
        pane_id: Option<String>,
        session_name: Option<String>,
    },
    Cmux {
        surface_id: Option<String>,
        workspace_id: Option<String>,
    },
    Kaku,
    Kitty {
        window_id: Option<String>,
    },
    Tmux {
        env: Option<String>,
    },
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
            TerminalType::Wave { .. } => "Wave",
            TerminalType::Zellij { .. } => "Zellij",
            TerminalType::Cmux { .. } => "cmux",
            TerminalType::Kaku => "Kaku",
            TerminalType::Kitty { .. } => "kitty",
            TerminalType::Tmux { .. } => "tmux",
            TerminalType::Other(s) => s.as_str(),
            TerminalType::Unknown => "",
        }
    }
}

/// Build a snapshot of the process tree using `ps`
#[cfg(not(target_os = "windows"))]
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

/// Build a snapshot of the process tree using the Windows ToolHelp API.
#[cfg(target_os = "windows")]
pub fn build_tree() -> HashMap<u32, ProcessInfo> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    let mut tree = HashMap::new();

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return tree;
        }

        let mut entry = PROCESSENTRY32W::default();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                let pid = entry.th32ProcessID;
                tree.insert(
                    pid,
                    ProcessInfo {
                        pid,
                        ppid: entry.th32ParentProcessID,
                        command: process_entry_name(&entry),
                        tty: None,
                    },
                );

                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }

        let _ = CloseHandle(snapshot);
    }

    tree
}

#[cfg(target_os = "windows")]
fn process_entry_name(
    entry: &windows_sys::Win32::System::Diagnostics::ToolHelp::PROCESSENTRY32W,
) -> String {
    let len = entry
        .szExeFile
        .iter()
        .position(|ch| *ch == 0)
        .unwrap_or(entry.szExeFile.len());
    String::from_utf16_lossy(&entry.szExeFile[..len])
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
        if lower.contains("waveterm") || lower.contains("commandline.wave") {
            return TerminalType::Wave {
                block_id: env.waveterm_block_id,
            };
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
        if lower.contains("wave") {
            return TerminalType::Wave {
                block_id: env.waveterm_block_id,
            };
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
    if env.zellij_pane_id.is_some() || env.zellij_session_name.is_some() {
        return TerminalType::Zellij {
            pane_id: env.zellij_pane_id,
            session_name: env.zellij_session_name,
        };
    }
    if env.cmux_surface_id.is_some() || env.cmux_workspace_id.is_some() {
        return TerminalType::Cmux {
            surface_id: env.cmux_surface_id,
            workspace_id: env.cmux_workspace_id,
        };
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
        if lower.contains("wave") {
            return TerminalType::Wave { block_id: None };
        }
        if lower == "kaku" {
            return TerminalType::Kaku;
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
        if lower.contains("zellij") {
            return TerminalType::Zellij {
                pane_id: None,
                session_name: None,
            };
        }
        if lower.contains("cmux") {
            return TerminalType::Cmux {
                surface_id: None,
                workspace_id: None,
            };
        }
        if lower.contains("kaku") {
            return TerminalType::Kaku;
        }
        return TerminalType::Other(cmd);
    }

    TerminalType::Unknown
}

/// Read process environment via kern.procargs2 (macOS sysctl).
/// Returns a newline-separated list of KEY=VALUE strings, or None on failure.
///
/// Implemented as a direct libc::sysctl call. Previously this forked python3
/// once per ancestor (up to ~20 times per `read_terminal_env`), which under
/// rapid jump-button clicks could spawn dozens of python3 processes
/// concurrently and contribute to system-wide memory pressure.
#[cfg(target_os = "macos")]
fn read_proc_environ(pid: u32) -> Option<String> {
    const KERN_PROCARGS2: libc::c_int = 49;
    const BUF_SIZE: usize = 256 * 1024;

    let mut mib: [libc::c_int; 3] = [libc::CTL_KERN, KERN_PROCARGS2, pid as libc::c_int];
    let mut size: libc::size_t = BUF_SIZE;
    let mut buf: Vec<u8> = vec![0u8; BUF_SIZE];

    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as libc::c_uint,
            buf.as_mut_ptr() as *mut libc::c_void,
            &mut size as *mut libc::size_t,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 || size < 4 {
        return None;
    }
    buf.truncate(size);

    // Layout: [argc:u32][exec_path \0][padding \0..][argv 0..argc][env 0..]
    let argc = u32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    let mut pos = 4usize;

    while pos < buf.len() && buf[pos] != 0 {
        pos += 1;
    }
    while pos < buf.len() && buf[pos] == 0 {
        pos += 1;
    }
    for _ in 0..argc {
        while pos < buf.len() && buf[pos] != 0 {
            pos += 1;
        }
        while pos < buf.len() && buf[pos] == 0 {
            pos += 1;
        }
    }
    if pos >= buf.len() {
        return None;
    }

    let mut out = String::new();
    for part in buf[pos..].split(|&b| b == 0) {
        if part.is_empty() {
            continue;
        }
        if let Ok(s) = std::str::from_utf8(part) {
            if s.contains('=') {
                out.push_str(s);
                out.push('\n');
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

#[cfg(not(target_os = "macos"))]
fn read_proc_environ(_pid: u32) -> Option<String> {
    None
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
        if env.waveterm_block_id.is_none() {
            if let Some(v) = line.strip_prefix("WAVETERM_BLOCKID=") {
                env.waveterm_block_id = Some(v.to_string());
            }
        }
        if env.waveterm_tab_id.is_none() {
            if let Some(v) = line.strip_prefix("WAVETERM_TABID=") {
                env.waveterm_tab_id = Some(v.to_string());
            }
        }
        if env.waveterm_jwt.is_none() {
            if let Some(v) = line.strip_prefix("WAVETERM_JWT=") {
                env.waveterm_jwt = Some(v.to_string());
            }
        }
        if env.zellij_pane_id.is_none() {
            if let Some(v) = line.strip_prefix("ZELLIJ_PANE_ID=") {
                env.zellij_pane_id = Some(v.to_string());
            }
        }
        if env.zellij_session_name.is_none() {
            if let Some(v) = line.strip_prefix("ZELLIJ_SESSION_NAME=") {
                env.zellij_session_name = Some(v.to_string());
            }
        }
        if env.cmux_surface_id.is_none() {
            if let Some(v) = line.strip_prefix("CMUX_SURFACE_ID=") {
                env.cmux_surface_id = Some(v.to_string());
            }
        }
        if env.cmux_workspace_id.is_none() {
            if let Some(v) = line.strip_prefix("CMUX_WORKSPACE_ID=") {
                env.cmux_workspace_id = Some(v.to_string());
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

#[cfg(test)]
mod tests {
    use super::{build_tree, find_terminal_app_name, ProcessInfo};
    use std::collections::HashMap;

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_build_tree_contains_current_process() {
        let tree = build_tree();
        let current = tree
            .get(&std::process::id())
            .expect("current process should be present in Windows process snapshot");
        assert_eq!(current.pid, std::process::id());
        assert!(!current.command.is_empty());
    }

    #[test]
    fn find_terminal_app_name_skips_codefuse_claude_path() {
        let mut tree = HashMap::new();
        tree.insert(
            10,
            ProcessInfo {
                pid: 10,
                ppid: 9,
                command: "/Users/me/.codefuse/fuse/engine/bin/claude/2.1.139/claude".to_string(),
                tty: Some("ttys006".to_string()),
            },
        );
        tree.insert(
            9,
            ProcessInfo {
                pid: 9,
                ppid: 8,
                command: "cfuse".to_string(),
                tty: Some("ttys006".to_string()),
            },
        );
        tree.insert(
            8,
            ProcessInfo {
                pid: 8,
                ppid: 7,
                command: "-zsh".to_string(),
                tty: Some("ttys006".to_string()),
            },
        );
        tree.insert(
            7,
            ProcessInfo {
                pid: 7,
                ppid: 1,
                command: "/Applications/iTerm.app/Contents/MacOS/iTerm2".to_string(),
                tty: None,
            },
        );

        assert_eq!(
            find_terminal_app_name(10, &tree).as_deref(),
            Some("/Applications/iTerm.app/Contents/MacOS/iTerm2")
        );
    }
}
