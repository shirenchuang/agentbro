// Pet market integration — spawns `npx abpets` to install/uninstall pets from
// the AgentBro pet community. The CLI lives at https://www.npmjs.com/package/abpets
// and writes pets into ~/.agentbro/pets/ and ~/.codex/pets/, which the existing
// pets::discover_all_pets() will pick up on the next refresh.

use crate::agents::executable;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

const SPAWN_TIMEOUT: Duration = Duration::from_secs(300);
const STATUS_CACHE_TTL: Duration = Duration::from_secs(30);
const MAX_SEGMENT_LEN: usize = 64;

static STATUS_CACHE: OnceLock<Mutex<Option<(AbpetsStatus, Instant)>>> = OnceLock::new();

fn status_cache() -> &'static Mutex<Option<(AbpetsStatus, Instant)>> {
    STATUS_CACHE.get_or_init(|| Mutex::new(None))
}

fn resolve_program(program: &str) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        for candidate in windows_program_candidates(program) {
            if let Some(path) = executable::find_binary(&candidate) {
                return path;
            }
        }
    }

    executable::find_binary(program).unwrap_or_else(|| PathBuf::from(program))
}

#[cfg(target_os = "windows")]
fn windows_program_candidates(program: &str) -> Vec<String> {
    let lower = program.to_ascii_lowercase();
    match lower.as_str() {
        "node" => vec!["node.exe".to_string(), "node.cmd".to_string()],
        "npm" | "npx" => vec![format!("{program}.cmd"), format!("{program}.exe")],
        _ if program.contains('.') => Vec::new(),
        _ => vec![format!("{program}.exe"), format!("{program}.cmd")],
    }
}

fn market_path() -> OsString {
    let mut paths = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();

    for program in ["node", "npm", "npx"] {
        if let Some(path) = executable::find_binary(program) {
            if let Some(parent) = path.parent() {
                paths.push(parent.to_path_buf());
            }
        }
    }

    std::env::join_paths(paths).unwrap_or_else(|_| OsString::from(""))
}

fn market_command(program: &str) -> Command {
    let mut command = Command::new(resolve_program(program));
    command.env("PATH", market_path());
    command
}

fn proxy_from_env() -> Option<String> {
    for key in ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"] {
        if let Ok(value) = std::env::var(key) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
async fn proxy_from_login_shell() -> Option<String> {
    None
}

#[cfg(not(target_os = "windows"))]
async fn proxy_from_login_shell() -> Option<String> {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "/bin/zsh".to_string());
    let output = Command::new(shell)
        .args([
            "-lc",
            "printf '%s' \"${HTTPS_PROXY:-${https_proxy:-${HTTP_PROXY:-${http_proxy:-}}}}\"",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

async fn market_http_client() -> reqwest::Client {
    let proxy = match proxy_from_env() {
        Some(value) => Some(value),
        None => proxy_from_login_shell().await,
    };

    let mut builder = reqwest::Client::builder();
    if let Some(proxy_url) = proxy {
        if let Ok(proxy) = reqwest::Proxy::all(&proxy_url) {
            builder = builder.proxy(proxy);
        }
    }
    builder.build().unwrap_or_else(|_| reqwest::Client::new())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AbpetsStatus {
    pub node_available: bool,
    pub abpets_callable: bool,
    pub node_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallLogEvent {
    pub job_id: String,
    pub stream: &'static str,
    pub line: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallDoneEvent {
    pub job_id: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
}

// ── Validation ──────────────────────────────────────────────────────────────
//
// Per-segment allowlist: `^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$`.
// Anything else is rejected before reaching the child process, so a malicious
// handle/slug coming from the wire can't smuggle shell metacharacters or path
// traversal sequences into the spawned argument vector.

fn validate_slug_segment(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err("slug segment is empty".into());
    }
    if s.len() > MAX_SEGMENT_LEN {
        return Err(format!("slug segment too long: {} chars", s.len()));
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphanumeric() {
        return Err(format!("slug segment must start with [A-Za-z0-9]: {}", s));
    }
    for c in chars {
        if !(c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(format!("slug segment contains invalid char '{}': {}", c, s));
        }
    }
    Ok(())
}

// ── Status check ────────────────────────────────────────────────────────────

async fn detect_node_version() -> Option<String> {
    let output = market_command("node")
        .arg("--version")
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

async fn detect_abpets_callable() -> bool {
    // `--no-install` returns non-zero if abpets isn't cached locally — we use
    // that as a hint to suggest a global install, not as a hard gate (install
    // still works via `npx --yes` which auto-fetches).
    market_command("npx")
        .args(["--no-install", "abpets", "--help"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

async fn compute_status() -> AbpetsStatus {
    let node_version = detect_node_version().await;
    let node_available = node_version.is_some();
    let abpets_callable = if node_available {
        detect_abpets_callable().await
    } else {
        false
    };
    AbpetsStatus {
        node_available,
        abpets_callable,
        node_version,
    }
}

fn read_cached_status() -> Option<AbpetsStatus> {
    let guard = status_cache().lock().ok()?;
    let (status, at) = guard.as_ref()?;
    if at.elapsed() < STATUS_CACHE_TTL {
        Some(status.clone())
    } else {
        None
    }
}

fn write_cached_status(status: &AbpetsStatus) {
    if let Ok(mut guard) = status_cache().lock() {
        *guard = Some((status.clone(), Instant::now()));
    }
}

fn invalidate_cached_status() {
    if let Ok(mut guard) = status_cache().lock() {
        *guard = None;
    }
}

#[tauri::command]
pub async fn check_abpets_available(force: Option<bool>) -> Result<AbpetsStatus, String> {
    if !force.unwrap_or(false) {
        if let Some(cached) = read_cached_status() {
            return Ok(cached);
        }
    }
    let status = compute_status().await;
    write_cached_status(&status);
    Ok(status)
}

// ── Spawn + stream ──────────────────────────────────────────────────────────

async fn spawn_and_stream(
    app: AppHandle,
    job_id: String,
    program: &str,
    args: Vec<String>,
) -> Result<(), String> {
    let mut child = market_command(program)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            let err = format!("failed to spawn {}: {}", program, e);
            let _ = app.emit(
                "market:install_done",
                InstallDoneEvent {
                    job_id: job_id.clone(),
                    success: false,
                    exit_code: None,
                    error: Some(err.clone()),
                },
            );
            err
        })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to capture stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "failed to capture stderr".to_string())?;

    let app_out = app.clone();
    let job_out = job_id.clone();
    let stdout_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = app_out.emit(
                "market:install_log",
                InstallLogEvent {
                    job_id: job_out.clone(),
                    stream: "stdout",
                    line,
                },
            );
        }
    });

    let app_err = app.clone();
    let job_err = job_id.clone();
    let stderr_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = app_err.emit(
                "market:install_log",
                InstallLogEvent {
                    job_id: job_err.clone(),
                    stream: "stderr",
                    line,
                },
            );
        }
    });

    let exit_status = match timeout(SPAWN_TIMEOUT, child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => {
            let err = format!("wait failed: {}", e);
            let _ = app.emit(
                "market:install_done",
                InstallDoneEvent {
                    job_id: job_id.clone(),
                    success: false,
                    exit_code: None,
                    error: Some(err.clone()),
                },
            );
            return Err(err);
        }
        Err(_) => {
            let _ = child.start_kill();
            let err = format!("timed out after {}s", SPAWN_TIMEOUT.as_secs());
            let _ = app.emit(
                "market:install_done",
                InstallDoneEvent {
                    job_id: job_id.clone(),
                    success: false,
                    exit_code: None,
                    error: Some(err.clone()),
                },
            );
            return Err(err);
        }
    };

    let _ = stdout_task.await;
    let _ = stderr_task.await;

    let success = exit_status.success();
    let exit_code = exit_status.code();
    let error = if success {
        None
    } else {
        Some(format!("exited with code {:?}", exit_code))
    };
    let _ = app.emit(
        "market:install_done",
        InstallDoneEvent {
            job_id: job_id.clone(),
            success,
            exit_code,
            error: error.clone(),
        },
    );

    if success {
        Ok(())
    } else {
        Err(error.unwrap_or_else(|| "unknown failure".into()))
    }
}

#[tauri::command]
pub async fn install_abpets_globally(app: AppHandle, job_id: String) -> Result<(), String> {
    let args = vec![
        "install".to_string(),
        "-g".to_string(),
        "abpets".to_string(),
    ];
    let result = spawn_and_stream(app, job_id, "npm", args).await;
    invalidate_cached_status();
    result
}

#[tauri::command]
pub async fn install_pet_from_market(
    app: AppHandle,
    job_id: String,
    handle: String,
    slug: String,
) -> Result<(), String> {
    validate_slug_segment(&handle)?;
    validate_slug_segment(&slug)?;
    let target = format!("{}/{}", handle, slug);
    let args = vec![
        "--yes".to_string(),
        "abpets".to_string(),
        "install".to_string(),
        target,
    ];
    spawn_and_stream(app, job_id, "npx", args).await
}

#[tauri::command]
pub async fn uninstall_pet_from_market(
    app: AppHandle,
    job_id: String,
    slug: String,
) -> Result<(), String> {
    validate_slug_segment(&slug)?;
    let args = vec![
        "--yes".to_string(),
        "abpets".to_string(),
        "uninstall".to_string(),
        slug,
    ];
    spawn_and_stream(app, job_id, "npx", args).await
}

// ── HTTP proxy commands ─────────────────────────────────────────────────────
//
// The browser webview is blocked by CORS when calling agentbro.net directly,
// so manifest and download-ping requests go through reqwest on the Rust side,
// using HTTPS_PROXY/HTTP_PROXY from the app process or the user's login shell.

fn resolve_base_url(base_url: Option<String>) -> String {
    base_url
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://api.agentbro.net".to_string())
        .trim_end_matches('/')
        .to_string()
}

#[tauri::command]
pub async fn fetch_market_manifest(base_url: Option<String>) -> Result<String, String> {
    let url = format!("{}/api/manifest", resolve_base_url(base_url));
    let resp = market_http_client()
        .await
        .get(&url)
        .header("Accept", "application/json")
        .header("User-Agent", "AgentBro/desktop")
        .send()
        .await
        .map_err(|e| format!("manifest request failed: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("manifest http {}", resp.status()));
    }
    resp.text()
        .await
        .map_err(|e| format!("manifest read body failed: {}", e))
}

#[tauri::command]
pub async fn ping_market_download(
    base_url: Option<String>,
    handle: String,
    slug: String,
) -> Result<(), String> {
    validate_slug_segment(&handle)?;
    validate_slug_segment(&slug)?;
    let url = format!(
        "{}/api/pets/{}/{}/download",
        resolve_base_url(base_url),
        handle,
        slug,
    );
    let _ = market_http_client()
        .await
        .post(&url)
        .header("Content-Type", "application/json")
        .header("User-Agent", "AgentBro/desktop")
        .body(r#"{"source":"client"}"#)
        .send()
        .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_segments() {
        assert!(validate_slug_segment("luffy").is_ok());
        assert!(validate_slug_segment("user-123").is_ok());
        assert!(validate_slug_segment("My_Pet42").is_ok());
        assert!(validate_slug_segment("a").is_ok());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_npm_shims_prefer_cmd_files() {
        assert_eq!(windows_program_candidates("npx")[0], "npx.cmd");
        assert_eq!(windows_program_candidates("npm")[0], "npm.cmd");
        assert_eq!(windows_program_candidates("node")[0], "node.exe");
    }

    #[test]
    fn rejects_empty_and_oversize() {
        assert!(validate_slug_segment("").is_err());
        assert!(validate_slug_segment(&"a".repeat(MAX_SEGMENT_LEN + 1)).is_err());
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(validate_slug_segment("..").is_err());
        assert!(validate_slug_segment("../etc").is_err());
        assert!(validate_slug_segment("a/b").is_err());
        assert!(validate_slug_segment(".hidden").is_err());
    }

    #[test]
    fn rejects_shell_metacharacters() {
        for bad in [
            "foo;rm",
            "foo bar",
            "foo$bar",
            "`whoami`",
            "$(pwd)",
            "foo|cat",
            "foo&bg",
            "foo>out",
            "foo\\nbar",
        ] {
            assert!(
                validate_slug_segment(bad).is_err(),
                "should reject: {}",
                bad
            );
        }
    }

    #[test]
    fn rejects_leading_non_alphanumeric() {
        assert!(validate_slug_segment("-foo").is_err());
        assert!(validate_slug_segment("_foo").is_err());
        assert!(validate_slug_segment("0-leading-digit-ok").is_ok());
    }
}
