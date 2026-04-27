pub mod scanner;

/// Directory where user themes are stored
pub fn themes_dir() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("~"))
        .join(".agent-island")
        .join("themes")
}
