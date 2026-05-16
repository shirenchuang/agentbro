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
    "WezTerm",
    "Tabby",
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
    let lower = app_name_or_command.to_lowercase();

    for name in TERMINAL_APP_NAMES {
        if lower.contains(&name.to_lowercase()) {
            return true;
        }
    }

    lower.contains("terminal") || lower.contains("iterm")
}

/// Check if a bundle identifier is a known terminal
pub fn is_terminal_bundle(bundle_id: &str) -> bool {
    TERMINAL_BUNDLE_IDS.contains(&bundle_id)
}

/// Get the app name to use in AppleScript for a given terminal command
pub fn applescript_app_name(command: &str) -> Option<&'static str> {
    let lower = command.to_lowercase();

    if lower.contains("iterm") {
        Some("iTerm2")
    } else if lower.contains("terminal") && !lower.contains("ghostty") {
        Some("Terminal")
    } else {
        None
    }
}
