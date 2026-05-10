pub mod agent_paths;
pub mod scanner;
pub mod registry;
pub mod installer;
pub mod sync;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillType {
    Skill,
    Mcp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallMode {
    Direct,
    Symlink,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillSource {
    Island,
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillState {
    pub agent: String,
    pub install_path: String,
    pub install_mode: InstallMode,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub skill_type: SkillType,
    pub icon: Option<String>,
    pub source: SkillSource,
    pub origin_url: Option<String>,
    pub has_update: bool,
    pub file_path: String,
    pub file_size: u64,
    pub modified_at: u64,
    pub agents: Vec<AgentSkillState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPack {
    pub id: String,
    pub name: String,
    pub description: String,
    pub skills: Vec<String>,
    pub target_agents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConfig {
    pub method: String,
    pub github_repo: Option<String>,
    pub github_token: Option<String>,
    pub last_sync_at: Option<String>,
    pub auto_sync: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetConfig {
    pub agent: String,
    pub install_mode: InstallMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeNode {
    pub name: String,
    pub node_type: String,
    pub path: String,
    pub children: Option<Vec<FileTreeNode>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub success: bool,
    pub message: String,
    pub conflicts: Vec<SyncConflict>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConflict {
    pub skill_id: String,
    pub local_modified: String,
    pub remote_modified: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPreview {
    pub to_copy: u32,
    pub to_skip: u32,
    pub to_update: u32,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictResolution {
    pub skill_id: String,
    pub action: String,
}
