// Tmux-based tool approval handler
// Sends approval/denial keystrokes to Claude Code running in a tmux pane.
//
// Claude Code in tmux shows numbered options:
//   "1" + Enter = Allow once
//   "2" + Enter = Allow always
//   "n" + Enter = Deny

use super::tmux;

/// Send "1\n" to approve once
pub fn approve_once(tmux_target: &str) -> Result<(), Box<dyn std::error::Error>> {
    let target = tmux::TmuxTarget::parse(tmux_target)
        .ok_or_else(|| format!("Invalid tmux target: {}", tmux_target))?;
    send_keys_with_enter(&target, "1")
}

/// Send "2\n" to approve always (allow for this session)
pub fn approve_always(tmux_target: &str) -> Result<(), Box<dyn std::error::Error>> {
    let target = tmux::TmuxTarget::parse(tmux_target)
        .ok_or_else(|| format!("Invalid tmux target: {}", tmux_target))?;
    send_keys_with_enter(&target, "2")
}

/// Send "n\n" to reject; if a message is provided, wait 100ms then send it
pub fn reject(tmux_target: &str, message: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let target = tmux::TmuxTarget::parse(tmux_target)
        .ok_or_else(|| format!("Invalid tmux target: {}", tmux_target))?;

    send_keys_with_enter(&target, "n")?;

    if let Some(msg) = message {
        if !msg.is_empty() {
            std::thread::sleep(std::time::Duration::from_millis(100));
            send_keys_with_enter(&target, msg)?;
        }
    }

    Ok(())
}

/// Find the tmux path (reuse logic from tmux module)
fn find_tmux_path() -> Option<String> {
    for path in &["/opt/homebrew/bin/tmux", "/usr/local/bin/tmux", "/usr/bin/tmux"] {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }
    let output = std::process::Command::new("which")
        .arg("tmux")
        .output()
        .ok()?;
    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return Some(path);
        }
    }
    None
}

/// Send literal text to a tmux pane, followed by Enter as a separate command.
/// Uses `-l` flag to prevent tmux from interpreting special characters.
fn send_keys_with_enter(target: &tmux::TmuxTarget, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let tmux_path = find_tmux_path()
        .ok_or("tmux not found")?;
    let target_str = target.target_string();

    // Send the literal text
    let status = std::process::Command::new(&tmux_path)
        .args(["send-keys", "-l", "-t", &target_str, text])
        .status()?;
    if !status.success() {
        return Err(format!("Failed to send keys to tmux pane {}", target_str).into());
    }

    // Send Enter as a separate command (not literal)
    let status = std::process::Command::new(&tmux_path)
        .args(["send-keys", "-t", &target_str, "Enter"])
        .status()?;
    if !status.success() {
        return Err(format!("Failed to send Enter to tmux pane {}", target_str).into());
    }

    Ok(())
}

/// Resolve a tmux target string for an agent by its PID.
/// Returns the "session:window.pane" string if found.
pub fn resolve_tmux_target(agent_pid: u32) -> Option<String> {
    tmux::find_pane_for_pid(agent_pid).map(|t| t.target_string())
}
