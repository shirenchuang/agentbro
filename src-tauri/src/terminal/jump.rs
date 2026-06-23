// Jump — Focus the terminal where an agent session is running
// Supports: iTerm2 (AppleScript by session ID), Ghostty (CWD/title matching),
//           Terminal.app (TTY matching), Kitty (kitten @), WezTerm (wezterm cli),
//           tmux (select-window + select-pane), and generic app activation

#![cfg_attr(target_os = "windows", allow(dead_code))]

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
    /// Wave Terminal block routing metadata for native RPC focus.
    pub waveterm_block_id: Option<String>,
    pub waveterm_tab_id: Option<String>,
    pub waveterm_jwt: Option<String>,
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
    /// TERM_PROGRAM saved by the hook payload.
    pub term_program: Option<String>,
    /// Frontmost bundle identifier saved by the hook payload.
    pub term_bundle_id: Option<String>,
    /// Agent source, used for native app fallback.
    pub agent_type: Option<String>,
}

/// Jump to the terminal containing the given agent process (simple PID-based entrypoint).
/// For precise tab targeting, use `jump_to_terminal_with_context`.
pub fn jump_to_terminal(pid: u32) -> JumpResult {
    let tree = process_tree::build_tree();

    #[cfg(target_os = "windows")]
    {
        let ctx = JumpContext {
            pid,
            ..Default::default()
        };
        jump_to_terminal_windows(&ctx, &tree)
    }

    #[cfg(not(target_os = "windows"))]
    {
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
}

/// Precise tab-level jump using full session context (iTerm2 session ID, Kitty window ID, etc.)
pub fn jump_to_terminal_with_context(ctx: &JumpContext) -> JumpResult {
    let tree = process_tree::build_tree();

    #[cfg(target_os = "windows")]
    {
        jump_to_terminal_windows(ctx, &tree)
    }

    #[cfg(not(target_os = "windows"))]
    {
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
                if let Some(term) = outer_terminal_app(ctx, &tree) {
                    let _ = activate_app(&term);
                }
                return res;
            }
        }

        if process_tree::is_in_tmux(ctx.pid, &tree) {
            let res = jump_via_tmux_pid(ctx.pid);
            if let Some(term) = outer_terminal_app(ctx, &tree) {
                let _ = activate_app(&term);
            }
            return res;
        }

        let terminal_app = outer_terminal_app(ctx, &tree);

        let Some(term_app) = terminal_app else {
            if let Some(bundle_id) = native_bundle_for_context(ctx) {
                if activate_bundle(bundle_id).is_ok() {
                    return JumpResult::Success;
                }
            }
            return JumpResult::TerminalNotFound;
        };

        if let Some(bundle_id) = ctx.term_bundle_id.as_deref() {
            if is_ide_host_bundle(bundle_id) && !native_bundle_matches_context(ctx, bundle_id) {
                return jump_to_ide_window(bundle_id, ctx.cwd.as_deref());
            }
        }

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
            return jump_ghostty(
                ctx.cwd.as_deref(),
                ctx.tty_path.as_deref(),
                None,
                ctx.agent_type.as_deref(),
            );
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

        // ── Wave: focus the owning block through Wave's native RPC ───────────────
        if lower.contains("wave") {
            return jump_wave(
                ctx.waveterm_block_id.as_deref(),
                ctx.waveterm_tab_id.as_deref(),
                ctx.waveterm_jwt.as_deref(),
            );
        }

        // ── Kitty: kitten @ focus-window ────────────────────────────────────────
        if lower.contains("kitty") {
            return jump_kitty(ctx.kitty_window_id.as_deref(), ctx.cwd.as_deref());
        }

        // ── Kaku: CLI pane targeting ─────────────────────────────────────────────
        if lower.contains("kaku") {
            return jump_kaku(
                ctx.wezterm_pane.as_deref(),
                ctx.tty_path.as_deref(),
                ctx.cwd.as_deref(),
            );
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
}

/// Best-effort jump when a session does not have enough metadata for tab-level targeting.
/// This intentionally activates the terminal app instead of failing the user action outright.
pub fn jump_to_terminal_app(terminal_name: &str) -> JumpResult {
    let trimmed = terminal_name.trim();
    if trimmed.is_empty() {
        return JumpResult::TerminalNotFound;
    }

    #[cfg(target_os = "windows")]
    {
        jump_to_terminal_app_windows(trimmed, None)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let normalized = normalized_app_name(trimmed);
        if let Some(app_name) = registry::applescript_app_name(normalized) {
            return jump_via_applescript(app_name);
        }

        jump_via_app_activation(normalized)
    }
}

/// Best-effort focus for IDE integrated terminals. We cannot reliably select an
/// internal terminal pane without app-specific Accessibility automation, so we
/// raise the IDE window whose title most likely matches the session cwd.
pub fn jump_to_ide_window(bundle_id: &str, cwd: Option<&str>) -> JumpResult {
    let activation = activate_bundle(bundle_id);
    let Some(folder) = cwd.and_then(cwd_folder_name) else {
        return match activation {
            Ok(()) => JumpResult::Success,
            Err(err) => JumpResult::Failed(err),
        };
    };

    let script = ide_window_script(bundle_id, folder);
    match run_osascript(&script) {
        JumpResult::Success => JumpResult::Success,
        JumpResult::Failed(_) if activation.is_ok() => JumpResult::Success,
        other => other,
    }
}

// ─── iTerm2 ──────────────────────────────────────────────────────────────────

fn jump_iterm_by_session(session_id: &str) -> JumpResult {
    // Strip "w0t0p0:" prefix (ITERM_SESSION_ID format)
    let sid = applescript_escape(session_id.split(':').next_back().unwrap_or(session_id));
    let script = format!(
        r#"tell application "iTerm2"
    activate
    repeat with aWindow in windows
        try
            if miniaturized of aWindow then set miniaturized of aWindow to false
        end try
        repeat with aTab in tabs of aWindow
            repeat with aSession in sessions of aTab
                if unique ID of aSession is "{sid}" then
                    select aTab
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
    let script = iterm_by_tty_or_cwd_script(tty, cwd);
    run_osascript(&script)
}

fn iterm_by_tty_or_cwd_script(tty: Option<&str>, cwd: Option<&str>) -> String {
    let tty_clause = tty
        .map(|t| {
            let dev = applescript_escape(&tty_to_dev_path(t));
            format!(
                r#"try
                    if tty of aSession contains "{dev}" then
                        select aTab
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
            let cwd = applescript_escape(c.trim_end_matches('/'));
            let folder = cwd_folder_name(c).map(applescript_escape).unwrap_or_default();
            format!(
                r#"try
                    if variable named "session.path" of aSession is "{cwd}" or path of aSession contains "{folder}" or name of aSession contains "{folder}" then
                        select aTab
                        select aSession
                        select aWindow
                        return
                    end if
                end try"#
            )
        })
        .unwrap_or_default();

    if tty_clause.is_empty() && cwd_clause.is_empty() {
        return r#"tell application "iTerm2" to activate"#.to_string();
    }

    format!(
        r#"tell application "iTerm2"
    activate
    repeat with aWindow in windows
        try
            if miniaturized of aWindow then set miniaturized of aWindow to false
        end try
        repeat with aTab in tabs of aWindow
            repeat with aSession in sessions of aTab
                {tty_clause}
                {cwd_clause}
            end repeat
        end repeat
    end repeat
end tell"#
    )
}

// ─── Ghostty ─────────────────────────────────────────────────────────────────

fn jump_ghostty(
    cwd: Option<&str>,
    tty: Option<&str>,
    session_id: Option<&str>,
    source: Option<&str>,
) -> JumpResult {
    let _ = run_osascript(r#"tell application "Ghostty" to activate"#);

    let cwd = match cwd.filter(|c| !c.is_empty()) {
        Some(c) => c,
        None => return JumpResult::Success,
    };

    let cwd_candidates = cwd_variants(cwd)
        .into_iter()
        .map(|candidate| {
            format!(
                r#""{}""#,
                applescript_escape(candidate.trim_end_matches('/'))
            )
        })
        .collect::<Vec<_>>();
    let cwd_list = if cwd_candidates.is_empty() {
        "{}".to_string()
    } else {
        format!("{{{}}}", cwd_candidates.join(", "))
    };
    let folder = cwd_folder_name(cwd).unwrap_or(cwd);
    let escaped_folder = applescript_escape(folder);
    let escaped_source = applescript_escape(source.unwrap_or(""));
    let escaped_session_prefix = applescript_escape(
        session_id
            .filter(|sid| !sid.is_empty())
            .map(|sid| sid.chars().take(8).collect::<String>())
            .as_deref()
            .unwrap_or(""),
    );

    // If we have a tty, try the precise FD-order mapping first.
    // Ghostty's ptmx FDs sorted by FD number correspond 1:1 with
    // the AppleScript terminal enumeration order within each tab.
    if let Some(tty) = tty.filter(|t| !t.is_empty()) {
        if let Some(index) = ghostty_terminal_index_for_tty(tty) {
            // Focus the terminal at the computed index, but verify CWD matches
            // to guard against stale mappings or multi-window mismatches.
            let script = format!(
                r#"tell application "Ghostty"
    repeat with w in windows
        repeat with t in tabs of w
            if (count terminals of t) ≥ {index} then
                set term to terminal {index} of t
                try
                    if {cwd_list} contains (working directory of term as text) then
                        focus term
                        activate
                        return "focused"
                    end if
                end try
            end if
        end repeat
    end repeat
end tell"#,
                index = index,
            );
            let result = run_osascript(&script);
            if matches!(result, JumpResult::Success) {
                return result;
            }
        }
    }

    let script = ghostty_cwd_title_script(
        &cwd_list,
        &escaped_folder,
        &escaped_source,
        &escaped_session_prefix,
    );
    run_osascript(&script)
}

fn ghostty_cwd_title_script(
    cwd_list: &str,
    folder: &str,
    source: &str,
    session_prefix: &str,
) -> String {
    format!(
        r#"tell application "Ghostty"
    set targetCwds to {cwd_list}
    set targetFolder to "{folder}"
    set targetSource to "{source}"
    set targetSession to "{session_prefix}"
    set matches to {{}}
    repeat with term in terminals
        try
            if targetCwds contains (working directory of term as text) then
                set end of matches to term
            end if
        end try
    end repeat
    if (count of matches) = 0 then
        repeat with term in terminals
            try
                set termName to (name of term as text)
                if targetFolder is not "" and termName contains targetFolder then
                    set end of matches to term
                end if
            end try
        end repeat
    end if
    if targetSession is not "" then
        repeat with term in matches
            try
                if name of term contains targetSession then
                    focus term
                    activate
                    return "focused"
                end if
            end try
        end repeat
    end if
    if targetSource is not "" then
        repeat with term in matches
            try
                if name of term contains targetSource then
                    focus term
                    activate
                    return "focused"
                end if
            end try
        end repeat
    end if
    if (count of matches) > 0 then
        focus (item 1 of matches)
    end if
    activate
end tell
try
    tell application "System Events"
        tell process "Ghostty"
            set frontmost to true
            repeat with w in windows
                try
                    if value of attribute "AXMinimized" of w is true then
                        set value of attribute "AXMinimized" of w to false
                    end if
                end try
            end repeat
        end tell
    end tell
end try"#
    )
}

/// Map a tty device path to a 1-based Ghostty AppleScript terminal index.
///
/// Ghostty opens one ptmx FD per terminal pane. The FD creation order matches
/// the AppleScript enumeration order of `terminals of tab`. We parse `lsof`
/// output for the Ghostty process, sort ptmx entries by FD number, and find
/// the position of the entry whose pty minor equals our tty's minor number.
fn ghostty_terminal_index_for_tty(tty: &str) -> Option<usize> {
    let tty_dev = tty_to_dev_path(tty);
    // Extract minor number from tty device: /dev/ttys003 → 3
    let tty_minor: u32 = tty_dev
        .strip_prefix("/dev/ttys")
        .and_then(|s| s.parse().ok())?;

    // Find Ghostty PID
    let pgrep_output = std::process::Command::new("pgrep")
        .args(["-x", "ghostty"])
        .output()
        .ok()?;
    let ghostty_pid = String::from_utf8_lossy(&pgrep_output.stdout)
        .lines()
        .next()?
        .trim()
        .to_string();

    // Parse lsof output for Ghostty's ptmx file descriptors
    let lsof_output = std::process::Command::new("lsof")
        .args(["-p", &ghostty_pid])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&lsof_output.stdout);

    // Collect (fd_number, pty_minor) pairs from ptmx entries
    // lsof line format: "ghostty 719 user   10u   CHR   15,1   0t645   611   /dev/ptmx"
    let mut fd_minor_pairs: Vec<(u32, u32)> = stdout
        .lines()
        .filter(|line| line.contains("/dev/ptmx"))
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 6 {
                return None;
            }
            // FD field like "10u" → extract number
            let fd_str = parts[3].trim_end_matches(|c: char| c.is_ascii_alphabetic());
            let fd_num: u32 = fd_str.parse().ok()?;
            // Device field like "15,1" → extract minor
            let dev_field = parts[5];
            let minor: u32 = dev_field.split(',').nth(1)?.parse().ok()?;
            Some((fd_num, minor))
        })
        .collect();

    // Sort by FD number (creation order = AppleScript enumeration order)
    fd_minor_pairs.sort_by_key(|&(fd, _)| fd);

    // Find the 1-based position of our tty's minor
    fd_minor_pairs
        .iter()
        .position(|&(_, minor)| minor == tty_minor)
        .map(|pos| pos + 1)
}

// ─── Terminal.app ────────────────────────────────────────────────────────────

fn jump_terminal_app(tty: Option<&str>, cwd: Option<&str>) -> JumpResult {
    let script = terminal_app_script(tty, cwd);
    run_osascript(&script)
}

fn terminal_app_script(tty: Option<&str>, cwd: Option<&str>) -> String {
    let target_tty = tty
        .filter(|value| !value.is_empty())
        .map(tty_to_dev_path)
        .map(|value| applescript_escape(&value))
        .unwrap_or_default();
    let target_dir = cwd
        .and_then(cwd_folder_name)
        .map(applescript_escape)
        .unwrap_or_default();
    let tty_clause = tty
        .filter(|value| !value.is_empty())
        .map(|_| {
            r#"if not found and targetTty is not "" then
            repeat with aTab in tabs of aWindow
                try
                    if tty of aTab is targetTty then
                        if miniaturized of aWindow then set miniaturized of aWindow to false
                        set selected tab of aWindow to aTab
                        set index of aWindow to 1
                        set found to true
                        exit repeat
                    end if
                end try
            end repeat
        end if"#
        })
        .unwrap_or("");
    let cwd_clause = cwd
        .and_then(cwd_folder_name)
        .map(|_| {
            r#"if not found and targetDir is not "" then
            repeat with aTab in tabs of aWindow
                try
                    if (name of aTab as text) contains targetDir or custom title of aTab contains targetDir then
                        if miniaturized of aWindow then set miniaturized of aWindow to false
                        set selected tab of aWindow to aTab
                        set index of aWindow to 1
                        set found to true
                        exit repeat
                    end if
                end try
            end repeat
        end if"#
        })
        .unwrap_or("");

    format!(
        r#"tell application "Terminal"
    set targetTty to "{target_tty}"
    set targetDir to "{target_dir}"
    set found to false
    activate
    repeat with aWindow in windows
        try
            if miniaturized of aWindow then set miniaturized of aWindow to false
        end try
        {tty_clause}
        {cwd_clause}
        if found then exit repeat
    end repeat
    if not found then
        repeat with aWindow in windows
            try
                if miniaturized of aWindow then
                    set miniaturized of aWindow to false
                    set index of aWindow to 1
                    exit repeat
                end if
            end try
        end repeat
    end if
end tell
try
    tell application "System Events"
        tell process "Terminal"
            set frontmost to true
            repeat with w in windows
                try
                    if value of attribute "AXMinimized" of w is true then
                        set value of attribute "AXMinimized" of w to false
                    end if
                end try
            end repeat
        end tell
    end tell
end try"#
    )
}

// ─── WezTerm ─────────────────────────────────────────────────────────────────

fn jump_wezterm(pane_id: Option<&str>, tty: Option<&str>, cwd: Option<&str>) -> JumpResult {
    let _ = activate_app("WezTerm");

    let Some(bin) = find_binary("wezterm") else {
        return JumpResult::Success;
    };

    if let Some(pid) = pane_id.filter(|value| !value.is_empty()) {
        if std::process::Command::new(&bin)
            .args(["cli", "activate-pane", "--pane-id", pid])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
        {
            return JumpResult::Success;
        }
    }

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

    let mut pane_match_id: Option<i64> = None;
    let mut tab_id: Option<i64> = None;

    if let Some(pid) = pane_id {
        if let Some(p) = panes.iter().find(|p| {
            let id_str = p["pane_id"].as_i64().map(|id| id.to_string());
            id_str.as_deref() == Some(pid)
        }) {
            pane_match_id = p["pane_id"].as_i64();
            tab_id = p["tab_id"].as_i64();
        }
    }

    if pane_match_id.is_none() && tab_id.is_none() {
        if let Some(ref tty_str) = tty_dev {
            if let Some(p) = panes
                .iter()
                .find(|p| p["tty_name"].as_str() == Some(tty_str.as_str()))
            {
                pane_match_id = p["pane_id"].as_i64();
                tab_id = p["tab_id"].as_i64();
            }
        }
    }

    if pane_match_id.is_none() && tab_id.is_none() {
        if let Some(cwd_str) = cwd {
            let cwd_url = format!("file://{}", cwd_str);
            if let Some(p) = panes.iter().find(|p| {
                let pane_cwd = p["cwd"].as_str().unwrap_or("");
                pane_cwd == cwd_url || pane_cwd == cwd_str
            }) {
                pane_match_id = p["pane_id"].as_i64();
                tab_id = p["tab_id"].as_i64();
            }
        }
    }

    if let Some(id) = pane_match_id {
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

// ─── Wave ───────────────────────────────────────────────────────────────────

fn jump_wave(block_id: Option<&str>, tab_id: Option<&str>, jwt: Option<&str>) -> JumpResult {
    let _ = activate_app("Wave").or_else(|_| activate_bundle("dev.commandline.waveterm"));

    let Some(block_id) = block_id.filter(|value| !value.is_empty()) else {
        return JumpResult::Success;
    };
    let Some(tab_id) = tab_id.filter(|value| !value.is_empty()) else {
        return JumpResult::Success;
    };
    let Some(jwt) = jwt.filter(|value| !value.is_empty()) else {
        return JumpResult::Success;
    };
    match crate::terminal::wave::focus_block(block_id, tab_id, jwt) {
        Ok(()) => JumpResult::Success,
        Err(err) => JumpResult::Failed(err),
    }
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

fn jump_kaku(pane_id: Option<&str>, tty: Option<&str>, cwd: Option<&str>) -> JumpResult {
    let _ = activate_app("Kaku");
    let Some(bin) = find_binary("kaku") else {
        return JumpResult::Success;
    };

    if let Some(pid) = pane_id.filter(|value| !value.is_empty()) {
        if std::process::Command::new(&bin)
            .args(["cli", "activate-pane", "--pane-id", pid])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
        {
            return JumpResult::Success;
        }
    }

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

    run(&["switch-client", "-t", pane]);
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

#[cfg(target_os = "windows")]
#[derive(Debug, Eq, PartialEq)]
struct WindowsTerminalCommand {
    program: String,
    args: Vec<String>,
    current_dir: Option<String>,
    new_console: bool,
}

#[cfg(target_os = "windows")]
fn jump_to_terminal_windows(
    ctx: &JumpContext,
    tree: &std::collections::HashMap<u32, process_tree::ProcessInfo>,
) -> JumpResult {
    if let Some(terminal) = outer_terminal_app(ctx, tree).or_else(|| ctx.terminal_app.clone()) {
        return jump_to_terminal_app_windows(&terminal, ctx.cwd.as_deref());
    }

    if ctx.cwd.as_deref().is_some_and(|cwd| !cwd.trim().is_empty()) {
        return jump_to_terminal_app_windows("", ctx.cwd.as_deref());
    }

    JumpResult::TerminalNotFound
}

#[cfg(target_os = "windows")]
fn jump_to_terminal_app_windows(terminal_name: &str, cwd: Option<&str>) -> JumpResult {
    let mut errors = Vec::new();
    for spec in windows_terminal_command_candidates(terminal_name, cwd) {
        match spawn_windows_terminal_command(&spec) {
            Ok(()) => return JumpResult::Success,
            Err(err) => errors.push(format!("{}: {err}", spec.program)),
        }
    }

    if errors.is_empty() {
        JumpResult::TerminalNotFound
    } else {
        JumpResult::Failed(errors.join("; "))
    }
}

#[cfg(target_os = "windows")]
fn windows_terminal_command_candidates(
    terminal_name: &str,
    cwd: Option<&str>,
) -> Vec<WindowsTerminalCommand> {
    let lower = terminal_name.to_ascii_lowercase();
    let mut kinds = if lower.contains("pwsh") {
        vec!["pwsh", "powershell", "wt", "cmd"]
    } else if lower.contains("powershell") {
        vec!["powershell", "pwsh", "wt", "cmd"]
    } else if lower.contains("cmd") || lower.contains("command prompt") {
        vec!["cmd", "wt", "powershell", "pwsh"]
    } else if lower.contains("windows terminal") || lower == "wt" || lower == "wt.exe" {
        vec!["wt", "cmd", "powershell", "pwsh"]
    } else {
        vec!["wt", "cmd", "powershell", "pwsh"]
    };

    kinds.dedup();
    kinds
        .into_iter()
        .map(|kind| windows_terminal_command(kind, cwd))
        .collect()
}

#[cfg(target_os = "windows")]
fn windows_terminal_command(kind: &str, cwd: Option<&str>) -> WindowsTerminalCommand {
    let cwd = cwd.map(str::trim).filter(|value| !value.is_empty());
    match kind {
        "cmd" => WindowsTerminalCommand {
            program: crate::agents::executable::command_path("cmd")
                .display()
                .to_string(),
            args: vec!["/K".to_string()],
            current_dir: cwd.map(ToString::to_string),
            new_console: true,
        },
        "powershell" | "pwsh" => {
            let mut args = vec!["-NoExit".to_string()];
            if let Some(cwd) = cwd {
                args.push("-Command".to_string());
                args.push(format!(
                    "Set-Location -LiteralPath {}",
                    windows_powershell_string_literal(cwd)
                ));
            }
            WindowsTerminalCommand {
                program: crate::agents::executable::command_path(kind)
                    .display()
                    .to_string(),
                args,
                current_dir: None,
                new_console: false,
            }
        }
        _ => {
            let mut args = Vec::new();
            if let Some(cwd) = cwd {
                args.extend(["-d".to_string(), cwd.to_string()]);
            }
            WindowsTerminalCommand {
                program: crate::agents::executable::command_path("wt")
                    .display()
                    .to_string(),
                args,
                current_dir: None,
                new_console: false,
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn spawn_windows_terminal_command(spec: &WindowsTerminalCommand) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    const CREATE_NEW_CONSOLE: u32 = 0x00000010;

    let mut command = std::process::Command::new(&spec.program);
    command.args(&spec.args);
    if let Some(cwd) = &spec.current_dir {
        command.current_dir(cwd);
    }
    if spec.new_console {
        command.creation_flags(CREATE_NEW_CONSOLE);
    }
    command.spawn().map(|_| ()).map_err(|err| err.to_string())
}

#[cfg(target_os = "windows")]
fn windows_powershell_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
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

fn ide_window_script(bundle_id: &str, folder: &str) -> String {
    let bundle_id = applescript_escape(bundle_id);
    let folder = applescript_escape(folder);
    format!(
        r#"tell application "System Events"
    set matchingProcesses to application processes whose bundle identifier is "{bundle_id}"
    if (count of matchingProcesses) is 0 then return
    set targetProcess to item 1 of matchingProcesses
    set frontmost of targetProcess to true
    set bestWindow to missing value
    set bestLen to 999999
    repeat with w in windows of targetProcess
        try
            set wName to name of w as text
            if wName contains "{folder}" then
                if value of attribute "AXMinimized" of w is true then
                    set value of attribute "AXMinimized" of w to false
                end if
                set wLen to count of wName
                if wLen < bestLen then
                    set bestWindow to w
                    set bestLen to wLen
                end if
            end if
        end try
    end repeat
    if bestWindow is not missing value then
        perform action "AXRaise" of bestWindow
    end if
end tell"#
    )
}

fn applescript_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn cwd_folder_name(cwd: &str) -> Option<&str> {
    let trimmed = cwd.trim().trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        return None;
    }
    trimmed.rsplit(['/', '\\']).find(|part| !part.is_empty())
}

fn native_bundle_for_context(ctx: &JumpContext) -> Option<&'static str> {
    if let Some(bundle_id) = ctx.term_bundle_id.as_deref() {
        if !registry::is_terminal_bundle(bundle_id) {
            if !native_bundle_matches_context(ctx, bundle_id) {
                return None;
            }
            return Some(match bundle_id {
                "com.openai.chat" => "com.openai.chat",
                "com.openai.codex" => "com.openai.codex",
                "com.todesktop.230313mzl4w4u92" => "com.todesktop.230313mzl4w4u92",
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

fn native_bundle_matches_context(ctx: &JumpContext, bundle_id: &str) -> bool {
    let lower = bundle_id.to_ascii_lowercase();
    matches!(
        (ctx.agent_type.as_deref(), lower.as_str()),
        (Some("codex"), "com.openai.codex")
            | (Some("cursor"), "com.todesktop.230313mzl4w4u92")
            | (Some("cursor-cli"), "com.todesktop.230313mzl4w4u92")
            | (Some("qoder"), "com.qoder.ide")
            | (Some("qoder-cli"), "com.qoder.ide")
            | (Some("droid"), "com.factory.app")
            | (Some("codebuddy"), "com.tencent.codebuddy")
            | (Some("codebuddycn"), "com.tencent.codebuddy.cn")
            | (Some("codybuddycn"), "com.tencent.codebuddy.cn")
            | (Some("stepfun"), "com.stepfun.app")
            | (Some("opencode"), "ai.opencode.desktop")
            | (Some("workbuddy"), "com.workbuddy.workbuddy")
    )
}

fn is_ide_host_bundle(bundle_id: &str) -> bool {
    let lower = bundle_id.to_ascii_lowercase();
    lower.contains("vscode")
        || lower.contains("vscodium")
        || lower.contains("todesktop.230313mzl4w4u92")
        || lower.contains("cursor")
        || lower.contains("windsurf")
        || lower.contains("codeium")
        || lower.contains("zed")
        || lower.contains("jetbrains")
        || lower.contains("xcode")
        || lower == "com.apple.dt.xcode"
        || lower.contains("panic.nova")
        || lower.contains("android.studio")
        || lower.contains("antigravity")
        || lower == "com.qoder.ide"
        || lower == "com.qoder.ide.helper"
        || lower == "com.factory.app"
        || lower == "com.tencent.codebuddy"
        || lower == "com.tencent.codebuddy.cn"
        || lower == "com.stepfun.app"
        || lower == "ai.opencode.desktop"
        || lower == "com.workbuddy.workbuddy"
}

fn outer_terminal_app(
    ctx: &JumpContext,
    tree: &std::collections::HashMap<u32, process_tree::ProcessInfo>,
) -> Option<String> {
    ctx.term_bundle_id
        .as_deref()
        .and_then(terminal_app_from_bundle_id)
        .map(ToString::to_string)
        .or_else(|| {
            ctx.term_program
                .as_deref()
                .and_then(terminal_app_from_term_program)
                .map(ToString::to_string)
        })
        .or_else(|| process_tree::find_terminal_app_name(ctx.pid, tree))
        .or_else(|| {
            ctx.terminal_app
                .as_deref()
                .filter(|app| registry::is_terminal(app))
                .map(normalized_app_name)
                .map(ToString::to_string)
        })
        .or_else(|| {
            ctx.tty_path
                .as_ref()
                .and_then(|tty| find_terminal_app_for_tty(tty))
        })
}

fn terminal_app_from_bundle_id(bundle_id: &str) -> Option<&'static str> {
    let lower = bundle_id.to_ascii_lowercase();
    if lower.contains("iterm") {
        Some("iTerm2")
    } else if lower.contains("apple.terminal") {
        Some("Terminal")
    } else if lower.contains("ghostty") {
        Some("Ghostty")
    } else if lower.contains("wezterm") {
        Some("WezTerm")
    } else if lower.contains("kitty") {
        Some("kitty")
    } else if lower.contains("kaku") {
        Some("Kaku")
    } else if lower.contains("cmux") {
        Some("cmux")
    } else if lower.contains("warp") {
        Some("Warp")
    } else if lower.contains("waveterm") || lower.contains("commandline.wave") {
        Some("Wave")
    } else if lower.contains("alacritty") {
        Some("Alacritty")
    } else if lower.contains("vscode") || lower.contains("microsoft.vscode") {
        Some("Code")
    } else if lower.contains("cursor") || lower.contains("todesktop.230313mzl4w4u92") {
        Some("Cursor")
    } else if lower.contains("windsurf") {
        Some("Windsurf")
    } else if lower.contains("zed") {
        Some("zed")
    } else {
        None
    }
}

fn terminal_app_from_term_program(term_program: &str) -> Option<&'static str> {
    let lower = term_program.to_ascii_lowercase();
    if lower.contains("iterm") {
        Some("iTerm2")
    } else if lower.contains("apple_terminal") || lower == "terminal" {
        Some("Terminal")
    } else if lower.contains("ghostty") {
        Some("Ghostty")
    } else if lower.contains("wezterm") {
        Some("WezTerm")
    } else if lower.contains("kitty") {
        Some("kitty")
    } else if lower.contains("warp") {
        Some("Warp")
    } else if lower.contains("wave") {
        Some("Wave")
    } else if lower.contains("alacritty") {
        Some("Alacritty")
    } else if lower.contains("vscode") {
        Some("Code")
    } else if lower.contains("cursor") {
        Some("Cursor")
    } else {
        None
    }
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
    } else if lower.contains("wave") {
        "Wave"
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
    let db_uri = sqlite_file_uri(db);
    let output = std::process::Command::new(sqlite)
        .args(["-readonly", db_uri.as_str(), query.as_str()])
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

fn sqlite_file_uri(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len() + 24);
    for ch in path.chars() {
        match ch {
            '%' => encoded.push_str("%25"),
            '?' => encoded.push_str("%3F"),
            '#' => encoded.push_str("%23"),
            ' ' => encoded.push_str("%20"),
            _ => encoded.push(ch),
        }
    }
    format!("file://{encoded}?mode=ro&nolock=1")
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
    } else if tty.starts_with("tty") {
        format!("/dev/{tty}")
    } else {
        format!("/dev/tty{}", tty)
    }
}

#[cfg(test)]
mod tests {
    use crate::terminal::process_tree;

    use super::{
        cwd_variants, ide_window_script, iterm_by_tty_or_cwd_script, native_bundle_for_context,
        normalized_app_name, outer_terminal_app, sqlite_file_uri, terminal_app_from_bundle_id,
        terminal_app_from_term_program, terminal_app_script, JumpContext,
    };
    #[cfg(target_os = "windows")]
    use super::{windows_powershell_string_literal, windows_terminal_command_candidates};
    use std::collections::HashMap;

    #[test]
    fn normalizes_session_terminal_labels_for_app_activation() {
        assert_eq!(normalized_app_name("iTerm·tmux"), "iTerm2");
        assert_eq!(normalized_app_name("Apple Terminal"), "Terminal");
        assert_eq!(normalized_app_name("WezTerm CLI"), "WezTerm");
        assert_eq!(normalized_app_name("Ghostty"), "Ghostty");
        assert_eq!(normalized_app_name("custom-term"), "custom-term");
    }

    #[test]
    fn resolves_terminal_app_from_bundle_and_term_program_metadata() {
        assert_eq!(
            terminal_app_from_bundle_id("com.googlecode.iterm2"),
            Some("iTerm2")
        );
        assert_eq!(
            terminal_app_from_bundle_id("dev.commandline.waveterm"),
            Some("Wave")
        );
        assert_eq!(terminal_app_from_term_program("iTerm.app"), Some("iTerm2"));
        assert_eq!(terminal_app_from_term_program("Wave"), Some("Wave"));
        assert_eq!(terminal_app_from_bundle_id("com.anthropic.claude"), None);
    }

    #[test]
    fn outer_terminal_uses_terminal_metadata_before_non_terminal_labels() {
        let ctx = JumpContext {
            pid: 0,
            terminal_app: Some("Claude".to_string()),
            term_bundle_id: Some("com.googlecode.iterm2".to_string()),
            ..Default::default()
        };

        assert_eq!(
            outer_terminal_app(&ctx, &HashMap::new()).as_deref(),
            Some("iTerm2")
        );
    }

    #[test]
    fn outer_terminal_uses_host_metadata_before_native_agent_fallbacks() {
        let ctx = JumpContext {
            pid: 0,
            agent_type: Some("opencode".to_string()),
            term_program: Some("iTerm.app".to_string()),
            ..Default::default()
        };

        assert_eq!(
            outer_terminal_app(&ctx, &HashMap::new()).as_deref(),
            Some("iTerm2")
        );
    }

    #[test]
    fn outer_terminal_ignores_non_terminal_labels_without_host_metadata() {
        let ctx = JumpContext {
            pid: 0,
            terminal_app: Some("AntCC".to_string()),
            ..Default::default()
        };

        assert_eq!(outer_terminal_app(&ctx, &HashMap::new()), None);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_terminal_jump_uses_native_launchers() {
        let specs = windows_terminal_command_candidates("cmd.exe", Some(r"C:\work\agentbro"));
        assert_eq!(specs[0].args, vec!["/K"]);
        assert_eq!(specs[0].current_dir.as_deref(), Some(r"C:\work\agentbro"));
        assert!(specs[0].program.to_ascii_lowercase().ends_with("cmd.exe"));

        let specs = windows_terminal_command_candidates("PowerShell", Some(r"C:\work\Bob's app"));
        assert!(specs[0]
            .args
            .iter()
            .any(|arg| arg.contains("'C:\\work\\Bob''s app'")));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_powershell_literals_escape_single_quotes() {
        assert_eq!(windows_powershell_string_literal("A'B"), "'A''B'");
    }

    #[test]
    fn outer_terminal_prefers_host_metadata_over_process_tree_guess() {
        let mut tree = HashMap::new();
        tree.insert(
            42,
            process_tree::ProcessInfo {
                pid: 42,
                ppid: 1,
                command: "Cursor".to_string(),
                tty: None,
            },
        );
        let ctx = JumpContext {
            pid: 42,
            term_program: Some("iTerm.app".to_string()),
            ..Default::default()
        };

        assert_eq!(outer_terminal_app(&ctx, &tree).as_deref(), Some("iTerm2"));
    }

    #[test]
    fn native_app_fallback_requires_matching_source_for_ide_bundles() {
        let claude_in_qoder = JumpContext {
            agent_type: Some("claude-code".to_string()),
            term_bundle_id: Some("com.qoder.ide".to_string()),
            ..Default::default()
        };
        assert_eq!(native_bundle_for_context(&claude_in_qoder), None);

        let qoder_app = JumpContext {
            agent_type: Some("qoder".to_string()),
            term_bundle_id: Some("com.qoder.ide".to_string()),
            ..Default::default()
        };
        assert_eq!(native_bundle_for_context(&qoder_app), Some("com.qoder.ide"));
    }

    #[test]
    fn terminal_app_script_matches_default_tab_name_before_custom_title() {
        let script = terminal_app_script(Some("ttys001"), Some("/Users/me/code/agentbro"));

        assert!(script.contains("set targetTty to \"/dev/ttys001\""));
        assert!(script.contains("set targetDir to \"agentbro\""));
        assert!(script.contains("name of aTab as text"));
        assert!(script.contains("custom title of aTab"));
        assert!(script.contains("AXMinimized"));
    }

    #[test]
    fn iterm_fallback_script_selects_tab_session_and_cwd_folder() {
        let script = iterm_by_tty_or_cwd_script(None, Some("/Users/me/code/agentbro"));

        assert!(script.contains("path of aSession contains \"agentbro\""));
        assert!(script.contains("name of aSession contains \"agentbro\""));
        assert!(script.contains("select aTab"));
        assert!(script.contains("select aSession"));
    }

    #[test]
    fn ide_window_script_targets_bundle_and_project_window() {
        let script = ide_window_script("com.todesktop.230313mzl4w4u92", "agentbro");

        assert!(script.contains(
            "application processes whose bundle identifier is \"com.todesktop.230313mzl4w4u92\""
        ));
        assert!(script.contains("wName contains \"agentbro\""));
        assert!(script.contains("AXRaise"));
    }

    #[test]
    fn warp_sqlite_uri_uses_read_only_nolock() {
        assert_eq!(
            sqlite_file_uri("/Users/me/My Project/warp#state.sqlite"),
            "file:///Users/me/My%20Project/warp%23state.sqlite?mode=ro&nolock=1"
        );
    }

    #[test]
    fn warp_cwd_variants_cover_firmlinks_and_trailing_slashes() {
        let variants = cwd_variants("/tmp/agentbro/");

        assert!(variants.contains(&"/tmp/agentbro".to_string()));
        assert!(variants.contains(&"/tmp/agentbro/".to_string()));
        assert!(variants.contains(&"/private/tmp/agentbro".to_string()));
        assert!(variants.contains(&"/private/tmp/agentbro/".to_string()));
    }
}
