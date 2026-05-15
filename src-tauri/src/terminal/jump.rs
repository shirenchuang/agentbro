// Jump — Focus the terminal where an agent session is running
// Supports: iTerm2 (AppleScript by session ID), Ghostty (CWD/title matching),
//           Terminal.app (TTY matching), Kitty (kitten @), WezTerm (wezterm cli),
//           tmux (select-window + select-pane), and generic app activation

use super::{process_tree, registry, tmux};

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
    /// WEZTERM_PANE env var
    pub wezterm_pane: Option<String>,
    /// Zellij pane/session identifiers
    pub zellij_pane_id: Option<String>,
    pub zellij_session_name: Option<String>,
    /// cmux surface/workspace identifiers
    pub cmux_surface_id: Option<String>,
    pub cmux_workspace_id: Option<String>,
    /// tmux pane target (e.g. "%3") — from running `tmux display-message -p '#D'`
    pub tmux_pane: Option<String>,
    /// TMUX env var value (socket path:session_id:window_id)
    pub tmux_env: Option<String>,
    /// Working directory of the agent (for CWD-based fallbacks)
    pub cwd: Option<String>,
    /// TTY device path (e.g. "/dev/ttys002")
    pub tty_path: Option<String>,
    /// App name saved by the hook payload.
    pub terminal_app: Option<String>,
    /// Frontmost bundle identifier saved by the hook payload.
    pub term_bundle_id: Option<String>,
    /// Agent source, used for native app fallback.
    pub agent_type: Option<String>,
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
    log::warn!(
        "Process tree walk failed for PID {}. Trying TTY fallback.",
        pid
    );
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

    if let Some(bundle_id) = native_bundle_for_context(ctx) {
        if activate_bundle(bundle_id).is_ok() {
            return JumpResult::Success;
        }
    }

    if ctx
        .cmux_surface_id
        .as_deref()
        .is_some_and(|v| !v.is_empty())
    {
        return jump_cmux(
            ctx.cmux_surface_id.as_deref(),
            ctx.cmux_workspace_id.as_deref(),
        );
    }

    if ctx.zellij_pane_id.as_deref().is_some_and(|v| !v.is_empty()) {
        let res = jump_zellij(
            ctx.zellij_pane_id.as_deref(),
            ctx.zellij_session_name.as_deref(),
        );
        if let Some(term) = outer_terminal_app(ctx, &tree) {
            let _ = activate_app(&term);
        }
        return res;
    }

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

    let terminal_app = outer_terminal_app(ctx, &tree);

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
        return jump_wezterm(
            ctx.wezterm_pane.as_deref(),
            ctx.tty_path.as_deref(),
            ctx.cwd.as_deref(),
        );
    }

    // ── Kitty: kitten @ focus-window ────────────────────────────────────────
    if lower.contains("kitty") {
        return jump_kitty(ctx.kitty_window_id.as_deref(), ctx.cwd.as_deref());
    }

    // ── Kaku: CLI pane targeting ─────────────────────────────────────────────
    if lower.contains("kaku") {
        return jump_kaku(ctx.tty_path.as_deref(), ctx.cwd.as_deref());
    }

    // ── Warp: activate first, then best-effort DB/CLI targeting when possible.
    if lower.contains("warp") {
        return jump_warp(ctx.cwd.as_deref());
    }

    // Generic fallback
    match activate_app(&term_app) {
        Ok(()) => JumpResult::Success,
        Err(e) => JumpResult::Failed(e),
    }
}

/// Best-effort jump when a session does not have enough metadata for tab-level targeting.
/// This intentionally activates the terminal app instead of failing the user action outright.
pub fn jump_to_terminal_app(terminal_name: &str) -> JumpResult {
    let trimmed = terminal_name.trim();
    if trimmed.is_empty() {
        return JumpResult::TerminalNotFound;
    }

    let normalized = normalized_app_name(trimmed);
    if let Some(app_name) = registry::applescript_app_name(normalized) {
        return jump_via_applescript(app_name);
    }

    jump_via_app_activation(normalized)
}

// ─── iTerm2 ──────────────────────────────────────────────────────────────────

fn jump_iterm_by_session(session_id: &str) -> JumpResult {
    // Strip "w0t0p0:" prefix (ITERM_SESSION_ID format)
    let sid = session_id.split(':').next_back().unwrap_or(session_id);
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
    let tty_clause = tty
        .map(|t| {
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
        })
        .unwrap_or_default();

    let cwd_clause = cwd
        .map(|c| {
            format!(
                r#"try
                    if variable named "session.path" of aSession is "{c}" then
                        select aSession
                        select aWindow
                        return
                    end if
                end try"#
            )
        })
        .unwrap_or_default();

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
    let tty_clause = tty
        .map(|t| {
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
        })
        .unwrap_or_default();

    let cwd_clause = cwd
        .map(|c| {
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
        })
        .unwrap_or_default();

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

fn jump_wezterm(pane_id: Option<&str>, tty: Option<&str>, cwd: Option<&str>) -> JumpResult {
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

    if let Some(pid) = pane_id {
        tab_id = panes.iter().find_map(|p| {
            let id_str = p["pane_id"].as_i64().map(|id| id.to_string());
            if id_str.as_deref() == Some(pid) {
                p["tab_id"].as_i64()
            } else {
                None
            }
        });
    }

    if tab_id.is_none() {
        if let Some(ref tty_str) = tty_dev {
            tab_id = panes.iter().find_map(|p| {
                if p["tty_name"].as_str() == Some(tty_str.as_str()) {
                    p["tab_id"].as_i64()
                } else {
                    None
                }
            });
        }
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

// ─── Zellij ──────────────────────────────────────────────────────────────────

fn jump_zellij(pane_id: Option<&str>, session_name: Option<&str>) -> JumpResult {
    let Some(pane_id) = pane_id else {
        return JumpResult::Success;
    };
    let Some(bin) = find_binary("zellij") else {
        return JumpResult::Success;
    };

    let Some(pane_id_int) = parse_zellij_pane_id(pane_id) else {
        return JumpResult::Success;
    };

    let mut list = std::process::Command::new(&bin);
    if let Some(name) = session_name.filter(|v| !v.is_empty()) {
        list.args(["--session", name]);
    }
    list.args(["action", "list-panes", "--json", "--tab"]);

    let output = match list.output() {
        Ok(o) if o.status.success() => o,
        _ => return JumpResult::Success,
    };
    let json: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(v) => v,
        Err(_) => return JumpResult::Success,
    };

    let panes = flatten_zellij_panes(&json);
    let tab_position = panes.iter().find_map(|pane| {
        if pane["id"].as_i64() == Some(pane_id_int) {
            pane["tab_position"].as_i64()
        } else {
            None
        }
    });
    let Some(tab_position) = tab_position else {
        return JumpResult::Success;
    };

    let mut cmd = std::process::Command::new(&bin);
    if let Some(name) = session_name.filter(|v| !v.is_empty()) {
        cmd.args(["--session", name]);
    }
    cmd.args(["action", "go-to-tab", &(tab_position + 1).to_string()]);

    match cmd.output() {
        Ok(output) if output.status.success() => JumpResult::Success,
        _ => JumpResult::Success,
    }
}

// ─── cmux ────────────────────────────────────────────────────────────────────

fn jump_cmux(surface_id: Option<&str>, workspace_id: Option<&str>) -> JumpResult {
    let _ = activate_bundle("com.cmuxterm.app").or_else(|_| activate_app("cmux"));
    let Some(surface_id) = surface_id else {
        return JumpResult::Success;
    };
    let Some(bin) = find_binary("cmux") else {
        return JumpResult::Success;
    };

    let mut cmd = std::process::Command::new(&bin);
    cmd.args(["focus-panel", "--panel", surface_id]);
    if let Some(workspace_id) = workspace_id.filter(|v| !v.is_empty()) {
        cmd.args(["--workspace", workspace_id]);
    }
    let _ = cmd.output();
    JumpResult::Success
}

// ─── Kaku ────────────────────────────────────────────────────────────────────

fn jump_kaku(tty: Option<&str>, cwd: Option<&str>) -> JumpResult {
    let _ = activate_app("Kaku");
    let Some(bin) = find_binary("kaku") else {
        return JumpResult::Success;
    };

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
    let Some(panes) = json.as_array() else {
        return JumpResult::Success;
    };

    let tty_dev = tty.map(tty_to_dev_path);
    let mut pane_id: Option<i64> = None;
    let mut tab_id: Option<i64> = None;

    if let Some(ref tty_str) = tty_dev {
        if let Some(pane) = panes
            .iter()
            .find(|pane| pane["tty_name"].as_str() == Some(tty_str.as_str()))
        {
            pane_id = pane["pane_id"].as_i64();
            tab_id = pane["tab_id"].as_i64();
        }
    }

    if pane_id.is_none() && tab_id.is_none() {
        if let Some(cwd) = cwd {
            let cwd_url = format!("file://{}", cwd);
            if let Some(pane) = panes.iter().find(|pane| {
                let pane_cwd = pane["cwd"].as_str().unwrap_or("");
                pane_cwd == cwd || pane_cwd == cwd_url
            }) {
                pane_id = pane["pane_id"].as_i64();
                tab_id = pane["tab_id"].as_i64();
            }
        }
    }

    if let Some(id) = pane_id {
        let _ = std::process::Command::new(&bin)
            .args(["cli", "activate-pane", "--pane-id", &id.to_string()])
            .output();
    } else if let Some(id) = tab_id {
        let _ = std::process::Command::new(&bin)
            .args(["cli", "activate-tab", "--tab-id", &id.to_string()])
            .output();
    }
    JumpResult::Success
}

// ─── Warp ────────────────────────────────────────────────────────────────────

fn jump_warp(cwd: Option<&str>) -> JumpResult {
    let _ = activate_app("Warp");

    let Some(cwd) = cwd.filter(|v| !v.is_empty()) else {
        return JumpResult::Success;
    };
    let Some(sqlite) = find_binary("sqlite3") else {
        return JumpResult::Success;
    };
    let Some(db) = warp_database_path() else {
        return JumpResult::Success;
    };

    if let Some(position) = warp_tab_position(&sqlite, &db, cwd) {
        send_warp_go_to_tab(position);
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
        Ok(o) => JumpResult::Failed(String::from_utf8_lossy(&o.stderr).trim().to_string()),
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

fn activate_bundle(bundle_id: &str) -> Result<(), String> {
    let output = std::process::Command::new("open")
        .args(["-b", bundle_id])
        .output()
        .map_err(|e| format!("Failed to run open: {}", e))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn native_bundle_for_context(ctx: &JumpContext) -> Option<&'static str> {
    if let Some(bundle_id) = ctx.term_bundle_id.as_deref() {
        if !registry::is_terminal_bundle(bundle_id) {
            return Some(match bundle_id {
                "com.openai.chat" => "com.openai.chat",
                "com.openai.codex" => "com.openai.codex",
                "com.todesktop.230313mzl4w4u92" => "com.todesktop.230313mzl4w4u92",
                "com.trae.app" => "com.trae.app",
                "com.qoder.ide" => "com.qoder.ide",
                "com.factory.app" => "com.factory.app",
                "com.tencent.codebuddy" => "com.tencent.codebuddy",
                "com.tencent.codebuddy.cn" => "com.tencent.codebuddy.cn",
                "com.stepfun.app" => "com.stepfun.app",
                "ai.opencode.desktop" => "ai.opencode.desktop",
                "com.workbuddy.workbuddy" => "com.workbuddy.workbuddy",
                _ => return None,
            });
        }
    }

    match ctx.agent_type.as_deref() {
        Some("cursor") => Some("com.todesktop.230313mzl4w4u92"),
        Some("trae") | Some("traecn") => Some("com.trae.app"),
        Some("qoder") => Some("com.qoder.ide"),
        Some("droid") => Some("com.factory.app"),
        Some("codebuddy") => Some("com.tencent.codebuddy"),
        Some("codebuddycn") | Some("codybuddycn") => Some("com.tencent.codebuddy.cn"),
        Some("stepfun") => Some("com.stepfun.app"),
        Some("opencode") => Some("ai.opencode.desktop"),
        Some("workbuddy") => Some("com.workbuddy.workbuddy"),
        _ => None,
    }
}

fn outer_terminal_app(
    ctx: &JumpContext,
    tree: &std::collections::HashMap<u32, process_tree::ProcessInfo>,
) -> Option<String> {
    process_tree::find_terminal_app_name(ctx.pid, tree)
        .or_else(|| ctx.terminal_app.clone())
        .or_else(|| {
            ctx.tty_path
                .as_ref()
                .and_then(|tty| find_terminal_app_for_tty(tty))
        })
}

fn normalized_app_name(name: &str) -> &str {
    let lower = name.to_ascii_lowercase();

    if lower.contains("iterm") {
        "iTerm2"
    } else if lower.contains("ghostty") {
        "Ghostty"
    } else if lower.contains("wezterm") || lower.contains("wez") {
        "WezTerm"
    } else if lower.contains("kitty") {
        "kitty"
    } else if lower.contains("kaku") {
        "Kaku"
    } else if lower.contains("cmux") {
        "cmux"
    } else if lower.contains("warp") {
        "Warp"
    } else if lower.contains("alacritty") {
        "Alacritty"
    } else if lower.contains("terminal") && !lower.contains("ghostty") {
        "Terminal"
    } else {
        name
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

fn warp_database_path() -> Option<String> {
    let home = dirs::home_dir()?;
    let candidates = [
        home.join("Library/Group Containers/2BBY89MBSN.dev.warp/Library/Application Support/dev.warp.Warp-Stable/warp.sqlite"),
        home.join("Library/Application Support/dev.warp.Warp-Stable/warp.sqlite"),
    ];
    candidates
        .into_iter()
        .find(|path| path.exists())
        .map(|path| path.display().to_string())
}

fn warp_tab_position(sqlite: &str, db: &str, cwd: &str) -> Option<i64> {
    let candidates = cwd_variants(cwd);
    if candidates.is_empty() {
        return None;
    }
    let values = candidates
        .into_iter()
        .map(|candidate| format!("'{}'", candidate.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(", ");
    let query = format!(
        r#"
SELECT tab_idx + 1
FROM (
    SELECT
        tp.id,
        tp.is_active,
        COALESCE(pl.is_focused, 0) AS focused,
        w.active_tab_index,
        (
            SELECT COUNT(*) FROM tabs t2
            WHERE t2.window_id = t.window_id AND t2.id < t.id
        ) AS tab_idx
    FROM terminal_panes tp
    LEFT JOIN pane_leaves pl ON tp.id = pl.pane_node_id AND pl.kind = 'terminal'
    LEFT JOIN pane_nodes pn ON tp.id = pn.id
    LEFT JOIN tabs t ON pn.tab_id = t.id
    LEFT JOIN windows w ON t.window_id = w.id
    WHERE tp.cwd IN ({values})
    ORDER BY tp.is_active DESC, focused DESC, tp.id DESC
    LIMIT 1
)
WHERE active_tab_index != tab_idx
  AND tab_idx BETWEEN 0 AND 8;
"#
    );
    let output = std::process::Command::new(sqlite)
        .args([db, query.as_str()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<i64>()
        .ok()
        .filter(|position| (1..=9).contains(position))
}

fn cwd_variants(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut seeds = vec![trimmed.to_string()];
    if trimmed.ends_with('/') && trimmed != "/" {
        seeds.push(trimmed.trim_end_matches('/').to_string());
    } else {
        seeds.push(format!("{trimmed}/"));
    }
    let mut variants = std::collections::BTreeSet::new();
    for seed in seeds {
        variants.insert(seed.clone());
        if let Some(rest) = seed.strip_prefix("/private/") {
            variants.insert(format!("/{rest}"));
        } else if seed.starts_with("/tmp") || seed.starts_with("/var") || seed.starts_with("/etc") {
            variants.insert(format!("/private{seed}"));
        }
    }
    variants.into_iter().collect()
}

fn send_warp_go_to_tab(position: i64) {
    if !(1..=9).contains(&position) {
        return;
    }
    let script = format!(
        r#"tell application "System Events"
    tell application process "Warp"
        keystroke "{}" using command down
    end tell
end tell"#,
        position
    );
    let _ = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output();
}

fn tty_to_dev_path(tty: &str) -> String {
    if tty.starts_with("/dev/") {
        tty.to_string()
    } else {
        format!("/dev/tty{}", tty)
    }
}

#[cfg(test)]
mod tests {
    use super::normalized_app_name;

    #[test]
    fn normalizes_session_terminal_labels_for_app_activation() {
        assert_eq!(normalized_app_name("iTerm·tmux"), "iTerm2");
        assert_eq!(normalized_app_name("Apple Terminal"), "Terminal");
        assert_eq!(normalized_app_name("WezTerm CLI"), "WezTerm");
        assert_eq!(normalized_app_name("Ghostty"), "Ghostty");
        assert_eq!(normalized_app_name("custom-term"), "custom-term");
    }
}
