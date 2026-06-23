// TerminalAppRegistry — Known terminal application names and bundle identifiers

/// Known terminal app names for process matching
const TERMINAL_APP_NAMES: &[&str] = &[
    "Terminal",
    "iTerm2",
    "iTerm",
    "Ghostty",
    "Alacritty",
    "kitty",
    "Hyper",
    "Warp",
    "Wave",
    "Wave Terminal",
    "waveterm",
    "WezTerm",
    "Tabby",
    "Windows Terminal",
    "WindowsTerminal",
    "WindowsTerminal.exe",
    "OpenConsole",
    "OpenConsole.exe",
    "conhost",
    "conhost.exe",
    "wt",
    "wt.exe",
    "Command Prompt",
    "cmd",
    "cmd.exe",
    "PowerShell",
    "powershell",
    "powershell.exe",
    "pwsh",
    "pwsh.exe",
    "Rio",
    "Contour",
    "foot",
    "st",
    "urxvt",
    "xterm",
    "Code", // VS Code
    "Code - Insiders",
    "Cursor",
    "Windsurf",
    "zed",
    "cmux",
    "Kaku",
    "kaku",
    "Zellij",
    "zellij",
];

/// Bundle identifiers for terminal apps (macOS)
const TERMINAL_BUNDLE_IDS: &[&str] = &[
    "com.apple.Terminal",
    "com.googlecode.iterm2",
    "com.mitchellh.ghostty",
    "io.alacritty",
    "org.alacritty",
    "net.kovidgoyal.kitty",
    "co.zeit.hyper",
    "dev.warp.Warp-Stable",
    "dev.commandline.waveterm",
    "com.github.wez.wezterm",
    "com.microsoft.VSCode",
    "com.microsoft.VSCodeInsiders",
    "com.todesktop.230313mzl4w4u92", // Cursor
    "com.exafunction.windsurf",
    "dev.zed.Zed",
    "com.kapeli.kaku",
    "com.cmuxterm.app",
    "fun.tw93.kaku",
];

/// Check if a command/app name belongs to a known terminal
pub fn is_terminal(app_name_or_command: &str) -> bool {
    let lower = app_name_or_command.to_ascii_lowercase();

    for name in TERMINAL_APP_NAMES {
        if contains_app_name(&lower, &name.to_ascii_lowercase()) {
            return true;
        }
    }

    false
}

/// Check if a bundle identifier is a known terminal
pub fn is_terminal_bundle(bundle_id: &str) -> bool {
    TERMINAL_BUNDLE_IDS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(bundle_id))
}

/// Get the app name to use in AppleScript for a given terminal command
pub fn applescript_app_name(command: &str) -> Option<&'static str> {
    let lower = command.to_lowercase();

    if lower.contains("wave") {
        None
    } else if lower.contains("iterm") {
        Some("iTerm2")
    } else if lower.contains("terminal") && !lower.contains("ghostty") {
        Some("Terminal")
    } else {
        None
    }
}

fn contains_app_name(haystack: &str, needle: &str) -> bool {
    if haystack == needle {
        return true;
    }

    let mut search_start = 0;
    while let Some(relative_index) = haystack[search_start..].find(needle) {
        let start = search_start + relative_index;
        let end = start + needle.len();
        if is_name_boundary(haystack[..start].chars().next_back())
            && is_name_boundary(haystack[end..].chars().next())
        {
            return true;
        }
        search_start = end;
    }

    false
}

fn is_name_boundary(ch: Option<char>) -> bool {
    ch.is_none_or(|c| !c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::is_terminal;

    #[test]
    fn detects_terminal_app_names_with_boundaries() {
        assert!(is_terminal("/Applications/iTerm.app/Contents/MacOS/iTerm2"));
        assert!(is_terminal(
            "/Applications/Visual Studio Code.app/Contents/MacOS/Electron"
        ));
        assert!(is_terminal("/Applications/Wave.app/Contents/MacOS/Wave"));
        assert!(is_terminal("iTerm·tmux"));
        assert!(is_terminal("Apple_Terminal"));
    }

    #[test]
    fn detects_windows_terminal_app_names() {
        assert!(is_terminal("Windows Terminal"));
        assert!(is_terminal("WindowsTerminal.exe"));
        assert!(is_terminal("OpenConsole.exe"));
        assert!(is_terminal("conhost.exe"));
        assert!(is_terminal("wt.exe"));
        assert!(is_terminal("cmd.exe"));
        assert!(is_terminal("PowerShell"));
        assert!(is_terminal("pwsh.exe"));
    }

    #[test]
    fn does_not_treat_agent_binary_paths_as_terminal_apps() {
        assert!(!is_terminal(
            "/Users/me/.codefuse/fuse/engine/bin/claude/2.1.139/claude"
        ));
        assert!(!is_terminal("claude"));
        assert!(!is_terminal("AntCC"));
    }
}
