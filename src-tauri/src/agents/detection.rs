// detection.rs — Auto-detection of installed AI coding CLI tools

use super::{executable, AdapterStatus};
use std::path::PathBuf;

/// Information about a detected tool installation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedTool {
    pub name: String,
    pub display_name: String,
    pub status: AdapterStatus,
    pub binary_path: Option<String>,
    pub config_dir: Option<String>,
}

/// Detect all installed AI coding tools by checking PATH and config directories
pub fn detect_installed_tools() -> Vec<DetectedTool> {
    vec![
        detect_tool("claude-code", "Claude Code", &["claude"], &[".claude"]),
        detect_tool("codex", "OpenAI Codex", &["codex"], &[".codex"]),
        detect_tool("gemini", "Google Gemini", &["gemini"], &[".gemini"]),
        detect_cursor(),
        detect_copilot(),
        detect_tool("qoder", "Qoder", &["qoder"], &[".qoder"]),
        detect_tool("codebuddy", "CodeBuddy", &["codebuddy"], &[".codebuddy"]),
        detect_tool("qwen", "Qwen Coder", &["qwen-coder", "qwen"], &[".qwen"]),
        detect_tool("kimi", "Kimi Code", &["kimi"], &[".kimi-code", ".kimi"]),
        detect_zcode(),
        detect_tool("deepseek", "DeepSeek", &["deepseek"], &[".deepseek"]),
        detect_tool(
            "opencode",
            "OpenCode",
            &["opencode"],
            &[".config/opencode", ".opencode"],
        ),
    ]
}

/// Generic detection: checks each binary name in PATH, then checks config dirs
fn detect_tool(
    name: &str,
    display_name: &str,
    binaries: &[&str],
    config_dirs: &[&str],
) -> DetectedTool {
    let binary_path = find_binary(binaries);
    let config_dir = find_config_dir(config_dirs);

    let status = if binary_path.is_some() {
        AdapterStatus::Available
    } else if config_dir.is_some() {
        // Config exists but no CLI found — likely an IDE extension
        AdapterStatus::Installed
    } else {
        AdapterStatus::Unavailable
    };

    DetectedTool {
        name: name.to_string(),
        display_name: display_name.to_string(),
        status,
        binary_path: binary_path.map(|p| p.display().to_string()),
        config_dir: config_dir.map(|p| p.display().to_string()),
    }
}

/// Cursor-specific detection (CLI or desktop app)
fn detect_cursor() -> DetectedTool {
    let binary_path = find_binary(&["cursor", "cursor-agent"]);
    let config_dir = find_config_dir(&[".cursor"]);
    let program_status = super::programs::detected_status_for_agent_program("cursor");

    let status = if binary_path.is_some() || matches!(program_status, AdapterStatus::Available) {
        AdapterStatus::Available
    } else if config_dir.is_some() || matches!(program_status, AdapterStatus::Installed) {
        AdapterStatus::Installed
    } else {
        AdapterStatus::Unavailable
    };

    DetectedTool {
        name: "cursor".to_string(),
        display_name: "Cursor".to_string(),
        status,
        binary_path: binary_path.map(|p| p.display().to_string()),
        config_dir: config_dir.map(|p| p.display().to_string()),
    }
}

fn detect_zcode() -> DetectedTool {
    let binary_path = find_binary(&["zcode"]);
    let config_dir = find_config_dir(&[".zcode"]);
    let program_status = super::programs::detected_status_for_agent_program("zcode");

    let status = if binary_path.is_some() || matches!(program_status, AdapterStatus::Available) {
        AdapterStatus::Available
    } else if config_dir.is_some() || matches!(program_status, AdapterStatus::Installed) {
        AdapterStatus::Installed
    } else {
        AdapterStatus::Unavailable
    };

    DetectedTool {
        name: "zcode".to_string(),
        display_name: "ZCode".to_string(),
        status,
        binary_path: binary_path.map(|path| path.display().to_string()),
        config_dir: config_dir.map(|path| path.display().to_string()),
    }
}

/// GitHub Copilot detection (requires `gh` CLI with copilot extension)
fn detect_copilot() -> DetectedTool {
    let gh_path = find_binary(&["gh"]);
    let config_dir = find_config_dir(&[".config/github-copilot"]);

    let copilot_available = gh_path.as_ref().is_some_and(|path| {
        crate::platform::process::background_command(path)
            .args(["copilot", "--version"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    });

    let status = if copilot_available {
        AdapterStatus::Available
    } else if config_dir.is_some() {
        AdapterStatus::Installed
    } else {
        AdapterStatus::Unavailable
    };

    DetectedTool {
        name: "copilot".to_string(),
        display_name: "GitHub Copilot".to_string(),
        status,
        binary_path: gh_path.map(|p| p.display().to_string()),
        config_dir: config_dir.map(|p| p.display().to_string()),
    }
}

/// Search PATH for any of the given binary names; return path of first found
fn find_binary(names: &[&str]) -> Option<PathBuf> {
    for name in names {
        if let Some(path) = executable::find_binary(name) {
            return Some(path);
        }
    }
    None
}

/// Check if any of the given config directory names exist under $HOME
fn find_config_dir(relative_dirs: &[&str]) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    for rel in relative_dirs {
        let dir = home.join(rel);
        if dir.exists() {
            return Some(dir);
        }
    }
    None
}
