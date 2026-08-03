use crate::agents::{executable, AdapterStatus, AgentAdapter};
use crate::commands::AppState;
use crate::skills::{agent_paths, registry};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
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
    hooks_installed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimePlatform {
    Macos,
    Windows,
    Linux,
}

const APP_TRASH_UNINSTALL_COMMAND: &str = "Move application to Trash";

impl RuntimePlatform {
    fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }
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
    if let Some(path) = installed_app_path(&agent_id, &meta) {
        return open_target(&path);
    }
    if let Some(binary) = find_agent_binary(&agent_id, &meta) {
        return launch_binary(&binary);
    }
    let paths = app_path_candidates(&agent_id, &meta).join(", ");
    if paths.is_empty() {
        Err(format!("No app path or launcher binary for {agent_id}"))
    } else {
        Err(format!("App is not installed at any known path: {paths}"))
    }
}

#[tauri::command]
pub async fn add_custom_agent(
    app: AppHandle,
    state: State<'_, AppState>,
    config: registry::CustomAgentConfig,
) -> Result<AgentProgramInfo, String> {
    let entry = registry::add_custom_agent(config)?;

    if let Err(error) = register_custom_claude_engine(&app, &state, &entry) {
        if let Err(rollback_error) = registry::remove_custom_agent(&entry.id) {
            log::error!(
                "Failed to roll back custom agent {} after engine registration failed: {}",
                entry.id,
                rollback_error
            );
        }
        return Err(error);
    }

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
pub async fn remove_custom_agent(
    app: AppHandle,
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<(), String> {
    let entry = registry::list_custom_agents()
        .into_iter()
        .find(|agent| agent.id == agent_id)
        .ok_or_else(|| format!("Custom agent not found: {agent_id}"))?;

    unregister_custom_claude_engine(&app, &state, &entry)?;
    registry::remove_custom_agent(&agent_id)
}

const CLAUDE_COMPATIBLE_CATEGORY: &str = "claude-compatible";

fn register_custom_claude_engine(
    app: &AppHandle,
    state: &AppState,
    entry: &registry::CustomAgentEntry,
) -> Result<(), String> {
    if entry.category != CLAUDE_COMPATIBLE_CATEGORY {
        return Ok(());
    }

    let config_root = custom_claude_config_root(entry)?;
    let canonical_root = config_root
        .canonicalize()
        .unwrap_or_else(|_| config_root.clone());
    let default_root = crate::agents::claude_code::default_config_root();
    if default_root.canonicalize().unwrap_or(default_root) == canonical_root {
        return Err("Use the built-in Claude Code agent for the default config root".to_string());
    }
    let mut config = state.config_store.get();
    if config.engine_instances.iter().any(|instance| {
        let existing = crate::agents::claude_code::expand_tilde(&instance.config_root);
        existing.canonicalize().unwrap_or(existing) == canonical_root
    }) {
        return Err(format!(
            "Claude Code engine already exists at {}",
            config_root.display()
        ));
    }

    let adapter = crate::agents::claude_code::ClaudeCodeAdapter::with_config_root(
        config_root.clone(),
        entry.display_name.clone(),
    );
    adapter.install_hooks().map_err(|error| error.to_string())?;

    config.engine_instances.push(crate::config::EngineInstance {
        id: entry.id.clone(),
        label: entry.display_name.clone(),
        config_root: config_root.display().to_string(),
        enabled: true,
    });
    if let Err(error) = state.config_store.update(config) {
        if let Err(cleanup_error) = adapter.remove_hooks() {
            log::warn!(
                "Failed to clean up hooks after custom engine registration failed: {}",
                cleanup_error
            );
        }
        return Err(error);
    }

    restart_conversation_watcher(app, state);
    Ok(())
}

fn unregister_custom_claude_engine(
    app: &AppHandle,
    state: &AppState,
    entry: &registry::CustomAgentEntry,
) -> Result<(), String> {
    if entry.category != CLAUDE_COMPATIBLE_CATEGORY {
        return Ok(());
    }

    let mut config = state.config_store.get();
    let instance = config
        .engine_instances
        .iter()
        .find(|instance| instance.id == entry.id)
        .cloned();

    if let Some(instance) = instance {
        let root = crate::agents::claude_code::expand_tilde(&instance.config_root);
        let adapter =
            crate::agents::claude_code::ClaudeCodeAdapter::with_config_root(root, instance.label);
        adapter.remove_hooks().map_err(|error| error.to_string())?;
        config
            .engine_instances
            .retain(|engine| engine.id != entry.id);
        state.config_store.update(config)?;
        restart_conversation_watcher(app, state);
    }

    Ok(())
}

fn custom_claude_config_root(entry: &registry::CustomAgentEntry) -> Result<PathBuf, String> {
    let raw_root = entry
        .config_dir
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Claude Code config root cannot be empty".to_string())?;
    let root = crate::agents::claude_code::expand_tilde(raw_root);
    if !root.is_dir() {
        return Err(format!(
            "Claude Code config root does not exist: {}",
            root.display()
        ));
    }
    Ok(root)
}

fn restart_conversation_watcher(app: &AppHandle, state: &AppState) {
    let roots = state
        .config_store
        .get()
        .engine_instances
        .into_iter()
        .filter(|instance| instance.enabled)
        .map(|instance| crate::agents::claude_code::expand_tilde(&instance.config_root))
        .collect::<Vec<_>>();
    let watcher =
        crate::hooks::file_watcher::ConversationWatcher::start_with_roots(app.clone(), &roots);
    match state.conversation_watcher.lock() {
        Ok(mut current) => *current = watcher,
        Err(error) => log::warn!("Failed to refresh conversation watcher: {}", error),
    }
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
    let is_kimi = id == "kimi";
    let meta = metadata_for(&id).unwrap_or_else(default_metadata);
    let binary_path = match &meta.kind {
        AgentProgramKind::Cli => which_agent_binary(&id, &meta),
        AgentProgramKind::App if id == "antigravity" => which_agent_binary(&id, &meta),
        AgentProgramKind::App => None,
    };
    let app_path = match &meta.kind {
        AgentProgramKind::Cli => None,
        AgentProgramKind::App => installed_app_path(&id, &meta),
    };
    let display_app_path = if id == "antigravity" {
        app_path.clone()
    } else {
        app_path
            .clone()
            .or_else(|| default_app_path_for_display(&id, &meta))
    };
    let installed =
        program_is_installed_for_agent(&id, &meta, binary_path.is_some(), app_path.is_some());
    let skills_dir = agent_paths::paths_for_agent(&id)
        .skill_dirs
        .first()
        .map(|path| path.display().to_string());
    let (installed_version, latest_version) = if installed {
        version_info_for(&meta, app_path.as_deref(), include_latest).await
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
        config_dir: if is_kimi {
            Some(agent_paths::kimi_code_home().display().to_string())
        } else {
            meta.config_dir.map(expand_home)
        },
        app_path: display_app_path,
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
    app_path: Option<&str>,
    include_latest: bool,
) -> (Option<String>, Option<String>) {
    if meta.kind == AgentProgramKind::App {
        let installed = match app_path {
            Some(path) => app_installed_version(path).await,
            None => None,
        };
        return (installed, None);
    }
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

async fn app_installed_version(app_path: &str) -> Option<String> {
    let info_plist = Path::new(app_path).join("Contents").join("Info.plist");
    for key in ["CFBundleShortVersionString", "CFBundleVersion"] {
        let output = timeout(
            Duration::from_secs(2),
            Command::new("/usr/bin/plutil")
                .args(["-extract", key, "raw", "-o", "-"])
                .arg(&info_plist)
                .output(),
        )
        .await
        .ok()?
        .ok()?;
        if !output.status.success() {
            continue;
        }
        if let Some(version) = String::from_utf8(output.stdout)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            return Some(version);
        }
    }
    None
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
    let hooks_installed = agent.category == CLAUDE_COMPATIBLE_CATEGORY
        && agent
            .settings_file
            .as_deref()
            .is_some_and(|path| crate::agents::hook_manager::has_agentbro_hooks(Path::new(path)));
    let config_dir = agent
        .config_dir
        .clone()
        .unwrap_or_else(|| agent.global_skills_dir.clone());
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
        config_dir: Some(config_dir),
        app_path: None,
        download_url: None,
        install_command: None,
        update_command: None,
        uninstall_command: None,
        hooks_installed,
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
    if operation == "uninstall"
        && !program_is_installed_for_agent(
            agent_id,
            &meta,
            find_agent_binary(agent_id, &meta).is_some(),
            installed_app_path(agent_id, &meta).is_some(),
        )
    {
        emit_output(
            &app,
            agent_id,
            operation,
            "info",
            "Agent is already uninstalled",
            true,
            Some(true),
        );
        return Ok(());
    }
    if operation == "uninstall"
        && meta.kind == AgentProgramKind::App
        && meta.uninstall_command == Some(APP_TRASH_UNINSTALL_COMMAND)
    {
        return trash_agent_app(app, agent_id, &meta).await;
    }
    let command = if agent_id == "aider" && operation == "uninstall" {
        aider_uninstall_command().await?
    } else {
        match operation {
            "install" => meta.install_command,
            "update" => meta.update_command,
            "uninstall" => meta.uninstall_command,
            _ => None,
        }
        .ok_or_else(|| format!("{operation} is not supported for {agent_id}"))?
        .to_string()
    };

    emit_output(
        &app,
        agent_id,
        operation,
        "info",
        &format!("$ {command}"),
        false,
        None,
    );

    let mut child = command_shell(&command)
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

async fn aider_uninstall_command() -> Result<String, String> {
    if command_output_contains("uv", &["tool", "list"], "aider-chat").await {
        return Ok("uv tool uninstall aider-chat".to_string());
    }
    if command_output_contains("pipx", &["list", "--short"], "aider-chat").await {
        return Ok("pipx uninstall aider-chat".to_string());
    }
    if command_succeeds("python3", &["-m", "pip", "show", "aider-chat"]).await {
        return Ok("python3 -m pip uninstall -y aider-chat".to_string());
    }
    Err("Aider is present, but its package manager could not be determined".to_string())
}

async fn command_output_contains(binary: &str, args: &[&str], needle: &str) -> bool {
    let Some(path) = executable::find_binary(binary) else {
        return false;
    };
    Command::new(path)
        .args(args)
        .output()
        .await
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| String::from_utf8_lossy(&output.stdout).contains(needle))
}

async fn command_succeeds(binary: &str, args: &[&str]) -> bool {
    let Some(path) = executable::find_binary(binary) else {
        return false;
    };
    Command::new(path)
        .args(args)
        .output()
        .await
        .is_ok_and(|output| output.status.success())
}

async fn trash_agent_app(
    app: AppHandle,
    agent_id: &str,
    meta: &ProgramMetadata,
) -> Result<(), String> {
    let app_path = installed_app_path(agent_id, meta)
        .ok_or_else(|| format!("Application is not installed for {agent_id}"))?;
    let path = Path::new(&app_path);
    let system_apps = Path::new("/Applications");
    let user_apps = dirs::home_dir().map(|home| home.join("Applications"));
    let safe_parent = path.parent().is_some_and(|parent| {
        parent == system_apps || user_apps.as_deref().is_some_and(|apps| parent == apps)
    });
    if path.extension().and_then(|value| value.to_str()) != Some("app") || !safe_parent {
        return Err(format!(
            "Refusing to trash unexpected application path: {app_path}"
        ));
    }

    emit_output(
        &app,
        agent_id,
        "uninstall",
        "info",
        &format!("Moving {app_path} to Trash"),
        false,
        None,
    );

    let output = Command::new("osascript")
        .arg("-e")
        .arg("on run argv")
        .arg("-e")
        .arg("tell application \"Finder\" to delete POSIX file (item 1 of argv)")
        .arg("-e")
        .arg("end run")
        .arg(&app_path)
        .output()
        .await
        .map_err(|error| error.to_string())?;
    let success = output.status.success();
    emit_output(
        &app,
        agent_id,
        "uninstall",
        "info",
        if success {
            "Application moved to Trash"
        } else {
            "Failed to move application to Trash"
        },
        true,
        Some(success),
    );

    if success {
        Ok(())
    } else {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if error.is_empty() {
            format!("uninstall failed for {agent_id}")
        } else {
            error
        })
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
        if let Some(path) = executable::augmented_path_env() {
            shell.env("PATH", path);
        }
        shell
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut shell = Command::new("sh");
        shell.args(["-lc", command]);
        if let Some(path) = executable::augmented_path_env() {
            shell.env("PATH", path);
        }
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
    {
        return open_target_windows(target);
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut command = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(target);
        c
    };

    #[cfg(not(target_os = "windows"))]
    command.spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(target_os = "windows")]
fn open_target_windows(target: &str) -> Result<(), String> {
    let target = target.trim();
    if target.is_empty() {
        return Err("Open target is empty".to_string());
    }

    let mut command = if is_url(target) {
        let mut c = std::process::Command::new("rundll32.exe");
        c.args(["url.dll,FileProtocolHandler", target]);
        c
    } else {
        let path = Path::new(target);
        if path.exists() && is_windows_executable_path(path) {
            std::process::Command::new(path)
        } else {
            let mut c = std::process::Command::new("explorer.exe");
            c.arg(target);
            c
        }
    };

    command.spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(target_os = "windows")]
fn is_windows_executable_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "exe" | "cmd" | "bat" | "com"
            )
        })
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn is_url(target: &str) -> bool {
    target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
        || target.starts_with("agentbro:")
        || target.starts_with("ccswitch:")
}

fn launch_binary(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        return open_target(&path.display().to_string());
    }

    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

fn which(binary: &str) -> Option<String> {
    executable::find_binary(binary).map(|path| path.display().to_string())
}

fn find_agent_binary(agent_id: &str, meta: &ProgramMetadata) -> Option<PathBuf> {
    binary_candidates_for_agent(agent_id, meta)
        .into_iter()
        .find_map(executable::find_binary)
}

fn which_agent_binary(agent_id: &str, meta: &ProgramMetadata) -> Option<String> {
    find_agent_binary(agent_id, meta).map(|path| path.display().to_string())
}

fn binary_candidates_for_agent(agent_id: &str, meta: &ProgramMetadata) -> Vec<&'static str> {
    let mut candidates = match agent_id {
        "antigravity" => vec!["agy"],
        "cline" => vec!["code", "cursor"],
        "codebuddycn" | "codybuddycn" => vec!["codybuddycn", "codebuddy"],
        "cursor-cli" => vec!["cursor-agent"],
        "droid" | "factory-droid" => vec!["factory", "droid"],
        "kiro" => vec!["kiro", "kiro-cli"],
        "qoder-cli" => vec!["qodercli", "qoder"],
        "qwen" => vec!["qwen-coder", "qwen"],
        _ => Vec::new(),
    };
    if let Some(binary) = meta.binary {
        candidates.push(binary);
    }

    let mut seen = std::collections::BTreeSet::new();
    candidates
        .into_iter()
        .filter(|binary| seen.insert(binary.to_ascii_lowercase()))
        .collect()
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
    #[cfg(target_os = "windows")]
    if let Some(rest) = path.strip_prefix("~\\") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).display().to_string();
        }
    }
    path.to_string()
}

fn program_is_installed(meta: &ProgramMetadata, has_binary: bool, has_app: bool) -> bool {
    match meta.kind {
        AgentProgramKind::Cli => has_binary,
        AgentProgramKind::App => has_app,
    }
}

fn program_is_installed_for_agent(
    agent_id: &str,
    meta: &ProgramMetadata,
    has_binary: bool,
    has_app: bool,
) -> bool {
    if agent_id == "antigravity" {
        has_binary || has_app
    } else {
        program_is_installed(meta, has_binary, has_app)
    }
}

fn app_path_candidates(agent_id: &str, meta: &ProgramMetadata) -> Vec<String> {
    app_path_candidates_for_platform(agent_id, meta, RuntimePlatform::current())
}

fn app_path_candidates_for_platform(
    agent_id: &str,
    meta: &ProgramMetadata,
    platform: RuntimePlatform,
) -> Vec<String> {
    let mut paths = match platform {
        RuntimePlatform::Macos => macos_app_path_candidates(meta),
        RuntimePlatform::Windows => windows_app_path_candidates(agent_id, meta),
        RuntimePlatform::Linux => linux_app_path_candidates(agent_id, meta),
    };
    paths.sort_unstable();
    paths.dedup();
    paths
}

fn default_app_path_for_display(agent_id: &str, meta: &ProgramMetadata) -> Option<String> {
    match RuntimePlatform::current() {
        RuntimePlatform::Linux => None,
        platform => app_path_candidates_for_platform(agent_id, meta, platform)
            .into_iter()
            .next(),
    }
}

fn installed_app_path(agent_id: &str, meta: &ProgramMetadata) -> Option<String> {
    app_path_candidates(agent_id, meta)
        .into_iter()
        .find(|path| Path::new(path).exists())
}

pub(crate) fn detected_status_for_agent_program(agent_id: &str) -> AdapterStatus {
    let Some(meta) = metadata_for(agent_id) else {
        return AdapterStatus::Unavailable;
    };
    let installed = program_is_installed_for_agent(
        agent_id,
        &meta,
        find_agent_binary(agent_id, &meta).is_some(),
        installed_app_path(agent_id, &meta).is_some(),
    );
    if installed {
        AdapterStatus::Available
    } else {
        AdapterStatus::Unavailable
    }
}

pub(crate) fn has_program_metadata(agent_id: &str) -> bool {
    metadata_for(agent_id).is_some()
}

fn macos_app_path_candidates(meta: &ProgramMetadata) -> Vec<String> {
    let mut paths = Vec::new();
    let Some(app_path) = meta.app_path else {
        return paths;
    };
    paths.push(app_path.to_string());
    if let Some(bundle_name) = app_path.rsplit('/').next() {
        if let Some(home) = dirs::home_dir() {
            paths.push(
                home.join("Applications")
                    .join(bundle_name)
                    .display()
                    .to_string(),
            );
        }
    }
    paths
}

fn linux_app_path_candidates(agent_id: &str, meta: &ProgramMetadata) -> Vec<String> {
    let mut paths = Vec::new();
    let binaries = binary_candidates_for_agent(agent_id, meta);
    for binary in binaries {
        paths.push(format!("/usr/bin/{binary}"));
        paths.push(format!("/usr/local/bin/{binary}"));
        paths.push(format!("/opt/{}/{binary}", linux_dir_name(agent_id)));
        if let Some(home) = dirs::home_dir() {
            paths.push(
                home.join(".local")
                    .join("bin")
                    .join(binary)
                    .display()
                    .to_string(),
            );
        }
    }
    if let Some(home) = dirs::home_dir() {
        paths.push(
            home.join(".local")
                .join("share")
                .join("applications")
                .join(format!("{}.desktop", linux_desktop_id(agent_id)))
                .display()
                .to_string(),
        );
    }
    paths.push(format!(
        "/usr/share/applications/{}.desktop",
        linux_desktop_id(agent_id)
    ));
    paths
}

fn linux_dir_name(agent_id: &str) -> String {
    display_name_for_agent(agent_id)
        .to_ascii_lowercase()
        .replace(' ', "-")
}

fn linux_desktop_id(agent_id: &str) -> String {
    match agent_id {
        "cline" => "code".to_string(),
        "droid" | "factory-droid" => "factory".to_string(),
        "codebuddycn" | "codybuddycn" => "codybuddycn".to_string(),
        other => other.to_string(),
    }
}

fn windows_app_path_candidates(agent_id: &str, meta: &ProgramMetadata) -> Vec<String> {
    let mut paths = Vec::new();
    let dirs = windows_app_directory_names(agent_id, meta);
    let exe_names = windows_executable_names(agent_id, meta);

    if let Some(local_app_data) = windows_local_app_data_dir() {
        for dir in &dirs {
            for exe in &exe_names {
                paths.push(windows_join(&local_app_data, &["Programs", dir, exe]));
                paths.push(windows_join(&local_app_data, &[dir, exe]));
            }
        }
    }

    for env_key in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
        let Some(root) = std::env::var(env_key).ok().filter(|v| !v.is_empty()) else {
            continue;
        };
        for dir in &dirs {
            for exe in &exe_names {
                paths.push(windows_join(&root, &[dir, exe]));
            }
        }
    }

    paths
}

fn windows_local_app_data_dir() -> Option<String> {
    std::env::var("LOCALAPPDATA")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            dirs::home_dir()
                .map(|home| windows_join(&home.display().to_string(), &["AppData", "Local"]))
        })
}

fn windows_join(root: &str, parts: &[&str]) -> String {
    let mut path = root.trim_end_matches(['\\', '/']).to_string();
    for part in parts {
        let trimmed = part.trim_matches(['\\', '/']);
        if trimmed.is_empty() {
            continue;
        }
        path.push('\\');
        path.push_str(trimmed);
    }
    path
}

fn windows_app_directory_names(agent_id: &str, meta: &ProgramMetadata) -> Vec<String> {
    let mut names = Vec::new();
    match agent_id {
        "cline" => names.extend(["Microsoft VS Code", "Visual Studio Code", "VSCode"]),
        "droid" | "factory-droid" => names.extend(["Factory", "Droid"]),
        "codebuddycn" | "codybuddycn" => names.extend(["CodyBuddyCN", "CodeBuddy CN", "CodeBuddy"]),
        "qwen" => names.extend(["Qwen Code", "Qwen"]),
        _ => {}
    }
    names.push(display_name_for_agent(agent_id));
    for binary in binary_candidates_for_agent(agent_id, meta) {
        names.push(binary);
    }
    dedupe_owned(names.into_iter().map(ToString::to_string).collect())
}

fn windows_executable_names(agent_id: &str, meta: &ProgramMetadata) -> Vec<String> {
    let mut names = Vec::new();
    match agent_id {
        "cline" => names.extend(["Code.exe", "Visual Studio Code.exe"]),
        "droid" | "factory-droid" => names.extend(["Factory.exe", "Droid.exe"]),
        "codebuddycn" | "codybuddycn" => {
            names.extend(["CodyBuddyCN.exe", "CodeBuddy CN.exe", "CodeBuddy.exe"])
        }
        "qwen" => names.extend(["Qwen Code.exe", "Qwen.exe"]),
        _ => {}
    }
    names.push(display_name_for_agent(agent_id));
    for binary in binary_candidates_for_agent(agent_id, meta) {
        names.push(binary);
    }
    dedupe_owned(
        names
            .into_iter()
            .flat_map(|name| {
                if name.to_ascii_lowercase().ends_with(".exe") {
                    vec![name.to_string()]
                } else {
                    vec![format!("{name}.exe")]
                }
            })
            .collect(),
    )
}

fn dedupe_owned(values: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    values
        .into_iter()
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert(value.to_ascii_lowercase()))
        .collect()
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
        "kimi" => "Kimi Code",
        "doubao" => "Doubao",
        "droid" | "factory-droid" => "Factory Droid",
        "stepfun" => "StepFun",
        "codebuddy" => "CodeBuddy",
        "codebuddycn" | "codybuddycn" => "CodyBuddyCN",
        "workbuddy" => "WorkBuddy",
        "zcode" => "ZCode",
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
        "copilot" => cli(
            "copilot",
            "npm",
            "@github/copilot",
            "npm install -g @github/copilot",
            "npm install -g @github/copilot@latest",
            "npm uninstall -g @github/copilot",
            "~/.copilot",
            "https://docs.github.com/copilot/how-tos/set-up/install-copilot-cli",
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
        "cline" => app_no_uninstall(
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
        "kimi" => cli(
            "kimi",
            "npm",
            "@moonshot-ai/kimi-code",
            "npm install -g @moonshot-ai/kimi-code",
            "npm install -g @moonshot-ai/kimi-code@latest",
            "npm uninstall -g @moonshot-ai/kimi-code",
            "~/.kimi-code",
            "https://www.kimi.com/code/docs/kimi-code-cli/guides/getting-started.html",
        ),
        "doubao" => app(
            "doubao",
            "/Applications/Doubao.app",
            "~/Library/Application Support/Doubao",
            "https://www.doubao.com/download/desktop",
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
        "antigravity" => app_no_uninstall(
            "agy",
            "/Applications/Antigravity.app",
            "~/.gemini/config",
            "https://antigravity.google/download",
        ),
        "workbuddy" => app(
            "workbuddy",
            "/Applications/WorkBuddy.app",
            "~/.workbuddy",
            "https://workbuddy.ai",
        ),
        "zcode" => app(
            "zcode",
            "/Applications/ZCode.app",
            "~/.zcode",
            "https://zcode.z.ai/cn",
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
            "uv",
            "aider-chat",
            "uv tool install --force --python python3.12 --with pip aider-chat@latest",
            "uv tool install --force --python python3.12 --with pip aider-chat@latest",
            "uv tool uninstall aider-chat",
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
        uninstall_command: Some(APP_TRASH_UNINSTALL_COMMAND),
        app_path: Some(app_path),
        config_dir: Some(config_dir),
        download_url: Some(url),
    }
}

fn app_no_uninstall(
    binary: &'static str,
    app_path: &'static str,
    config_dir: &'static str,
    url: &'static str,
) -> ProgramMetadata {
    let mut meta = app(binary, app_path, config_dir, url);
    meta.uninstall_command = None;
    meta
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::v2::agent_meta;

    fn custom_claude_entry(config_root: String) -> registry::CustomAgentEntry {
        registry::CustomAgentEntry {
            id: "custom-codefuse".to_string(),
            display_name: "CodeFuse Claude Code".to_string(),
            category: CLAUDE_COMPATIBLE_CATEGORY.to_string(),
            global_skills_dir: format!("{config_root}/skills"),
            icon_name: Some("claude-code".to_string()),
            is_enabled: true,
            config_dir: Some(config_root.clone()),
            settings_file: Some(format!("{config_root}/settings.json")),
            mcp_config: Some(format!("{config_root}/settings.json")),
            plugin_dir: Some(format!("{config_root}/plugins/cache")),
        }
    }

    #[test]
    fn custom_claude_engine_requires_an_existing_config_root() {
        let missing = std::env::temp_dir().join(format!(
            "agentbro-missing-custom-claude-{}",
            uuid::Uuid::new_v4()
        ));
        let entry = custom_claude_entry(missing.display().to_string());

        let error = custom_claude_config_root(&entry).expect_err("missing root must fail");

        assert!(error.contains("config root does not exist"));
    }

    #[test]
    fn custom_claude_engine_accepts_an_existing_config_root() {
        let root =
            std::env::temp_dir().join(format!("agentbro-custom-claude-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create temp config root");
        let entry = custom_claude_entry(root.display().to_string());

        assert_eq!(
            custom_claude_config_root(&entry).expect("existing root"),
            root
        );

        std::fs::remove_dir(&root).expect("remove temp config root");
    }

    #[test]
    fn visible_skill_agents_have_program_metadata() {
        for agent_id in agent_meta::visible_agent_ids() {
            if !agent_paths::known_agent_ids().contains(&agent_id.as_str()) {
                continue;
            }
            assert!(
                agent_paths::known_agent_ids().contains(&agent_id.as_str()),
                "{agent_id} is shown in Agent management but missing from known agent paths"
            );

            let meta = metadata_for(&agent_id).unwrap_or_else(|| {
                panic!("{agent_id} is shown in Agent management but missing program metadata")
            });
            assert!(
                meta.install_command.is_some() || meta.download_url.is_some(),
                "{agent_id} program metadata must expose an install command or download URL"
            );
        }
    }

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

        let kimi = metadata_for("kimi").expect("kimi metadata");
        assert_eq!(display_name_for_agent("kimi"), "Kimi Code");
        assert_eq!(kimi.kind, AgentProgramKind::Cli);
        assert_eq!(kimi.binary, Some("kimi"));
        assert_eq!(kimi.package_manager, Some("npm"));
        assert_eq!(kimi.package_name, Some("@moonshot-ai/kimi-code"));
        assert_eq!(
            kimi.update_command,
            Some("npm install -g @moonshot-ai/kimi-code@latest"),
        );
        assert_eq!(kimi.config_dir, Some("~/.kimi-code"));
    }

    #[test]
    fn standalone_apps_can_move_to_trash_without_exposing_host_apps() {
        let kiro = metadata_for("kiro").expect("kiro metadata");
        assert_eq!(kiro.uninstall_command, Some(APP_TRASH_UNINSTALL_COMMAND));

        let cline = metadata_for("cline").expect("cline metadata");
        assert_eq!(cline.uninstall_command, None);
    }

    #[test]
    fn cli_program_state_requires_the_executable() {
        let aider = metadata_for("aider").expect("aider metadata");
        assert!(!program_is_installed(&aider, false, false));
        assert!(program_is_installed(&aider, true, false));
        assert_eq!(aider.package_manager, Some("uv"));
        assert_eq!(
            aider.uninstall_command,
            Some("uv tool uninstall aider-chat")
        );

        let copilot = metadata_for("copilot").expect("copilot metadata");
        assert_eq!(copilot.binary, Some("copilot"));
        assert_eq!(copilot.package_manager, Some("npm"));
        assert_eq!(copilot.package_name, Some("@github/copilot"));
        assert_eq!(
            copilot.uninstall_command,
            Some("npm uninstall -g @github/copilot")
        );
    }

    #[test]
    fn doubao_metadata_uses_the_macos_app_and_official_download() {
        let doubao = metadata_for("doubao").expect("doubao metadata");

        assert_eq!(doubao.kind, AgentProgramKind::App);
        assert_eq!(doubao.app_path, Some("/Applications/Doubao.app"));
        assert_eq!(
            doubao.config_dir,
            Some("~/Library/Application Support/Doubao")
        );
        assert_eq!(
            doubao.download_url,
            Some("https://www.doubao.com/download/desktop")
        );
    }

    #[test]
    fn antigravity_metadata_supports_desktop_and_agy_cli() {
        let antigravity = metadata_for("antigravity").expect("antigravity metadata");

        assert_eq!(antigravity.kind, AgentProgramKind::App);
        assert_eq!(antigravity.binary, Some("agy"));
        assert_eq!(antigravity.app_path, Some("/Applications/Antigravity.app"));
        assert_eq!(antigravity.config_dir, Some("~/.gemini/config"));
        assert_eq!(
            antigravity.download_url,
            Some("https://antigravity.google/download")
        );
        assert!(program_is_installed_for_agent(
            "antigravity",
            &antigravity,
            true,
            false
        ));
        assert!(program_is_installed_for_agent(
            "antigravity",
            &antigravity,
            false,
            true
        ));
        assert!(!program_is_installed_for_agent(
            "antigravity",
            &antigravity,
            false,
            false
        ));
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn reads_the_installed_version_from_an_app_info_plist() {
        let root =
            std::env::temp_dir().join(format!("agentbro-app-version-{}", uuid::Uuid::new_v4()));
        let app = root.join("Doubao.app");
        let contents = app.join("Contents");
        std::fs::create_dir_all(&contents).unwrap();
        std::fs::write(
            contents.join("Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleShortVersionString</key><string>2.19.9</string>
</dict></plist>"#,
        )
        .unwrap();

        assert_eq!(
            app_installed_version(&app.display().to_string()).await,
            Some("2.19.9".to_string())
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn linux_app_candidates_do_not_use_macos_bundles() {
        let cursor = metadata_for("cursor").expect("cursor metadata");
        let paths = app_path_candidates_for_platform("cursor", &cursor, RuntimePlatform::Linux);

        assert!(paths.iter().all(|path| !path.contains("/Applications/")));
        assert!(paths.iter().any(|path| path == "/usr/bin/cursor"));
        assert!(paths
            .iter()
            .any(|path| path.ends_with("/.local/share/applications/cursor.desktop")));
    }

    #[test]
    fn windows_app_candidates_use_exe_locations() {
        let cursor = metadata_for("cursor").expect("cursor metadata");
        let paths = app_path_candidates_for_platform("cursor", &cursor, RuntimePlatform::Windows);

        assert!(paths.iter().all(|path| !path.contains("/Applications/")));
        assert!(paths
            .iter()
            .any(|path| path.ends_with("\\Programs\\Cursor\\Cursor.exe")));
    }

    #[test]
    fn binary_candidates_include_cross_platform_aliases() {
        let qwen = metadata_for("qwen").expect("qwen metadata");
        assert_eq!(
            binary_candidates_for_agent("qwen", &qwen),
            vec!["qwen-coder", "qwen"]
        );

        let cursor_cli = metadata_for("cursor-cli").expect("cursor-cli metadata");
        assert_eq!(
            binary_candidates_for_agent("cursor-cli", &cursor_cli),
            vec!["cursor-agent"]
        );

        let antigravity = metadata_for("antigravity").expect("antigravity metadata");
        assert_eq!(
            binary_candidates_for_agent("antigravity", &antigravity),
            vec!["agy"]
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_open_target_recognizes_urls() {
        assert!(is_url("https://github.com/shirenchuang/agentbro"));
        assert!(is_url("agentbro://settings"));
        assert!(is_url("ccswitch://provider"));
        assert!(!is_url(
            r"C:\Users\admin\AppData\Local\Programs\AgentBro\AgentBro.exe"
        ));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_executable_detection_includes_common_launcher_extensions() {
        assert!(is_windows_executable_path(Path::new(r"C:\Tools\agent.exe")));
        assert!(is_windows_executable_path(Path::new(r"C:\Tools\agent.CMD")));
        assert!(is_windows_executable_path(Path::new(r"C:\Tools\agent.bat")));
        assert!(!is_windows_executable_path(Path::new(
            r"C:\Tools\agent.txt"
        )));
        assert!(!is_windows_executable_path(Path::new(r"C:\Tools\agent")));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_program_paths_expand_backslash_home_prefix() {
        let expanded = expand_home(r"~\.cursor");
        assert!(expanded.ends_with(r"\.cursor"));
        assert!(!expanded.starts_with('~'));
    }
}
