use crate::agents::{executable, AdapterStatus, AgentAdapter};
use crate::commands::AppState;
use crate::skills::{agent_paths, registry};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Stdio;
use tauri::{AppHandle, Emitter, State};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::{timeout, Duration};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum AgentProgramKind {
    Cli,
    App,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum AgentProgramStatus {
    Installed,
    NotInstalled,
    UpdateAvailable,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProgramInfo {
    pub id: String,
    pub display_name: String,
    pub icon: String,
    pub kind: AgentProgramKind,
    pub status: AgentProgramStatus,
    pub package_manager: Option<String>,
    pub package_name: Option<String>,
    pub installed_version: Option<String>,
    pub latest_version: Option<String>,
    pub binary_path: Option<String>,
    pub config_dir: Option<String>,
    pub app_path: Option<String>,
    pub download_url: Option<String>,
    pub install_command: Option<String>,
    pub update_command: Option<String>,
    pub uninstall_command: Option<String>,
    pub hooks_installed: bool,
    pub skills_dir: Option<String>,
    pub is_custom: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentOutputEvent {
    pub agent_id: String,
    pub operation: String,
    pub stream: String,
    pub line: String,
    pub done: bool,
    pub success: Option<bool>,
}

#[derive(Debug, Clone)]
struct ProgramMetadata {
    kind: AgentProgramKind,
    binary: Option<&'static str>,
    package_manager: Option<&'static str>,
    package_name: Option<&'static str>,
    install_command: Option<&'static str>,
    update_command: Option<&'static str>,
    uninstall_command: Option<&'static str>,
    app_path: Option<&'static str>,
    config_dir: Option<&'static str>,
    download_url: Option<&'static str>,
}

#[derive(Clone)]
struct AgentProgramSeed {
    id: String,
    display_name: String,
    icon: String,
    adapter_installed: bool,
    hooks_installed: bool,
}

#[tauri::command]
pub async fn agent_list(state: State<'_, AppState>) -> Result<Vec<AgentProgramInfo>, String> {
    Ok(build_agent_list(&state.adapters, false).await)
}

#[tauri::command]
pub async fn agent_refresh(state: State<'_, AppState>) -> Result<Vec<AgentProgramInfo>, String> {
    Ok(build_agent_list(&state.adapters, true).await)
}

#[tauri::command]
pub async fn agent_install(
    app: AppHandle,
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<(), String> {
    run_agent_command(app, &state.adapters, &agent_id, "install").await
}

#[tauri::command]
pub async fn agent_update(
    app: AppHandle,
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<(), String> {
    run_agent_command(app, &state.adapters, &agent_id, "update").await
}

#[tauri::command]
pub async fn agent_uninstall(
    app: AppHandle,
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<(), String> {
    run_agent_command(app, &state.adapters, &agent_id, "uninstall").await
}

#[tauri::command]
pub async fn agent_open_download(
    _state: State<'_, AppState>,
    agent_id: String,
) -> Result<(), String> {
    let meta = metadata_for(&agent_id).ok_or_else(|| format!("Unknown agent: {agent_id}"))?;
    let url = meta
        .download_url
        .ok_or_else(|| format!("No download URL for {agent_id}"))?;
    open_target(url)
}

#[tauri::command]
pub async fn agent_open_app(state: State<'_, AppState>, agent_id: String) -> Result<(), String> {
    let _ = state;
    let meta = metadata_for(&agent_id).ok_or_else(|| format!("Unknown agent: {agent_id}"))?;
    let path = installed_app_path(&meta).ok_or_else(|| {
        let paths = app_path_candidates(&meta).join(", ");
        if paths.is_empty() {
            format!("No app path for {agent_id}")
        } else {
            format!("App is not installed at any known path: {paths}")
        }
    })?;
    open_target(&path)
}

#[tauri::command]
pub async fn add_custom_agent(
    config: registry::CustomAgentConfig,
) -> Result<AgentProgramInfo, String> {
    let entry = registry::add_custom_agent(config)?;
    Ok(info_for_custom_agent(entry))
}

#[tauri::command]
pub async fn update_custom_agent(
    agent_id: String,
    config: registry::UpdateCustomAgentConfig,
) -> Result<AgentProgramInfo, String> {
    let entry = registry::update_custom_agent(&agent_id, config)?;
    Ok(info_for_custom_agent(entry))
}

#[tauri::command]
pub async fn remove_custom_agent(agent_id: String) -> Result<(), String> {
    registry::remove_custom_agent(&agent_id)
}

async fn build_agent_list(
    adapters: &[std::sync::Arc<dyn AgentAdapter>],
    include_latest: bool,
) -> Vec<AgentProgramInfo> {
    let mut seeds = std::collections::BTreeMap::<String, AgentProgramSeed>::new();
    for id in agent_paths::known_agent_ids() {
        if metadata_for(id).is_some() {
            seeds.insert(
                (*id).to_string(),
                AgentProgramSeed {
                    id: (*id).to_string(),
                    display_name: display_name_for_agent(id).to_string(),
                    icon: icon_for_agent(id).to_string(),
                    adapter_installed: false,
                    hooks_installed: false,
                },
            );
        }
    }

    for adapter in adapters {
        let id = adapter.name().to_string();
        if id == "claude-code" && adapter.display_name() != display_name_for_agent("claude-code") {
            continue;
        }
        seeds.insert(
            id.clone(),
            AgentProgramSeed {
                id,
                display_name: adapter.display_name().to_string(),
                icon: adapter.icon().to_string(),
                adapter_installed: matches!(
                    adapter.status(),
                    AdapterStatus::Active | AdapterStatus::Installed | AdapterStatus::Available
                ),
                hooks_installed: adapter.hooks_installed(),
            },
        );
    }

    let mut handles = Vec::with_capacity(seeds.len());
    for seed in seeds.into_values() {
        handles.push(tokio::spawn(async move {
            info_for_agent_seed(seed, include_latest).await
        }));
    }

    let mut agents = Vec::with_capacity(handles.len());
    for handle in handles {
        if let Ok(agent) = handle.await {
            agents.push(agent);
        }
    }

    let mut built_in_ids: std::collections::HashSet<&str> =
        agent_paths::known_agent_ids().iter().copied().collect();
    built_in_ids.extend(adapters.iter().map(|adapter| adapter.name()));
    agents.extend(
        registry::list_custom_agents()
            .into_iter()
            .filter(|agent| agent.is_enabled && !built_in_ids.contains(agent.id.as_str()))
            .map(info_for_custom_agent),
    );
    agents
}

async fn info_for_agent_seed(seed: AgentProgramSeed, include_latest: bool) -> AgentProgramInfo {
    let id = seed.id;
    let meta = metadata_for(&id).unwrap_or_else(default_metadata);
    let binary_path = meta.binary.and_then(which);
    let app_path = installed_app_path(&meta);
    let installed = binary_path.is_some() || app_path.is_some() || seed.adapter_installed;
    let skills_dir = agent_paths::paths_for_agent(&id)
        .skill_dirs
        .first()
        .map(|path| path.display().to_string());
    let (installed_version, latest_version) = if installed {
        version_info_for(&meta, include_latest).await
    } else {
        (None, None)
    };
    let update_available =
        versions_indicate_update(installed_version.as_deref(), latest_version.as_deref());

    AgentProgramInfo {
        id,
        display_name: seed.display_name,
        icon: seed.icon,
        kind: meta.kind,
        status: if installed && update_available {
            AgentProgramStatus::UpdateAvailable
        } else if installed {
            AgentProgramStatus::Installed
        } else if meta.install_command.is_some() || meta.download_url.is_some() {
            AgentProgramStatus::NotInstalled
        } else {
            AgentProgramStatus::Unavailable
        },
        package_manager: meta.package_manager.map(ToString::to_string),
        package_name: meta.package_name.map(ToString::to_string),
        installed_version,
        latest_version,
        binary_path,
        config_dir: meta.config_dir.map(expand_home),
        app_path: app_path.or_else(|| meta.app_path.map(ToString::to_string)),
        download_url: meta.download_url.map(ToString::to_string),
        install_command: meta.install_command.map(ToString::to_string),
        update_command: meta.update_command.map(ToString::to_string),
        uninstall_command: meta.uninstall_command.map(ToString::to_string),
        hooks_installed: seed.hooks_installed,
        skills_dir,
        is_custom: false,
    }
}

async fn version_info_for(
    meta: &ProgramMetadata,
    include_latest: bool,
) -> (Option<String>, Option<String>) {
    if meta.package_manager != Some("npm") {
        return (None, None);
    }
    let package = match meta.package_name {
        Some(package) => package,
        None => return (None, None),
    };
    if !include_latest {
        return (npm_installed_version(package).await, None);
    }
    let (installed, latest) =
        tokio::join!(npm_installed_version(package), npm_latest_version(package),);
    (installed, latest)
}

async fn npm_installed_version(package: &str) -> Option<String> {
    let output = timeout(
        Duration::from_secs(4),
        Command::new(command_name("npm"))
            .args(["list", "-g", package, "--depth=0", "--json"])
            .output(),
    )
    .await
    .ok()?
    .ok()?;

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    json.get("dependencies")?
        .get(package)?
        .get("version")?
        .as_str()
        .map(ToString::to_string)
}

async fn npm_latest_version(package: &str) -> Option<String> {
    let output = timeout(
        Duration::from_secs(5),
        Command::new(command_name("npm"))
            .args(["view", package, "version", "--silent"])
            .output(),
    )
    .await
    .ok()?
    .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn versions_indicate_update(installed: Option<&str>, latest: Option<&str>) -> bool {
    match (
        installed.map(normalize_version),
        latest.map(normalize_version),
    ) {
        (Some(installed), Some(latest)) => version_is_newer(&latest, &installed),
        _ => false,
    }
}

fn normalize_version(version: &str) -> String {
    version
        .trim()
        .trim_start_matches('v')
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string()
}

fn version_is_newer(latest: &str, installed: &str) -> bool {
    let latest_parts = numeric_version_parts(latest);
    let installed_parts = numeric_version_parts(installed);
    if latest_parts.is_empty() || installed_parts.is_empty() {
        return latest != installed;
    }

    let max_len = latest_parts.len().max(installed_parts.len());
    for index in 0..max_len {
        let latest_part = *latest_parts.get(index).unwrap_or(&0);
        let installed_part = *installed_parts.get(index).unwrap_or(&0);
        if latest_part > installed_part {
            return true;
        }
        if latest_part < installed_part {
            return false;
        }
    }
    false
}

fn numeric_version_parts(version: &str) -> Vec<u64> {
    version
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect()
}

fn info_for_custom_agent(agent: registry::CustomAgentEntry) -> AgentProgramInfo {
    let skills_path = Path::new(&agent.global_skills_dir);
    let detected =
        skills_path.exists() || skills_path.parent().is_some_and(|parent| parent.exists());
    AgentProgramInfo {
        id: agent.id,
        display_name: agent.display_name,
        icon: agent.icon_name.unwrap_or_else(|| "custom".to_string()),
        kind: AgentProgramKind::Cli,
        status: if detected {
            AgentProgramStatus::Installed
        } else {
            AgentProgramStatus::NotInstalled
        },
        package_manager: Some("custom".to_string()),
        package_name: None,
        installed_version: None,
        latest_version: None,
        binary_path: None,
        config_dir: Some(agent.global_skills_dir.clone()),
        app_path: None,
        download_url: None,
        install_command: None,
        update_command: None,
        uninstall_command: None,
        hooks_installed: false,
        skills_dir: Some(agent.global_skills_dir),
        is_custom: true,
    }
}

async fn run_agent_command(
    app: AppHandle,
    _adapters: &[std::sync::Arc<dyn AgentAdapter>],
    agent_id: &str,
    operation: &str,
) -> Result<(), String> {
    let meta = metadata_for(agent_id).ok_or_else(|| format!("Unknown agent: {agent_id}"))?;
    let command = match operation {
        "install" => meta.install_command,
        "update" => meta.update_command,
        "uninstall" => meta.uninstall_command,
        _ => None,
    }
    .ok_or_else(|| format!("{operation} is not supported for {agent_id}"))?;

    emit_output(
        &app,
        agent_id,
        operation,
        "info",
        &format!("$ {command}"),
        false,
        None,
    );

    let mut child = command_shell(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let out_app = app.clone();
    let out_id = agent_id.to_string();
    let out_op = operation.to_string();
    let stdout_task = tokio::spawn(async move {
        if let Some(stdout) = stdout {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                emit_output(&out_app, &out_id, &out_op, "stdout", &line, false, None);
            }
        }
    });

    let err_app = app.clone();
    let err_id = agent_id.to_string();
    let err_op = operation.to_string();
    let stderr_task = tokio::spawn(async move {
        if let Some(stderr) = stderr {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                emit_output(&err_app, &err_id, &err_op, "stderr", &line, false, None);
            }
        }
    });

    let status = child.wait().await.map_err(|e| e.to_string())?;
    let _ = stdout_task.await;
    let _ = stderr_task.await;

    let success = status.success();
    emit_output(
        &app,
        agent_id,
        operation,
        "info",
        if success {
            "Operation completed"
        } else {
            "Operation failed"
        },
        true,
        Some(success),
    );

    if success {
        Ok(())
    } else {
        Err(format!("{operation} failed for {agent_id}"))
    }
}

fn emit_output(
    app: &AppHandle,
    agent_id: &str,
    operation: &str,
    stream: &str,
    line: &str,
    done: bool,
    success: Option<bool>,
) {
    let _ = app.emit(
        "agent-output",
        AgentOutputEvent {
            agent_id: agent_id.to_string(),
            operation: operation.to_string(),
            stream: stream.to_string(),
            line: line.to_string(),
            done,
            success,
        },
    );
}

fn command_shell(command: &str) -> Command {
    #[cfg(target_os = "windows")]
    {
        let mut shell = Command::new(command_name("cmd"));
        shell.args(["/C", command]);
        shell
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut shell = Command::new("sh");
        shell.args(["-lc", command]);
        shell
    }
}

fn open_target(target: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = std::process::Command::new("open");
        c.arg(target);
        c
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", "", target]);
        c
    };

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut command = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(target);
        c
    };

    command.spawn().map(|_| ()).map_err(|e| e.to_string())
}

fn which(binary: &str) -> Option<String> {
    executable::find_binary(binary).map(|path| path.display().to_string())
}

fn command_name(binary: &str) -> String {
    which(binary).unwrap_or_else(|| binary.to_string())
}

fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).display().to_string();
        }
    }
    path.to_string()
}

fn app_path_candidates(meta: &ProgramMetadata) -> Vec<&'static str> {
    let mut paths = Vec::new();
    if let Some(path) = meta.app_path {
        paths.push(path);
    }

    paths.sort_unstable();
    paths.dedup();
    if let Some(primary) = meta.app_path {
        if let Some(index) = paths.iter().position(|path| *path == primary) {
            paths.swap(0, index);
        }
    }
    paths
}

fn installed_app_path(meta: &ProgramMetadata) -> Option<String> {
    app_path_candidates(meta)
        .into_iter()
        .find(|path| Path::new(path).exists())
        .map(ToString::to_string)
}

fn default_metadata() -> ProgramMetadata {
    ProgramMetadata {
        kind: AgentProgramKind::Cli,
        binary: None,
        package_manager: None,
        package_name: None,
        install_command: None,
        update_command: None,
        uninstall_command: None,
        app_path: None,
        config_dir: None,
        download_url: None,
    }
}

fn display_name_for_agent(id: &str) -> &'static str {
    match id {
        "claude-code" => "Claude Code",
        "cline" => "Cline",
        "codex" => "OpenAI Codex",
        "gemini" | "gemini-cli" => "Gemini CLI",
        "cursor" => "Cursor",
        "cursor-cli" => "Cursor CLI",
        "opencode" => "OpenCode",
        "copilot" => "GitHub Copilot",
        "qoder" => "Qoder",
        "qoder-cli" => "Qoder CLI",
        "hermes" => "Hermes",
        "antigravity" => "Antigravity",
        "qwen" => "Qwen Code",
        "deepseek" => "DeepSeek",
        "kimi" => "Kimi",
        "droid" | "factory-droid" => "Factory Droid",
        "stepfun" => "StepFun",
        "codebuddy" => "CodeBuddy",
        "codebuddycn" | "codybuddycn" => "CodyBuddyCN",
        "workbuddy" => "WorkBuddy",
        "kiro" => "Kiro",
        "pi" => "Pi",
        "junie" => "Junie",
        "windsurf" => "Windsurf",
        "augment" => "Augment",
        "kilocode" => "KiloCode",
        "ob1" => "OB1",
        "amp" => "Amp",
        "aider" => "Aider",
        "openclaw" => "OpenClaw",
        "qclaw" => "QClaw",
        "easyclaw" => "EasyClaw",
        "easyclaw-v2" => "EasyClaw V2",
        "autoclaw" => "AutoClaw",
        _ => "Custom Agent",
    }
}

fn icon_for_agent(id: &str) -> String {
    match id {
        "codebuddycn" | "codybuddycn" => "codebuddy".to_string(),
        "gemini-cli" => "gemini".to_string(),
        "cursor-cli" => "cursor".to_string(),
        "qoder-cli" => "qoder".to_string(),
        "factory-droid" => "factory-droid".to_string(),
        "easyclaw-v2" => "easyclaw".to_string(),
        other => other.to_string(),
    }
}

fn metadata_for(id: &str) -> Option<ProgramMetadata> {
    let meta = match id {
        "claude-code" => cli(
            "claude",
            "npm",
            "@anthropic-ai/claude-code",
            "npm install -g @anthropic-ai/claude-code",
            "npm install -g @anthropic-ai/claude-code@latest",
            "npm uninstall -g @anthropic-ai/claude-code",
            "~/.claude",
            "https://docs.anthropic.com/en/docs/claude-code",
        ),
        "codex" => cli(
            "codex",
            "npm",
            "@openai/codex",
            "npm install -g @openai/codex",
            "npm install -g @openai/codex@latest",
            "npm uninstall -g @openai/codex",
            "~/.codex",
            "https://developers.openai.com/codex",
        ),
        "gemini" => cli(
            "gemini",
            "npm",
            "@google/gemini-cli",
            "npm install -g @google/gemini-cli",
            "npm install -g @google/gemini-cli@latest",
            "npm uninstall -g @google/gemini-cli",
            "~/.gemini",
            "https://github.com/google-gemini/gemini-cli",
        ),
        "opencode" => cli(
            "opencode",
            "npm",
            "opencode-ai",
            "npm install -g opencode-ai",
            "npm install -g opencode-ai@latest",
            "npm uninstall -g opencode-ai",
            "~/.config/opencode",
            "https://opencode.ai",
        ),
        "qoder-cli" => cli_no_uninstall(
            "qoder",
            "vendor",
            "qoder-cli",
            None,
            None,
            "~/.qoder",
            "https://qoder.com",
        ),
        "copilot" => cli_no_uninstall(
            "gh",
            "gh",
            "github/gh-copilot",
            Some("gh extension install github/gh-copilot"),
            Some("gh extension upgrade gh-copilot"),
            "~/.config/gh",
            "https://github.com/github/gh-copilot",
        ),
        "cursor-cli" => cli_no_uninstall(
            "cursor-agent",
            "vendor",
            "cursor-cli",
            None,
            None,
            "~/.cursor",
            "https://cursor.com/cli",
        ),
        "cursor" => app(
            "cursor",
            "/Applications/Cursor.app",
            "~/.cursor",
            "https://cursor.com",
        ),
        "cline" => app(
            "cline",
            "/Applications/Visual Studio Code.app",
            "~/Documents/Cline",
            "https://cline.bot",
        ),
        "qoder" => app(
            "qoder",
            "/Applications/Qoder.app",
            "~/.qoder",
            "https://qoder.com",
        ),
        "codebuddy" => app(
            "codebuddy",
            "/Applications/CodeBuddy.app",
            "~/.codebuddy",
            "https://codebuddy.ai",
        ),
        "codebuddycn" | "codybuddycn" => app(
            "codebuddy",
            "/Applications/CodyBuddyCN.app",
            "~/.codybuddycn",
            "https://www.codebuddy.ai",
        ),
        "qwen" => app(
            "qwen",
            "/Applications/Qwen Code.app",
            "~/.qwen",
            "https://qwenlm.github.io",
        ),
        "deepseek" => app(
            "deepseek",
            "/Applications/DeepSeek.app",
            "~/.deepseek",
            "https://www.deepseek.com",
        ),
        "kimi" => app(
            "kimi",
            "/Applications/Kimi.app",
            "~/.kimi",
            "https://www.kimi.com",
        ),
        "droid" => app(
            "droid",
            "/Applications/Factory.app",
            "~/.factory",
            "https://www.factory.ai",
        ),
        "stepfun" => app(
            "stepfun",
            "/Applications/StepFun.app",
            "~/.stepfun",
            "https://platform.stepfun.com",
        ),
        "antigravity" => app(
            "antigravity",
            "/Applications/Antigravity.app",
            "~/.antigravity",
            "https://antigravity.google",
        ),
        "workbuddy" => app(
            "workbuddy",
            "/Applications/WorkBuddy.app",
            "~/.workbuddy",
            "https://workbuddy.ai",
        ),
        "hermes" => app(
            "hermes",
            "/Applications/Hermes.app",
            "~/.hermes",
            "https://hermes.ai",
        ),
        "pi" => app("pi", "/Applications/Pi.app", "~/.pi", "https://pi.ai"),
        "kiro" => app(
            "kiro",
            "/Applications/Kiro.app",
            "~/.kiro",
            "https://kiro.dev",
        ),
        "factory-droid" => app(
            "droid",
            "/Applications/Factory.app",
            "~/.factory",
            "https://www.factory.ai",
        ),
        "junie" => cli_no_uninstall(
            "junie",
            "vendor",
            "junie",
            None,
            None,
            "~/.junie",
            "https://www.jetbrains.com/junie/",
        ),
        "windsurf" => app(
            "windsurf",
            "/Applications/Windsurf.app",
            "~/.codeium/windsurf",
            "https://windsurf.com",
        ),
        "augment" => app(
            "augment",
            "/Applications/Augment.app",
            "~/.augment",
            "https://www.augmentcode.com",
        ),
        "kilocode" => cli_no_uninstall(
            "kilocode",
            "vendor",
            "kilocode",
            None,
            None,
            "~/.kilocode",
            "https://kilocode.ai",
        ),
        "ob1" => cli_no_uninstall(
            "ob1",
            "vendor",
            "ob1",
            None,
            None,
            "~/.ob1",
            "https://ob1.ai",
        ),
        "amp" => cli(
            "amp",
            "npm",
            "@sourcegraph/amp",
            "npm install -g @sourcegraph/amp",
            "npm install -g @sourcegraph/amp@latest",
            "npm uninstall -g @sourcegraph/amp",
            "~/.amp",
            "https://ampcode.com",
        ),
        "aider" => cli(
            "aider",
            "pipx",
            "aider-chat",
            "pipx install aider-chat",
            "pipx upgrade aider-chat",
            "pipx uninstall aider-chat",
            "~/.aider",
            "https://aider.chat",
        ),
        "openclaw" => app(
            "openclaw",
            "/Applications/OpenClaw.app",
            "~/.openclaw",
            "https://github.com/openclaw-ai/openclaw",
        ),
        "qclaw" => app(
            "qclaw",
            "/Applications/QClaw.app",
            "~/.qclaw",
            "https://github.com/openclaw-ai/qclaw",
        ),
        "easyclaw" => app(
            "easyclaw",
            "/Applications/EasyClaw.app",
            "~/.easyclaw",
            "https://github.com/openclaw-ai/easyclaw",
        ),
        "easyclaw-v2" => app(
            "easyclaw",
            "/Applications/EasyClaw.app",
            "~/.easyclaw-20260322-01",
            "https://github.com/openclaw-ai/easyclaw",
        ),
        "autoclaw" => app(
            "autoclaw",
            "/Applications/AutoClaw.app",
            "~/.openclaw-autoclaw",
            "https://github.com/openclaw-ai/autoclaw",
        ),
        _ => return None,
    };
    Some(meta)
}

#[allow(clippy::too_many_arguments)]
fn cli(
    binary: &'static str,
    manager: &'static str,
    package: &'static str,
    install: &'static str,
    update: &'static str,
    uninstall: &'static str,
    config_dir: &'static str,
    url: &'static str,
) -> ProgramMetadata {
    ProgramMetadata {
        kind: AgentProgramKind::Cli,
        binary: Some(binary),
        package_manager: Some(manager),
        package_name: Some(package),
        install_command: Some(install),
        update_command: Some(update),
        uninstall_command: Some(uninstall),
        app_path: None,
        config_dir: Some(config_dir),
        download_url: Some(url),
    }
}

fn cli_no_uninstall(
    binary: &'static str,
    manager: &'static str,
    package: &'static str,
    install: Option<&'static str>,
    update: Option<&'static str>,
    config_dir: &'static str,
    url: &'static str,
) -> ProgramMetadata {
    ProgramMetadata {
        kind: AgentProgramKind::Cli,
        binary: Some(binary),
        package_manager: Some(manager),
        package_name: Some(package),
        install_command: install,
        update_command: update,
        uninstall_command: None,
        app_path: None,
        config_dir: Some(config_dir),
        download_url: Some(url),
    }
}

fn app(
    binary: &'static str,
    app_path: &'static str,
    config_dir: &'static str,
    url: &'static str,
) -> ProgramMetadata {
    ProgramMetadata {
        kind: AgentProgramKind::App,
        binary: Some(binary),
        package_manager: Some("app"),
        package_name: None,
        install_command: None,
        update_command: None,
        uninstall_command: None,
        app_path: Some(app_path),
        config_dir: Some(config_dir),
        download_url: Some(url),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npm_agent_updates_install_latest_package_directly() {
        let claude = metadata_for("claude-code").expect("claude metadata");
        assert_eq!(
            claude.update_command,
            Some("npm install -g @anthropic-ai/claude-code@latest"),
        );

        let codex = metadata_for("codex").expect("codex metadata");
        assert_eq!(
            codex.update_command,
            Some("npm install -g @openai/codex@latest"),
        );
    }
}
