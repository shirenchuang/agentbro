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

#[cfg(not(target_os = "macos"))]
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
}
