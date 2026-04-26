// ProcessTree — Build and query the system process tree
// Used for: terminal discovery, tmux detection, TTY mapping

use std::collections::HashMap;
use super::registry;

/// Information about a single process
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: u32,
    pub command: String,
    pub tty: Option<String>,
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
        let tty = if parts[2] == "??" { None } else { Some(parts[2].to_string()) };
        let command = parts[3..].join(" ");

        tree.insert(pid, ProcessInfo { pid, ppid, command, tty });
    }

    tree
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
