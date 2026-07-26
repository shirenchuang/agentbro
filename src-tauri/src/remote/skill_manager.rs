use super::installer::{base64_encode, run_ssh, run_ssh_with_input};
use super::manager::RemoteHost;
use serde_json::Value;
use std::path::{Path, PathBuf};

const RESPONSE_MARKER: &str = "__AGENTBRO_REMOTE_SKILL_MANAGER__";
const REMOTE_RUNNER_COMMAND: &str = "\"${SHELL:-/bin/bash}\" -lc 'python3 -'";

pub async fn invoke(host: &RemoteHost, command: &str, mut args: Value) -> Result<Value, String> {
    let staged = stage_local_skill_source(host, command, &mut args).await?;
    let result = invoke_remote(host, command, args).await;
    if let Some(staged) = staged {
        let cleanup = format!("rm -rf \"$HOME/.agentbro/uploads/{}\"", staged.upload_id);
        let _ = run_ssh(host, &cleanup, 15).await;
    }
    result
}

async fn invoke_remote(host: &RemoteHost, command: &str, args: Value) -> Result<Value, String> {
    let script = render_script(command, args)?;
    let result = run_ssh_with_input(
        host,
        REMOTE_RUNNER_COMMAND,
        script.into_bytes(),
        command_timeout(command),
    )
    .await;
    if result.exit_code != 0 {
        let detail = if result.stderr.trim().is_empty() {
            result.stdout.trim()
        } else {
            result.stderr.trim()
        };
        return Err(detail.chars().take(1600).collect());
    }
    let payload = result
        .stdout
        .lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix(RESPONSE_MARKER))
        .ok_or_else(|| "Remote Skill Manager returned no response".to_string())?;
    serde_json::from_str(payload).map_err(|error| format!("Decode remote Skill response: {error}"))
}

fn render_script(command: &str, args: Value) -> Result<String, String> {
    let payload = serde_json::json!({
        "command": command,
        "args": args,
    });
    let payload = serde_json::to_vec(&payload).map_err(|error| error.to_string())?;
    Ok(include_str!("skill_manager.py")
        .replace("__AGENTBRO_REQUEST_B64__", &base64_encode(&payload)))
}

struct StagedLocalSource {
    upload_id: String,
}

async fn stage_local_skill_source(
    host: &RemoteHost,
    command: &str,
    args: &mut Value,
) -> Result<Option<StagedLocalSource>, String> {
    if !matches!(
        command,
        "preview_add_center_skill" | "execute_add_center_skill"
    ) {
        return Ok(None);
    }
    let Some(input) = args.get_mut("input").and_then(Value::as_object_mut) else {
        return Ok(None);
    };
    if input.get("sourceType").and_then(Value::as_str) != Some("local_folder") {
        return Ok(None);
    }
    if input.get("sourceLocation").and_then(Value::as_str) == Some("remote") {
        return Ok(None);
    }
    let Some(source) = input
        .get("sourcePath")
        .and_then(Value::as_str)
        .map(PathBuf::from)
    else {
        return Ok(None);
    };
    if !source.exists() {
        return Ok(None);
    }
    let file_name = source
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Local Skill source has no file name".to_string())?
        .to_string();
    let parent = source
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let archive = tokio::process::Command::new("tar")
        .args(["-czf", "-", "-C"])
        .arg(parent)
        .arg(&file_name)
        .output()
        .await
        .map_err(|error| format!("Archive local Skill source: {error}"))?;
    if !archive.status.success() {
        return Err(format!(
            "Archive local Skill source: {}",
            String::from_utf8_lossy(&archive.stderr).trim()
        ));
    }
    let upload_id = uuid::Uuid::new_v4().to_string();
    let remote_dir = format!("$HOME/.agentbro/uploads/{upload_id}");
    let upload_command = format!("mkdir -p \"{remote_dir}\" && tar -xzf - -C \"{remote_dir}\"");
    let uploaded = run_ssh_with_input(host, &upload_command, archive.stdout, 120).await;
    if uploaded.exit_code != 0 {
        let detail = if uploaded.stderr.trim().is_empty() {
            uploaded.stdout.trim()
        } else {
            uploaded.stderr.trim()
        };
        return Err(format!("Upload local Skill source: {detail}"));
    }
    input.insert(
        "sourcePath".to_string(),
        Value::String(format!("~/.agentbro/uploads/{upload_id}/{file_name}")),
    );
    input.insert("sourceUri".to_string(), Value::Null);
    input.insert("importMode".to_string(), Value::String("copy".to_string()));
    Ok(Some(StagedLocalSource { upload_id }))
}

fn command_timeout(command: &str) -> u64 {
    match command {
        "execute_marketplace_skill_batch"
        | "import_github_repo_skills"
        | "execute_add_center_skill"
        | "agent_install"
        | "agent_update"
        | "agent_uninstall" => 360,
        "skill_manager_init"
        | "skill_manager_refresh"
        | "skill_manager_refresh_overview"
        | "run_skill_manager_diagnosis"
        | "test_mcp_server_connection_cmd"
        | "inspect_mcp_server_cmd"
        | "call_mcp_tool_cmd"
        | "get_mcp_prompt_cmd" => 90,
        _ => 35,
    }
}

#[cfg(test)]
mod tests {
    use super::{command_timeout, render_script, REMOTE_RUNNER_COMMAND, RESPONSE_MARKER};
    use serde_json::Value;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::process::{Command, Stdio};

    #[test]
    fn install_commands_receive_a_longer_timeout() {
        assert_eq!(command_timeout("execute_add_center_skill"), 360);
        assert_eq!(command_timeout("skill_manager_overview"), 35);
        assert_eq!(
            REMOTE_RUNNER_COMMAND,
            "\"${SHELL:-/bin/bash}\" -lc 'python3 -'"
        );
    }

    #[test]
    fn remote_script_supports_python_3_7() {
        let script = render_script("skill_manager_overview", serde_json::json!({}))
            .expect("render remote script");
        for unsupported in [
            "return server, *",
            "missing_ok=",
            ".removeprefix(",
            ".removesuffix(",
            "dirs_exist_ok=",
            ".is_relative_to(",
            ".readlink(",
            ":=",
        ] {
            assert!(
                !script.contains(unsupported),
                "remote script uses a post-Python 3.7 feature: {unsupported}"
            );
        }
    }

    #[test]
    fn remote_script_preserves_overview_and_distribution_shapes() {
        let home = std::env::temp_dir().join(format!(
            "agentbro-remote-skill-manager-test-{}",
            uuid::Uuid::new_v4()
        ));
        let skill = home.join(".agentbro/skills/demo");
        std::fs::create_dir_all(&skill).expect("center skill directory");
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: Remote Demo\ndescription: From SSH\n---\n",
        )
        .expect("skill markdown");
        let openclaw_workspace = home.join("custom-openclaw-workspace");
        let openclaw_skill = openclaw_workspace.join("skills/openclaw-demo");
        std::fs::create_dir_all(&openclaw_skill).expect("OpenClaw workspace skill");
        std::fs::write(
            openclaw_skill.join("SKILL.md"),
            "---\nname: OpenClaw Demo\n---\n",
        )
        .expect("OpenClaw skill markdown");
        std::fs::create_dir_all(home.join(".openclaw")).expect("OpenClaw state directory");
        std::fs::write(
            home.join(".openclaw/openclaw.json"),
            serde_json::json!({
                "agents": {
                    "defaults": {
                        "workspace": openclaw_workspace,
                    },
                },
            })
            .to_string(),
        )
        .expect("OpenClaw config");
        let openclaw_binary = home.join(".nvm/versions/node/v22.22.0/bin/openclaw");
        std::fs::create_dir_all(openclaw_binary.parent().expect("OpenClaw binary parent"))
            .expect("OpenClaw binary directory");
        std::fs::write(&openclaw_binary, "#!/bin/sh\necho 2026.7.0\n").expect("OpenClaw binary");
        let mut permissions = std::fs::metadata(&openclaw_binary)
            .expect("OpenClaw binary metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&openclaw_binary, permissions)
            .expect("OpenClaw binary permissions");
        std::fs::create_dir_all(home.join("中文目录")).expect("unicode source directory");

        let overview = run_script(&home, "skill_manager_overview", serde_json::json!({}));
        assert_eq!(overview["metrics"]["centerSkillCount"], 1);
        assert_eq!(overview["skills"][0]["name"], "Remote Demo");
        let openclaw = overview["agents"]
            .as_array()
            .expect("overview agents")
            .iter()
            .find(|agent| agent["id"] == "openclaw")
            .expect("OpenClaw overview agent");
        assert_eq!(openclaw["installed"], true);
        assert_eq!(
            openclaw["skillsDir"],
            openclaw_workspace.join("skills").display().to_string()
        );
        assert_eq!(openclaw["unmanagedSkillCount"], 1);

        let programs = run_script(&home, "agent_list", serde_json::json!({}));
        let openclaw = programs
            .as_array()
            .expect("Agent programs")
            .iter()
            .find(|agent| agent["id"] == "openclaw")
            .expect("OpenClaw program");
        assert_eq!(openclaw["status"], "installed");
        assert_eq!(openclaw["packageManager"], "npm");
        assert_eq!(openclaw["packageName"], "openclaw");
        assert_eq!(openclaw["installedVersion"], "2026.7.0");
        assert_eq!(
            openclaw["binaryPath"],
            openclaw_binary
                .canonicalize()
                .expect("canonical OpenClaw binary")
                .display()
                .to_string()
        );
        assert_eq!(
            openclaw["skillsDir"],
            openclaw_workspace.join("skills").display().to_string()
        );

        let sources = run_script(
            &home,
            "browse_remote_skill_sources",
            serde_json::json!({ "path": home }),
        );
        let entries = sources["entries"].as_array().expect("source entries");
        assert_eq!(entries[0]["name"], ".agentbro");
        assert_eq!(entries[0]["entryType"], "directory");
        assert!(entries.iter().any(|entry| entry["name"] == "中文目录"));

        let linked_source = home.join("sources/linked");
        std::fs::create_dir_all(&linked_source).expect("linked source directory");
        std::fs::write(
            linked_source.join("SKILL.md"),
            "---\nname: Linked Remote Skill\n---\n",
        )
        .expect("linked source markdown");
        let opened = run_script(
            &home,
            "open_skill_path",
            serde_json::json!({ "path": linked_source }),
        );
        assert_eq!(opened["isDirectory"], true);
        assert_eq!(opened["name"], "linked");
        let revealed = run_script(
            &home,
            "reveal_skill_path",
            serde_json::json!({ "path": linked_source.join("SKILL.md") }),
        );
        assert_eq!(revealed["isDirectory"], false);
        assert_eq!(revealed["name"], "SKILL.md");
        assert_eq!(revealed["parentPath"], linked_source.display().to_string());
        let linked_preview = run_script(
            &home,
            "preview_add_center_skill",
            serde_json::json!({
                "input": {
                    "sourcePath": linked_source,
                    "sourceType": "local_folder",
                    "sourceLocation": "remote",
                    "sourceUri": linked_source,
                    "importMode": "link",
                }
            }),
        );
        assert_eq!(linked_preview["candidates"][0]["skillId"], "linked");
        run_script(
            &home,
            "execute_add_center_skill",
            serde_json::json!({
                "input": {
                    "sourcePath": linked_source,
                    "sourceType": "local_folder",
                    "sourceLocation": "remote",
                    "sourceUri": linked_source,
                    "importMode": "link",
                },
                "decisions": [],
            }),
        );
        assert!(home.join(".agentbro/skills/linked").is_symlink());

        let preview = run_script(
            &home,
            "preview_distribute_skill",
            serde_json::json!({
                "skillIds": ["demo"],
                "targetAgents": ["claude-code"],
                "requestedMode": "link",
            }),
        );
        assert_eq!(preview["changes"][0]["action"], "create");

        let executed = run_script(
            &home,
            "execute_distribute_skill",
            serde_json::json!({ "preview": preview }),
        );
        assert_eq!(executed["changes"][0]["actualMode"], "link");
        assert!(home.join(".claude/skills/demo").is_symlink());

        std::fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn remote_response_sanitizes_surrogates() {
        let home = std::env::temp_dir().join(format!(
            "agentbro-remote-skill-manager-surrogate-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&home).expect("remote home");
        let script = render_script("skill_manager_overview", serde_json::json!({}))
            .expect("render remote script")
            .replace(
                "RESPONSE = json_safe(dispatch())",
                r#"RESPONSE = json_safe({"name": "\udcdd"})"#,
            );
        let response = run_rendered_script(&home, &script);
        assert_eq!(response["name"], "\u{fffd}");
        std::fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn concurrent_initialization_keeps_remote_state_atomic() {
        let home = std::env::temp_dir().join(format!(
            "agentbro-remote-skill-manager-concurrent-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&home).expect("remote home");
        let state_dir = home.join(".agentbro");
        let legacy_center = home.join(".agents/skills");
        let legacy_skill = legacy_center.join("legacy");
        std::fs::create_dir_all(&legacy_skill).expect("legacy center skill");
        std::fs::write(legacy_skill.join("SKILL.md"), "# Legacy\n").expect("legacy skill markdown");
        std::fs::create_dir_all(&state_dir).expect("remote state directory");
        std::fs::write(
            state_dir.join("skill-manager-remote.json"),
            serde_json::json!({
                "settings": {
                    "centerPath": legacy_center,
                },
            })
            .to_string(),
        )
        .expect("legacy remote state");
        let handles = (0..8)
            .map(|_| {
                let home = home.clone();
                std::thread::spawn(move || {
                    run_script(&home, "skill_manager_bootstrap", serde_json::json!({}))
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            assert_eq!(handle.join().expect("bootstrap thread"), Value::Null);
        }

        let state: Value = serde_json::from_str(
            &std::fs::read_to_string(state_dir.join("skill-manager-remote.json"))
                .expect("remote state"),
        )
        .expect("valid remote state");
        assert_eq!(
            state["settings"]["centerPath"],
            home.canonicalize()
                .expect("canonical remote home")
                .join(".agentbro/skills")
                .display()
                .to_string()
        );
        assert!(home.join(".agentbro/skills/legacy/SKILL.md").is_file());
        assert!(home.join(".agents/skills/legacy/SKILL.md").is_file());
        assert!(std::fs::read_dir(&state_dir)
            .expect("remote state directory")
            .all(|entry| !entry
                .expect("state entry")
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")));

        std::fs::remove_dir_all(home).expect("remove test home");
    }

    fn run_script(home: &Path, command: &str, args: Value) -> Value {
        let script = render_script(command, args).expect("render remote script");
        run_rendered_script(home, &script)
    }

    fn run_rendered_script(home: &Path, script: &str) -> Value {
        let mut child = Command::new("python3")
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("HOME", home)
            .env("PATH", "/usr/bin:/bin")
            .spawn()
            .expect("start remote script");
        child
            .stdin
            .take()
            .expect("remote script stdin")
            .write_all(script.as_bytes())
            .expect("write remote script");
        let output = child.wait_with_output().expect("run remote script");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        let payload = stdout
            .lines()
            .rev()
            .find_map(|line| line.strip_prefix(RESPONSE_MARKER))
            .expect("marked response");
        serde_json::from_str(payload).expect("response JSON")
    }
}
