// Smart Suppression — Detect if the agent's terminal is already in focus
// Used to suppress notifications/sounds when the user is already looking at the terminal

use super::{process_tree, tmux, registry};

/// Check if an agent session's terminal is currently in the foreground
/// Returns true if the user is already looking at it (suppress notifications)
pub fn is_terminal_focused(agent_pid: u32) -> bool {
    // First check if any terminal is the frontmost app
    if !is_terminal_frontmost() {
        return false;
    }

    let tree = process_tree::build_tree();
    let in_tmux = process_tree::is_in_tmux(agent_pid, &tree);

    if in_tmux {
        // For tmux: terminal is frontmost AND the agent's pane is the active one
        tmux::is_pane_active(agent_pid)
    } else {
        // For non-tmux: check if the agent's terminal app is the frontmost app
        is_agent_terminal_frontmost(agent_pid, &tree)
    }
}

/// Check if the frontmost (active) application is a terminal
fn is_terminal_frontmost() -> bool {
    // Use AppleScript to get the frontmost app's bundle ID
    let output = match std::process::Command::new("osascript")
        .args(["-e", r#"tell application "System Events" to get bundle identifier of first application process whose frontmost is true"#])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return false,
    };

    let bundle_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    registry::is_terminal_bundle(&bundle_id)
}

/// Check if a specific agent's terminal is the frontmost app
fn is_agent_terminal_frontmost(
    agent_pid: u32,
    tree: &std::collections::HashMap<u32, process_tree::ProcessInfo>,
) -> bool {
    // Find the terminal PID for the agent
    let terminal_pid = match process_tree::find_terminal_pid(agent_pid, tree) {
        Some(pid) => pid,
        None => return false,
    };

    // Get the frontmost app PID
    let output = match std::process::Command::new("osascript")
        .args(["-e", r#"tell application "System Events" to get unix id of first application process whose frontmost is true"#])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return false,
    };

    let frontmost_pid: u32 = match String::from_utf8_lossy(&output.stdout).trim().parse() {
        Ok(p) => p,
        Err(_) => return false,
    };

    terminal_pid == frontmost_pid
}

/// Check if any terminal window is visible on the current screen
/// (Less strict than frontmost — terminal could be visible but not focused)
pub fn is_any_terminal_visible() -> bool {
    // Use osascript to check for visible terminal windows
    let script = r#"
tell application "System Events"
    set visibleApps to name of every application process whose visible is true
end tell
return visibleApps as text
"#;

    let output = match std::process::Command::new("osascript")
        .args(["-e", script])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return false,
    };

    let apps = String::from_utf8_lossy(&output.stdout).to_lowercase();
    registry::is_terminal(&apps)
}
