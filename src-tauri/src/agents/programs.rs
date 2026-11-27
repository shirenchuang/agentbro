use crate::agents::{executable, AdapterStatus, AgentAdapter};
use crate::commands::AppState;
use crate::platform::process::background_tokio_command;
use crate::skills::{agent_paths, registry};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::process::{ExitStatus, Stdio};
use tauri::{AppHandle, Emitter, State};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::{Mutex, MutexGuard};
use tokio::time::{timeout, Duration};

mod npm_installation;
#[cfg(target_os = "windows")]
mod windows_app_paths;

use npm_installation::{npm_installation, NpmInstallation};

static AGENT_PACKAGE_OPERATION: Mutex<()> = Mutex::const_new(());
static AGENT_PACKAGE_OUTPUT: std::sync::Mutex<BTreeMap<String, AgentPackageOutput>> =
    std::sync::Mutex::new(BTreeMap::new());
const PACKAGE_OUTPUT_LINES: usize = 10;
const PACKAGE_OUTPUT_LINE_BYTES: usize = 4096;
const PACKAGE_OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);

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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentUpdateOutcome {
    Completed,
    NoNewerVersion,
}

enum PackageOperationOutcome {
    Completed(&'static str),
    NoNewerVersion,
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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPackageOutput {
    agent_id: String,
    revision: u64,
    lines: VecDeque<AgentPackageOutputLine>,
}

#[derive(Clone, Serialize)]
struct AgentPackageOutputLine {
    stream: String,
    line: String,
}

impl AgentPackageOutput {
    fn record(&mut self, event: Option<&AgentOutputEvent>) {
        self.revision += 1;
        if let Some(event) = event {
            if self.lines.len() == PACKAGE_OUTPUT_LINES {
                self.lines.pop_front();
            }
            let mut end = event.line.len().min(PACKAGE_OUTPUT_LINE_BYTES);
            while !event.line.is_char_boundary(end) {
                end -= 1;
            }
            self.lines.push_back(AgentPackageOutputLine {
                stream: event.stream.clone(),
                line: event.line[..end].to_string(),
            });
        } else {
            self.lines.clear();
        }
    }
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

impl ProgramMetadata {
    fn npm_installation(&self, launcher: Option<&Path>) -> Option<NpmInstallation> {
        if self.package_manager != Some("npm") {
            return None;
        }
        npm_installation(launcher?, self.package_name?)
    }

    fn reviewed_npm_installation(
        &self,
        launcher: Option<&Path>,
        expected_binary_path: Option<&str>,
    ) -> Result<Option<NpmInstallation>, String> {
        let installation = self.npm_installation(launcher);
        if expected_binary_path.is_some_and(|expected| {
            !installation
                .as_ref()
                .is_some_and(|selected| selected.launcher == Path::new(expected))
        }) {
            return Err("The selected npm Agent launcher changed. Refresh Agent Management and review the installation again.".to_string());
        }
        Ok(installation)
    }
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
const CLAUDE_NATIVE_UPDATE_COMMAND: &str = "claude update";

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
pub fn agent_package_output(agent_id: String) -> Option<AgentPackageOutput> {
    AGENT_PACKAGE_OUTPUT.lock().ok()?.get(&agent_id).cloned()
}

#[tauri::command]
pub async fn agent_install(
    app: AppHandle,
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<(), String> {
    run_agent_command(app, &state.adapters, &agent_id, "install", None)
        .await
        .map(|_| ())
}

#[tauri::command]
pub async fn agent_update(
    app: AppHandle,
    state: State<'_, AppState>,
    agent_id: String,
    expected_binary_path: Option<String>,
) -> Result<AgentUpdateOutcome, String> {
    run_agent_command(
        app,
        &state.adapters,
        &agent_id,
        "update",
        expected_binary_path.as_deref(),
    )
    .await
    .map(|outcome| match outcome {
        PackageOperationOutcome::Completed(_) => AgentUpdateOutcome::Completed,
        PackageOperationOutcome::NoNewerVersion => AgentUpdateOutcome::NoNewerVersion,
    })
}

#[tauri::command]
pub async fn agent_uninstall(
    app: AppHandle,
    state: State<'_, AppState>,
    agent_id: String,
    expected_binary_path: Option<String>,
) -> Result<(), String> {
    run_agent_command(
        app,
        &state.adapters,
        &agent_id,
        "uninstall",
        expected_binary_path.as_deref(),
    )
    .await
    .map(|_| ())
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
    open_agent_app(&agent_id)
}

pub(crate) fn open_agent_app(agent_id: &str) -> Result<(), String> {
    let meta = metadata_for(agent_id).ok_or_else(|| format!("Unknown agent: {agent_id}"))?;
    if let Some(path) = installed_app_path(agent_id, &meta) {
        return open_target(&path);
    }
    if let Some(binary) = find_agent_binary(agent_id, &meta) {
        return launch_binary(&binary);
    }
    let paths = app_path_candidates(agent_id, &meta).join(", ");
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
    let mut meta = metadata_for(&id).unwrap_or_else(default_metadata);
    let binary_path = match &meta.kind {
        AgentProgramKind::Cli => find_agent_binary(&id, &meta),
        AgentProgramKind::App if id == "antigravity" => find_agent_binary(&id, &meta),
        AgentProgramKind::App => None,
    };
    let desktop_codex = desktop_codex_launcher(&id, binary_path.as_deref());
    let native_claude = native_claude_launcher(&id, binary_path.as_deref());
    let npm = meta.npm_installation(binary_path.as_deref());
    if desktop_codex.is_some() || native_claude.is_some() {
        meta.package_manager = Some(if desktop_codex.is_some() {
            "app"
        } else {
            "native"
        });
        meta.package_name = None;
        meta.install_command = None;
        meta.update_command = native_claude.map(|_| CLAUDE_NATIVE_UPDATE_COMMAND);
        meta.uninstall_command = None;
    } else if binary_path.is_some() && meta.package_manager == Some("npm") && npm.is_none() {
        meta.update_command = None;
        meta.uninstall_command = None;
    }
    let app_path = match &meta.kind {
        AgentProgramKind::Cli => None,
        AgentProgramKind::App => installed_app_path(&id, &meta),
    };
    if meta.uninstall_command == Some(APP_TRASH_UNINSTALL_COMMAND)
        && app_path
            .as_deref()
            .is_some_and(|path| !is_trashable_app_path(Path::new(path)))
    {
        meta.uninstall_command = None;
    }
    let display_app_path = if id == "antigravity" {
        app_path.clone()
    } else {
        app_path
            .clone()
            .or_else(|| default_app_path_for_display(&id, &meta, RuntimePlatform::current()))
    };
    let installed =
        program_is_installed_for_agent(&id, &meta, binary_path.is_some(), app_path.is_some());
    let paths = agent_paths::paths_for_agent(&id);
    let skill_path = paths.skill_dirs.first().map(PathBuf::as_path);
    let skills_dir = skill_path.map(|path| path.display().to_string());
    let (installed_version, latest_version) = if let Some(launcher) = desktop_codex {
        (desktop_codex_version(launcher).await, None)
    } else if let Some(launcher) = native_claude {
        (native_claude_version(launcher).await, None)
    } else if let ("qoder-cli", Some(launcher)) = (id.as_str(), binary_path.as_deref()) {
        let version = selected_cli_version(launcher, |line| {
            Some(line.strip_prefix('v').unwrap_or(line))
        })
        .await;
        (version, None)
    } else if installed {
        version_info_for(
            &id,
            &meta,
            app_path.as_deref(),
            npm.as_ref(),
            include_latest,
        )
        .await
    } else {
        (None, None)
    };
    let update_available =
        versions_indicate_update(installed_version.as_deref(), latest_version.as_deref());
    let config_dir = match id.as_str() {
        "kimi" => Some(agent_paths::kimi_code_home().display().to_string()),
        "claude-code" | "deepseek-harness" | "qoder-cli" => skill_path
            .and_then(Path::parent)
            .map(|path| path.display().to_string()),
        _ => meta.config_dir.map(expand_home),
    };

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
        binary_path: npm
            .as_ref()
            .map(|npm| &npm.launcher)
            .or(binary_path.as_ref())
            .map(|path| path.display().to_string()),
        config_dir,
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

async fn desktop_codex_version(launcher: &Path) -> Option<String> {
    selected_cli_version(launcher, |line| line.strip_prefix("codex-cli ")).await
}

async fn native_claude_version(launcher: &Path) -> Option<String> {
    selected_cli_version(launcher, |line| {
        Some(line.strip_suffix(" (Claude Code)").unwrap_or(line))
    })
    .await
}

async fn selected_cli_version(
    launcher: &Path,
    version_from_line: fn(&str) -> Option<&str>,
) -> Option<String> {
    let path = executable::augmented_path_env();
    let mut command = package_command(launcher, path.as_deref());
    let output = package_probe_output(command.arg("--version"), Duration::from_secs(5))
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    std::str::from_utf8(&output.stdout)
        .ok()?
        .lines()
        .find_map(|line| {
            let version = version_from_line(line.trim())?.trim();
            parse_semver(version).map(|_| version.to_string())
        })
}

async fn version_info_for(
    _agent_id: &str,
    meta: &ProgramMetadata,
    app_path: Option<&str>,
    npm: Option<&NpmInstallation>,
    include_latest: bool,
) -> (Option<String>, Option<String>) {
    if meta.kind == AgentProgramKind::App {
        #[cfg(target_os = "windows")]
        if _agent_id == super::gemini_desktop::ID {
            let version = app_path
                .and_then(|path| super::gemini_desktop::windows_release(Path::new(path)))
                .map(|version| version.to_string());
            return (version, None);
        }
        let installed = match app_path {
            Some(path) => app_installed_version(path).await,
            None => None,
        };
        return (installed, None);
    }
    let (Some(installation), Some(package)) = (npm, meta.package_name) else {
        return (None, None);
    };
    let latest = if include_latest {
        let npm = executable::command_path("npm");
        let path = executable::augmented_path_env();
        npm_latest_version(
            package_command(&npm, path.as_deref()),
            package,
            &installation.prefix,
        )
        .await
        .ok()
        .map(|version| version.to_string())
    } else {
        None
    };
    (Some(installation.version.clone()), latest)
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "windows")]
async fn app_installed_version(app_path: &str) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;

    let path: Vec<u16> = Path::new(app_path)
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    tokio::task::spawn_blocking(move || read_windows_app_version(&path))
        .await
        .ok()
        .flatten()
}

#[cfg(target_os = "windows")]
fn read_windows_app_version(path: &[u16]) -> Option<String> {
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
    };

    let mut ignored_handle = 0;
    let length = unsafe { GetFileVersionInfoSizeW(path.as_ptr(), &mut ignored_handle) };
    if length == 0 {
        return None;
    }
    let mut version = vec![0_u8; length as usize];
    if unsafe { GetFileVersionInfoW(path.as_ptr(), 0, length, version.as_mut_ptr().cast()) } == 0 {
        return None;
    }

    let root = [u16::from(b'\\'), 0];
    let mut fixed = std::ptr::null_mut();
    let mut size = 0;
    if unsafe {
        VerQueryValueW(
            version.as_ptr().cast(),
            root.as_ptr(),
            &mut fixed,
            &mut size,
        )
    } == 0
        || fixed.is_null()
        || (size as usize) < std::mem::size_of::<VS_FIXEDFILEINFO>()
    {
        return None;
    }
    let fixed = unsafe { fixed.cast::<VS_FIXEDFILEINFO>().read_unaligned() };
    if fixed.dwSignature != 0xfeef04bd {
        return None;
    }
    format_windows_app_version(
        (fixed.dwProductVersionMS, fixed.dwProductVersionLS),
        (fixed.dwFileVersionMS, fixed.dwFileVersionLS),
    )
}

#[cfg(any(target_os = "windows", test))]
fn format_windows_app_version(product: (u32, u32), file: (u32, u32)) -> Option<String> {
    let (high, low) = if product == (0, 0) { file } else { product };
    (high != 0 || low != 0).then(|| {
        format!(
            "{}.{}.{}.{}",
            high >> 16,
            high & 0xffff,
            low >> 16,
            low & 0xffff
        )
    })
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
async fn app_installed_version(_app_path: &str) -> Option<String> {
    None
}

async fn npm_latest_version(
    mut npm: Command,
    package: &str,
    prefix: &Path,
) -> Result<semver::Version, String> {
    npm.args([
        "view",
        &format!("{package}@latest"),
        "version",
        "--silent",
        "--no-update-notifier",
        "--fetch-retries=0",
        "--fetch-timeout=4000",
    ]);
    set_npm_prefix(&mut npm, prefix);
    npm.arg("--prefer-online");
    let output = package_probe_output(&mut npm, Duration::from_secs(5))
        .await
        .map_err(|error| format!("Could not start package command: {error}"))?;

    if !output.status.success() {
        return Err(format!(
            "npm version check failed for {package} ({})",
            output.status
        ));
    }

    let value = std::str::from_utf8(&output.stdout).ok().map(str::trim);
    value
        .and_then(parse_semver)
        .ok_or_else(|| format!("npm returned no valid latest version for {package}"))
}

async fn package_probe_output(
    command: &mut Command,
    deadline: Duration,
) -> std::io::Result<std::process::Output> {
    timeout(deadline, command.kill_on_drop(true).output())
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "Version check timed out"))?
}

async fn npm_update_command_line(
    agent_id: &str,
    meta: &ProgramMetadata,
    installation: &NpmInstallation,
) -> Result<Option<String>, String> {
    let package = meta
        .package_name
        .ok_or_else(|| format!("No npm package configured for {agent_id}"))?;
    let installed = parse_semver(&installation.version)
        .ok_or_else(|| format!("Could not verify the installed npm version for {package}"))?;
    let npm = executable::command_path("npm");
    let path = executable::augmented_path_env();
    let latest = npm_latest_version(
        package_command(&npm, path.as_deref()),
        package,
        &installation.prefix,
    )
    .await?;
    let current_binary = find_agent_binary(agent_id, meta);
    if meta.npm_installation(current_binary.as_deref()).as_ref() != Some(installation) {
        return Err("The npm Agent installation changed while checking for updates. Refresh Agent Management and try again.".to_string());
    }
    Ok(latest
        .cmp_precedence(&installed)
        .is_gt()
        .then(|| format!("npm install -g {package}@{latest}")))
}

fn versions_indicate_update(installed: Option<&str>, latest: Option<&str>) -> bool {
    match (
        installed.and_then(parse_semver),
        latest.and_then(parse_semver),
    ) {
        (Some(installed), Some(latest)) => latest.cmp_precedence(&installed).is_gt(),
        _ => false,
    }
}

fn parse_semver(version: &str) -> Option<semver::Version> {
    let version = version.trim();
    semver::Version::parse(version.strip_prefix('v').unwrap_or(version)).ok()
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

fn claim_agent_package_operation() -> Result<MutexGuard<'static, ()>, String> {
    AGENT_PACKAGE_OPERATION
        .try_lock()
        .map_err(|_| "agent_package_operation_in_progress".to_string())
}

async fn run_agent_command(
    app: AppHandle,
    _adapters: &[std::sync::Arc<dyn AgentAdapter>],
    agent_id: &str,
    operation: &str,
    expected_binary_path: Option<&str>,
) -> Result<PackageOperationOutcome, String> {
    let meta = metadata_for(agent_id).ok_or_else(|| format!("Unknown agent: {agent_id}"))?;
    let _package_operation = claim_agent_package_operation()?;
    let installed_binary = if meta.package_manager == Some("npm") || operation == "uninstall" {
        find_agent_binary(agent_id, &meta)
    } else {
        None
    };
    if desktop_codex_launcher(agent_id, installed_binary.as_deref()).is_some() {
        return Err("The selected Codex installation is managed by its desktop client".to_string());
    }
    publish_package_output(&app, agent_id, None);
    let result = execute_package_operation(
        &app,
        agent_id,
        operation,
        &meta,
        installed_binary.as_deref(),
        expected_binary_path,
    )
    .await;
    let (stream, message) = match &result {
        Ok(PackageOperationOutcome::Completed(message)) => ("info", *message),
        Ok(PackageOperationOutcome::NoNewerVersion) => ("info", "npm offers no newer version"),
        Err(error) => ("stderr", error.as_str()),
    };
    emit_output(
        &app,
        agent_id,
        operation,
        stream,
        message,
        true,
        Some(result.is_ok()),
    );
    result
}

async fn execute_package_operation(
    app: &AppHandle,
    agent_id: &str,
    operation: &str,
    meta: &ProgramMetadata,
    installed_binary: Option<&Path>,
    expected_binary_path: Option<&str>,
) -> Result<PackageOperationOutcome, String> {
    let npm = meta.reviewed_npm_installation(installed_binary, expected_binary_path)?;
    if operation == "uninstall"
        && !program_is_installed_for_agent(
            agent_id,
            meta,
            installed_binary.is_some(),
            installed_app_path(agent_id, meta).is_some(),
        )
    {
        return Ok(PackageOperationOutcome::Completed(
            "Agent is already uninstalled",
        ));
    }
    if operation == "uninstall"
        && meta.kind == AgentProgramKind::App
        && meta.uninstall_command == Some(APP_TRASH_UNINSTALL_COMMAND)
    {
        return trash_agent_app(app, agent_id, meta)
            .await
            .map(PackageOperationOutcome::Completed);
    }
    let (command_line, command) = if let Some(launcher) =
        native_claude_launcher(agent_id, installed_binary)
    {
        (
            CLAUDE_NATIVE_UPDATE_COMMAND.to_string(),
            native_claude_package_command(launcher, operation)?,
        )
    } else if agent_id == "aider" && operation == "uninstall" {
        aider_uninstall_command(RuntimePlatform::current(), executable::find_binary).await?
    } else {
        let mut command_line = match operation {
            "install" => meta.install_command,
            "update" => meta.update_command,
            "uninstall" => meta.uninstall_command,
            _ => None,
        }
        .ok_or_else(|| format!("{operation} is not supported for {agent_id}"))?
        .to_string();
        if npm.is_none()
            && meta.package_manager == Some("npm")
            && (installed_binary.is_some() || operation != "install")
        {
            return Err(format!(
                "Could not locate the npm installation for the detected {agent_id} launcher"
            ));
        }
        if let Some(installation) = &npm {
            emit_output(
                app,
                agent_id,
                operation,
                "info",
                &format!("npm global directory: {}", installation.prefix.display()),
                false,
                None,
            );
            if operation == "update" {
                let Some(update) = npm_update_command_line(agent_id, meta, installation).await?
                else {
                    return Ok(PackageOperationOutcome::NoNewerVersion);
                };
                command_line = update;
            }
        }
        let command =
            package_operation_command(&command_line, npm.as_ref().map(|npm| npm.prefix.as_path()));
        (command_line, command)
    };

    emit_output(
        app,
        agent_id,
        operation,
        "info",
        &format!("$ {command_line}"),
        false,
        None,
    );

    let status = run_package_command(
        command,
        |line| emit_output(app, agent_id, operation, "stdout", line, false, None),
        |line| emit_output(app, agent_id, operation, "stderr", line, false, None),
    )
    .await?;
    if status.success() {
        Ok(PackageOperationOutcome::Completed("Operation completed"))
    } else {
        Err(format!("{operation} failed for {agent_id} ({status})"))
    }
}

async fn run_package_command(
    mut command: Command,
    mut emit_stdout: impl FnMut(&str),
    mut emit_stderr: impl FnMut(&str),
) -> Result<ExitStatus, String> {
    #[cfg(target_os = "windows")]
    let (stdout, stderr) = {
        use crate::platform::process::windows_output_pipe;

        let (stdout, child_stdout) = windows_output_pipe()
            .await
            .map_err(|error| format!("Could not prepare package stdout: {error}"))?;
        let (stderr, child_stderr) = windows_output_pipe()
            .await
            .map_err(|error| format!("Could not prepare package stderr: {error}"))?;
        command.stdout(child_stdout).stderr(child_stderr);
        (Some(stdout), Some(stderr))
    };
    #[cfg(not(target_os = "windows"))]
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not start package command: {error}"))?;
    drop(command);
    #[cfg(not(target_os = "windows"))]
    let (stdout, stderr) = (child.stdout.take(), child.stderr.take());

    let mut stdout_line = Vec::new();
    let mut stderr_line = Vec::new();
    let status = {
        let mut output = pin!(async {
            let _ = tokio::join!(
                forward_package_output(stdout, &mut stdout_line, &mut emit_stdout),
                forward_package_output(stderr, &mut stderr_line, &mut emit_stderr),
            );
        });
        tokio::select! {
            status = child.wait() => {
                // A background helper can inherit the pipes and keep them open after exit.
                if status.is_ok() {
                    let _ = timeout(PACKAGE_OUTPUT_DRAIN_TIMEOUT, output).await;
                }
                status
            }
            () = &mut output => child.wait().await,
        }
    };
    if !stdout_line.is_empty() {
        emit_stdout(&String::from_utf8_lossy(&stdout_line));
    }
    if !stderr_line.is_empty() {
        emit_stderr(&String::from_utf8_lossy(&stderr_line));
    }
    status.map_err(|error| format!("Could not wait for package command: {error}"))
}

async fn forward_package_output(
    pipe: Option<impl AsyncRead + Unpin>,
    line: &mut Vec<u8>,
    mut emit: impl FnMut(&str),
) -> io::Result<()> {
    let Some(pipe) = pipe else {
        return Ok(());
    };
    let mut reader = BufReader::new(pipe);
    while reader.read_until(b'\n', line).await? != 0 {
        if line.last() == Some(&b'\n') {
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
        }
        emit(&String::from_utf8_lossy(line));
        line.clear();
    }
    Ok(())
}

fn native_claude_package_command(launcher: &Path, operation: &str) -> Result<Command, String> {
    if operation != "update" {
        return Err(format!(
            "{operation} is not supported for the native Claude Code installation"
        ));
    }
    let path = executable::augmented_path_env();
    let mut command = package_command(launcher, path.as_deref());
    command.arg("update").stdin(Stdio::null());
    Ok(command)
}

async fn aider_uninstall_command(
    platform: RuntimePlatform,
    find_binary: impl Fn(&str) -> Option<PathBuf>,
) -> Result<(String, Command), String> {
    let path = executable::augmented_path_env();
    let python_launchers: &[&str] = if platform == RuntimePlatform::Windows {
        &["python", "py", "python3"]
    } else {
        &["python3"]
    };
    let tool_managers = [
        (
            "uv",
            &["tool", "list"][..],
            &["tool", "uninstall", "aider-chat"][..],
        ),
        (
            "pipx",
            &["list", "--short"][..],
            &["uninstall", "aider-chat"][..],
        ),
    ];
    let pip_managers = python_launchers.iter().map(|&launcher| {
        (
            launcher,
            &["-m", "pip", "show", "aider-chat"][..],
            &["-m", "pip", "uninstall", "-y", "aider-chat"][..],
        )
    });
    for (binary, probe_args, uninstall_args) in tool_managers.into_iter().chain(pip_managers) {
        let Some(launcher) = find_binary(binary) else {
            continue;
        };
        let is_pip = python_launchers.contains(&binary);
        let command_for_launcher = || {
            let mut command = package_command(&launcher, path.as_deref());
            if is_pip && platform == RuntimePlatform::Windows {
                command.env("PYTHON_MANAGER_AUTOMATIC_INSTALL", "false");
            }
            command
        };
        let mut probe = command_for_launcher();
        let output = package_probe_output(probe.args(probe_args), Duration::from_secs(5)).await;
        let package_found = output.is_ok_and(|output| {
            output.status.success()
                && (is_pip || String::from_utf8_lossy(&output.stdout).contains("aider-chat"))
        });
        if package_found {
            let mut command = command_for_launcher();
            command.args(uninstall_args);
            return Ok((format!("{binary} {}", uninstall_args.join(" ")), command));
        }
    }
    Err("Aider is present, but its package manager could not be determined".to_string())
}

async fn trash_agent_app(
    app: &AppHandle,
    agent_id: &str,
    meta: &ProgramMetadata,
) -> Result<&'static str, String> {
    let app_path = installed_app_path(agent_id, meta)
        .ok_or_else(|| format!("Application is not installed for {agent_id}"))?;
    if !is_trashable_app_path(Path::new(&app_path)) {
        return Err(format!(
            "Refusing to trash unexpected application path: {app_path}"
        ));
    }

    emit_output(
        app,
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
    if output.status.success() {
        Ok("Application moved to Trash")
    } else {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if error.is_empty() {
            format!("uninstall failed for {agent_id}")
        } else {
            error
        })
    }
}

fn is_trashable_app_path(path: &Path) -> bool {
    let user_apps = dirs::home_dir().map(|home| home.join("Applications"));
    path.extension() == Some(OsStr::new("app"))
        && path.parent().is_some_and(|parent| {
            parent == Path::new("/Applications") || user_apps.as_deref() == Some(parent)
        })
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
    let event = AgentOutputEvent {
        agent_id: agent_id.to_string(),
        operation: operation.to_string(),
        stream: stream.to_string(),
        line: line.to_string(),
        done,
        success,
    };
    publish_package_output(app, agent_id, Some(&event));
    let _ = app.emit("agent-output", event);
}

fn publish_package_output(app: &AppHandle, agent_id: &str, event: Option<&AgentOutputEvent>) {
    let snapshot = AGENT_PACKAGE_OUTPUT.lock().ok().map(|mut history| {
        let output = history
            .entry(agent_id.to_string())
            .or_insert_with(|| AgentPackageOutput {
                agent_id: agent_id.to_string(),
                revision: 0,
                lines: VecDeque::new(),
            });
        output.record(event);
        output.clone()
    });
    if let Some(snapshot) = snapshot {
        let _ = app.emit("agent-package-output", snapshot);
    }
}

fn package_command(program: impl AsRef<OsStr>, path: Option<&OsStr>) -> Command {
    let mut command = background_tokio_command(program);
    if let Some(path) = path {
        command.env("PATH", path);
    }
    command
}

fn set_npm_prefix(command: &mut Command, prefix: &Path) {
    #[cfg(target_os = "windows")]
    command.env("npm_config_prefix", prefix);
    #[cfg(not(target_os = "windows"))]
    command.arg("--prefix").arg(prefix);
}

fn package_operation_command(command: &str, npm_prefix: Option<&Path>) -> Command {
    let path = executable::augmented_path_env();
    if let Some(arguments) = command.strip_prefix("npm ") {
        let launcher = executable::command_path("npm");
        let mut process = package_command(&launcher, path.as_deref());
        process.args(arguments.split_ascii_whitespace());
        if let Some(prefix) = npm_prefix {
            set_npm_prefix(&mut process, prefix);
        }
        #[cfg(target_os = "windows")]
        if launcher
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
            })
        {
            // Rust runs batch shims via cmd /C, which can lose an exit /B status.
            process.raw_arg("& exit");
        }
        return process;
    }

    #[cfg(target_os = "windows")]
    let (program, flag) = (executable::command_path("cmd"), "/C");
    #[cfg(not(target_os = "windows"))]
    let (program, flag) = ("sh", "-lc");
    let mut shell = package_command(program, path.as_deref());
    shell.args([flag, command]);
    shell
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

fn find_agent_binary(agent_id: &str, meta: &ProgramMetadata) -> Option<PathBuf> {
    if agent_id == super::gemini_desktop::ID {
        return None;
    }
    if agent_id == "qoder-cli" {
        return executable::find_qoder_cli_binary();
    }
    binary_candidates_for_agent(agent_id, meta)
        .into_iter()
        .find_map(executable::find_binary)
}

fn desktop_codex_launcher<'a>(agent_id: &str, binary: Option<&'a Path>) -> Option<&'a Path> {
    binary.filter(|path| agent_id == "codex" && executable::is_desktop_bundled_codex_cli(path))
}

fn native_claude_launcher<'a>(agent_id: &str, binary: Option<&'a Path>) -> Option<&'a Path> {
    if agent_id != "claude-code" {
        return None;
    }
    let launcher = binary?;
    let home = dirs::home_dir()?;
    is_native_claude_launcher(launcher, &home, RuntimePlatform::current()).then_some(launcher)
}

fn is_native_claude_launcher(binary: &Path, home: &Path, platform: RuntimePlatform) -> bool {
    let local = home.join(".local");
    if platform == RuntimePlatform::Windows {
        let launcher = local.join("bin").join("claude.exe");
        let same_path = binary.components().count() == launcher.components().count()
            && binary
                .components()
                .zip(launcher.components())
                .all(|(left, right)| left.as_os_str().eq_ignore_ascii_case(right.as_os_str()));
        return same_path
            || binary
                .canonicalize()
                .ok()
                .zip(launcher.canonicalize().ok())
                .is_some_and(|(binary, launcher)| binary == launcher);
    }
    let versions = local.join("share/claude/versions");
    let versions = versions.canonicalize().unwrap_or(versions);
    binary
        .canonicalize()
        .is_ok_and(|path| path.starts_with(versions))
}

fn binary_candidates_for_agent(agent_id: &str, meta: &ProgramMetadata) -> Vec<&'static str> {
    let mut candidates = match agent_id {
        "antigravity" => vec!["agy"],
        "cline" => vec!["code", "cursor"],
        "codebuddycn" | "codybuddycn" => vec!["codybuddycn", "codebuddy"],
        "cursor-cli" => vec!["cursor-agent"],
        "droid" | "factory-droid" => vec!["factory", "droid"],
        "kiro" => vec!["kiro", "kiro-cli"],
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
        RuntimePlatform::Macos => {
            let mut paths = macos_app_path_candidates(meta);
            if agent_id == "qoder" {
                if let Some(legacy) = metadata_for("qoder-desktop") {
                    paths.extend(macos_app_path_candidates(&legacy));
                }
            }
            paths
        }
        RuntimePlatform::Windows => windows_app_path_candidates(agent_id, meta),
        RuntimePlatform::Linux => linux_app_path_candidates(agent_id, meta),
    };
    paths.sort_unstable();
    paths.dedup();
    paths
}

fn default_app_path_for_display(
    agent_id: &str,
    meta: &ProgramMetadata,
    platform: RuntimePlatform,
) -> Option<String> {
    if meta.kind != AgentProgramKind::App || platform == RuntimePlatform::Linux {
        return None;
    }
    app_path_candidates_for_platform(agent_id, meta, platform)
        .into_iter()
        .next()
}

pub(crate) fn installed_app_path_for_agent(agent_id: &str) -> Option<String> {
    installed_app_path(agent_id, &metadata_for(agent_id)?)
}

fn installed_app_path(agent_id: &str, meta: &ProgramMetadata) -> Option<String> {
    #[cfg(target_os = "windows")]
    if agent_id == super::gemini_desktop::ID {
        if let Some(path) = windows_app_paths::gemini_install_location() {
            return Some(path);
        }
    }
    let existing = app_path_candidates(agent_id, meta)
        .into_iter()
        .find(|path| {
            let candidate = Path::new(path);
            if cfg!(target_os = "windows") {
                candidate.is_file() && app_path_matches_agent(agent_id, candidate)
            } else {
                candidate.exists() && app_path_matches_agent(agent_id, candidate)
            }
        });
    #[cfg(target_os = "macos")]
    let existing = existing.or_else(|| {
        macos_agent_bundle_ids(agent_id)
            .iter()
            .flat_map(|id| crate::platform::macos_applications::registered_app_paths(id))
            .find_map(|path| path.into_os_string().into_string().ok())
    });
    #[cfg(target_os = "windows")]
    let existing = existing.or_else(|| {
        windows_app_paths::find_registered(&windows_registration_names(agent_id, meta), |path| {
            app_path_matches_agent(agent_id, Path::new(path))
        })
    });
    existing
}

#[cfg(target_os = "macos")]
fn macos_agent_bundle_ids(agent_id: &str) -> &[&str] {
    match agent_id {
        super::gemini_desktop::ID => &[super::gemini_desktop::BUNDLE_ID],
        "cursor" => &["com.todesktop.230313mzl4w4u92"],
        "qoder" => &["com.qoder.ide"],
        "qoder-desktop" => &["com.qoder.app"],
        "codebuddy" => &["com.tencent.codebuddy"],
        "codebuddycn" | "codybuddycn" => &["com.tencent.codebuddy.cn"],
        "qwenwork-cn" => &["cn.qwenwork.desktop.mac"],
        "deepseek-harness" => &["com.deepseek.dsh"],
        "stepfun" => &["com.stepfun.app"],
        "droid" | "factory-droid" => &["com.factory.app"],
        "workbuddy" => &["com.tencent.workbuddy.mac", "com.workbuddy.workbuddy"],
        "workbuddy-ai" => &["com.workbuddy.workbuddy-ai"],
        "zcode" => &["dev.zcode.app"],
        _ => &[],
    }
}

fn app_path_matches_agent(agent_id: &str, path: &Path) -> bool {
    if agent_id == super::gemini_desktop::ID {
        #[cfg(target_os = "macos")]
        return super::gemini_desktop::is_macos_bundle(path);
        #[cfg(target_os = "windows")]
        return super::gemini_desktop::windows_release(path).is_some();
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        return false;
    }
    if !matches!(agent_id, "qoder" | "qoder-desktop") {
        return true;
    }
    #[cfg(target_os = "macos")]
    let source = super::qoder::qoder_app_source(path);
    #[cfg(target_os = "windows")]
    let source = super::qoder::windows_qoder_app_source(path);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let source = {
        let _ = path;
        Some(agent_id)
    };
    source == Some(agent_id)
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
        if agent_id == super::gemini_desktop::ID {
            paths.push(windows_join(
                &local_app_data,
                &["Google", "Gemini", "Gemini.exe"],
            ));
        }
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
        if agent_id == super::gemini_desktop::ID {
            paths.push(windows_join(&root, &["Google", "Gemini", "Gemini.exe"]));
        }
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
        "qoder" => names.extend(["Qoder IDE", "Qoder"]),
        "qoder-desktop" => names.push("Qoder"),
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
        "qwenwork-cn" => names.push("Launcher.exe"),
        "qoder" => names.extend(["Qoder IDE.exe", "Qoder.exe"]),
        "qoder-desktop" => names.push("Qoder.exe"),
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

#[cfg(target_os = "windows")]
fn windows_registration_names(agent_id: &str, meta: &ProgramMetadata) -> Vec<String> {
    if meta.kind != AgentProgramKind::App || agent_id == "cline" {
        return Vec::new();
    }
    windows_executable_names(agent_id, meta)
        .into_iter()
        .filter(|name| !name.eq_ignore_ascii_case("Launcher.exe"))
        .filter(|name| match agent_id {
            "codebuddycn" | "codybuddycn" => !name.eq_ignore_ascii_case("CodeBuddy.exe"),
            "easyclaw-v2" => !name.eq_ignore_ascii_case("EasyClaw.exe"),
            "antigravity" => !name.eq_ignore_ascii_case("agy.exe"),
            "kiro" => !name.eq_ignore_ascii_case("kiro-cli.exe"),
            "qwen" => !name.eq_ignore_ascii_case("qwen-coder.exe"),
            _ => true,
        })
        .collect()
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
        super::gemini_desktop::ID => "Gemini Desktop",
        "cursor" => "Cursor",
        "cursor-cli" => "Cursor CLI",
        "opencode" => "OpenCode",
        "copilot" => "GitHub Copilot",
        "qoder" => "Qoder IDE",
        "qoder-desktop" => "Qoder Desktop",
        "qoder-cli" => "Qoder CLI",
        "hermes" => "Hermes",
        "antigravity" => "Antigravity",
        "qwen" => "Qwen Code",
        "qwenwork-cn" => "千问办公 (QwenWork)",
        "deepseek" => "DeepSeek",
        "deepseek-harness" => "DeepSeek Harness",
        "kimi" => "Kimi Code",
        "doubao" => "Doubao",
        "droid" | "factory-droid" => "Factory Droid",
        "stepfun" => "StepFun",
        "codebuddy" => "CodeBuddy",
        "codebuddycn" | "codybuddycn" => "CodyBuddyCN",
        "workbuddy" => "WorkBuddy",
        "workbuddy-ai" => "WorkBuddy AI",
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
        "gemini-cli" | super::gemini_desktop::ID => "gemini".to_string(),
        "cursor-cli" => "cursor".to_string(),
        "qoder-cli" | "qoder-desktop" => "qoder".to_string(),
        "qwenwork-cn" => "qwen".to_string(),
        "deepseek-harness" => "deepseek".to_string(),
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
        super::gemini_desktop::ID => ProgramMetadata {
            config_dir: cfg!(target_os = "macos").then_some("~/.gemini/config"),
            ..app_no_uninstall(
                "Gemini",
                "/Applications/Gemini.app",
                "",
                "https://gemini.google/desktop/",
            )
        },
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
            "https://docs.qoder.com/cli/installation",
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
            "/Applications/Qoder IDE.app",
            "~/.qoder",
            "https://qoder.com",
        ),
        "qoder-desktop" => app_no_uninstall(
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
        "qwenwork-cn" => app(
            "QwenWorkCN",
            "/Applications/QwenWorkCN.app",
            "~/.qwenworkcn",
            "https://qwenwork.cn/",
        ),
        "deepseek" => app(
            "deepseek",
            "/Applications/DeepSeek.app",
            "~/.deepseek",
            "https://www.deepseek.com",
        ),
        "deepseek-harness" => app(
            "dsh",
            "/Applications/DeepSeek Harness.app",
            "~/.dsh",
            "https://www.deepseek.com/harness/",
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
            "https://workbuddy.cn",
        ),
        "workbuddy-ai" => app(
            "WorkBuddyAI",
            "/Applications/WorkBuddy AI.app",
            "~/.workbuddy-ai",
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
        uninstall_command: (RuntimePlatform::current() == RuntimePlatform::Macos)
            .then_some(APP_TRASH_UNINSTALL_COMMAND),
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
    #[test]
    fn gemini_desktop_versions_come_from_the_packaged_app() {
        let home =
            crate::skills::installer::TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let launcher = home.path().join("Gemini.exe");
        std::fs::write(&launcher, "").unwrap();
        let release = super::super::gemini_desktop::windows_release;
        assert!(release(&launcher).is_none());
        for (version, complete) in [("1.9.0", true), ("1.21.1", true), ("1.23.0", false)] {
            let app = home.path().join(format!("app-{version}"));
            std::fs::create_dir(&app).unwrap();
            std::fs::write(app.join("Gemini.exe"), "").unwrap();
            if complete {
                std::fs::write(app.join("GEMINI_BUILD_INFO.txt"), "").unwrap();
            }
        }
        assert_eq!(release(&launcher).unwrap().to_string(), "1.21.1");
        assert_eq!(
            release(&home.path().join("app-1.9.0/Gemini.exe"))
                .unwrap()
                .to_string(),
            "1.9.0"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn gemini_desktop_requires_its_bundle_identifier() {
        let temp =
            crate::skills::installer::TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let bundle = temp.path().join("Gemini.app");
        std::fs::create_dir_all(bundle.join("Contents")).unwrap();
        let plist = bundle.join("Contents/Info.plist");
        for (id, detected) in [
            ("com.google.GeminiMacOS", true),
            ("com.google.Chrome.app.gemini", false),
        ] {
            std::fs::write(
                &plist,
                format!(
                    r#"<plist version="1.0"><dict><key>CFBundleIdentifier</key><string>{id}</string></dict></plist>"#
                ),
            )
            .unwrap();
            assert_eq!(
                super::app_path_matches_agent("gemini-desktop", &bundle),
                detected
            );
        }
    }

    #[tokio::test]
    async fn package_output_continues_after_non_utf8_bytes() {
        let mut bytes = "开始\r\n\r\nlegacy ".as_bytes().to_vec();
        bytes.extend_from_slice(b"\xff line\r\nfinished\nfinal line");
        let mut received = Vec::new();

        super::forward_package_output(Some(bytes.as_slice()), &mut Vec::new(), |line| {
            received.push(line.to_string());
        })
        .await
        .unwrap();

        assert_eq!(
            received,
            ["开始", "", "legacy � line", "finished", "final line"]
        );
    }

    #[tokio::test]
    async fn package_command_finishes_while_a_helper_still_holds_its_pipes() {
        use std::io::Write;
        use std::time::{Duration, Instant};
        const TEST: &str =
            "agents::programs::tests::package_command_finishes_while_a_helper_still_holds_its_pipes";
        const DIRECTORY: &str = "AGENTBRO_PACKAGE_PIPE_TEST_DIRECTORY";
        const HOLDER: &str = "AGENTBRO_PACKAGE_PIPE_TEST_HOLDER";

        let executable = std::env::current_exe().unwrap();
        if let Some(directory) = std::env::var_os(DIRECTORY).map(std::path::PathBuf::from) {
            let ready = directory.join("holder-ready");
            if std::env::var_os(HOLDER).is_some() {
                std::fs::write(&ready, b"ready").unwrap();
                let deadline = Instant::now() + Duration::from_secs(30);
                while directory.is_dir() && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(25));
                }
                return;
            }

            let mut helper = crate::platform::process::background_command(&executable)
                .args(["--exact", TEST, "--nocapture"])
                .env(HOLDER, "1")
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            while !ready.exists() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(25));
            }
            assert!(ready.exists() && helper.try_wait().unwrap().is_none());
            println!("package stdout before exit");
            eprint!("package stderr without newline");
            std::io::stderr().flush().unwrap();
            std::process::exit(7);
        }

        let directory =
            crate::skills::installer::TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let mut command = super::package_command(&executable, None);
        command
            .args(["--exact", TEST, "--nocapture"])
            .env(DIRECTORY, directory.path());
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let result = tokio::time::timeout(
            Duration::from_secs(12),
            super::run_package_command(
                command,
                |line| stdout.push(line.to_owned()),
                |line| stderr.push(line.to_owned()),
            ),
        )
        .await
        .expect("the package command should finish before its helper releases the pipes")
        .unwrap();
        assert!(directory.path().join("holder-ready").is_file());
        assert_eq!(result.code(), Some(7));
        assert!(stdout
            .iter()
            .any(|line| line == "package stdout before exit"));
        assert_eq!(stderr, ["package stderr without newline"]);
    }

    #[test]
    fn package_output_keeps_recent_lines_and_resets_without_reusing_revisions() {
        let mut output = super::AgentPackageOutput {
            agent_id: "claude-code".into(),
            revision: 0,
            lines: Default::default(),
        };
        let mut event = super::AgentOutputEvent {
            agent_id: "claude-code".into(),
            operation: "update".into(),
            stream: "stdout".into(),
            line: String::new(),
            done: false,
            success: None,
        };
        for index in 0..=super::PACKAGE_OUTPUT_LINES {
            event.line = index.to_string();
            output.record(Some(&event));
        }
        assert_eq!(output.lines.len(), super::PACKAGE_OUTPUT_LINES);
        assert_eq!(output.lines[0].line, "1");
        assert_eq!(output.lines[0].stream, "stdout");

        event.line = format!("{}界", "a".repeat(super::PACKAGE_OUTPUT_LINE_BYTES - 1));
        event.stream = "stderr".into();
        output.record(Some(&event));
        let last = output.lines.back().unwrap();
        assert_eq!(last.stream, "stderr");
        assert_eq!(last.line.len(), super::PACKAGE_OUTPUT_LINE_BYTES - 1);

        let revision = output.revision;
        output.record(None);
        assert!(output.lines.is_empty());
        assert_eq!(output.revision, revision + 1);
    }

    #[test]
    fn agent_package_operation_rejects_overlap_until_the_guard_is_released() {
        let running = super::claim_agent_package_operation().unwrap();
        assert_eq!(
            super::claim_agent_package_operation().unwrap_err(),
            "agent_package_operation_in_progress"
        );
        drop(running);
        assert!(super::claim_agent_package_operation().is_ok());
    }

    #[cfg(any(unix, windows))]
    #[tokio::test]
    async fn qoder_cli_program_shows_the_selected_version() {
        const LAUNCHER: &str = "AGENTBRO_TEST_QODER_CLI_LAUNCHER";
        const TEST: &str = "agents::programs::tests::qoder_cli_program_shows_the_selected_version";

        if let Some(launcher) = std::env::var_os(LAUNCHER) {
            let info = super::info_for_agent_seed(
                super::AgentProgramSeed {
                    id: "qoder-cli".into(),
                    display_name: "Qoder CLI".into(),
                    icon: "qoder".into(),
                    hooks_installed: false,
                },
                true,
            )
            .await;
            assert_eq!(
                info.binary_path.as_deref().map(Path::new),
                Some(Path::new(&launcher))
            );
            assert_eq!(info.status, super::AgentProgramStatus::Installed);
            assert_eq!(info.installed_version.as_deref(), Some("1.1.98"));
            assert!(info.latest_version.is_none());
            return;
        }

        let temp =
            crate::skills::installer::TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let bin = temp.path().join("Qoder CLI 工具");
        std::fs::create_dir(&bin).unwrap();
        #[cfg(windows)]
        let (launcher, script) = (
            bin.join("qodercli.cmd"),
            "@echo off\r\nif not \"%~1\"==\"--version\" exit /b 1\r\necho 1.1.98\r\n",
        );
        #[cfg(unix)]
        let (launcher, script) = (
            bin.join("qodercli"),
            "#!/bin/sh\n[ \"$1\" = '--version' ] || exit 1\necho '1.1.98'\n",
        );
        std::fs::write(&launcher, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::copy(&launcher, temp.path().join(launcher.file_name().unwrap())).unwrap();
        let path = std::env::var_os("PATH");
        let path = std::env::join_paths(
            std::iter::once(bin).chain(path.as_deref().into_iter().flat_map(std::env::split_paths)),
        )
        .unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST])
            .current_dir(temp.path())
            .env("PATH", path)
            .env(LAUNCHER, launcher)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[cfg(any(unix, windows))]
    #[tokio::test]
    async fn desktop_codex_version_runs_the_selected_launcher() {
        let temporary =
            crate::skills::installer::TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        #[cfg(windows)]
        let (launcher, script) = (
            temporary.path().join("bundled codex.cmd"),
            "@echo off\r\nif not \"%~1\"==\"--version\" exit /b 1\r\necho codex-cli 0.169.1\r\n",
        );
        #[cfg(unix)]
        let (launcher, script) = (
            temporary.path().join("bundled codex"),
            "#!/bin/sh\n[ \"$1\" = '--version' ] || exit 1\necho 'codex-cli 0.169.1'\n",
        );
        std::fs::write(&launcher, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        assert_eq!(
            super::desktop_codex_version(&launcher).await.as_deref(),
            Some("0.169.1")
        );
    }

    #[test]
    fn native_claude_windows_launcher_is_distinct_from_npm_shims() {
        let home = Path::new("test-user");
        assert!(super::is_native_claude_launcher(
            &home.join(".local/bin/CLAUDE.EXE"),
            home,
            RuntimePlatform::Windows,
        ));
        assert!(!super::is_native_claude_launcher(
            &home.join("AppData/Roaming/npm/claude.cmd"),
            home,
            RuntimePlatform::Windows,
        ));
    }

    #[cfg(unix)]
    #[test]
    fn native_claude_unix_launcher_resolves_to_the_native_version_store() {
        let temporary =
            crate::skills::installer::TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let home = temporary.path();
        let local_bin = home.join(".local/bin");
        let version_directory = home.join(".local/share/claude/versions");
        let launcher = local_bin.join("claude");
        let installed_version = version_directory.join("2.1.89");
        let npm = home.join("npm-global/claude");
        for directory in [&local_bin, &version_directory, npm.parent().unwrap()] {
            std::fs::create_dir_all(directory).unwrap();
        }
        std::fs::write(&installed_version, "").unwrap();
        std::fs::write(&npm, "").unwrap();
        std::os::unix::fs::symlink(&installed_version, &launcher).unwrap();

        assert!(super::is_native_claude_launcher(
            &launcher,
            home,
            RuntimePlatform::Macos
        ));
        assert!(!super::is_native_claude_launcher(
            &npm,
            home,
            RuntimePlatform::Macos
        ));
        std::fs::remove_file(&launcher).unwrap();
        std::os::unix::fs::symlink(&npm, &launcher).unwrap();
        assert!(!super::is_native_claude_launcher(
            &launcher,
            home,
            RuntimePlatform::Macos
        ));
    }

    #[cfg(any(unix, windows))]
    #[tokio::test]
    async fn native_claude_version_and_update_use_the_selected_launcher() {
        let temporary =
            crate::skills::installer::TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        #[cfg(windows)]
        let (launcher, script) = (
            temporary.path().join("native launcher.cmd"),
            concat!(
                "@echo off\r\n",
                "if \"%~1\"==\"--version\" (\r\n",
                "    echo 2.1.89 ^(Claude Code^)\r\n",
                "    exit /b 0\r\n",
                ")\r\n",
                "if not \"%~1\"==\"update\" exit /b 1\r\n",
                "echo native updater ran\r\n",
            ),
        );
        #[cfg(unix)]
        let (launcher, script) = (
            temporary.path().join("native launcher"),
            "#!/bin/sh\ncase \"$1\" in\n--version) echo '2.1.89 (Claude Code)' ;;\nupdate) echo 'native updater ran' ;;\n*) exit 1 ;;\nesac\n",
        );
        std::fs::write(&launcher, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        assert_eq!(
            super::native_claude_version(&launcher).await.as_deref(),
            Some("2.1.89")
        );
        assert!(super::native_claude_package_command(&launcher, "install").is_err());
        assert!(super::native_claude_package_command(&launcher, "uninstall").is_err());
        let output = super::native_claude_package_command(&launcher, "update")
            .unwrap()
            .output()
            .await
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "native updater ran"
        );
    }

    #[test]
    fn desktop_fallback_paths_do_not_replace_windows_cli_launchers() {
        use super::{default_app_path_for_display, metadata_for, RuntimePlatform};

        let cli = metadata_for("claude-code").unwrap();
        assert!(
            default_app_path_for_display("claude-code", &cli, RuntimePlatform::Windows).is_none()
        );

        let desktop = metadata_for("cursor").unwrap();
        assert!(
            default_app_path_for_display("cursor", &desktop, RuntimePlatform::Windows).is_some()
        );
    }

    #[test]
    fn npm_updates_require_a_higher_semver_precedence() {
        for (installed, latest, expected) in [
            (Some("1.2.3-rc.9"), Some("1.2.3"), true),
            (Some("1.2.3"), Some("1.2.3-rc.9"), false),
            (Some("1.2.3-beta.9"), Some("1.2.3-beta.10"), true),
            (Some("2.0.0-beta.1"), Some("1.9.9"), false),
            (Some("1.2.3+build.1"), Some("1.2.3+build.2"), false),
            (Some("v1.2.3"), Some("1.2.4"), true),
            (Some("1.2.3"), Some("1.2.3"), false),
            (Some("unknown"), Some("1.2.3"), false),
            (Some("1.2.3"), Some("latest"), false),
            (None, Some("1.2.3"), false),
        ] {
            assert_eq!(
                super::versions_indicate_update(installed, latest),
                expected,
                "{installed:?} vs. {latest:?}",
            );
        }
    }

    #[test]
    fn windows_app_version_prefers_product_and_falls_back_to_file() {
        let product = (3 << 16 | 2, 8 << 16 | 4);
        let file = (1 << 16, 2 << 16 | 3);

        assert_eq!(
            super::format_windows_app_version(product, file).as_deref(),
            Some("3.2.8.4")
        );
        assert_eq!(
            super::format_windows_app_version((0, 0), file).as_deref(),
            Some("1.0.2.3")
        );
        assert!(super::format_windows_app_version((0, 0), (0, 0)).is_none());
    }

    use super::*;
    use crate::skills::v2::agent_meta;

    #[test]
    fn standalone_desktop_agents_have_distinct_macos_bundles() {
        for (id, path) in [
            ("qwenwork-cn", "/Applications/QwenWorkCN.app"),
            ("deepseek-harness", "/Applications/DeepSeek Harness.app"),
            ("workbuddy", "/Applications/WorkBuddy.app"),
            ("workbuddy-ai", "/Applications/WorkBuddy AI.app"),
        ] {
            let meta = metadata_for(id).unwrap();
            let candidates = app_path_candidates_for_platform(id, &meta, RuntimePlatform::Macos);
            assert_eq!(meta.kind, AgentProgramKind::App);
            assert!(candidates.iter().any(|candidate| candidate == path));
            assert!(agent_paths::known_agent_ids().contains(&id));
        }
    }

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
    fn npm_agents_expose_update_metadata() {
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
    fn desktop_trash_uninstall_is_available_only_on_macos() {
        let kiro = metadata_for("kiro").expect("kiro metadata");
        assert_eq!(
            kiro.uninstall_command,
            cfg!(target_os = "macos").then_some(APP_TRASH_UNINSTALL_COMMAND)
        );

        let cline = metadata_for("cline").expect("cline metadata");
        assert_eq!(cline.uninstall_command, None);
    }

    #[test]
    fn windows_qoder_desktop_detection_keeps_the_ide_and_cli_separate() {
        let fixture = crate::skills::installer::TempInstallDir::create_in(&std::env::temp_dir())
            .expect("temporary installations");
        let app = fixture.path().join("Qoder");
        let old_ide = fixture.path().join("Legacy Qoder");
        let ide = fixture.path().join("Qoder IDE");
        let cli = fixture.path().join("bin");
        for directory in [&app, &old_ide, &ide, &cli] {
            std::fs::create_dir_all(directory).unwrap();
        }
        std::fs::create_dir_all(app.join("resources")).unwrap();
        std::fs::create_dir_all(old_ide.join("resources/app")).unwrap();
        std::fs::write(app.join("resources/app.asar"), "").unwrap();
        std::fs::write(old_ide.join("resources/app/product.json"), "{}").unwrap();
        let classify = super::super::qoder::windows_qoder_app_source;
        assert_eq!(classify(&app.join("Qoder.exe")), Some("qoder-desktop"));
        assert_eq!(classify(&old_ide.join("Qoder.exe")), Some("qoder"));
        assert_eq!(classify(&ide.join("Qoder IDE.exe")), Some("qoder"));
        assert_eq!(classify(&cli.join("qoder.exe")), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_qoder_detection_uses_the_bundle_identifier() {
        let fixture = crate::skills::installer::TempInstallDir::create_in(&std::env::temp_dir())
            .expect("temporary installation");
        let app = fixture.path().join("Qoder.app");
        std::fs::create_dir_all(app.join("Contents")).unwrap();
        let info = app.join("Contents/Info.plist");
        let bundle = |identifier| {
            format!(
                r#"<plist version="1.0"><dict><key>CFBundleIdentifier</key><string>{identifier}</string></dict></plist>"#
            )
        };
        std::fs::write(&info, bundle("com.qoder.app")).unwrap();
        assert!(super::app_path_matches_agent("qoder-desktop", &app));
        assert!(!super::app_path_matches_agent("qoder", &app));
        std::fs::write(&info, bundle("com.qoder.ide")).unwrap();
        assert!(super::app_path_matches_agent("qoder", &app));
        assert!(!super::app_path_matches_agent("qoder-desktop", &app));
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

    #[cfg(any(unix, windows))]
    #[tokio::test]
    async fn windows_aider_uninstall_reuses_the_detected_python_launcher() {
        let directory =
            std::env::temp_dir().join(format!("agentbro aider {}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        #[cfg(windows)]
        let (launcher, script) = (
            directory.join("python launcher.cmd"),
            concat!(
                "@echo off\r\n",
                "if not \"%PYTHON_MANAGER_AUTOMATIC_INSTALL%\"==\"false\" exit /b 1\r\n",
                "if \"%*\"==\"-m pip show aider-chat\" exit /b 0\r\n",
                "if not \"%*\"==\"-m pip uninstall -y aider-chat\" exit /b 1\r\n",
                "echo removed aider-chat\r\n",
            ),
        );
        #[cfg(unix)]
        let (launcher, script) = (
            directory.join("python launcher"),
            r#"#!/bin/sh
[ "$PYTHON_MANAGER_AUTOMATIC_INSTALL" = false ] || exit 1
case "$*" in
    "-m pip show aider-chat") exit 0 ;;
    "-m pip uninstall -y aider-chat") echo "removed aider-chat" ;;
    *) exit 1 ;;
esac
"#,
        );
        std::fs::write(&launcher, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        for binary in ["python", "py", "python3"] {
            let (line, mut command) =
                super::aider_uninstall_command(RuntimePlatform::Windows, |name| {
                    (name == binary).then(|| launcher.clone())
                })
                .await
                .unwrap();
            assert_eq!(line, format!("{binary} -m pip uninstall -y aider-chat"));
            assert_eq!(command.as_std().get_program(), launcher.as_os_str());
            let result = command.output().await.unwrap();
            assert!(result.status.success());
            assert_eq!(
                String::from_utf8_lossy(&result.stdout).trim(),
                "removed aider-chat"
            );
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn npm_operations_and_versions_use_the_detected_toolchain() {
        use std::os::unix::fs::PermissionsExt;

        const CHILD_PROBE: &str = "AGENTBRO_TEST_NPM_CHILD_PROBE";
        if let Some(prefix) = std::env::var_os(CHILD_PROBE).map(PathBuf::from) {
            let meta = metadata_for("claude-code").unwrap();
            let info = super::info_for_agent_seed(
                super::AgentProgramSeed {
                    id: "claude-code".to_string(),
                    display_name: "Claude Code".to_string(),
                    icon: "claude".to_string(),
                    hooks_installed: false,
                },
                true,
            )
            .await;
            assert_eq!(
                (info.installed_version, info.latest_version),
                (Some("2.0.0".to_string()), Some("3.0.0".to_string()))
            );
            assert_eq!(
                info.binary_path.as_deref(),
                prefix.join("bin/claude").to_str()
            );
            let installation = meta
                .npm_installation(Some(&prefix.join("bin/claude")))
                .unwrap();
            let update_command = npm_update_command_line("claude-code", &meta, &installation)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                update_command,
                "npm install -g @anthropic-ai/claude-code@3.0.0"
            );
            for registry_version in ["1.0.0", "2.0.0+registry", "2.0.0-rc.1"] {
                std::fs::write(prefix.join("registry-version"), registry_version).unwrap();
                assert!(npm_update_command_line("claude-code", &meta, &installation)
                    .await
                    .unwrap()
                    .is_none());
            }
            for (command, prefix_arg) in [
                (meta.install_command.unwrap(), None),
                (update_command.as_str(), Some(prefix.as_path())),
                (meta.uninstall_command.unwrap(), Some(prefix.as_path())),
            ] {
                let output = super::package_operation_command(command, prefix_arg)
                    .output()
                    .await
                    .unwrap();
                assert!(output.status.success(), "{command}");
                assert_eq!(
                    String::from_utf8_lossy(&output.stdout).trim(),
                    if prefix_arg.is_some() {
                        format!("selected {command} --prefix {}", prefix.display())
                    } else {
                        format!("selected {command}")
                    }
                );
            }
            return;
        }

        let home = std::env::temp_dir().join(format!("agentbro-npm-{}", uuid::Uuid::new_v4()));
        let launcher = home.join("agent launchers");
        let login_launcher = home.join("login tools");
        let toolchain = home.join(".nvm/versions/node/v22.0.0/bin");
        let prefix = home.join("selected prefix");
        let package = prefix.join("lib/node_modules/@anthropic-ai/claude-code");
        std::fs::create_dir_all(&package).unwrap();
        std::fs::create_dir_all(prefix.join("bin")).unwrap();
        std::fs::write(
            package.join("package.json"),
            r#"{"name":"@anthropic-ai/claude-code","version":"2.0.0","bin":{"claude":"cli.js"}}"#,
        )
        .unwrap();
        std::fs::write(package.join("cli.js"), "#!/bin/sh\necho 2.0.0\n").unwrap();
        std::fs::set_permissions(
            package.join("cli.js"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        std::os::unix::fs::symlink(package.join("cli.js"), prefix.join("bin/claude")).unwrap();
        std::fs::create_dir_all(&launcher).unwrap();
        std::fs::create_dir_all(&login_launcher).unwrap();
        std::fs::create_dir_all(&toolchain).unwrap();
        std::fs::write(
            home.join(".profile"),
            r#"export PATH="$HOME/login tools:$PATH""#,
        )
        .unwrap();
        for (path, contents) in [
            (
                launcher.join("npm"),
                "#!/usr/bin/env agentbro-npm-test-node\n",
            ),
            (
                login_launcher.join("npm"),
                "#!/bin/sh\nprintf '%s\\n' 'login npm'\n",
            ),
            (
                toolchain.join("agentbro-npm-test-node"),
                r#"#!/bin/sh
case "$2" in
  list)
    [ "$7" = "--no-update-notifier" ] || exit 1
    printf '%s\n' '{"dependencies":{"@anthropic-ai/claude-code":{"version":"9.0.0"}}}'
    ;;
  view)
    [ "$3" = "@anthropic-ai/claude-code@latest" ] || exit 1
    [ "$6 $7 $8" = "--no-update-notifier --fetch-retries=0 --fetch-timeout=4000" ] || exit 1
    [ "$9" = "--prefix" ] && [ "${10}" = "$AGENTBRO_TEST_NPM_CHILD_PROBE" ] || exit 1
    if [ -f "${10}/registry-version" ]; then
        cat "${10}/registry-version"
    else
        printf '%s\n' '3.0.0'
    fi
    ;;
  install|uninstall) shift; printf 'selected npm %s\n' "$*" ;;
  *) exit 1 ;;
esac
"#,
            ),
        ] {
            std::fs::write(&path, contents).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = std::env::join_paths([
            prefix.join("bin").as_path(),
            launcher.as_path(),
            Path::new("/usr/bin"),
            Path::new("/bin"),
        ])
        .unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .current_dir(&home)
            .env(CHILD_PROBE, prefix.canonicalize().unwrap())
            .env("npm_config_prefix", home.join("default prefix"))
            .env("HOME", &home)
            .env("SHELL", "/bin/sh")
            .env("PATH", path)
            .args([
                "--exact",
                "agents::programs::tests::npm_operations_and_versions_use_the_detected_toolchain",
                "--nocapture",
            ])
            .output()
            .unwrap();
        std::fs::remove_dir_all(home).unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("running 1 test"), "{stdout}");
        assert!(
            output.status.success(),
            "{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn timed_out_npm_probe_stops_its_process() {
        let pid_file =
            std::env::temp_dir().join(format!("agentbro-npm-pid-{}", uuid::Uuid::new_v4()));
        let mut npm = tokio::process::Command::new("/bin/sh");
        npm.args(["-c", r#"printf '%s' "$$" > "$1"; exec /bin/sleep 3"#, "npm"])
            .arg(&pid_file);

        assert!(
            super::package_probe_output(&mut npm, tokio::time::Duration::from_millis(500))
                .await
                .is_err()
        );
        let pid = std::fs::read_to_string(&pid_file).expect("npm probe process started");
        std::fs::remove_file(&pid_file).unwrap();
        let stopped = tokio::time::timeout(tokio::time::Duration::from_secs(1), async {
            loop {
                let status = tokio::process::Command::new("/bin/kill")
                    .args(["-0", &pid])
                    .stderr(std::process::Stdio::null())
                    .status()
                    .await
                    .unwrap();
                if !status.success() {
                    return;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
            }
        })
        .await;
        assert!(stopped.is_ok(), "the timed-out npm probe was still running");
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
            .any(|path| Path::new(path).ends_with(".local/share/applications/cursor.desktop")));
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
    fn windows_qwenwork_candidates_include_the_packaged_launcher() {
        let qwenwork = metadata_for("qwenwork-cn").expect("QwenWork metadata");
        let paths =
            app_path_candidates_for_platform("qwenwork-cn", &qwenwork, RuntimePlatform::Windows);

        for executable in ["Launcher.exe", "QwenWorkCN.exe"] {
            let suffix = format!("\\Programs\\QwenWorkCN\\{executable}");
            assert!(paths.iter().any(|path| path.ends_with(&suffix)));
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_registration_names_match_only_the_desktop_agent() {
        let names = |agent| {
            super::windows_registration_names(agent, &metadata_for(agent).expect("Agent metadata"))
        };

        assert_eq!(names("cursor"), ["Cursor.exe"]);
        assert!(names("cursor-cli").is_empty());
        assert!(names("cline").is_empty());
        assert!(names("qwenwork-cn").contains(&"QwenWorkCN.exe".to_string()));
        assert!(!names("qwenwork-cn").contains(&"Launcher.exe".to_string()));
        assert_eq!(names("easyclaw-v2"), ["EasyClaw V2.exe"]);
        assert!(!names("codebuddycn")
            .iter()
            .any(|name| name.eq_ignore_ascii_case("CodeBuddy.exe")));
    }

    #[test]
    fn windows_workbuddy_launchers_match_the_two_installer_editions() {
        for (agent, name) in [("workbuddy", "WorkBuddy"), ("workbuddy-ai", "WorkBuddyAI")] {
            let meta = metadata_for(agent).expect("WorkBuddy metadata");
            let paths = app_path_candidates_for_platform(agent, &meta, RuntimePlatform::Windows);
            assert!(paths
                .iter()
                .any(|path| path.ends_with(&format!(r"\Programs\{name}\{name}.exe"))));
            let other = if agent == "workbuddy" {
                "WorkBuddyAI.exe"
            } else {
                "WorkBuddy.exe"
            };
            assert!(!paths.iter().any(|path| path.ends_with(other)));
        }
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
