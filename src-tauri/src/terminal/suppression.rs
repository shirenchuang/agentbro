// Smart Suppression — Tab-level detection of whether the agent's terminal is focused
// Uses ITERM_SESSION_ID, tmux pane, KITTY_WINDOW_ID for precise tab-level checks.

use super::{process_tree, registry, tmux};

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
pub fn is_terminal_focused_with_explicit_ids(
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

/// Suppression check using metadata captured by the compiled hook bridge.
pub fn is_terminal_focused_with_session(
    agent_pid: u32,
    term_bundle_id: Option<&str>,
    wezterm_pane: Option<&str>,
    zellij_pane_id: Option<&str>,
    cmux_surface_id: Option<&str>,
    tty_path: Option<&str>,
) -> bool {
    let tree = process_tree::build_tree();
    let env = process_tree::read_terminal_env(agent_pid, &tree);

    if let Some(bundle_id) = term_bundle_id.or(env.cf_bundle_identifier.as_deref()) {
        if !bundle_id.is_empty() && !registry::is_terminal_bundle(bundle_id) {
            return is_bundle_frontmost(bundle_id);
        }
    }

    if !is_terminal_frontmost() {
        return false;
    }

    let wezterm_pane = wezterm_pane.or(env.wezterm_pane.as_deref());
    if wezterm_pane.is_some() {
        return is_wezterm_pane_focused(wezterm_pane);
    }

    let zellij_pane_id = zellij_pane_id.or(env.zellij_pane_id.as_deref());
    if zellij_pane_id.is_some() {
        return is_zellij_pane_focused(zellij_pane_id);
    }

    let cmux_surface_id = cmux_surface_id.or(env.cmux_surface_id.as_deref());
    if cmux_surface_id.is_some() {
        return is_cmux_surface_focused(cmux_surface_id);
    }

    if let Some(tty) = tty_path {
        if is_kaku_focused(tty) {
            return true;
        }
    }

    is_terminal_focused(agent_pid)
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

fn is_bundle_frontmost(expected_bundle_id: &str) -> bool {
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

    String::from_utf8_lossy(&output.stdout).trim() == expected_bundle_id
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
    let sid = session_id.split(':').next_back().unwrap_or(session_id);
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

    let output = match std::process::Command::new(&bin).args(["@", "ls"]).output() {
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

// ─── Zellij pane check ───────────────────────────────────────────────────────

fn is_zellij_pane_focused(pane_id: Option<&str>) -> bool {
    let Some(pane_id) = pane_id else {
        return false;
    };
    let Some(bin) = find_binary("zellij") else {
        return false;
    };

    let output = match std::process::Command::new(&bin)
        .args(["action", "list-panes", "--json", "--tab"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return false,
    };

    let Some(pane_id) = parse_zellij_pane_id(pane_id) else {
        return false;
    };
    let json: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(v) => v,
        Err(_) => return false,
    };
    flatten_zellij_panes(&json).iter().any(|pane| {
        pane["id"].as_i64() == Some(pane_id)
            && (pane["is_focused"].as_bool().unwrap_or(false)
                || pane["is_active"].as_bool().unwrap_or(false))
    })
}

// ─── cmux surface check ──────────────────────────────────────────────────────

fn is_cmux_surface_focused(surface_id: Option<&str>) -> bool {
    let Some(surface_id) = surface_id else {
        return false;
    };
    let Some(bin) = find_binary("cmux") else {
        return false;
    };

    let output = match std::process::Command::new(&bin)
        .args(["status", "--json"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return false,
    };
    let json: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(v) => v,
        Err(_) => return false,
    };
    json.get("activeSurfaceId")
        .or_else(|| json.get("active_surface_id"))
        .or_else(|| json.get("activePanelId"))
        .or_else(|| json.get("active_panel_id"))
        .and_then(|v| v.as_str())
        == Some(surface_id)
}

// ─── Kaku pane check ─────────────────────────────────────────────────────────

fn is_kaku_focused(tty: &str) -> bool {
    let Some(bin) = find_binary("kaku") else {
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
    let expected = if tty.starts_with("/dev/") {
        tty.to_string()
    } else {
        format!("/dev/{tty}")
    };
    json.as_array().is_some_and(|panes| {
        panes.iter().any(|pane| {
            pane["tty_name"].as_str() == Some(expected.as_str())
                && pane["is_active"].as_bool().unwrap_or(false)
        })
    })
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn find_binary(name: &str) -> Option<String> {
    let paths = [
        format!("/opt/homebrew/bin/{}", name),
        format!("/usr/local/bin/{}", name),
        format!("/usr/bin/{}", name),
        format!("/Applications/cmux.app/Contents/Resources/bin/{}", name),
        format!(
            "{}/Applications/cmux.app/Contents/Resources/bin/{}",
            dirs::home_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            name
        ),
        format!("/Applications/Kaku.app/Contents/MacOS/{}", name),
        format!(
            "{}/Applications/Kaku.app/Contents/MacOS/{}",
            dirs::home_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            name
        ),
        format!(
            "{}/.local/bin/{}",
            dirs::home_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            name
        ),
    ];
    paths
        .iter()
        .find(|p| std::path::Path::new(p.as_str()).exists())
        .cloned()
}

fn parse_zellij_pane_id(raw: &str) -> Option<i64> {
    if let Ok(value) = raw.parse::<i64>() {
        return Some(value);
    }
    raw.strip_prefix("terminal_")?.parse::<i64>().ok()
}

fn flatten_zellij_panes(json: &serde_json::Value) -> Vec<&serde_json::Value> {
    if let Some(arr) = json.as_array() {
        return arr.iter().collect();
    }
    if let Some(obj) = json.as_object() {
        return obj
            .values()
            .filter_map(|value| value.as_array())
            .flat_map(|arr| arr.iter())
            .collect();
    }
    Vec::new()
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
