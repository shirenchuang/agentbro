// Smart Suppression — Tab-level detection of whether the agent's terminal is focused
// Uses ITERM_SESSION_ID, tmux pane, KITTY_WINDOW_ID for precise tab-level checks.

use super::{process_tree, tmux, registry};

/// Check if an agent session's terminal is currently in the foreground.
/// Returns true if the user is already looking at it (suppress notifications).
pub fn is_terminal_focused(agent_pid: u32) -> bool {
    if !is_terminal_frontmost() {
        return false;
    }

    let tree = process_tree::build_tree();
    let terminal_type = process_tree::detect_terminal_type(agent_pid, &tree);

    match &terminal_type {
        process_tree::TerminalType::Tmux { .. } => tmux::is_pane_active(agent_pid),

        process_tree::TerminalType::ITerm2 { session_id } => {
            if let Some(sid) = session_id {
                is_iterm_session_active(sid)
            } else {
                is_agent_terminal_frontmost(agent_pid, &tree)
            }
        }

        process_tree::TerminalType::Kitty { window_id } => {
            if let Some(wid) = window_id {
                is_kitty_window_focused(wid)
            } else {
                is_agent_terminal_frontmost(agent_pid, &tree)
            }
        }

        process_tree::TerminalType::WezTerm => {
            let env = process_tree::read_terminal_env(agent_pid, &tree);
            is_wezterm_pane_focused(env.wezterm_pane.as_deref())
        }

        _ => is_agent_terminal_frontmost(agent_pid, &tree),
    }
}

/// Tab-level suppression with explicit session identifiers (for callers that have them).
pub fn is_terminal_focused_with_session(
    agent_pid: u32,
    iterm_session_id: Option<&str>,
    kitty_window_id: Option<&str>,
    tmux_pane: Option<&str>,
) -> bool {
    if !is_terminal_frontmost() {
        return false;
    }

    let tree = process_tree::build_tree();

    // tmux: check active pane
    if tmux_pane.is_some() || process_tree::is_in_tmux(agent_pid, &tree) {
        return tmux::is_pane_active(agent_pid);
    }

    // iTerm2: precise session check
    if let Some(sid) = iterm_session_id {
        return is_iterm_session_active(sid);
    }

    // Kitty: window focus check
    if let Some(wid) = kitty_window_id {
        return is_kitty_window_focused(wid);
    }

    is_agent_terminal_frontmost(agent_pid, &tree)
}

// ─── Frontmost checks ─────────────────────────────────────────────────────────

fn is_terminal_frontmost() -> bool {
    let output = match std::process::Command::new("osascript")
        .args([
            "-e",
            r#"tell application "System Events" to get bundle identifier of first application process whose frontmost is true"#,
        ])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return false,
    };

    let bundle_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    registry::is_terminal_bundle(&bundle_id)
}

fn is_agent_terminal_frontmost(
    agent_pid: u32,
    tree: &std::collections::HashMap<u32, process_tree::ProcessInfo>,
) -> bool {
    let terminal_pid = match process_tree::find_terminal_pid(agent_pid, tree) {
        Some(pid) => pid,
        None => return false,
    };

    let output = match std::process::Command::new("osascript")
        .args([
            "-e",
            r#"tell application "System Events" to get unix id of first application process whose frontmost is true"#,
        ])
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

// ─── iTerm2 session check ─────────────────────────────────────────────────────

fn is_iterm_session_active(session_id: &str) -> bool {
    let sid = session_id.split(':').last().unwrap_or(session_id);
    let script = format!(
        r#"tell application "iTerm2"
    try
        set sess to current session of current window
        if unique ID of sess is "{sid}" then
            return "yes"
        end if
    end try
    return "no"
end tell"#
    );

    matches!(
        std::process::Command::new("osascript")
            .args(["-e", &script])
            .output(),
        Ok(o) if String::from_utf8_lossy(&o.stdout).trim() == "yes"
    )
}

// ─── Kitty window check ───────────────────────────────────────────────────────

fn is_kitty_window_focused(window_id: &str) -> bool {
    let Some(bin) = find_binary("kitten") else {
        return false;
    };

    let output = match std::process::Command::new(&bin)
        .args(["@", "ls"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return false,
    };

    let json: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(v) => v,
        Err(_) => return false,
    };

    if let Some(os_windows) = json.as_array() {
        for os_win in os_windows {
            if let Some(tabs) = os_win["tabs"].as_array() {
                for tab in tabs {
                    if let Some(windows) = tab["windows"].as_array() {
                        for win in windows {
                            let id_str = win["id"].as_u64().map(|id| id.to_string());
                            if id_str.as_deref() == Some(window_id) {
                                return win["is_focused"].as_bool().unwrap_or(false);
                            }
                        }
                    }
                }
            }
        }
    }

    false
}

// ─── WezTerm pane check ───────────────────────────────────────────────────────

fn is_wezterm_pane_focused(pane_id: Option<&str>) -> bool {
    let Some(pane_id) = pane_id else {
        return false;
    };

    let Some(bin) = find_binary("wezterm") else {
        return false;
    };

    let output = match std::process::Command::new(&bin)
        .args(["cli", "list", "--format", "json"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return false,
    };

    let json: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(v) => v,
        Err(_) => return false,
    };

    if let Some(panes) = json.as_array() {
        for pane in panes {
            let id_str = pane["pane_id"].as_i64().map(|id| id.to_string());
            if id_str.as_deref() == Some(pane_id) {
                return pane["is_active"].as_bool().unwrap_or(false);
            }
        }
    }

    false
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn find_binary(name: &str) -> Option<String> {
    let paths = [
        format!("/opt/homebrew/bin/{}", name),
        format!("/usr/local/bin/{}", name),
        format!("/usr/bin/{}", name),
    ];
    paths
        .iter()
        .find(|p| std::path::Path::new(p.as_str()).exists())
        .cloned()
}

/// Check if any terminal window is visible (less strict than frontmost check)
pub fn is_any_terminal_visible() -> bool {
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
