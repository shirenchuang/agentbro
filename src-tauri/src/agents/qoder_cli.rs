// QoderCliAdapter — Agent adapter for Qoder CLI (separate from IDE)

use super::profiles;
use super::qoder::QoderAdapter;
use super::{AdapterStatus, AgentAdapter, AgentEvent};
use std::path::PathBuf;

pub struct QoderCliAdapter {
    status: AdapterStatus,
}

impl QoderCliAdapter {
    pub fn new() -> Self {
        Self {
            status: Self::detect_status(),
        }
    }

    fn detect_status() -> AdapterStatus {
        super::programs::detected_status_for_agent_program("qoder-cli")
    }

    fn settings_path(&self) -> PathBuf {
        profiles::configuration_url(&profiles::qoder_cli_profile())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::{agent_paths, installer::TempInstallDir, v2::service::Service};

    #[test]
    fn qoder_cli_uses_one_configured_root_for_hooks_and_skills() {
        const PROBE: &str = "AGENTBRO_QODER_ROOT_PROBE";
        #[cfg(target_os = "macos")]
        const TIMEOUT_PROBE: &str = "AGENTBRO_QODER_ROOT_TIMEOUT_PROBE";
        const TEST: &str =
            "agents::qoder_cli::tests::qoder_cli_uses_one_configured_root_for_hooks_and_skills";
        #[cfg(target_os = "macos")]
        if std::env::var_os(TIMEOUT_PROBE).is_some() {
            assert_eq!(
                agent_paths::qoder_cli_home(),
                dirs::home_dir().unwrap().join(".qoder")
            );
            return;
        }
        if let Some(root) = std::env::var_os(PROBE).map(PathBuf::from) {
            let home = dirs::home_dir().unwrap();
            let settings = root.join("settings.json");
            let skill_root = root.join("skills");
            let adapter = QoderCliAdapter::new();
            assert_eq!(adapter.hook_config_paths(), vec![settings.clone()]);
            assert_eq!(
                profiles::configuration_url(&profiles::qoder_cli_profile()),
                settings
            );
            assert_eq!(
                agent_paths::paths_for_agent("qoder-cli").skill_dirs,
                vec![skill_root.clone()]
            );
            assert_eq!(
                agent_paths::paths_for_agent("qoder-desktop").skill_dirs,
                vec![home.join(".qoder/skills")]
            );
            assert_eq!(
                profiles::configuration_url(&profiles::qoder_profile()),
                home.join(".qoder/settings.json")
            );

            std::fs::write(&settings, r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"agentbro-bridge --source qoder-cli"},{"type":"command","command":"echo user-hook"}]}]}}"#).unwrap();
            let skill = skill_root.join("location-probe");
            std::fs::create_dir_all(&skill).unwrap();
            std::fs::write(
                skill.join("SKILL.md"),
                "---\nname: location-probe\ndescription: A configured Qoder Skill\n---\nFixture.\n",
            )
            .unwrap();
            let database = root.join("inventory.sqlite");
            std::fs::File::create(&database).unwrap();
            let service = Service::new(&database, home).unwrap();
            let scan = service.scan_one_agent_into_db("qoder-cli").unwrap();
            assert_eq!(scan.unmanaged, 1);
            let detail = service.get_agent_detail("qoder-cli").unwrap();
            assert_eq!(detail.config_path.as_deref(), settings.to_str());
            assert_eq!(detail.skills_dir.as_deref(), skill_root.to_str());
            assert!(adapter.hooks_installed());
            adapter.remove_hooks().unwrap();
            assert!(!adapter.hooks_installed());
            assert!(std::fs::read_to_string(&settings)
                .unwrap()
                .contains("echo user-hook"));
            return;
        }

        let fixture = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let root = fixture.path().join("Qoder 配置");
        std::fs::create_dir(&root).unwrap();
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", TEST, "--nocapture"])
            .env(PROBE, &root)
            .env("QODER_CONFIG_DIR", &root);
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        #[cfg(target_os = "macos")]
        {
            std::fs::write(
                fixture.path().join(".zshrc"),
                r#"export QODER_CONFIG_DIR="$AGENTBRO_QODER_ROOT_PROBE""#,
            )
            .unwrap();
            let output = command
                .env_remove("QODER_CONFIG_DIR")
                .env("SHELL", "/bin/zsh")
                .env("ZDOTDIR", fixture.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );

            std::fs::write(
                fixture.path().join(".zshrc"),
                "/bin/sleep 7\nexport QODER_CONFIG_DIR=\"$AGENTBRO_QODER_ROOT_PROBE\"\n",
            )
            .unwrap();
            let output = command.env(TIMEOUT_PROBE, "1").output().unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
