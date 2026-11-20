use crate::agents::{codex, hook_manager, profiles};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::process::{Output, Stdio};
use std::time::Duration;
use tokio::io::AsyncWriteExt;

const READ_CONFIG: &str = r#"
set -eu
target=$(realpath -m -- "$HOME/$1")
if [ -f "$target" ]; then
  printf 'present\n'
  cat -- "$target"
elif [ ! -e "$target" ]; then
  printf 'missing\n'
else
  exit 66
fi
"#;

const WRITE_CONFIG: &str = r#"
set -eu
umask 077
target=$(realpath -m -- "$HOME/$1")
current_hash() {
  if [ -f "$target" ]; then
    hash=$(sha256sum < "$target")
    printf '%s' "${hash%% *}"
  elif [ ! -e "$target" ]; then
    printf 'missing'
  else
    return 66
  fi
}
mkdir -p -- "$(dirname -- "$target")"
temp=$(mktemp -- "$target.agentbro.XXXXXX")
trap 'rm -f -- "$temp"' EXIT
cat > "$temp"
if [ -f "$target" ]; then
  chmod --reference="$target" "$temp"
fi
if [ "$(current_hash)" != "$2" ]; then
  printf 'The WSL settings changed while AgentBro was editing them. Try again.\n' >&2
  exit 75
fi
mv -f -- "$temp" "$target"
"#;

static EDIT_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WslHookStatus {
    agent_id: &'static str,
    installed: bool,
    error: Option<String>,
}

fn executable() -> Option<PathBuf> {
    if !cfg!(target_os = "windows") {
        return None;
    }
    let executable = PathBuf::from(std::env::var_os("SystemRoot")?)
        .join("System32")
        .join("wsl.exe");
    executable.is_file().then_some(executable)
}

async fn run(args: &[&str], input: Option<&[u8]>) -> Result<Output, String> {
    let program = executable().ok_or("WSL is not available on this computer")?;
    let mut command = super::process::background_tokio_command(program);
    command
        .args(args)
        .kill_on_drop(true)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    tokio::time::timeout(Duration::from_secs(20), async {
        if let (Some(mut stdin), Some(input)) = (child.stdin.take(), input) {
            stdin.write_all(input).await?;
        }
        child.wait_with_output().await
    })
    .await
    .map_err(|_| "WSL did not respond in time".to_string())?
    .map_err(|error| error.to_string())
}

fn decode_wsl_output(bytes: &[u8]) -> String {
    let bom = bytes.strip_prefix(&[0xff, 0xfe]);
    let (units, _) = bom.unwrap_or(bytes).as_chunks::<2>();
    if bom.is_some() || units.iter().any(|unit| unit[1] == 0) {
        let words: Vec<_> = units.iter().copied().map(u16::from_le_bytes).collect();
        return String::from_utf16_lossy(&words);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn valid_distribution(distribution: &str) -> bool {
    !distribution.is_empty()
        && !distribution.starts_with('-')
        && distribution.len() <= 256
        && !distribution.chars().any(char::is_control)
}

#[tauri::command]
pub async fn list_wsl_distributions() -> Result<Vec<String>, String> {
    if executable().is_none() {
        return Ok(Vec::new());
    }
    let output = run(&["--list", "--quiet"], None).await?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    Ok(decode_wsl_output(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|name| valid_distribution(name))
        .map(ToOwned::to_owned)
        .collect())
}

fn successful(output: Output) -> Result<Vec<u8>, String> {
    if output.status.success() {
        return Ok(output.stdout);
    }
    let error = decode_wsl_output(&output.stderr);
    if error.trim().is_empty() {
        Err(format!("The WSL command failed ({})", output.status))
    } else {
        Err(error.trim().to_string())
    }
}

async fn shell(
    distribution: &str,
    script: &str,
    arguments: &[&str],
    input: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    if !valid_distribution(distribution) {
        return Err("Invalid WSL distribution name".to_string());
    }
    let mut args = vec![
        "--distribution",
        distribution,
        "--exec",
        "/bin/sh",
        "-c",
        script,
        "agentbro",
    ];
    args.extend(arguments);
    successful(run(&args, input).await?)
}

async fn read_config(distribution: &str, path: &str) -> Result<Option<Vec<u8>>, String> {
    let output = shell(distribution, READ_CONFIG, &[path], None).await?;
    if output == b"missing\n" {
        return Ok(None);
    }
    output
        .strip_prefix(b"present\n")
        .map(|content| Some(content.to_vec()))
        .ok_or_else(|| "Could not read the WSL Hook settings".to_string())
}

async fn write_config(
    distribution: &str,
    path: &str,
    previous: Option<&[u8]>,
    next: &[u8],
) -> Result<(), String> {
    if previous == Some(next) {
        return Ok(());
    }
    let hash = previous.map_or_else(
        || "missing".to_string(),
        |content| format!("{:x}", Sha256::digest(content)),
    );
    shell(distribution, WRITE_CONFIG, &[path, &hash], Some(next)).await?;
    Ok(())
}

fn parse_settings(content: Option<&[u8]>) -> Result<Value, String> {
    content.map_or_else(
        || Ok(serde_json::json!({})),
        |content| serde_json::from_slice(content).map_err(|error| error.to_string()),
    )
}

fn marker(distribution: &str, agent: &str) -> String {
    format!(
        " --wsl-distro {} --source {agent} ",
        hook_manager::shell_quote(distribution)
    )
}

fn bridge_command(distribution: &str, agent: &str, bridge: &str, port: u16) -> String {
    format!(
        "if [ \"${{WSL_DISTRO_NAME-}}\" = {} ]; then {}{}--hook-port {port}; fi # AgentBro WSL",
        hook_manager::shell_quote(distribution),
        hook_manager::shell_quote(bridge),
        marker(distribution, agent),
    )
}

#[tauri::command]
pub async fn get_wsl_hook_status(distribution: String) -> Result<Vec<WslHookStatus>, String> {
    let mut status = Vec::new();
    for profile in [profiles::claude_code_profile(), profiles::codex_profile()] {
        let result = read_config(&distribution, profile.configuration_path)
            .await
            .and_then(|content| parse_settings(content.as_deref()));
        status.push(WslHookStatus {
            agent_id: profile.id,
            installed: result.as_ref().is_ok_and(|settings| {
                profiles::wsl_hooks_installed(settings, &marker(&distribution, profile.id))
            }),
            error: result.err(),
        });
    }
    Ok(status)
}

#[tauri::command]
pub async fn set_wsl_hook(
    distribution: String,
    agent_id: String,
    install: bool,
) -> Result<(), String> {
    let profile = match agent_id.as_str() {
        "claude-code" => profiles::claude_code_profile(),
        "codex" => profiles::codex_profile(),
        _ => return Err("This Agent does not have a WSL Hook profile".to_string()),
    };
    let _guard = EDIT_LOCK.lock().await;
    let previous = read_config(&distribution, profile.configuration_path).await?;
    let mut settings = parse_settings(previous.as_deref())?;
    let marker = marker(&distribution, profile.id);
    let command = if install {
        let bridge = hook_manager::ensure_bridge_binary().map_err(|error| error.to_string())?;
        let path = bridge
            .to_str()
            .ok_or("The Windows bridge path is not valid Unicode")?;
        let output = run(
            &[
                "--distribution",
                &distribution,
                "--exec",
                "wslpath",
                "-u",
                path,
            ],
            None,
        )
        .await?;
        let linux_path =
            String::from_utf8(successful(output)?).map_err(|error| error.to_string())?;
        let linux_path = linux_path.trim_end_matches(['\r', '\n']);
        if !linux_path.starts_with('/') {
            return Err("WSL could not translate the Windows bridge path".to_string());
        }
        let output = run(
            &[
                "--distribution",
                &distribution,
                "--exec",
                linux_path,
                "--source",
                profile.id,
            ],
            None,
        )
        .await?;
        successful(output)?;
        Some(bridge_command(
            &distribution,
            profile.id,
            linux_path,
            crate::hook_endpoint::current().tcp_port,
        ))
    } else if profiles::wsl_hooks_installed(&settings, &marker) {
        None
    } else {
        return Ok(());
    };
    profiles::edit_wsl_hooks(&mut settings, &profile, &marker, command.as_deref())?;
    let content = serde_json::to_vec_pretty(&settings).map_err(|error| error.to_string())?;
    if let (true, Some(path)) = (install, profile.activation_path) {
        let config = read_config(&distribution, path).await?;
        let original = std::str::from_utf8(config.as_deref().unwrap_or_default())
            .map_err(|error| error.to_string())?;
        let enabled = codex::ensure_codex_hooks_feature(original);
        write_config(&distribution, path, config.as_deref(), enabled.as_bytes()).await?;
    }
    write_config(
        &distribution,
        profile.configuration_path,
        previous.as_deref(),
        &content,
    )
    .await
}

#[cfg(target_os = "windows")]
pub(crate) fn open_workspace(distribution: &str, cwd: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    if !valid_distribution(distribution) {
        return Err("Invalid WSL distribution name".to_string());
    }
    let program = executable().ok_or("WSL is not available on this computer")?;
    let directory = if cwd.starts_with('/') { cwd } else { "~" };
    std::process::Command::new(program)
        .creation_flags(0x0000_0010)
        .args(["--distribution", distribution, "--cd", directory])
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decodes_windows_and_linux_wsl_output() {
        let text = "Ubuntu\r\n開発\r\n";
        let utf16: Vec<_> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(decode_wsl_output(&utf16), text);
        let bom: Vec<_> = [0xff, 0xfe].into_iter().chain(utf16).collect();
        assert_eq!(decode_wsl_output(&bom), text);
        assert_eq!(decode_wsl_output(text.as_bytes()), text);
        assert!(!valid_distribution("--user"));
        assert!(valid_distribution("Ubuntu 開発"));
    }

    #[test]
    fn edits_only_the_matching_wsl_hooks() {
        let profile = profiles::claude_code_profile();
        let first = marker("Ubuntu", profile.id);
        let second = marker("Debian", profile.id);
        let native = r#"C:/Users/me/.agentbro/bin/agentbro-bridge.exe --source claude-code"#;
        let mut settings = json!({
            "extra": true,
            "hooks": {
                "SessionStart": [{"hooks": [
                    {"type": "command", "command": native},
                    {"type": "command", "command": "echo user"},
                ]}]
            }
        });
        let original = settings.clone();
        for (name, marker) in [("Ubuntu", &first), ("Debian", &second)] {
            let command = bridge_command(
                name,
                profile.id,
                "/mnt/c/User Name/agentbro-bridge.exe",
                12345,
            );
            profiles::edit_wsl_hooks(&mut settings, &profile, marker, Some(&command)).unwrap();
            assert!(profiles::wsl_hooks_installed(&settings, marker));
        }
        let both = settings.clone();
        let command = bridge_command(
            "Ubuntu",
            profile.id,
            "/mnt/c/User Name/agentbro-bridge.exe",
            12345,
        );
        profiles::edit_wsl_hooks(&mut settings, &profile, &first, Some(&command)).unwrap();
        for event in profile.events {
            assert_eq!(
                settings["hooks"][event.name].as_array().unwrap().len(),
                both["hooks"][event.name].as_array().unwrap().len()
            );
        }
        profiles::edit_wsl_hooks(&mut settings, &profile, &first, None).unwrap();
        assert!(!profiles::wsl_hooks_installed(&settings, &first));
        assert!(profiles::wsl_hooks_installed(&settings, &second));
        profiles::edit_wsl_hooks(&mut settings, &profile, &second, None).unwrap();
        assert_eq!(settings, original);
    }

    #[cfg(unix)]
    #[test]
    fn wsl_command_checks_the_distribution_and_preserves_arguments() {
        let dir = std::env::temp_dir().join(format!("agentbro wsl-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let bridge = dir.join("agentbro-bridge.exe");
        std::fs::write(&bridge, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bridge, std::fs::Permissions::from_mode(0o700)).unwrap();
        let distribution = "Ubuntu '開発'";
        let command = bridge_command(distribution, "codex", bridge.to_str().unwrap(), 17894);
        let run = |distribution: &str| {
            std::process::Command::new("/bin/sh")
                .args(["-c", &command])
                .env("WSL_DISTRO_NAME", distribution)
                .output()
                .unwrap()
        };
        let output = run(distribution);
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            [
                "--wsl-distro",
                distribution,
                "--source",
                "codex",
                "--hook-port",
                "17894"
            ]
        );
        let output = run("Debian");
        assert!(output.status.success());
        assert!(output.stdout.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
