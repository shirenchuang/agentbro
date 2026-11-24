// QoderCliAdapter — Agent adapter for Qoder CLI (separate from IDE)

use super::profiles;
use super::qoder::QoderAdapter;
use super::{AdapterStatus, AgentAdapter, AgentEvent};
use std::path::PathBuf;

pub struct QoderCliAdapter {
    config_root: PathBuf,
    status: AdapterStatus,
}

impl QoderCliAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
        let config_root = home.join(".qoder");
        let status = Self::detect_status();
        Self {
            config_root,
            status,
        }
    }

    fn detect_status() -> AdapterStatus {
        super::programs::detected_status_for_agent_program("qoder-cli")
    }

    fn settings_path(&self) -> PathBuf {
        self.config_root.join("settings.json")
    }
}

impl AgentAdapter for QoderCliAdapter {
    fn name(&self) -> &str {
        "qoder-cli"
    }
    fn display_name(&self) -> &str {
        "Qoder CLI"
    }
    fn icon(&self) -> &str {
        "qoder"
    }

    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::install_at(&profiles::qoder_cli_profile(), &self.settings_path())?;
        log::info!("Qoder CLI hooks installed");
        Ok(())
    }

    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>> {
        profiles::uninstall_at(&profiles::qoder_cli_profile(), &self.settings_path())?;
        log::info!("Qoder CLI hooks removed");
        Ok(())
    }

    fn status(&self) -> AdapterStatus {
        self.status.clone()
    }

    fn detect_status_now(&self) -> AdapterStatus {
        Self::detect_status()
    }

    fn parse_event(
        &self,
        raw: &serde_json::Value,
    ) -> Result<AgentEvent, Box<dyn std::error::Error>> {
        Ok(QoderAdapter::parse_hook_event(self.name(), raw))
    }

    fn hook_config_paths(&self) -> Vec<PathBuf> {
        vec![self.settings_path()]
    }

    fn hooks_installed(&self) -> bool {
        profiles::is_installed_at(&profiles::qoder_cli_profile(), &self.settings_path())
    }
}
