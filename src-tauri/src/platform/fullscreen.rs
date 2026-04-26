// Fullscreen detection via AppleScript (macOS only)

/// Check if the frontmost application's window is in fullscreen mode.
/// Returns false on non-macOS platforms or if the check fails.
pub fn check_fullscreen() -> bool {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("osascript")
            .args([
                "-e",
                r#"try
    tell application "System Events" to get value of attribute "AXFullScreen" of window 1 of (first application process whose frontmost is true)
on error
    false
end try"#,
            ])
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                stdout.trim() == "true"
            }
            Err(_) => false,
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}
