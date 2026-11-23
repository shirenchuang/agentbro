use super::{AdapterStatus, AgentAdapter, AgentEvent};
use serde_json::Value;
use std::path::{Path, PathBuf};

const ID: &str = "deepseek-harness";
const PLUGIN: &str = "agentbro-bridge-deepseek-harness";

pub struct DeepSeekHarnessAdapter;

impl DeepSeekHarnessAdapter {
    pub fn new() -> Self {
        Self
    }

    pub(super) fn profile() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        crate::skills::agent_paths::deepseek_harness_home_for(&home).join("profiles/desktop")
    }

    pub(super) fn plugin_enabled(manifest_path: &Path) -> bool {
        let manifest: Option<Value> = std::fs::read(manifest_path)
            .ok()
            .and_then(|data| serde_json::from_slice(&data).ok());
        manifest.is_some_and(|manifest| {
            manifest
                .pointer("/dsh/profile/bundles")
                .and_then(Value::as_array)
                .is_some_and(|bundles| bundles.iter().any(|bundle| bundle == PLUGIN))
                && manifest_path.parent().is_some_and(|profile| {
                    profile
                        .join("node_modules")
                        .join(PLUGIN)
                        .join("index.js")
                        .is_file()
                })
        })
    }

    fn command() -> Option<PathBuf> {
        if !cfg!(any(target_os = "macos", target_os = "windows")) {
            return None;
        }
        let app = super::programs::installed_app_path_for_agent(ID)?;
        let app = Path::new(&app);
        let command = if cfg!(target_os = "windows") {
            app.parent()?.join("resources/runtime/cli/bin/dsh.cmd")
        } else {
            app.join("Contents/Resources/runtime/cli/bin/dsh")
        };
        command.is_file().then_some(command)
    }

    fn plugin_command(args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
        let command = Self::command().ok_or("DeepSeek Harness desktop CLI was not found")?;
        let profile = Self::profile();
        if !profile.join("package.json").is_file() {
            return Err("Open DeepSeek Harness once to initialize its desktop profile".into());
        }
        let home = profile
            .ancestors()
            .nth(2)
            .ok_or("Invalid desktop profile path")?;
        let output = crate::platform::process::background_command(command)
            .env("DSH_HOME", home)
            .args(["plugin", "--profile", "desktop"])
            .args(args)
            .output()?;
        if !output.status.success() {
            return Err(format!(
                "Quit DeepSeek Harness before changing its AgentBro integration. Desktop CLI: {}",
                String::from_utf8_lossy(&output.stderr).trim(),
            )
            .into());
        }
        Ok(())
    }
}

impl AgentAdapter for DeepSeekHarnessAdapter {
    fn name(&self) -> &str {
        ID
    }

    fn display_name(&self) -> &str {
        "DeepSeek Harness"
    }

    fn icon(&self) -> &str {
        "deepseek"
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        let home = dirs::home_dir().ok_or("Home directory was not found")?;
        let package = home.join(".agentbro").join(PLUGIN);
        std::fs::create_dir_all(&package)?;
        std::fs::write(
            package.join("package.json"),
            serde_json::to_vec(&serde_json::json!({
                "name": PLUGIN,
                "version": "1.0.0",
                "type": "module",
                "exports": "./index.js",
                "agentbroHookPort": crate::hook_endpoint::current().tcp_port,
                "dsh": { "bundle": { "patch": "./bundle.yml" } }
            }))?,
        )?;
        std::fs::write(
            package.join("bundle.yml"),
            format!("- insert:\n    - id: {PLUGIN}\n      name: {PLUGIN}\n"),
        )?;
        std::fs::write(
            package.join("index.js"),
            include_bytes!("deepseek_harness_bridge.js"),
        )?;
        Self::plugin_command(&[
            "add",
            &format!("file:{}", package.display()),
            "--offline",
            "--ignore-scripts",
        ])
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.hooks_installed() {
            Self::plugin_command(&["remove", PLUGIN])?;
        }
        Ok(())
    }

    fn status(&self) -> AdapterStatus {
        if Self::command().is_some() {
            AdapterStatus::Available
        } else {
            AdapterStatus::Unavailable
        }
    }

    fn hook_config_paths(&self) -> Vec<PathBuf> {
        vec![Self::profile().join("package.json")]
    }

    fn hooks_installed(&self) -> bool {
        Self::plugin_enabled(&Self::profile().join("package.json"))
    }

    fn parse_event(&self, raw: &Value) -> Result<AgentEvent, Box<dyn std::error::Error>> {
        if raw.get("agent").and_then(Value::as_str) != Some(ID) {
            return Err("Not a DeepSeek Harness event".into());
        }
        let session_id = raw
            .get("session_id")
            .and_then(Value::as_str)
            .filter(|id| id.starts_with("deepseek-harness:") && id.len() <= 256)
            .ok_or("Missing DeepSeek Harness session ID")?
            .to_string();
        let event = match raw.get("event").and_then(Value::as_str) {
            Some("processing") => AgentEvent::Processing {
                session_id,
                description: "DeepSeek Harness is working".to_string(),
            },
            Some("turn_end") => match raw.get("outcome").and_then(Value::as_str) {
                Some("error" | "blocked") => AgentEvent::Error {
                    session_id,
                    message: "DeepSeek Harness turn failed".to_string(),
                },
                Some("aborted" | "interrupted") => AgentEvent::Interrupt { session_id },
                _ => AgentEvent::TaskComplete {
                    session_id,
                    summary: raw
                        .get("summary")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|summary| !summary.is_empty())
                        .unwrap_or("DeepSeek Harness turn ended")
                        .to_string(),
                },
            },
            Some("permission_request") => AgentEvent::PermissionRequest {
                session_id,
                tool_name: raw
                    .get("tool_name")
                    .and_then(Value::as_str)
                    .filter(|tool| !tool.is_empty() && tool.len() <= 128)
                    .ok_or("Missing DeepSeek Harness permission tool")?
                    .to_string(),
                diff: None,
                options: None,
            },
            _ => return Err("Unknown DeepSeek Harness event".into()),
        };
        Ok(event)
    }
}
