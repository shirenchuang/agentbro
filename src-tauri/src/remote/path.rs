use std::path::PathBuf;

pub fn expand_tilde(path: &str) -> String {
    if path == "~" {
        return dirs::home_dir()
            .unwrap_or_else(|| std::env::temp_dir())
            .display()
            .to_string();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return dirs::home_dir()
            .unwrap_or_else(|| std::env::temp_dir())
            .join(rest)
            .display()
            .to_string();
    }
    PathBuf::from(path).display().to_string()
}

#[cfg(test)]
mod tests {
    use super::expand_tilde;

    #[test]
    fn leaves_absolute_paths_unchanged() {
        assert_eq!(expand_tilde("/tmp/key"), "/tmp/key");
    }

    #[test]
    fn expands_home_relative_paths() {
        let expanded = expand_tilde("~/.ssh/id_ed25519");
        assert!(expanded.ends_with("/.ssh/id_ed25519"));
        assert!(!expanded.starts_with('~'));
    }
}
