use crate::agents::{hook_manager, AdapterStatus, AgentAdapter};
use crate::commands::AppState;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Stdio;
use tauri::{AppHandle, Emitter, State};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

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

#[tauri::command]
pub async fn agent_list(state: State<'_, AppState>) -> Result<Vec<AgentProgramInfo>, String> {
    Ok(build_agent_list(&state.adapters))
}

#[tauri::command]
pub async fn agent_refresh(state: State<'_, AppState>) -> Result<Vec<AgentProgramInfo>, String> {
    Ok(build_agent_list(&state.adapters))
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
    let _ = state
        .adapters
        .iter()
        .find(|a| a.name() == agent_id)
        .ok_or_else(|| format!("Unknown agent: {agent_id}"))?;
    let meta = metadata_for(&agent_id).ok_or_else(|| format!("Unknown agent: {agent_id}"))?;
    let path = meta
        .app_path
        .ok_or_else(|| format!("No app path for {agent_id}"))?;
    if !Path::new(path).exists() {
        return Err(format!("App is not installed at {path}"));
    }
    open_target(path)
}

fn build_agent_list(adapters: &[std::sync::Arc<dyn AgentAdapter>]) -> Vec<AgentProgramInfo> {
    adapters
        .iter()
        .map(|adapter| info_for_adapter(adapter.as_ref()))
        .collect()
}

fn info_for_adapter(adapter: &dyn AgentAdapter) -> AgentProgramInfo {
    let id = adapter.name().to_string();
    let meta = metadata_for(&id).unwrap_or_else(default_metadata);
    let binary_path = meta.binary.and_then(which);
    let app_path = meta
        .app_path
        .filter(|path| Path::new(path).exists())
        .map(ToString::to_string);
    let installed = binary_path.is_some()
        || app_path.is_some()
        || matches!(
            adapter.status(),
            AdapterStatus::Active | AdapterStatus::Installed | AdapterStatus::Available
        );

    let hooks_installed = adapter
        .hook_config_paths()
        .iter()
        .any(|path| hook_manager::has_agentbro_hooks(path));

    AgentProgramInfo {
        id,
        display_name: adapter.display_name().to_string(),
        icon: adapter.icon().to_string(),
        kind: meta.kind,
        status: if installed {
            AgentProgramStatus::Installed
        } else if meta.install_command.is_some() || meta.download_url.is_some() {
            AgentProgramStatus::NotInstalled
        } else {
            AgentProgramStatus::Unavailable
        },
        package_manager: meta.package_manager.map(ToString::to_string),
        package_name: meta.package_name.map(ToString::to_string),
        installed_version: None,
        latest_version: None,
        binary_path,
        config_dir: meta.config_dir.map(expand_home),
        app_path: meta.app_path.map(ToString::to_string),
        download_url: meta.download_url.map(ToString::to_string),
        install_command: meta.install_command.map(ToString::to_string),
        update_command: meta.update_command.map(ToString::to_string),
        uninstall_command: meta.uninstall_command.map(ToString::to_string),
        hooks_installed,
    }
}

async fn run_agent_command(
    app: AppHandle,
    adapters: &[std::sync::Arc<dyn AgentAdapter>],
    agent_id: &str,
    operation: &str,
) -> Result<(), String> {
    let _ = adapters
        .iter()
        .find(|a| a.name() == agent_id)
        .ok_or_else(|| format!("Unknown agent: {agent_id}"))?;
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

    let mut child = Command::new("sh")
        .arg("-lc")
        .arg(command)
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
    std::process::Command::new("which")
        .arg(binary)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
}

fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).display().to_string();
        }
    }
    path.to_string()
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

fn metadata_for(id: &str) -> Option<ProgramMetadata> {
    let meta = match id {
        "claude-code" => cli(
            "claude",
            "npm",
            "@anthropic-ai/claude-code",
            "npm install -g @anthropic-ai/claude-code",
            "npm update -g @anthropic-ai/claude-code",
            "npm uninstall -g @anthropic-ai/claude-code",
            "~/.claude",
            "https://docs.anthropic.com/en/docs/claude-code",
        ),
        "codex" => cli(
            "codex",
            "npm",
            "@openai/codex",
            "npm install -g @openai/codex",
            "npm update -g @openai/codex",
            "npm uninstall -g @openai/codex",
            "~/.codex",
            "https://developers.openai.com/codex",
        ),
        "gemini" => cli(
            "gemini",
            "npm",
            "@google/gemini-cli",
            "npm install -g @google/gemini-cli",
            "npm update -g @google/gemini-cli",
            "npm uninstall -g @google/gemini-cli",
            "~/.gemini",
            "https://github.com/google-gemini/gemini-cli",
        ),
        "opencode" => cli(
            "opencode",
            "npm",
            "opencode-ai",
            "npm install -g opencode-ai",
            "npm update -g opencode-ai",
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
        "trae" => app(
            "trae",
            "/Applications/Trae.app",
            "~/.trae",
            "https://www.trae.ai",
        ),
        "traecn" => app(
            "trae",
            "/Applications/Trae CN.app",
            "~/.trae",
            "https://www.trae.com.cn",
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
        "codebuddycn" => app(
            "codebuddy",
            "/Applications/CodeBuddy CN.app",
            "~/.codebuddy",
            "https://www.codebuddy.ai",
        ),
        "qwen" => app(
            "qwen",
            "/Applications/Qwen Code.app",
            "~/.qwen",
            "https://qwenlm.github.io",
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
        _ => return None,
    };
    Some(meta)
}

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
