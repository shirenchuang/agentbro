// AgentAdapter trait — implemented by each supported AI coding tool

use std::path::PathBuf;
use super::{AdapterStatus, AgentEvent};

pub trait AgentAdapter: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn display_name(&self) -> &str;
    fn icon(&self) -> &str;
    fn install_hooks(&self) -> Result<(), Box<dyn std::error::Error>>;
    fn remove_hooks(&self) -> Result<(), Box<dyn std::error::Error>>;
    fn status(&self) -> AdapterStatus;
    fn parse_event(&self, raw: &serde_json::Value) -> Result<AgentEvent, Box<dyn std::error::Error>>;
    fn hook_config_paths(&self) -> Vec<PathBuf>;
}
