// Jump — Focus the terminal where an agent session is running
// Supports: iTerm2 (AppleScript by session ID), Ghostty (CWD/title matching),
//           Terminal.app (TTY matching), Kitty (kitten @), WezTerm (wezterm cli),
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

/// Extended context for precise tab-level terminal jumping.
/// Fields are sourced from agent process environment variables.
#[derive(Debug, Clone, Default)]
pub struct JumpContext {
    pub pid: u32,
    /// ITERM_SESSION_ID env var (e.g. "w0t0p0:XXXXXXXX-XXXX-...")
    pub iterm_session_id: Option<String>,
    /// KITTY_WINDOW_ID env var
    pub kitty_window_id: Option<String>,
    /// tmux pane target (e.g. "%3") — from running `tmux display-message -p '#D'`
    pub tmux_pane: Option<String>,
    /// TMUX env var value (socket path:session_id:window_id)
    pub tmux_env: Option<String>,
    /// Working directory of the agent (for CWD-based fallbacks)
    pub cwd: Option<String>,
    /// TTY device path (e.g. "/dev/ttys002")
    pub tty_path: Option<String>,
}

/// Jump to the terminal containing the given agent process (simple PID-based entrypoint).
/// For precise tab targeting, use `jump_to_terminal_with_context`.
pub fn jump_to_terminal(pid: u32) -> JumpResult {
    let tree = process_tree::build_tree();

    // Check if the agent is in tmux first
    if process_tree::is_in_tmux(pid, &tree) {
        return jump_via_tmux_pid(pid);
    }

    // Find the terminal app in the process tree
    if let Some(terminal_app) = process_tree::find_terminal_app_name(pid, &tree) {
        let lower = terminal_app.to_lowercase();

        // iTerm2: read iTerm session ID from process env for precise tab jump
        if lower.contains("iterm") {
            if let Some(session_id) = process_tree::read_env_var(pid, "ITERM_SESSION_ID") {
                return jump_iterm_by_session(&session_id);
            }
        }

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

/// Precise tab-level jump using full session context (iTerm2 session ID, Kitty window ID, etc.)
pub fn jump_to_terminal_with_context(ctx: &JumpContext) -> JumpResult {
    let tree = process_tree::build_tree();

    // ── tmux: switch pane first ──────────────────────────────────────────────
    if let Some(pane) = &ctx.tmux_pane {
        if !pane.is_empty() {
            let res = jump_via_tmux_pane(pane, ctx.tmux_env.as_deref());
            // Also activate the outer terminal app hosting tmux
            if let Some(term) = process_tree::find_terminal_app_name(ctx.pid, &tree) {
                let _ = activate_app(&term);
            }
            return res;
        }
    }

    if process_tree::is_in_tmux(ctx.pid, &tree) {
        let res = jump_via_tmux_pid(ctx.pid);
        if let Some(term) = process_tree::find_terminal_app_name(ctx.pid, &tree) {
            let _ = activate_app(&term);
        }
        return res;
    }

    let terminal_app = process_tree::find_terminal_app_name(ctx.pid, &tree)
        .or_else(|| {
            ctx.tty_path.as_ref().and_then(|tty| find_terminal_app_for_tty(tty))
        });

    let Some(term_app) = terminal_app else {
        return JumpResult::TerminalNotFound;
    };

    let lower = term_app.to_lowercase();

    // ── iTerm2: precise session ID targeting ─────────────────────────────────
    if lower.contains("iterm") {
        if let Some(session_id) = &ctx.iterm_session_id {
            if !session_id.is_empty() {
                return jump_iterm_by_session(session_id);
            }
        }
        return jump_iterm_by_tty_or_cwd(ctx.tty_path.as_deref(), ctx.cwd.as_deref());
    }

    // ── Ghostty: AppleScript CWD/title matching ──────────────────────────────
    if lower.contains("ghostty") {
        return jump_ghostty(ctx.cwd.as_deref(), ctx.tty_path.as_deref());
    }

    // ── Terminal.app: TTY tab matching ───────────────────────────────────────
    if lower.contains("terminal") || lower.contains("apple_terminal") {
        return jump_terminal_app(ctx.tty_path.as_deref(), ctx.cwd.as_deref());
    }

    // ── WezTerm: CLI tab targeting ───────────────────────────────────────────
    if lower.contains("wezterm") || lower.contains("wez") {
        return jump_wezterm(ctx.tty_path.as_deref(), ctx.cwd.as_deref());
    }

    // ── Kitty: kitten @ focus-window ────────────────────────────────────────
    if lower.contains("kitty") {
        return jump_kitty(ctx.kitty_window_id.as_deref(), ctx.cwd.as_deref());
    }

    // Generic fallback
    match activate_app(&term_app) {
        Ok(()) => JumpResult::Success,
        Err(e) => JumpResult::Failed(e),
    }
}

// ─── iTerm2 ──────────────────────────────────────────────────────────────────

fn jump_iterm_by_session(session_id: &str) -> JumpResult {
    // Strip "w0t0p0:" prefix (ITERM_SESSION_ID format)
    let sid = session_id.split(':').last().unwrap_or(session_id);
    let script = format!(
        r#"tell application "iTerm2"
    activate
    repeat with aWindow in windows
        repeat with aTab in tabs of aWindow
            repeat with aSession in sessions of aTab
                if unique ID of aSession is "{sid}" then
                    select aSession
                    select aWindow
                    return
                end if
            end repeat
        end repeat
    end repeat
end tell"#
    );
    run_osascript(&script)
}

fn jump_iterm_by_tty_or_cwd(tty: Option<&str>, cwd: Option<&str>) -> JumpResult {
    // Build TTY match clause
    let tty_clause = tty.map(|t| {
        let dev = tty_to_dev_path(t);
        format!(
            r#"try
                    if tty of aSession is "{dev}" then
                        select aSession
                        select aWindow
                        return
                    end if
                end try"#
        )
    }).unwrap_or_default();

    let cwd_clause = cwd.map(|c| {
        format!(
            r#"try
                    if variable named "session.path" of aSession is "{c}" then
                        select aSession
                        select aWindow
                        return
                    end if
                end try"#
        )
    }).unwrap_or_default();

    if tty_clause.is_empty() && cwd_clause.is_empty() {
        return run_osascript(r#"tell application "iTerm2" to activate"#);
    }

    let script = format!(
        r#"tell application "iTerm2"
    activate
    repeat with aWindow in windows
        repeat with aTab in tabs of aWindow
            repeat with aSession in sessions of aTab
                {tty_clause}
                {cwd_clause}
            end repeat
        end repeat
    end repeat
end tell"#
    );
    run_osascript(&script)
}

// ─── Ghostty ─────────────────────────────────────────────────────────────────

fn jump_ghostty(cwd: Option<&str>, tty: Option<&str>) -> JumpResult {
    let _ = run_osascript(r#"tell application "Ghostty" to activate"#);

    let match_clause = if let Some(c) = cwd {
        let folder = c.rsplit('/').next().unwrap_or(c);
        format!(
            r#"if name of w contains "{folder}" then
                    perform action "AXRaise" of w
                    return
                end if"#
        )
    } else if let Some(t) = tty {
        let dev = tty_to_dev_path(t);
        format!(
            r#"if name of w contains "{dev}" then
                    perform action "AXRaise" of w
                    return
                end if"#
        )
    } else {
        return JumpResult::Success;
    };

    let script = format!(
        r#"tell application "System Events"
    tell process "Ghostty"
        set frontmost to true
        repeat with w in windows
            try
                {match_clause}
            end try
        end repeat
    end tell
end tell"#
    );
    run_osascript(&script)
}

// ─── Terminal.app ────────────────────────────────────────────────────────────

fn jump_terminal_app(tty: Option<&str>, cwd: Option<&str>) -> JumpResult {
    let tty_clause = tty.map(|t| {
        let dev = tty_to_dev_path(t);
        format!(
            r#"repeat with aTab in tabs of aWindow
                try
                    if tty of aTab is "{dev}" then
                        set selected tab of aWindow to aTab
                        set index of aWindow to 1
                        return
                    end if
                end try
            end repeat"#
        )
    }).unwrap_or_default();

    let cwd_clause = cwd.map(|c| {
        let folder = c.rsplit('/').next().unwrap_or(c);
        format!(
            r#"repeat with aTab in tabs of aWindow
                try
                    if custom title of aTab contains "{folder}" then
                        set selected tab of aWindow to aTab
                        set index of aWindow to 1
                        return
                    end if
                end try
            end repeat"#
        )
    }).unwrap_or_default();

    let script = format!(
        r#"tell application "Terminal"
    activate
    repeat with aWindow in windows
        {tty_clause}
        {cwd_clause}
    end repeat
end tell"#
    );
    run_osascript(&script)
}

// ─── WezTerm ─────────────────────────────────────────────────────────────────

fn jump_wezterm(tty: Option<&str>, cwd: Option<&str>) -> JumpResult {
    let _ = activate_app("WezTerm");

    let Some(bin) = find_binary("wezterm") else {
        return JumpResult::Success;
    };

    let tty_dev = tty.map(tty_to_dev_path);

    let output = match std::process::Command::new(&bin)
        .args(["cli", "list", "--format", "json"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return JumpResult::Success,
    };

    let json: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(v) => v,
        Err(_) => return JumpResult::Success,
    };

    let panes = match json.as_array() {
        Some(a) => a,
        None => return JumpResult::Success,
    };

    let mut tab_id: Option<i64> = None;

    if let Some(ref tty_str) = tty_dev {
        tab_id = panes.iter().find_map(|p| {
            if p["tty_name"].as_str() == Some(tty_str.as_str()) {
                p["tab_id"].as_i64()
            } else {
                None
            }
        });
    }

    if tab_id.is_none() {
        if let Some(cwd_str) = cwd {
            let cwd_url = format!("file://{}", cwd_str);
            tab_id = panes.iter().find_map(|p| {
                let pane_cwd = p["cwd"].as_str().unwrap_or("");
                if pane_cwd == cwd_url || pane_cwd == cwd_str {
                    p["tab_id"].as_i64()
                } else {
                    None
                }
            });
        }
    }

    if let Some(id) = tab_id {
        let _ = std::process::Command::new(&bin)
            .args(["cli", "activate-tab", "--tab-id", &id.to_string()])
            .output();
    }

    JumpResult::Success
}

// ─── Kitty ───────────────────────────────────────────────────────────────────

fn jump_kitty(window_id: Option<&str>, cwd: Option<&str>) -> JumpResult {
    let _ = activate_app("kitty");

    let Some(bin) = find_binary("kitten") else {
        return JumpResult::Success;
    };

    if let Some(wid) = window_id {
        if !wid.is_empty() {
            let _ = std::process::Command::new(&bin)
                .args(["@", "focus-window", "--match", &format!("id:{}", wid)])
                .output();
            return JumpResult::Success;
        }
    }

    if let Some(cwd_str) = cwd {
        if std::process::Command::new(&bin)
            .args(["@", "focus-tab", "--match", &format!("cwd:{}", cwd_str)])
            .output()
            .map(|o| !o.status.success())
            .unwrap_or(true)
        {
            let _ = std::process::Command::new(&bin)
                .args(["@", "focus-tab", "--match", "title:claude"])
                .output();
        }
    }

    JumpResult::Success
}

// ─── tmux ────────────────────────────────────────────────────────────────────

fn jump_via_tmux_pid(pid: u32) -> JumpResult {
    let target = match tmux::find_pane_for_pid(pid) {
        Some(t) => t,
        None => return JumpResult::Failed("Could not find tmux pane for agent".to_string()),
    };

    if let Err(e) = tmux::switch_to_pane(&target) {
        return JumpResult::Failed(format!("Failed to switch tmux pane: {}", e));
    }

    let tree = process_tree::build_tree();
    if let Some(terminal_app) = process_tree::find_terminal_app_name(pid, &tree) {
        let _ = activate_app(&terminal_app);
    }

    JumpResult::Success
}

fn jump_via_tmux_pane(pane: &str, tmux_env: Option<&str>) -> JumpResult {
    let Some(bin) = find_binary("tmux") else {
        return JumpResult::Failed("tmux not found".to_string());
    };

    let run = |args: &[&str]| {
        let mut cmd = std::process::Command::new(&bin);
        cmd.args(args);
        if let Some(env) = tmux_env {
            if !env.is_empty() {
                cmd.env("TMUX", env);
            }
        }
        let _ = cmd.output();
    };

    run(&["select-window", "-t", pane]);
    run(&["select-pane", "-t", pane]);

    JumpResult::Success
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn run_osascript(script: &str) -> JumpResult {
    match std::process::Command::new("osascript")
        .args(["-e", script])
        .output()
    {
        Ok(o) if o.status.success() => JumpResult::Success,
        Ok(o) => JumpResult::Failed(
            String::from_utf8_lossy(&o.stderr).trim().to_string(),
        ),
        Err(e) => JumpResult::Failed(format!("osascript: {}", e)),
    }
}

fn jump_via_applescript(app_name: &str) -> JumpResult {
    let script = format!(r#"tell application "{}" to activate"#, app_name);
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

fn jump_via_app_activation(terminal_command: &str) -> JumpResult {
    match activate_app(terminal_command) {
        Ok(()) => JumpResult::Success,
        Err(e) => JumpResult::Failed(e),
    }
}

fn activate_app(app_name: &str) -> Result<(), String> {
    let name = app_name
        .rsplit('/')
        .next()
        .unwrap_or(app_name)
        .trim_end_matches(".app");

    let output = std::process::Command::new("open")
        .args(["-a", name])
        .output()
        .map_err(|e| format!("Failed to run open: {}", e))?;

    if output.status.success() {
        return Ok(());
    }

    let script = format!(r#"tell application "{}" to activate"#, name);
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

/// Find which terminal app owns a given TTY device using lsof
fn find_terminal_app_for_tty(tty: &str) -> Option<String> {
    let tty_path = tty_to_dev_path(tty);

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

fn tty_to_dev_path(tty: &str) -> String {
    if tty.starts_with("/dev/") {
        tty.to_string()
    } else {
        format!("/dev/tty{}", tty)
    }
}
