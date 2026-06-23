// Tmux integration — Find panes, switch windows, detect active pane

use super::process_tree;

/// A tmux target: session:window.pane
#[derive(Debug, Clone)]
pub struct TmuxTarget {
    pub session: String,
    pub window: String,
    pub pane: String,
}

impl TmuxTarget {
    /// Parse from "session:window.pane" string
    pub fn parse(target_str: &str) -> Option<Self> {
        let (session, rest) = target_str.split_once(':')?;
        let (window, pane) = rest.split_once('.')?;
        Some(Self {
            session: session.to_string(),
            window: window.to_string(),
            pane: pane.to_string(),
        })
    }

    /// Get the full target string
    pub fn target_string(&self) -> String {
        format!("{}:{}.{}", self.session, self.window, self.pane)
    }
}

/// Find the path to tmux binary
fn find_tmux_path() -> Option<String> {
    let tmux = crate::agents::executable::command_path("tmux");
    if crate::agents::executable::command_exists("tmux") {
        return Some(tmux.display().to_string());
    }

    // Check common paths
    for path in &[
        "/opt/homebrew/bin/tmux",
        "/usr/local/bin/tmux",
        "/usr/bin/tmux",
    ] {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }

    None
}

/// Find the tmux pane containing a given agent PID
pub fn find_pane_for_pid(agent_pid: u32) -> Option<TmuxTarget> {
    let tmux_path = find_tmux_path()?;

    let output = std::process::Command::new(&tmux_path)
        .args([
            "list-panes",
            "-a",
            "-F",
            "#{session_name}:#{window_index}.#{pane_index} #{pane_pid}",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let tree = process_tree::build_tree();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() != 2 {
            continue;
        }

        let target_str = parts[0];
        let pane_pid: u32 = match parts[1].parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Check if agent_pid is a descendant of this pane's shell PID
        if process_tree::is_descendant(agent_pid, pane_pid, &tree) {
            return TmuxTarget::parse(target_str);
        }
    }

    None
}

/// Switch to a specific tmux pane (select-window + select-pane)
pub fn switch_to_pane(target: &TmuxTarget) -> Result<(), Box<dyn std::error::Error>> {
    let tmux_path = find_tmux_path().ok_or("tmux not found")?;

    // Select the window first
    let status = std::process::Command::new(&tmux_path)
        .args([
            "select-window",
            "-t",
            &format!("{}:{}", target.session, target.window),
        ])
        .status()?;

    if !status.success() {
        return Err(format!(
            "Failed to select tmux window {}:{}",
            target.session, target.window
        )
        .into());
    }

    // Then select the pane
    let status = std::process::Command::new(&tmux_path)
        .args(["select-pane", "-t", &target.target_string()])
        .status()?;

    if !status.success() {
        return Err(format!("Failed to select tmux pane {}", target.target_string()).into());
    }

    Ok(())
}

/// Check if a session's tmux pane is currently active
pub fn is_pane_active(agent_pid: u32) -> bool {
    let tmux_path = match find_tmux_path() {
        Some(p) => p,
        None => return false,
    };

    // Find which pane the agent is in
    let target = match find_pane_for_pid(agent_pid) {
        Some(t) => t,
        None => return false,
    };

    // Get the currently active pane
    let output = match std::process::Command::new(&tmux_path)
        .args([
            "display-message",
            "-p",
            "#{session_name}:#{window_index}.#{pane_index}",
        ])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return false,
    };

    let active = String::from_utf8_lossy(&output.stdout).trim().to_string();
    target.target_string() == active
}
