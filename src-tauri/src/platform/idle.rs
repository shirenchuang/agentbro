pub fn user_idle_seconds() -> Option<u64> {
    user_idle_nanoseconds().map(|value| value / 1_000_000_000)
}

#[cfg(target_os = "macos")]
fn user_idle_nanoseconds() -> Option<u64> {
    let output = std::process::Command::new("/usr/sbin/ioreg")
        .args(["-c", "IOHIDSystem"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_hid_idle_time_ns(&text)
}

#[cfg(target_os = "windows")]
fn user_idle_nanoseconds() -> Option<u64> {
    user_idle_millis_windows().map(|value| value.saturating_mul(1_000_000))
}

#[cfg(target_os = "windows")]
fn user_idle_millis_windows() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    let ok = unsafe { GetLastInputInfo(&mut info) };
    if ok == 0 {
        return None;
    }
    let now = unsafe { GetTickCount() };
    Some(now.wrapping_sub(info.dwTime) as u64)
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn user_idle_nanoseconds() -> Option<u64> {
    None
}

#[cfg(any(target_os = "macos", test))]
fn parse_hid_idle_time_ns(text: &str) -> Option<u64> {
    text.lines()
        .find_map(|line| line.split_once("HIDIdleTime"))
        .and_then(|(_, rest)| rest.split_once('='))
        .and_then(|(_, value)| parse_u64_value(value.trim()))
}

#[cfg(any(target_os = "macos", test))]
fn parse_u64_value(value: &str) -> Option<u64> {
    let trimmed = value.trim_matches(|c: char| c == '"' || c == ';' || c.is_whitespace());
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).ok()
    } else {
        trimmed.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::parse_hid_idle_time_ns;

    #[test]
    fn parses_decimal_hid_idle_time() {
        let text = r#"    | |   "HIDIdleTime" = 9876543210"#;
        assert_eq!(parse_hid_idle_time_ns(text), Some(9_876_543_210));
    }

    #[test]
    fn parses_hex_hid_idle_time() {
        let text = r#"    | |   "HIDIdleTime" = 0x3b9aca00"#;
        assert_eq!(parse_hid_idle_time_ns(text), Some(1_000_000_000));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_idle_time_is_available() {
        assert!(super::user_idle_seconds().is_some());
    }
}
