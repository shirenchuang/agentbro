// Jump — Focus the terminal where an agent session is running
// Supports: iTerm2 (AppleScript), Terminal.app (AppleScript), Ghostty (activate),
//           tmux (select-window + select-pane), and generic app activation

use super::{process_tree, tmux, registry};

/// Result of a jump operation
#[derive(Debug)]
pub enum JumpResult {
    /// Successfully jumped to terminal
    Success,
    /// Session not found or no PID
    SessionNotFound,
    /// Terminal app not found in process tree
    TerminalNotFound,
    /// Jump attempted but failed
    Failed(String),
}

/// Jump to the terminal containing the given agent process
pub fn jump_to_terminal(pid: u32) -> JumpResult {
    let tree = process_tree::build_tree();

    // Check if the agent is in tmux first
    if process_tree::is_in_tmux(pid, &tree) {
        return jump_via_tmux(pid);
    }

    // Find the terminal app in the process tree
    if let Some(terminal_app) = process_tree::find_terminal_app_name(pid, &tree) {
        // Try AppleScript for supported terminals (iTerm2, Terminal.app)
        if let Some(app_name) = registry::applescript_app_name(&terminal_app) {
            return jump_via_applescript(app_name);
        }
        // Generic activation: use `open -a` for the terminal app
        return jump_via_app_activation(&terminal_app);
    }

    // Fallback: use TTY device to find which terminal app owns this session
    log::warn!("Process tree walk failed for PID {}. Trying TTY fallback.", pid);
    if let Some(tty) = process_tree::get_tty(pid, &tree) {
        if let Some(terminal_app) = find_terminal_app_for_tty(&tty) {
            log::info!("TTY fallback found terminal app: {}", terminal_app);
            if let Some(app_name) = registry::applescript_app_name(&terminal_app) {
                return jump_via_applescript(app_name);
            }
            return jump_via_app_activation(&terminal_app);
        }
    }

    // Last resort: find any terminal app that shares the same TTY
    if let Some(tty) = process_tree::get_tty(pid, &tree) {
        log::warn!("Trying shared-TTY scan for {}", tty);
        for info in tree.values() {
            if info.tty.as_deref() == Some(&tty) && registry::is_terminal(&info.command) {
                log::info!("Shared-TTY scan found: {}", info.command);
                if let Some(app_name) = registry::applescript_app_name(&info.command) {
                    return jump_via_applescript(app_name);
                }
                return jump_via_app_activation(&info.command);
            }
        }
    }

    JumpResult::TerminalNotFound
}

/// Find which terminal app owns a given TTY device using lsof
fn find_terminal_app_for_tty(tty: &str) -> Option<String> {
    // Convert short TTY name (e.g. "s002") to device path "/dev/ttys002"
    let tty_path = if tty.starts_with("/dev/") {
        tty.to_string()
    } else {
        format!("/dev/tty{}", tty)
    };

    let output = std::process::Command::new("lsof")
        .arg(&tty_path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        let command = parts[0];
        if registry::is_terminal(command) {
            return Some(command.to_string());
        }
    }
    None
}

/// Jump to a tmux pane
fn jump_via_tmux(pid: u32) -> JumpResult {
    // Find the tmux pane for this agent
    let target = match tmux::find_pane_for_pid(pid) {
        Some(t) => t,
        None => return JumpResult::Failed("Could not find tmux pane for agent".to_string()),
    };

    // Switch to the pane
    if let Err(e) = tmux::switch_to_pane(&target) {
        return JumpResult::Failed(format!("Failed to switch tmux pane: {}", e));
    }

    // Also try to activate the terminal app that hosts tmux
    let tree = process_tree::build_tree();
    if let Some(terminal_app) = process_tree::find_terminal_app_name(pid, &tree) {
        let _ = activate_app(&terminal_app);
    }

    JumpResult::Success
}

/// Jump using AppleScript (iTerm2, Terminal.app)
fn jump_via_applescript(app_name: &str) -> JumpResult {
    let script = format!(
        r#"tell application "{}" to activate"#,
        app_name
    );

    match std::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
    {
        Ok(output) if output.status.success() => JumpResult::Success,
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            JumpResult::Failed(format!("AppleScript failed: {}", stderr))
        }
        Err(e) => JumpResult::Failed(format!("Failed to run osascript: {}", e)),
    }
}

/// Jump by activating the app (works for Ghostty, Alacritty, etc.)
fn jump_via_app_activation(terminal_command: &str) -> JumpResult {
    match activate_app(terminal_command) {
        Ok(()) => JumpResult::Success,
        Err(e) => JumpResult::Failed(e),
    }
}

/// Activate a macOS app by name
fn activate_app(app_name: &str) -> Result<(), String> {
    // Extract just the app name from a full path
    let name = app_name
        .rsplit('/')
        .next()
        .unwrap_or(app_name)
        .trim_end_matches(".app");

    // Try `open -a` first — works for most apps
    let output = std::process::Command::new("open")
        .args(["-a", name])
        .output()
        .map_err(|e| format!("Failed to run open: {}", e))?;

    if output.status.success() {
        return Ok(());
    }

    // Fall back to AppleScript activate
    let script = format!(
        r#"tell application "{}" to activate"#,
        name
    );
    let output = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| format!("Failed to run osascript: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Could not activate {}: {}",
            name,
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}
