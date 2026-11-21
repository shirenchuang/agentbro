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
    use super::{command_timeout, render_script, REMOTE_RUNNER_COMMAND};

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
}

#[cfg(all(test, unix))]
mod unix_tests {
    use super::{render_script, RESPONSE_MARKER};
    use serde_json::Value;
    use std::io::{Read, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    #[test]
    fn remote_snapshot_refresh_and_write_failure_preserve_complete_files() {
        let home = crate::skills::installer::TempInstallDir::create_in(&std::env::temp_dir())
            .expect("temporary remote home");
        let command = "skill_manager_export_snapshot";
        let args = serde_json::json!({});
        let path = run_script(home.path(), command, args.clone());
        let path = Path::new(path.as_str().expect("snapshot path"));
        let original = std::fs::read(path).expect("original snapshot");
        let mut reader = std::fs::File::open(path).expect("open original snapshot");
        let skill = home.path().join(".agentbro/skills/snapshot-demo");
        write_remote_skill(&skill, "Snapshot 演示", "Remote content");

        run_script(home.path(), command, args.clone());

        let mut observed = Vec::new();
        reader
            .read_to_end(&mut observed)
            .expect("read original file");
        assert_eq!(observed, original);
        let published = std::fs::read(path).expect("published snapshot");
        let snapshot: Value = serde_json::from_slice(&published).expect("valid JSON snapshot");
        assert_eq!(snapshot["skills"][0]["name"], "Snapshot 演示");

        let failure = run_script_error_with_prelude(
            home.path(),
            command,
            args,
            r#"
def fail_snapshot_write(value, output, **options):
    output.write("{")
    raise OSError("fixture snapshot write failed")
json.dump = fail_snapshot_write
"#,
        );
        assert!(failure.contains("fixture snapshot write failed"));
        assert_eq!(std::fs::read(path).unwrap(), published);
        assert!(std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")));
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
    fn remote_one_click_takeover_preserves_center_and_links_matching_agent_copies() {
        let home = std::env::temp_dir().join(format!(
            "agentbro-remote-center-takeover-{}",
            uuid::Uuid::new_v4()
        ));
        let center = home.join(".agentbro/skills/matching");
        let matching = home.join(".codex/skills/matching");
        let unmatched = home.join(".codex/skills/unmatched");
        let other_agent = home.join(".claude/skills/matching");
        write_remote_skill(&center, "Matching", "center\n");
        write_remote_skill(&matching, "Matching", "agent copy\n");
        write_remote_skill(&unmatched, "Unmatched", "only in agent\n");
        write_remote_skill(&other_agent, "Matching", "other agent\n");

        let result = run_script(
            &home,
            "takeover_center_agent_skills",
            serde_json::json!({
                "agentId": "codex",
                "unmanagedIds": ["codex::matching", "codex::unmatched", "claude-code::matching"],
            }),
        );

        assert!(!home.join(".agentbro/skills/unmatched").exists());
        assert_eq!(result["items"][0]["skillId"], "matching");
        assert!(result["items"][1]["error"].is_string());
        assert!(result["items"][2]["error"].is_string());
        assert_eq!(
            std::fs::read_to_string(center.join("SKILL.md")).expect("center document"),
            "---\nname: Matching\n---\ncenter\n"
        );
        assert_eq!(
            std::fs::read_link(&matching).expect("agent link"),
            center.canonicalize().expect("canonical center path")
        );
        assert!(unmatched.is_dir());
        assert!(!unmatched.is_symlink());
        assert!(other_agent.is_dir());
        assert!(!other_agent.is_symlink());
        std::fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn remote_one_click_takeover_restores_agent_copy_when_link_activation_fails() {
        const FAIL_LINK_ACTIVATION: &str = r#"
_agentbro_original_replace = os.replace
def _agentbro_fail_link_activation(source, destination):
    staged = pathlib.Path(source)
    if staged.name == "staged" and staged.is_symlink():
        raise OSError("injected link activation failure")
    return _agentbro_original_replace(source, destination)
os.replace = _agentbro_fail_link_activation
"#;
        let home = std::env::temp_dir().join(format!(
            "agentbro-remote-center-takeover-rollback-{}",
            uuid::Uuid::new_v4()
        ));
        let center = home.join(".agentbro/skills/matching");
        let agent_copy = home.join(".codex/skills/matching");
        write_remote_skill(&center, "Matching", "center\n");
        write_remote_skill(&agent_copy, "Matching", "agent copy\n");
        let script = render_script(
            "takeover_center_agent_skills",
            serde_json::json!({
                "agentId": "codex",
                "unmanagedIds": ["codex::matching"],
            }),
        )
        .expect("render remote takeover")
        .replace(
            "\ntry:\n    RESPONSE = json_safe(dispatch())",
            &format!("\n{FAIL_LINK_ACTIVATION}\ntry:\n    RESPONSE = json_safe(dispatch())"),
        );
        let result = run_rendered_script(&home, &script);

        assert!(result["items"][0]["skillId"].is_null());
        assert!(result["items"][0]["error"]
            .as_str()
            .is_some_and(|error| error.contains("injected link activation failure")));
        assert_eq!(
            std::fs::read_to_string(center.join("SKILL.md")).expect("center document"),
            "---\nname: Matching\n---\ncenter\n"
        );
        assert!(!agent_copy.is_symlink());
        assert_eq!(
            std::fs::read_to_string(agent_copy.join("SKILL.md")).expect("original Agent document"),
            "---\nname: Matching\n---\nagent copy\n"
        );
        std::fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn remote_direct_pack_move_and_revoke_keep_unrelated_targets_and_claims() {
        let home = std::env::temp_dir().join(format!(
            "agentbro-remote-direct-pack-move-{}",
            uuid::Uuid::new_v4()
        ));
        let center = home.join(".agentbro/skills");
        let agent = home.join(".codex/skills");
        for skill in ["selected", "already-present", "absent-peer"] {
            write_remote_skill(&center.join(skill), skill, "center\n");
        }
        for skill in ["selected", "already-present"] {
            write_remote_skill(&agent.join(skill), skill, "direct copy\n");
        }
        for (pack, skills) in [
            ("team", vec!["already-present", "absent-peer"]),
            ("partner", vec!["selected"]),
        ] {
            run_script(
                &home,
                "execute_upsert_skill_pack",
                serde_json::json!({"pack": {"id": pack, "skillIds": skills}}),
            );
        }

        let args = serde_json::json!({"targetId": "codex::selected", "packId": "team"});
        let preview = run_script(&home, "preview_move_direct_skill_to_pack", args.clone());
        assert_eq!(preview["otherMemberCount"], 2);
        let moved = run_script(&home, "execute_move_direct_skill_to_pack", args);
        assert!(
            !agent.join("absent-peer").exists(),
            "moving one Skill must not install the rest of the Pack"
        );
        assert_eq!(preview["distribution"]["changes"], serde_json::json!([]));
        assert_eq!(moved["distribution"]["changes"], serde_json::json!([]));
        let detail = run_script(
            &home,
            "get_agent_detail_v2",
            serde_json::json!({"agentId": "codex"}),
        );
        let targets = detail["skills"].as_array().expect("Agent targets");
        let selected = targets
            .iter()
            .find(|target| target["skillId"] == "selected")
            .expect("selected target");
        let other = targets
            .iter()
            .find(|target| target["skillId"] == "already-present")
            .expect("unrelated target");
        assert_eq!(selected["claims"][0]["packId"], "team");
        assert_eq!(other["claims"][0]["claimType"], "direct");
        assert_eq!(detail["appliedPacks"][0]["packId"], "team");

        let another_move = serde_json::json!({"targetId": "codex::selected", "packId": "partner"});
        let error = run_script_error(&home, "execute_move_direct_skill_to_pack", another_move);
        assert!(error.contains("does not have a direct Skill distribution"));
        let member_args = serde_json::json!({"packId": "team", "skillId": "already-present", "alsoRemoveTargets": true});
        let member_preview =
            run_script(&home, "preview_remove_skill_from_pack", member_args.clone());
        assert_eq!(member_preview["affectedTargets"], serde_json::json!([]));
        run_script(&home, "execute_remove_skill_from_pack", member_args);
        assert!(agent.join("already-present/SKILL.md").is_file());
        std::fs::write(
            agent.join("selected/SKILL.md"),
            "---\nname: selected\n---\ncenter\n",
        )
        .expect("make the selected target reusable");
        run_script(
            &home,
            "execute_apply_skill_pack",
            serde_json::json!({"packId": "partner", "targetAgents": ["codex"], "requestedMode": "copy"}),
        );
        let revoke_args = serde_json::json!({"packId": "team", "agentId": "codex"});
        let revoke_preview = run_script(
            &home,
            "preview_remove_skill_pack_from_agent",
            revoke_args.clone(),
        );
        assert_eq!(revoke_preview["willRemoveTargets"], 0);
        assert_eq!(revoke_preview["willPreserveTargets"], 1);
        let revoked = run_script(&home, "execute_remove_skill_pack_from_agent", revoke_args);
        assert_eq!(revoked["removedClaims"], 1);
        assert_eq!(revoked["preservedTargets"], 1);
        assert!(agent.join("selected/SKILL.md").is_file());
        assert!(agent.join("already-present/SKILL.md").is_file());
        let detail = run_script(
            &home,
            "get_agent_detail_v2",
            serde_json::json!({"agentId": "codex"}),
        );
        let selected = detail["skills"]
            .as_array()
            .unwrap()
            .iter()
            .find(|target| target["skillId"] == "selected")
            .expect("preserved target");
        assert_eq!(selected["claims"].as_array().unwrap().len(), 1);
        assert_eq!(selected["claims"][0]["packId"], "partner");
        std::fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn remote_direct_move_supports_the_builtin_pack_without_touching_other_skills() {
        let home = std::env::temp_dir().join(format!(
            "agentbro-remote-default-pack-move-{}",
            uuid::Uuid::new_v4()
        ));
        for skill in ["selected", "unrelated"] {
            write_remote_skill(
                &home.join(".agentbro/skills").join(skill),
                skill,
                "center\n",
            );
            write_remote_skill(&home.join(".codex/skills").join(skill), skill, "direct\n");
        }
        let args = serde_json::json!({"targetId": "codex::selected", "packId": "default"});
        let moved = run_script(&home, "execute_move_direct_skill_to_pack", args);
        assert_eq!(moved["willAddToPack"], false);
        let detail = run_script(
            &home,
            "get_agent_detail_v2",
            serde_json::json!({"agentId": "codex"}),
        );
        let target = detail["skills"]
            .as_array()
            .unwrap()
            .iter()
            .find(|target| target["skillId"] == "selected")
            .expect("moved target");
        assert_eq!(target["claims"][0]["packId"], "default");
        let revoked = run_script(
            &home,
            "execute_remove_skill_pack_from_agent",
            serde_json::json!({"packId": "default", "agentId": "codex"}),
        );
        assert_eq!(revoked["removedTargets"], 1);
        assert!(!home.join(".codex/skills/selected").exists());
        assert!(home.join(".codex/skills/unrelated/SKILL.md").is_file());
        let state = std::fs::read_to_string(home.join(".agentbro/skill-manager-remote.json"))
            .expect("read persisted state");
        let state: serde_json::Value = serde_json::from_str(&state).expect("parse persisted state");
        assert!(state["packs"].get("default").is_none());
        std::fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn remote_batch_delete_and_composite_refresh_each_build_one_inventory() {
        let home = std::env::temp_dir().join(format!(
            "agentbro-remote-skill-manager-fast-delete-test-{}",
            uuid::Uuid::new_v4()
        ));
        let root = home.join(".codex/skills");
        let ids = (0..42)
            .map(|index| {
                let name = format!("delete-{index:02}");
                write_remote_skill(&root.join(&name), &name, "delete\n");
                format!("codex::{name}")
            })
            .collect::<Vec<_>>();

        let deleted = run_script_with_inventory_count(
            &home,
            "delete_unmanaged_agent_skills",
            serde_json::json!({
                "agentId": "codex",
                "unmanagedIds": ids,
            }),
        );
        assert_eq!(deleted["inventoryCalls"], 1);
        assert_eq!(deleted["result"]["deleted"], 42);
        assert!(deleted["result"]["failures"]
            .as_array()
            .expect("delete failures")
            .is_empty());

        let refreshed = run_script_with_inventory_count(
            &home,
            "refresh_agent_skill_view_v2",
            serde_json::json!({ "agentId": "codex" }),
        );
        assert_eq!(refreshed["inventoryCalls"], 1);
        assert_eq!(refreshed["result"]["agentDetail"]["id"], "codex");
        assert!(refreshed["result"]["overview"]["agents"].is_array());
        assert!(refreshed["result"]["unmanaged"].is_array());

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

    #[test]
    fn remote_shared_mutations_detach_center_links_and_validate_owners() {
        let home = std::env::temp_dir().join(format!(
            "agentbro-remote-shared-detach-test-{}",
            uuid::Uuid::new_v4()
        ));
        let shared_root = home.join(".agents/skills");
        let center = home.join(".agentbro/skills");
        let shared = shared_root.join("same");
        write_remote_skill(&shared, "Same", "same-v1\n");
        std::fs::create_dir_all(&center).expect("center directory");
        std::os::unix::fs::symlink(&shared, center.join("same")).expect("same-name center link");
        std::os::unix::fs::symlink(&shared, center.join("alias")).expect("center alias link");
        std::os::unix::fs::symlink(center.join("alias"), center.join("alias-chain"))
            .expect("center alias chain");

        let detail = run_script(
            &home,
            "get_agent_detail_v2",
            serde_json::json!({ "agentId": "codex" }),
        );
        assert_eq!(detail["inheritsSharedSkills"], true);
        assert!(detail.get("inheritedSkills").is_none());
        let managed = detail["inheritedManagedSkills"]
            .as_array()
            .expect("managed shared skills");
        assert_eq!(managed.len(), 1);
        assert_eq!(managed[0]["id"], "agents::same");
        assert_eq!(managed[0]["agentId"], "agents");
        assert!(managed[0]["claims"]
            .as_array()
            .is_some_and(|value| !value.is_empty()));

        run_script(
            &home,
            "delete_skill_target_distribution",
            serde_json::json!({ "targetId": "agents::same" }),
        );
        assert!(!shared.exists());
        for name in ["same", "alias", "alias-chain"] {
            let detached = center.join(name);
            assert!(detached.is_dir(), "{name}");
            assert!(!detached.is_symlink(), "{name}");
            assert_eq!(
                std::fs::read_to_string(detached.join("SKILL.md")).expect("detached Skill"),
                "---\nname: Same\n---\nsame-v1\n"
            );
        }

        let adopt_source = shared_root.join("adopt-source");
        write_remote_skill(&adopt_source, "Adopt Source", "adopt-v1\n");
        let adopt_alias = center.join("adopt-alias");
        std::os::unix::fs::symlink(&adopt_source, &adopt_alias).expect("adopt alias");
        let adopted = run_script(
            &home,
            "execute_adopt_agent_skill",
            serde_json::json!({
                "agentId": "agents",
                "unmanagedId": "agents::adopt-source",
                "option": "import_cleanup",
                "renamedId": null,
            }),
        );
        assert_eq!(adopted, "adopt-source");
        assert!(!adopt_source.exists());
        assert!(center.join("adopt-source/SKILL.md").is_file());
        assert!(adopt_alias.is_dir());
        assert!(!adopt_alias.is_symlink());

        let blocked = shared_root.join("blocked");
        write_remote_skill(&blocked, "Blocked", "blocked-v1\n");
        std::os::unix::fs::symlink(blocked.join("missing"), blocked.join("dangling"))
            .expect("dangling nested link");
        let blocked_center = center.join("blocked");
        std::os::unix::fs::symlink(&blocked, &blocked_center).expect("blocked center link");
        let blocked_error = run_script_error(
            &home,
            "delete_skill_target_distribution",
            serde_json::json!({ "targetId": "agents::blocked" }),
        );
        assert!(!blocked_error.is_empty());
        assert!(blocked.is_dir());
        assert!(blocked_center.is_symlink());

        let blocked_adopt = shared_root.join("blocked-adopt");
        write_remote_skill(&blocked_adopt, "Blocked Adopt", "blocked-adopt-v1\n");
        std::os::unix::fs::symlink(
            blocked_adopt.join("missing"),
            blocked_adopt.join("dangling"),
        )
        .expect("dangling adopt link");
        let blocked_adopt_alias = center.join("blocked-adopt-alias");
        std::os::unix::fs::symlink(&blocked_adopt, &blocked_adopt_alias)
            .expect("blocked adopt alias");
        let blocked_adopt_error = run_script_error(
            &home,
            "execute_adopt_agent_skill",
            serde_json::json!({
                "agentId": "agents",
                "unmanagedId": "agents::blocked-adopt",
                "option": "import_cleanup",
                "renamedId": null,
            }),
        );
        assert!(!blocked_adopt_error.is_empty());
        assert!(blocked_adopt.is_dir());
        assert!(blocked_adopt_alias.is_symlink());
        assert!(!center.join("blocked-adopt").exists());
        std::fs::remove_file(&blocked_adopt_alias).expect("remove blocked adopt alias");
        std::fs::remove_dir_all(&blocked_adopt).expect("remove blocked adopt source");

        let private = home.join(".claude/skills/private");
        write_remote_skill(&private, "Private", "private-v1\n");
        let owner_error = run_script_error(
            &home,
            "delete_unmanaged_agent_skill",
            serde_json::json!({
                "agentId": "codex",
                "unmanagedId": "claude-code::private",
            }),
        );
        assert!(owner_error.contains("does not belong to this Agent"));
        assert!(private.is_dir());

        let inventory_only = shared_root.join("inventory-only");
        write_remote_skill(&inventory_only, "Inventory Only", "inventory-v1\n");
        let shared_owner_error = run_script_error(
            &home,
            "delete_unmanaged_agent_skill",
            serde_json::json!({
                "agentId": "codex",
                "unmanagedId": "agents::inventory-only",
            }),
        );
        assert!(shared_owner_error.contains("does not belong to this Agent"));
        assert!(inventory_only.is_dir());

        let inventory = run_script(
            &home,
            "list_agent_skill_inventory_v2",
            serde_json::json!({}),
        );
        let virtual_owner = inventory
            .as_array()
            .expect("Agent inventory")
            .iter()
            .find(|agent| agent["agentId"] == "agents")
            .expect("virtual shared owner");
        assert_eq!(virtual_owner["unmanagedCount"], 1);
        let inventory_item = virtual_owner["items"]
            .as_array()
            .expect("virtual owner items")
            .iter()
            .find(|item| item["id"] == "agents::inventory-only")
            .expect("shared unmanaged item");
        assert_eq!(inventory_item["reason"], "shared_agents_directory");

        let overview = run_script(&home, "skill_manager_overview", serde_json::json!({}));
        let visible = overview["agents"].as_array().expect("visible Agents");
        assert!(!visible.iter().any(|agent| agent["id"] == "agents"));
        assert!(!visible.iter().any(|agent| agent["id"] == "zcode"));

        std::fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn remote_shared_writes_reject_symlinked_roots() {
        let home = std::env::temp_dir().join(format!(
            "agentbro-remote-shared-write-guard-test-{}",
            uuid::Uuid::new_v4()
        ));
        let center = home.join(".agentbro/skills/demo");
        write_remote_skill(&center, "Demo", "center-v1\n");
        let external_root = home.join("external-shared");
        let external = external_root.join("demo");
        write_remote_skill(&external, "Demo", "shared-v1\n");
        std::fs::create_dir_all(home.join(".agents")).expect("shared parent");
        std::os::unix::fs::symlink(&external_root, home.join(".agents/skills"))
            .expect("shared root link");

        for (command, args) in [
            (
                "execute_sync_copy_target",
                serde_json::json!({
                    "targetId": "agents::demo",
                    "action": "center_over_agent",
                }),
            ),
            (
                "execute_sync_copy_target",
                serde_json::json!({
                    "targetId": "agents::demo",
                    "action": "agent_over_center",
                }),
            ),
            (
                "execute_fix_diagnosis_issue",
                serde_json::json!({
                    "issueType": "copy_diverged",
                    "entityId": "agents::demo",
                }),
            ),
        ] {
            let error = run_script_error(&home, command, args);
            assert!(error.contains("symbolic-link parent"), "{error}");
            assert_eq!(
                std::fs::read_to_string(external.join("SKILL.md")).expect("shared Skill"),
                "---\nname: Demo\n---\nshared-v1\n"
            );
            assert_eq!(
                std::fs::read_to_string(center.join("SKILL.md")).expect("center Skill"),
                "---\nname: Demo\n---\ncenter-v1\n"
            );
        }

        std::fs::remove_dir_all(home).expect("remove test home");

        let overlap_home = std::env::temp_dir().join(format!(
            "agentbro-remote-shared-overlap-guard-test-{}",
            uuid::Uuid::new_v4()
        ));
        let overlap_shared = overlap_home.join(".agents/skills/demo");
        write_remote_skill(&overlap_shared, "Demo", "shared-v1\n");
        std::fs::create_dir_all(overlap_home.join(".agentbro")).expect("center parent");
        std::os::unix::fs::symlink(
            overlap_home.join(".agents/skills"),
            overlap_home.join(".agentbro/skills"),
        )
        .expect("overlapping center root");
        let overlap_error = run_script_error(
            &overlap_home,
            "delete_skill_target_distribution",
            serde_json::json!({ "targetId": "agents::demo" }),
        );
        assert!(overlap_error.contains("unsafe center root"));
        assert!(overlap_shared.is_dir());
        std::fs::remove_dir_all(overlap_home).expect("remove overlap test home");
    }

    #[test]
    fn remote_shared_center_over_agent_requires_an_independent_center_skill() {
        let home = std::env::temp_dir().join(format!(
            "agentbro-remote-shared-invalid-center-test-{}",
            uuid::Uuid::new_v4()
        ));
        let shared_root = home.join(".agents/skills");
        let center = home.join(".agentbro/skills");
        std::fs::create_dir_all(&center).expect("center directory");

        let invalid_directory = center.join("invalid-directory");
        std::fs::create_dir_all(&invalid_directory).expect("invalid center directory");
        std::fs::write(invalid_directory.join("sentinel"), "keep-directory\n")
            .expect("invalid center sentinel");
        let regular_file = center.join("regular-file");
        std::fs::write(&regular_file, "keep-file\n").expect("invalid center file");
        let dangling_link = center.join("dangling-link");
        std::os::unix::fs::symlink(center.join("missing"), &dangling_link)
            .expect("dangling center link");

        for skill_id in ["invalid-directory", "regular-file", "dangling-link"] {
            let shared = shared_root.join(skill_id);
            write_remote_skill(&shared, skill_id, "shared-v1\n");
            let preview = run_script(
                &home,
                "preview_adopt_agent_skill",
                serde_json::json!({
                    "agentId": "agents",
                    "unmanagedId": format!("agents::{skill_id}"),
                }),
            );
            assert!(preview["options"]
                .as_array()
                .expect("adoption options")
                .iter()
                .all(|option| option["value"] != "center_over_agent"));
            let error = run_script_error(
                &home,
                "execute_adopt_agent_skill",
                serde_json::json!({
                    "agentId": "agents",
                    "unmanagedId": format!("agents::{skill_id}"),
                    "option": "center_over_agent",
                    "renamedId": null,
                }),
            );
            assert!(error.contains("not allowed"), "{error}");
            assert_eq!(
                std::fs::read_to_string(shared.join("SKILL.md")).expect("shared Skill"),
                format!("---\nname: {skill_id}\n---\nshared-v1\n")
            );
        }

        assert_eq!(
            std::fs::read_to_string(invalid_directory.join("sentinel"))
                .expect("invalid directory preserved"),
            "keep-directory\n"
        );
        assert_eq!(
            std::fs::read_to_string(&regular_file).expect("regular file preserved"),
            "keep-file\n"
        );
        assert!(dangling_link.is_symlink());
        assert_eq!(
            std::fs::read_link(&dangling_link).expect("dangling target"),
            center.join("missing")
        );

        std::fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn remote_agent_copy_reconciliation_preserves_the_destination_on_faults() {
        const FAIL_COPY: &str = r#"
def _agentbro_fail_copy(source, destination, *args, **kwargs):
    pathlib.Path(destination).mkdir(parents=True, exist_ok=True)
    (pathlib.Path(destination) / "partial").write_text("partial")
    raise OSError("injected copy failure")
shutil.copytree = _agentbro_fail_copy
"#;
        const FAIL_ACTIVATION: &str = r#"
_agentbro_original_replace = os.replace
def _agentbro_fail_activation(source, destination):
    if pathlib.Path(source).name == "staged":
        raise OSError("injected activation failure")
    return _agentbro_original_replace(source, destination)
os.replace = _agentbro_fail_activation
"#;
        let home = std::env::temp_dir().join(format!(
            "agentbro-remote-agent-sync-test-{}",
            uuid::Uuid::new_v4()
        ));
        let center = home.join(".agentbro/skills/demo");
        let agent = home.join(".codex/skills/demo");
        write_remote_skill(&center, "Demo", "center version\n");
        write_remote_skill(&agent, "Demo", "agent version\n");
        std::fs::write(agent.join("reference.md"), "agent notes").unwrap();
        std::os::unix::fs::symlink("reference.md", agent.join("alias.md")).unwrap();

        for (command, args) in [
            (
                "execute_sync_copy_target",
                serde_json::json!({"targetId": "codex::demo", "action": "center_over_agent"}),
            ),
            (
                "execute_sync_copy_target",
                serde_json::json!({"targetId": "codex::demo", "action": "agent_over_center"}),
            ),
            (
                "execute_fix_diagnosis_issue",
                serde_json::json!({"issueType": "copy_diverged", "entityId": "codex::demo"}),
            ),
        ] {
            for (fault, message) in [
                (FAIL_COPY, "injected copy failure"),
                (FAIL_ACTIVATION, "injected activation failure"),
            ] {
                let error = run_script_error_with_prelude(&home, command, args.clone(), fault);
                assert!(error.contains(message), "{error}");
                assert_eq!(
                    std::fs::read_to_string(center.join("SKILL.md")).unwrap(),
                    "---\nname: Demo\n---\ncenter version\n"
                );
                assert_eq!(
                    std::fs::read_to_string(agent.join("SKILL.md")).unwrap(),
                    "---\nname: Demo\n---\nagent version\n"
                );
                assert!(recovery_artifacts(center.parent().unwrap()).is_empty());
                assert!(recovery_artifacts(agent.parent().unwrap()).is_empty());
            }
        }
        let result = run_script(
            &home,
            "execute_sync_copy_target",
            serde_json::json!({"targetId": "codex::demo", "action": "agent_over_center"}),
        );
        assert_eq!(result["state"], "ok");
        assert_eq!(
            std::fs::read_link(center.join("alias.md")).unwrap(),
            Path::new("reference.md")
        );
        assert_eq!(
            std::fs::read(center.join("SKILL.md")).unwrap(),
            std::fs::read(agent.join("SKILL.md")).unwrap()
        );
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn remote_shared_copy_replacements_restore_destinations_on_faults() {
        const FAIL_ACTIVATION: &str = r#"
_agentbro_original_replace = os.replace
def _agentbro_fail_activation(source, destination):
    if pathlib.Path(source).name == "staged" and ".shared-skill-replace-" in str(source):
        raise OSError("injected activation failure")
    return _agentbro_original_replace(source, destination)
os.replace = _agentbro_fail_activation
"#;
        const FAIL_STAGING_COPY: &str = r#"
_agentbro_original_copytree = shutil.copytree
def _agentbro_fail_staging_copy(source, destination, *args, **kwargs):
    if pathlib.Path(destination).name == "staged" and ".shared-skill-replace-" in str(destination):
        pathlib.Path(destination).mkdir(parents=True, exist_ok=True)
        (pathlib.Path(destination) / "partial").write_text("partial")
        raise OSError("injected staging copy failure")
    return _agentbro_original_copytree(source, destination, *args, **kwargs)
shutil.copytree = _agentbro_fail_staging_copy
"#;

        let home = std::env::temp_dir().join(format!(
            "agentbro-remote-shared-atomic-copy-test-{}",
            uuid::Uuid::new_v4()
        ));
        let shared_root = home.join(".agents/skills");
        let center_root = home.join(".agentbro/skills");

        let adopt_source = shared_root.join("adopt");
        let adopt_destination = center_root.join("adopt");
        write_remote_skill(&adopt_source, "Adopt", "shared-adopt-v1\n");
        std::fs::create_dir_all(&adopt_destination).expect("invalid adopt destination");
        std::fs::write(adopt_destination.join("sentinel"), "old-adopt\n").expect("adopt sentinel");
        let adopt_error = run_script_error_with_prelude(
            &home,
            "execute_adopt_agent_skill",
            serde_json::json!({
                "agentId": "agents",
                "unmanagedId": "agents::adopt",
                "option": "overwrite_center",
                "renamedId": null,
            }),
            FAIL_ACTIVATION,
        );
        assert!(
            adopt_error.contains("injected activation failure"),
            "{adopt_error}"
        );
        assert_eq!(
            std::fs::read_to_string(adopt_destination.join("sentinel"))
                .expect("old adopt destination"),
            "old-adopt\n"
        );
        assert_eq!(
            std::fs::read_to_string(adopt_source.join("SKILL.md")).expect("adopt source"),
            "---\nname: Adopt\n---\nshared-adopt-v1\n"
        );

        let upload_center = center_root.join("upload");
        let upload_shared = shared_root.join("upload");
        write_remote_skill(&upload_center, "Upload", "old-center-v1\n");
        write_remote_skill(&upload_shared, "Upload", "shared-v2\n");
        let upload_error = run_script_error_with_prelude(
            &home,
            "execute_sync_copy_target",
            serde_json::json!({
                "targetId": "agents::upload",
                "action": "agent_over_center",
            }),
            FAIL_ACTIVATION,
        );
        assert!(
            upload_error.contains("injected activation failure"),
            "{upload_error}"
        );
        assert_eq!(
            std::fs::read_to_string(upload_center.join("SKILL.md")).expect("old center"),
            "---\nname: Upload\n---\nold-center-v1\n"
        );
        assert_eq!(
            std::fs::read_to_string(upload_shared.join("SKILL.md")).expect("upload source"),
            "---\nname: Upload\n---\nshared-v2\n"
        );

        let download_center = center_root.join("download");
        let download_shared = shared_root.join("download");
        write_remote_skill(&download_center, "Download", "center-v2\n");
        write_remote_skill(&download_shared, "Download", "old-shared-v1\n");
        let download_error = run_script_error_with_prelude(
            &home,
            "execute_sync_copy_target",
            serde_json::json!({
                "targetId": "agents::download",
                "action": "center_over_agent",
            }),
            FAIL_ACTIVATION,
        );
        assert!(
            download_error.contains("injected activation failure"),
            "{download_error}"
        );
        assert_eq!(
            std::fs::read_to_string(download_shared.join("SKILL.md"))
                .expect("old shared destination"),
            "---\nname: Download\n---\nold-shared-v1\n"
        );
        assert_eq!(
            std::fs::read_to_string(download_center.join("SKILL.md")).expect("download source"),
            "---\nname: Download\n---\ncenter-v2\n"
        );

        let diagnosis_center = center_root.join("diagnosis");
        let diagnosis_shared = shared_root.join("diagnosis");
        write_remote_skill(&diagnosis_center, "Diagnosis", "center-v2\n");
        write_remote_skill(&diagnosis_shared, "Diagnosis", "old-shared-v1\n");
        let diagnosis_error = run_script_error_with_prelude(
            &home,
            "execute_fix_diagnosis_issue",
            serde_json::json!({
                "issueType": "copy_diverged",
                "entityId": "agents::diagnosis",
            }),
            FAIL_ACTIVATION,
        );
        assert!(
            diagnosis_error.contains("injected activation failure"),
            "{diagnosis_error}"
        );
        assert_eq!(
            std::fs::read_to_string(diagnosis_shared.join("SKILL.md"))
                .expect("old diagnosis destination"),
            "---\nname: Diagnosis\n---\nold-shared-v1\n"
        );
        assert_eq!(
            std::fs::read_to_string(diagnosis_center.join("SKILL.md")).expect("diagnosis source"),
            "---\nname: Diagnosis\n---\ncenter-v2\n"
        );

        for (command, args) in [
            (
                "execute_adopt_agent_skill",
                serde_json::json!({
                    "agentId": "agents",
                    "unmanagedId": "agents::adopt",
                    "option": "overwrite_center",
                    "renamedId": null,
                }),
            ),
            (
                "execute_sync_copy_target",
                serde_json::json!({
                    "targetId": "agents::upload",
                    "action": "agent_over_center",
                }),
            ),
            (
                "execute_sync_copy_target",
                serde_json::json!({
                    "targetId": "agents::download",
                    "action": "center_over_agent",
                }),
            ),
            (
                "execute_fix_diagnosis_issue",
                serde_json::json!({
                    "issueType": "copy_diverged",
                    "entityId": "agents::diagnosis",
                }),
            ),
        ] {
            let copy_error = run_script_error_with_prelude(&home, command, args, FAIL_STAGING_COPY);
            assert!(
                copy_error.contains("injected staging copy failure"),
                "{command}: {copy_error}"
            );
        }
        assert_eq!(
            std::fs::read_to_string(adopt_destination.join("sentinel"))
                .expect("copy-fault adopt destination"),
            "old-adopt\n"
        );
        assert_eq!(
            std::fs::read_to_string(adopt_source.join("SKILL.md"))
                .expect("copy-fault adopt source"),
            "---\nname: Adopt\n---\nshared-adopt-v1\n"
        );
        assert_eq!(
            std::fs::read_to_string(upload_center.join("SKILL.md"))
                .expect("copy-fault upload destination"),
            "---\nname: Upload\n---\nold-center-v1\n"
        );
        assert_eq!(
            std::fs::read_to_string(upload_shared.join("SKILL.md"))
                .expect("copy-fault upload source"),
            "---\nname: Upload\n---\nshared-v2\n"
        );
        assert_eq!(
            std::fs::read_to_string(download_shared.join("SKILL.md"))
                .expect("copy-fault download destination"),
            "---\nname: Download\n---\nold-shared-v1\n"
        );
        assert_eq!(
            std::fs::read_to_string(download_center.join("SKILL.md"))
                .expect("copy-fault download source"),
            "---\nname: Download\n---\ncenter-v2\n"
        );
        assert_eq!(
            std::fs::read_to_string(diagnosis_shared.join("SKILL.md"))
                .expect("copy-fault diagnosis destination"),
            "---\nname: Diagnosis\n---\nold-shared-v1\n"
        );
        assert_eq!(
            std::fs::read_to_string(diagnosis_center.join("SKILL.md"))
                .expect("copy-fault diagnosis source"),
            "---\nname: Diagnosis\n---\ncenter-v2\n"
        );
        assert!(recovery_artifacts(&center_root).is_empty());
        assert!(recovery_artifacts(&shared_root).is_empty());

        std::fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn remote_shared_alias_rollback_failure_retains_recovery_artifacts() {
        const FAIL_ACTIVATION_AND_ROLLBACK: &str = r#"
_agentbro_original_replace = os.replace
def _agentbro_fail_activation_and_rollback(source, destination):
    source_name = pathlib.Path(source).name
    if source_name in ("staged", "backup") and ".shared-skill-replace-" in str(source):
        raise OSError("injected alias replace failure")
    return _agentbro_original_replace(source, destination)
os.replace = _agentbro_fail_activation_and_rollback
"#;

        let home = std::env::temp_dir().join(format!(
            "agentbro-remote-shared-alias-recovery-test-{}",
            uuid::Uuid::new_v4()
        ));
        let source = home.join(".agents/skills/source");
        let center = home.join(".agentbro/skills");
        let alias = center.join("alias");
        write_remote_skill(&source, "Source", "shared-v1\n");
        std::fs::create_dir_all(&center).expect("center directory");
        std::os::unix::fs::symlink(&source, &alias).expect("center alias");

        let error = run_script_error_with_prelude(
            &home,
            "delete_unmanaged_agent_skill",
            serde_json::json!({
                "agentId": "agents",
                "unmanagedId": "agents::source",
            }),
            FAIL_ACTIVATION_AND_ROLLBACK,
        );
        assert!(error.contains("Recovery artifacts retained at"), "{error}");
        assert_eq!(
            std::fs::read_to_string(source.join("SKILL.md")).expect("shared source"),
            "---\nname: Source\n---\nshared-v1\n"
        );
        assert!(!alias.exists());
        assert!(!alias.is_symlink());
        let recoveries = recovery_artifacts(&center);
        assert_eq!(recoveries.len(), 1);
        let recovery = &recoveries[0];
        assert!(recovery.join("backup").is_symlink());
        assert_eq!(
            std::fs::read_to_string(recovery.join("staged/SKILL.md"))
                .expect("staged independent copy"),
            "---\nname: Source\n---\nshared-v1\n"
        );

        std::fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn remote_codex_mcp_inventory_reads_toml_without_returning_secrets() {
        let home = std::env::temp_dir().join(format!(
            "agentbro-remote-codex-mcp-{}",
            uuid::Uuid::new_v4()
        ));
        let codex = home.join(".codex");
        std::fs::create_dir_all(&codex).expect("test Codex home");
        let config = r#"
[mcp_servers."stdio.fixture"]
command = "python3"
args = ["-u", "fixture.py"]
env_vars = ["FIXTURE_ENV", { name = "FIXTURE_TYPED_ENV", source = "local" }]

[mcp_servers."stdio.fixture".env]
API_TOKEN = "stdio-fixture-value"
MODE = "fixture"

[mcp_servers.http_fixture]
url = "https://example.invalid/mcp?token=query-fixture-value"
enabled = false
bearer_token_env_var = "FIXTURE_BEARER"
http_headers = { Authorization = "Bearer http-fixture-value", "X-Static" = "static-fixture-value" }
env_http_headers = { "X-Context" = "FIXTURE_CONTEXT" }
"#;
        let path = codex.join("config.toml");
        std::fs::write(&path, config).expect("Codex MCP fixture config");
        let args = serde_json::json!({"agent": "codex"});
        let has_toml = execute_rendered_script(
            &home,
            "try:\n    import tomllib\nexcept ImportError:\n    import tomli\n",
        )
        .status
        .success();

        if has_toml {
            let inventory = run_script(&home, "list_mcp_inventory_cmd", args.clone());
            let servers = inventory["servers"].as_array().expect("MCP servers");
            let stdio = servers
                .iter()
                .find(|entry| entry["name"] == "stdio.fixture")
                .expect("stdio server");
            assert_eq!(stdio["transport"], "stdio");
            assert_eq!(stdio["args"], serde_json::json!(["-u", "fixture.py"]));
            let http = servers
                .iter()
                .find(|entry| entry["name"] == "http_fixture")
                .expect("HTTP server");
            assert_eq!(http["transport"], "http");
            assert_eq!(http["enabled"], false);
            assert_eq!(http["disabledByAgentbro"], false);
            assert_eq!(http["url"], "https://example.invalid/mcp");
            assert!(stdio["env"]
                .as_array()
                .expect("stdio environment")
                .iter()
                .any(|entry| entry["key"] == "FIXTURE_TYPED_ENV"
                    && entry["secret"] == true
                    && entry["value"].is_null()));
            assert!(http["headers"]
                .as_array()
                .expect("HTTP headers")
                .iter()
                .any(|entry| entry["key"] == "X-Context"
                    && entry["secret"] == true
                    && entry["value"].is_null()));
            assert!(!inventory.to_string().contains("fixture-value"));
            assert_eq!(inventory["capabilities"]["editable"], false);

            let malformed = "[mcp_servers.broken\ncommand = \"fixture-value\"\n";
            std::fs::write(&path, malformed).expect("malformed test configuration");
            let error = run_script_error(&home, "list_mcp_inventory_cmd", args);
            assert!(error.contains("Fix the remote Codex MCP TOML"), "{error}");
            assert!(!error.contains("fixture-value"));
            assert_eq!(
                std::fs::read_to_string(&path).expect("malformed config"),
                malformed
            );
            std::fs::write(&path, config).expect("restore test configuration");
        } else {
            let error = run_script_error(&home, "list_mcp_inventory_cmd", args);
            assert!(error.contains("TOML"), "{error}");
        }
        assert_eq!(std::fs::read_to_string(path).expect("read config"), config);
        std::fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn remote_mcp_mutations_reject_stale_revisions() {
        let home =
            std::env::temp_dir().join(format!("agentbro-remote-mcp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).expect("test home");
        let path = home.join(".claude.json");
        let before = br#"{"mcpServers":{"demo":{"type":"stdio","command":"initial"}}}"#;
        let current = br#"{"mcpServers":{"demo":{"type":"stdio","command":"external"}},"other":{"keep":true}}"#;
        let agent = serde_json::json!({"agent": "claude-code"});
        std::fs::write(&path, before).expect("initial configuration");
        let inventory = run_script(&home, "list_mcp_inventory_cmd", agent.clone());
        let revision = &inventory["revision"];
        std::fs::write(&path, current).expect("external edit");
        let draft = serde_json::json!({"name":"demo","transport":"stdio","command":"draft"});

        for (command, args) in [
            (
                "save_mcp_server_cmd",
                serde_json::json!({"agent":"claude-code","revision":revision,"originalName":"demo","server":draft}),
            ),
            (
                "set_mcp_server_enabled_cmd",
                serde_json::json!({"agent":"claude-code","revision":revision,"serverName":"demo","enabled":false}),
            ),
            (
                "delete_mcp_server_v2_cmd",
                serde_json::json!({"agent":"claude-code","revision":revision,"serverName":"demo"}),
            ),
        ] {
            let error = run_script_error(&home, command, args);
            assert!(error.contains("changed after it was opened"), "{error}");
            assert_eq!(std::fs::read(&path).expect("configuration"), current);
        }
        std::fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn remote_mcp_writes_preserve_malformed_and_unrelated_configuration() {
        let home =
            std::env::temp_dir().join(format!("agentbro-remote-mcp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).expect("test home");
        let path = home.join(".claude.json");
        let agent = serde_json::json!({"agent":"claude-code"});
        let inventory = run_script(&home, "list_mcp_inventory_cmd", agent.clone());
        let draft = serde_json::json!({"name":"demo","transport":"stdio","command":"fixture"});
        let save = |revision: &Value, original: Option<&str>| {
            serde_json::json!({
                "agent": "claude-code",
                "revision": revision,
                "originalName": original,
                "server": draft,
            })
        };
        let missing = save(&inventory["revision"], None);
        let created = run_script(&home, "save_mcp_server_cmd", missing);
        assert_eq!(created["servers"][0]["command"], "fixture");

        for malformed in [
            r#"{"mcpServers":,"keep":{"already":"present"}}"#,
            r#"{"mcpServers":[],"keep":{"already":"present"}}"#,
        ] {
            use sha2::{Digest, Sha256};
            std::fs::write(&path, malformed).expect("malformed configuration");
            let revision = serde_json::json!(format!("{:x}", Sha256::digest(malformed.as_bytes())));
            let error =
                run_script_error(&home, "save_mcp_server_cmd", save(&revision, Some("demo")));
            assert!(error.contains("Fix the remote MCP JSON"), "{error}");
            let error = run_script_error(&home, "list_mcp_inventory_cmd", agent.clone());
            assert!(error.contains("Fix the remote MCP JSON"), "{error}");
            assert_eq!(
                std::fs::read_to_string(&path).expect("configuration"),
                malformed
            );
        }

        let source = br#"{"mcpServers":{"demo":{"type":"stdio","command":"previous","timeout":30}},"other":{"keep":true}}"#;
        std::fs::write(&path, source).expect("current configuration");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640))
            .expect("config permissions");
        let inventory = run_script(&home, "list_mcp_inventory_cmd", agent);
        let edit = save(&inventory["revision"], Some("demo"));
        let error = run_script_error_with_prelude(
            &home,
            "save_mcp_server_cmd",
            edit.clone(),
            "def fail_replace(self, target):\n    raise OSError('injected failure before replace')\npathlib.Path.replace = fail_replace\n",
        );
        assert!(error.contains("injected failure before replace"), "{error}");
        assert_eq!(std::fs::read(&path).expect("configuration"), source);
        assert!(!std::fs::read_dir(&home).expect("test home").any(|entry| {
            entry
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        }));

        let target = home.join("linked.json");
        std::fs::rename(&path, &target).expect("move config");
        std::os::unix::fs::symlink(&target, &path).expect("link config");
        let updated = run_script(&home, "save_mcp_server_cmd", edit);
        assert_eq!(updated["servers"][0]["command"], "fixture");
        assert!(path.is_symlink());
        assert_eq!(
            std::fs::metadata(&target)
                .expect("target")
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
        let disk: Value =
            serde_json::from_slice(&std::fs::read(&target).expect("target configuration"))
                .expect("valid JSON");
        assert_eq!(disk["other"]["keep"], true);
        assert_eq!(disk["mcpServers"]["demo"]["timeout"], 30);
        std::fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn remote_mcp_mutations_from_the_same_revision_are_serialized() {
        let home =
            std::env::temp_dir().join(format!("agentbro-remote-mcp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).expect("test home");
        let inventory = run_script(
            &home,
            "list_mcp_inventory_cmd",
            serde_json::json!({"agent":"claude-code"}),
        );
        let scripts = ["first", "second"].map(|name| {
            render_script(
                "save_mcp_server_cmd",
                serde_json::json!({
                    "agent": "claude-code",
                    "revision": inventory["revision"],
                    "server": {"name":name,"transport":"stdio","command":"fixture"},
                }),
            )
            .expect("remote save script")
        });
        let [first, second] = std::thread::scope(|scope| {
            let jobs = scripts.each_ref().map(|script| {
                let home = &home;
                scope.spawn(move || execute_rendered_script(home, script))
            });
            jobs.map(|job| job.join().expect("remote save output"))
        });
        assert_ne!(first.status.success(), second.status.success());
        let failed = if first.status.success() {
            second
        } else {
            first
        };
        assert!(String::from_utf8_lossy(&failed.stderr).contains("changed after it was opened"));
        let after = run_script(
            &home,
            "list_mcp_inventory_cmd",
            serde_json::json!({"agent":"claude-code"}),
        );
        assert_eq!(after["servers"].as_array().expect("server list").len(), 1);
        std::fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn remote_mcp_inventory_hides_credentials_but_host_connections_receive_them() {
        let home =
            std::env::temp_dir().join(format!("agentbro-remote-mcp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).expect("test home");
        std::fs::write(
            home.join(".claude.json"),
            r#"{"mcpServers":{
                "cli":{"type":"stdio","command":"fixture","env":{"API_TOKEN":"fixture-token","MODE":"test"}},
                "remote":{"type":"http","url":"https://example.invalid/mcp","headers":{"Authorization":"Bearer fixture-value","X-Mode":"test"}}
            }}"#,
        ).expect("fixture configuration");
        let agent = serde_json::json!({"agent":"claude-code"});
        let listed = run_script(&home, "list_mcp_inventory_cmd", agent);
        for (name, field, secret, ordinary, fixture) in [
            ("cli", "env", "API_TOKEN", "MODE", "fixture-token"),
            (
                "remote",
                "headers",
                "Authorization",
                "X-Mode",
                "Bearer fixture-value",
            ),
        ] {
            let server = listed["servers"]
                .as_array()
                .expect("inventory")
                .iter()
                .find(|entry| entry["name"] == name)
                .expect("fixture server");
            let items = server[field].as_array().expect("config values");
            let masked = items
                .iter()
                .find(|item| item["key"] == secret)
                .expect("secret field");
            assert_eq!(masked["secret"], true);
            assert_eq!(masked["configured"], true);
            assert!(masked["value"].is_null());
            let ordinary_item = items
                .iter()
                .find(|item| item["key"] == ordinary)
                .expect("ordinary field");
            assert_eq!(ordinary_item["value"], "test");
            assert!(!listed.to_string().contains(fixture));

            let script = render_script(
                "test_mcp_server_connection_cmd",
                serde_json::json!({"agent":"claude-code","serverName":name}),
            )
            .expect("connection run")
            .replace(
                "\ntry:\n    RESPONSE = json_safe(dispatch())",
                r#"
@contextmanager
def assert_local_values(server):
    if server["transport"] == "stdio":
        values = {item["key"]: item["value"] for item in server["env"]}
        assert values["API_TOKEN"] == "fixture-token"
    else:
        values = {item["key"]: item["value"] for item in server["headers"]}
        assert values["Authorization"] == "Bearer fixture-value"
    yield {"serverInfo": {"name": "fixture"}}, lambda method, params: {"tools": []}
stdio_mcp_session = assert_local_values
http_mcp_session = assert_local_values
try:
    RESPONSE = json_safe(dispatch())"#,
            );
            let response = run_rendered_script(&home, &script);
            assert_eq!(response["success"], true);
            assert!(!response.to_string().contains(fixture));
        }
        std::fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn remote_mcp_inspection_uses_the_advertised_capabilities_of_one_stdio_session() {
        let home = std::env::temp_dir().join(format!(
            "agentbro-remote-mcp-capabilities-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&home).expect("test home");
        let fixture = home.join("mcp_fixture.py");
        std::fs::write(
            &fixture,
            r#"import json
from pathlib import Path
import sys

mode = sys.argv[1]
trace = Path.home() / ("trace-" + mode)
capabilities = {
    "tools": {"tools": {}},
    "resources": {"resources": {}},
    "partial": {"tools": {}, "resources": {}, "prompts": {}},
    "bounded": {"tools": {}, "resources": {}, "prompts": {}},
}[mode]
cursor_token = " page +/% "
for line in sys.stdin:
    request = json.loads(line)
    method = request["method"]
    with trace.open("a") as output:
        print(method, file=output)
    if "id" not in request:
        continue
    response = {"jsonrpc": "2.0", "id": request["id"]}
    cursor = request.get("params", {}).get("cursor")
    if method == "initialize":
        response["result"] = {
            "protocolVersion": "2025-06-18",
            "serverInfo": {"name": mode},
            "capabilities": capabilities,
        }
    elif method == "tools/list" and mode == "bounded":
        offset = int(cursor) if cursor is not None else 0
        response["result"] = {
            "tools": [{"name": "tool-{}".format(index)} for index in range(offset, offset + 260)],
            "nextCursor": str(offset + 260),
        }
    elif method == "resources/list" and mode == "bounded":
        page = int(cursor) if cursor is not None else 0
        response["result"] = {"resources": [], "nextCursor": str(page + 1)}
    elif method == "prompts/list" and mode == "bounded":
        response["result"] = {"prompts": [{"name": "fixture"}], "nextCursor": 42}
    elif method == "tools/list" and "tools" in capabilities:
        response["result"] = {"tools": [{"name": "seen"}] if mode == "partial" else []}
    elif method == "resources/list" and mode == "resources":
        if cursor is None:
            response["result"] = {"resources": [], "nextCursor": cursor_token}
        elif cursor == cursor_token:
            response["result"] = {"resources": [{"name": "fixture", "uri": "file:///fixture"}]}
    elif method == "resources/list" and mode == "partial":
        if cursor is None:
            response["result"] = {
                "resources": [{"name": "first", "uri": "file:///first"}],
                "nextCursor": cursor_token,
            }
        elif cursor == cursor_token:
            response["error"] = {"code": -32000, "message": "Catalog failed"}
    elif method == "prompts/list" and mode == "partial":
        response["result"] = {
            "prompts": [{"name": "initial" if cursor is None else "continued"}],
            "nextCursor": cursor_token,
        }
    else:
        response["error"] = {"code": -32601, "message": "Method not found"}
    print(json.dumps(response), flush=True)
"#,
        )
        .expect("stdio server fixture");
        let names = ["tools", "resources", "partial", "bounded"];
        let servers: serde_json::Map<String, Value> = names
            .iter()
            .map(|name| {
                (
                    (*name).to_string(),
                    serde_json::json!({"command": "python3", "args": [&fixture, name]}),
                )
            })
            .collect();
        std::fs::write(
            home.join(".claude.json"),
            serde_json::json!({"mcpServers": servers}).to_string(),
        )
        .expect("fixture configuration");
        let reports: Vec<_> = names
            .into_iter()
            .map(|name| {
                let script = render_script(
                    "inspect_mcp_server_cmd",
                    serde_json::json!({
                        "agent": "claude-code", "serverName": name,
                        "inspectionId": uuid::Uuid::new_v4().to_string(),
                    }),
                )
                .expect("render inspection");
                let output = execute_rendered_script(&home, &script);
                let trace = std::fs::read_to_string(home.join(format!("trace-{name}")))
                    .expect("fixture trace");
                (name, output, trace)
            })
            .collect();
        std::fs::remove_dir_all(home).expect("remove test home");

        for (name, output, trace) in reports {
            assert!(
                output.status.success(),
                "{name}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let stdout = String::from_utf8_lossy(&output.stdout);
            let payload = stdout
                .lines()
                .find_map(|line| line.strip_prefix(RESPONSE_MARKER))
                .expect("marked response");
            let report: Value = serde_json::from_str(payload).expect("inspection JSON");
            let expected = match name {
                "tools" => vec!["initialize", "notifications/initialized", "tools/list"],
                "resources" => vec![
                    "initialize",
                    "notifications/initialized",
                    "resources/list",
                    "resources/list",
                ],
                "bounded" => [
                    &[
                        "initialize",
                        "notifications/initialized",
                        "tools/list",
                        "tools/list",
                    ][..],
                    &["resources/list"; 10],
                    &["prompts/list"],
                ]
                .concat(),
                _ => vec![
                    "initialize",
                    "notifications/initialized",
                    "tools/list",
                    "resources/list",
                    "resources/list",
                    "prompts/list",
                    "prompts/list",
                ],
            };
            assert_eq!(trace.lines().collect::<Vec<_>>(), expected, "{name}");
            assert_eq!(
                report["status"],
                if matches!(name, "partial" | "bounded") {
                    "partial"
                } else {
                    "connected"
                }
            );
            assert_eq!(report["capabilities"]["tools"], name != "resources");
            assert_eq!(report["capabilities"]["resources"], name != "tools");
            assert_eq!(
                report["capabilities"]["prompts"],
                matches!(name, "partial" | "bounded")
            );
            if name == "resources" {
                assert_eq!(report["resources"][0]["uri"], "file:///fixture");
            }
            if name == "partial" {
                assert_eq!(report["tools"][0]["name"], "seen");
                assert_eq!(report["resources"][0]["uri"], "file:///first");
                assert_eq!(report["prompts"][0]["name"], "initial");
                assert_eq!(report["prompts"][1]["name"], "continued");
                assert!(report["warnings"][0]
                    .as_str()
                    .unwrap_or_default()
                    .contains("Catalog failed"));
                assert!(report["warnings"][1]
                    .as_str()
                    .unwrap_or_default()
                    .contains("repeated"));
                assert!(!report.to_string().contains("page +/%"));
            }
            if name == "bounded" {
                assert_eq!(report["tools"].as_array().expect("tools").len(), 500);
                assert_eq!(report["tools"][499]["name"], "tool-499");
                assert_eq!(report["prompts"][0]["name"], "fixture");
                assert_eq!(
                    report["warnings"],
                    serde_json::json!([
                        "tools/list: Catalog stopped at the 500-item limit",
                        "resources/list: Catalog stopped at the 10-page limit",
                        "prompts/list: Server returned an invalid pagination cursor",
                    ])
                );
            }
        }
    }

    #[test]
    fn remote_mcp_stdio_reads_combined_and_split_messages_despite_server_logs() {
        let home = std::env::temp_dir().join(format!(
            "agentbro-remote-mcp-stdio-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&home).expect("test home");
        let fixture = home.join("stdio_fixture.py");
        std::fs::write(
            &fixture,
            r#"import json
import os
import sys
import time

def frame(value):
    return json.dumps(value, ensure_ascii=False).encode("utf-8") + b"\n"

for line in sys.stdin:
    request = json.loads(line)
    if "id" not in request:
        continue
    method = request["method"]
    response = {"jsonrpc": "2.0", "id": request["id"]}
    if method == "initialize":
        response["result"] = {"capabilities": {"tools": {}, "resources": {}, "logging": {}}}
        os.write(1, frame(response))
    elif method == "tools/list":
        response["result"] = {"tools": [{"name": "fixture"}]}
        notification = {"jsonrpc": "2.0", "method": "notifications/message",
                        "params": {"level": "info", "data": "Catalog ready"}}
        os.write(1, frame(notification) + frame(response))
    elif method == "resources/list":
        response["result"] = {"resources": [{"uri": "file:///fixture", "name": "雪"}]}
        data = frame(response)
        split = data.index("雪".encode("utf-8")) + 1
        os.write(1, data[:split])
        time.sleep(0.05)
        os.write(1, data[split:])
    elif method == "tools/call":
        sys.stderr.write("stdio fixture log\n" * 100000)
        sys.stderr.flush()
        response["result"] = {"content": [{"type": "text", "text": "工具完成"}]}
        os.write(1, frame(response))
"#,
        )
        .expect("stdio server fixture");
        std::fs::write(
            home.join(".claude.json"),
            serde_json::json!({
                "mcpServers": {"fixture": {"command": "python3", "args": [&fixture]}}
            })
            .to_string(),
        )
        .expect("fixture configuration");
        let report = run_script(
            &home,
            "inspect_mcp_server_cmd",
            serde_json::json!({
                "agent": "claude-code", "serverName": "fixture",
                "inspectionId": uuid::Uuid::new_v4().to_string(),
            }),
        );
        let operation = run_script(
            &home,
            "call_mcp_tool_cmd",
            serde_json::json!({
                "agent": "claude-code", "serverName": "fixture", "toolName": "fixture",
                "operationId": uuid::Uuid::new_v4().to_string(),
            }),
        );
        std::fs::remove_dir_all(home).expect("remove test home");

        assert_eq!(report["status"], "connected", "{report}");
        assert_eq!(report["tools"][0]["name"], "fixture");
        assert_eq!(report["resources"][0]["name"], "雪");
        assert_eq!(operation["result"]["content"][0]["text"], "工具完成");
        assert!(!operation.to_string().contains("stdio fixture log"));
    }

    #[test]
    fn remote_mcp_cancellation_stops_the_original_stdio_call() {
        let home = std::env::temp_dir().join(format!(
            "agentbro-remote-mcp-cancel-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&home).expect("test home");
        let fixture = home.join("mcp_fixture.py");
        let started = home.join("started");
        let release = home.join("release");
        std::fs::write(
            &fixture,
            r#"import json
from pathlib import Path
import os
import sys
import time

started, release = map(Path, sys.argv[1:])
for line in sys.stdin:
    request = json.loads(line)
    if request["method"] == "initialize":
        result = {"serverInfo": {"name": "fixture"}}
    elif request["method"] == "tools/call":
        started.write_text(str(os.getpid()))
        deadline = time.monotonic() + 8
        while not release.exists() and time.monotonic() < deadline:
            time.sleep(0.02)
        result = {"content": []}
    else:
        continue
    print(json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": result}), flush=True)
"#,
        )
        .expect("stdio server fixture");
        std::fs::write(
            home.join(".claude.json"),
            serde_json::json!({"mcpServers": {"fixture": {
                "command": "python3",
                "args": [&fixture, &started, &release]
            }}})
            .to_string(),
        )
        .expect("fixture configuration");
        let operation_id = uuid::Uuid::new_v4().to_string();
        let script = render_script(
            "call_mcp_tool_cmd",
            serde_json::json!({
                "agent": "claude-code",
                "serverName": "fixture",
                "operationId": operation_id,
                "toolName": "wait",
                "arguments": {}
            }),
        )
        .expect("render tool call");
        let mut call = start_rendered_script(&home, &script);
        let running = wait_for_fixture(|| started.exists());
        let unrelated = run_script(
            &home,
            "cancel_mcp_operation_cmd",
            serde_json::json!({"operationId": uuid::Uuid::new_v4().to_string()}),
        );
        let still_running = call.try_wait().expect("poll tool call").is_none();
        let cancelled = run_script(
            &home,
            "cancel_mcp_operation_cmd",
            serde_json::json!({"operationId": operation_id}),
        );
        let stopped = wait_for_fixture(|| call.try_wait().expect("poll tool call").is_some());
        std::fs::write(release, "").expect("release fixture");
        let output = call.wait_with_output().expect("finish fixture");
        let pid = std::fs::read_to_string(&started)
            .ok()
            .and_then(|text| text.parse::<libc::pid_t>().ok());
        std::fs::remove_dir_all(home).expect("remove test home");

        assert!(running, "stdio fixture did not start");
        assert!(
            unrelated.is_null() && still_running,
            "unrelated operation was affected"
        );
        assert!(cancelled.is_null());
        assert!(
            stopped,
            "cancel returned while the original tool call was still running"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("MCP operation cancelled"));
        assert!(!output.status.success());
        let pid = pid.expect("fixture process ID");
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1, "fixture was not reaped");
    }

    #[test]
    fn remote_mcp_http_completes_and_closes_the_negotiated_session() {
        let home = std::env::temp_dir().join(format!(
            "agentbro-remote-mcp-http-lifecycle-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&home).expect("test home");
        let (mut fixture, port) = start_http_fixture(
            &home,
            r#"from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import threading
import time

home = Path.home()
versions = {"strict": "2025-06-18", "legacy": "2025-03-26", "failing": "2025-06-18"}
states = {}
resources_requested = threading.Event()

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def reply(self, status, content=None, session_id=None):
        self.send_response(status)
        if session_id:
            self.send_header("Mcp-Session-Id", session_id)
        if content is not None:
            self.send_header("Content-Type", "application/json")
        self.end_headers()
        if content is not None:
            self.wfile.write(json.dumps(content).encode())

    def stream_tools(self, request):
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream; charset=utf-8")
        self.end_headers()
        notice = json.dumps({"jsonrpc": "2.0", "method": "notifications/message",
                             "params": {"level": "info", "data": "Sending tools"}})
        reply = json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": {
            "tools": [{"name": "second", "title": "雪"}],
        }}, indent=2, ensure_ascii=False)
        self.wfile.write(b"\xef\xbb\xbf: AgentBro fixture\r")
        self.wfile.flush()
        time.sleep(0.05)
        body = "\ndata: " + notice + "\n\nevent: message\r"
        body += "".join("data: " + line + "\r" for line in reply.splitlines()) + "\r"
        self.wfile.write(body.encode("utf-8"))
        self.wfile.flush()
        if not resources_requested.wait(3):
            (home / "stream-closed-before-next-request").touch()

    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        name = self.path.lstrip("/")
        method = request["method"]
        if self.headers.get("Authorization") != "Bearer fixture":
            self.reply(401)
            return
        if method == "initialize":
            if name == "strict" and name in states:
                self.reply(400)
                return
            states[name] = "initializing"
            capabilities = {"tools": {}, "resources": {}} if name == "strict" else {"tools": {}}
            self.reply(200, {"jsonrpc": "2.0", "id": request["id"], "result": {
                "protocolVersion": versions[name],
                "serverInfo": {"name": name},
                "capabilities": capabilities,
            }}, "session-" + name)
        elif not self.has_session(name):
            self.reply(400)
        elif method == "notifications/initialized" and states[name] == "initializing" and "id" not in request:
            states[name] = "ready"
            self.reply(202)
        elif method == "tools/list" and states[name] == "ready" and request["id"] == 2 and request["params"] == {}:
            states[name] = "used"
            if name == "failing":
                self.reply(503)
            else:
                result = {"tools": [{"name": "fixture"}]}
                if name == "strict":
                    result["nextCursor"] = " tools/?= "
                self.reply(200, {"jsonrpc": "2.0", "id": request["id"], "result": result})
        elif name == "strict" and method == "tools/list" and states[name] == "used" and request["id"] == 3 and request["params"] == {"cursor": " tools/?= "}:
            states[name] = "listed-tools"
            self.stream_tools(request)
        elif name == "strict" and method == "resources/list" and states[name] == "listed-tools" and request["id"] == 4 and request["params"] == {}:
            resources_requested.set()
            states[name] = "listed-resources"
            self.reply(200, {"jsonrpc": "2.0", "id": request["id"], "result": {"resources": []}})
        else:
            self.reply(400)

    def has_session(self, name):
        return (self.headers.get("MCP-Protocol-Version") == versions[name] and
                self.headers.get("Mcp-Session-Id") == "session-" + name)

    def do_DELETE(self):
        name = self.path.lstrip("/")
        expected = "listed-resources" if name == "strict" else "used"
        if states.get(name) == expected and self.has_session(name) and self.headers.get("Authorization") == "Bearer fixture":
            (home / ("closed-" + name)).touch()
            self.reply(204 if name == "strict" else 405)
        else:
            self.reply(400)

with ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server:
    (home / "http-port").write_text(str(server.server_port) + "\n")
    server.serve_forever()
"#,
        );
        let names = ["strict", "legacy", "failing"];
        let servers: serde_json::Map<String, Value> = names
            .iter()
            .map(|name| {
                ((*name).to_string(), serde_json::json!({
                    "url": format!("http://127.0.0.1:{port}/{name}"),
                    "headers": {"Authorization": "Bearer fixture", "MCP-Protocol-Version": "override-me"},
                }))
            })
            .collect();
        std::fs::write(
            home.join(".claude.json"),
            serde_json::json!({"mcpServers": servers}).to_string(),
        )
        .expect("fixture configuration");
        let results: Vec<_> = names
            .into_iter()
            .map(|name| {
                let result = if name == "strict" {
                    run_script(
                        &home,
                        "inspect_mcp_server_cmd",
                        serde_json::json!({
                            "agent": "claude-code", "serverName": name,
                            "inspectionId": uuid::Uuid::new_v4().to_string(),
                        }),
                    )
                } else {
                    run_script(
                        &home,
                        "test_mcp_server_connection_cmd",
                        serde_json::json!({"agent": "claude-code", "serverName": name}),
                    )
                };
                (name, result, home.join(format!("closed-{name}")).is_file())
            })
            .collect();
        let waited_for_stream_close = home.join("stream-closed-before-next-request").is_file();
        fixture.kill().expect("stop fixture");
        fixture.wait_with_output().expect("wait for fixture");
        std::fs::remove_dir_all(home).expect("remove test home");

        assert!(
            !waited_for_stream_close,
            "client waited for the SSE stream to close"
        );
        for (name, result, tried_to_close) in results {
            if name == "strict" {
                assert_eq!(result["status"], "connected", "{result}");
                assert_eq!(result["capabilities"]["resources"], true);
                assert_eq!(result["tools"][0]["name"], "fixture");
                assert_eq!(result["tools"][1]["name"], "second");
                assert_eq!(result["tools"][1]["title"], "雪");
                assert_eq!(result["resources"], serde_json::json!([]));
            } else if name == "failing" {
                assert_eq!(result["success"], false, "{result}");
                assert!(
                    result["message"]
                        .as_str()
                        .unwrap_or_default()
                        .contains("503"),
                    "{result}"
                );
            } else {
                assert_eq!(result["success"], true, "{result}");
                assert_eq!(result["toolCount"], 1);
            }
            assert!(tried_to_close, "did not terminate the session");
        }
    }

    #[test]
    fn remote_mcp_cancelled_inspection_closes_the_http_connection() {
        let home = std::env::temp_dir().join(format!(
            "agentbro-remote-http-cancel-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&home).expect("test home");
        let (mut http, port) = start_http_fixture(
            &home,
            r#"from http.server import BaseHTTPRequestHandler, HTTPServer
import json
from pathlib import Path
import socket
import time

class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        if request["method"] == "notifications/initialized":
            self.send_response(202)
            self.end_headers()
            return
        result = {"jsonrpc": "2.0", "id": request["id"], "result": {}}
        encoded = json.dumps(result).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        if request["method"] == "initialize":
            self.send_header("Mcp-Session-Id", "fixture-session")
        self.end_headers()
        if request["method"] == "resources/list":
            (Path.home() / "waiting-http").touch()
            self.connection.settimeout(4)
            try:
                if self.connection.recv(1) == b"":
                    (Path.home() / "closed-http").touch()
            except socket.timeout:
                pass
        else:
            self.wfile.write(encoded)

    def do_DELETE(self):
        assert self.headers["Mcp-Session-Id"] == "fixture-session"
        if (Path.home() / "waiting-http").exists():
            (Path.home() / "terminating-http").touch()
            time.sleep(5)
        self.send_response(204)
        self.end_headers()

server = HTTPServer(("127.0.0.1", 0), Handler)
(Path.home() / "http-port").write_text(str(server.server_port) + "\n")
server.serve_forever()
"#,
        );
        std::fs::write(
            home.join(".claude.json"),
            serde_json::json!({"mcpServers": {"fixture": {
                "url": format!("http://127.0.0.1:{port}/mcp")
            }}})
            .to_string(),
        )
        .expect("fixture configuration");
        let inspection_id = uuid::Uuid::new_v4().to_string();
        let script = render_script(
            "inspect_mcp_server_cmd",
            serde_json::json!({
                "agent": "claude-code",
                "serverName": "fixture",
                "inspectionId": inspection_id
            }),
        )
        .expect("render inspection");
        let mut inspection = start_rendered_script(&home, &script);
        let waiting = wait_for_fixture(|| home.join("waiting-http").is_file());
        run_script(
            &home,
            "cancel_mcp_operation_cmd",
            serde_json::json!({"operationId": inspection_id}),
        );
        let still_running = inspection.try_wait().expect("poll inspection").is_none();
        run_script(
            &home,
            "cancel_mcp_inspection_cmd",
            serde_json::json!({"inspectionId": inspection_id}),
        );
        let stopped =
            wait_for_fixture(|| inspection.try_wait().expect("poll inspection").is_some());
        let closed = wait_for_fixture(|| home.join("closed-http").is_file());
        let tried_to_terminate = home.join("terminating-http").is_file();
        http.kill().expect("stop HTTP fixture");
        http.wait_with_output().expect("finish HTTP fixture");
        let output = inspection.wait_with_output().expect("finish inspection");
        std::fs::remove_dir_all(home).expect("remove test home");

        assert!(waiting, "inspection did not reach resources/list");
        assert!(
            still_running,
            "an operation cancellation affected an inspection"
        );
        assert!(stopped, "cancelled inspection was still running");
        assert!(
            closed,
            "cancelled inspection did not close its HTTP connection"
        );
        assert!(
            tried_to_terminate,
            "cancelled inspection did not terminate its session"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("MCP inspection cancelled"));
    }

    #[test]
    fn remote_mcp_cancel_can_arrive_before_the_operation() {
        let home = std::env::temp_dir().join(format!(
            "agentbro-remote-mcp-early-cancel-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&home).expect("test home");
        let operation_id = uuid::Uuid::new_v4().to_string();
        run_script(
            &home,
            "cancel_mcp_operation_cmd",
            serde_json::json!({"operationId": operation_id}),
        );
        let error = run_script_error(
            &home,
            "get_mcp_prompt_cmd",
            serde_json::json!({
                "agent": "claude-code",
                "serverName": "fixture",
                "operationId": operation_id,
                "promptName": "preview"
            }),
        );
        std::fs::remove_dir_all(home).expect("remove test home");
        assert!(error.contains("MCP operation cancelled"));
    }

    #[test]
    fn remote_mcp_edits_resolve_redacted_credentials_before_writing() {
        let home =
            std::env::temp_dir().join(format!("agentbro-remote-mcp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).expect("test home");
        let path = home.join(".claude.json");
        std::fs::write(
            &path,
            r#"{"mcpServers":{"before":{"type":"http","url":"https://example.invalid/mcp","headers":{"Authorization":"Bearer fixture-value"}}}}"#,
        ).expect("fixture configuration");
        let agent = serde_json::json!({"agent":"claude-code"});
        let listed = run_script(&home, "list_mcp_inventory_cmd", agent);
        let save = |revision: &Value, original: &str, headers: Value| {
            serde_json::json!({
                "agent": "claude-code", "revision": revision, "originalName": original,
                "server": {"name":"renamed","transport":"http","url":"https://example.invalid/mcp","headers":headers},
            })
        };
        let preserved = run_script(
            &home,
            "save_mcp_server_cmd",
            save(
                &listed["revision"],
                "before",
                serde_json::json!([
                    {"key":"Authorization","value":null,"secret":true,"configured":true},
                ]),
            ),
        );
        let disk = || {
            serde_json::from_slice::<Value>(&std::fs::read(&path).expect("configuration"))
                .expect("valid JSON")
        };
        assert!(disk()["mcpServers"]["before"].is_null());
        assert_eq!(
            disk()["mcpServers"]["renamed"]["headers"]["Authorization"],
            "Bearer fixture-value"
        );
        assert!(preserved["servers"][0]["headers"][0]["value"].is_null());

        let updated = run_script(
            &home,
            "save_mcp_server_cmd",
            save(
                &preserved["revision"],
                "renamed",
                serde_json::json!([
                    {"key":"Authorization","value":"Bearer replacement","secret":true,"configured":true},
                ]),
            ),
        );
        assert_eq!(
            disk()["mcpServers"]["renamed"]["headers"]["Authorization"],
            "Bearer replacement"
        );
        assert!(updated["servers"][0]["headers"][0]["value"].is_null());

        let before = std::fs::read(&path).expect("updated configuration");
        let error = run_script_error(
            &home,
            "save_mcp_server_cmd",
            save(
                &updated["revision"],
                "renamed",
                serde_json::json!([
                    {"key":"X-API-KEY","value":null,"secret":true,"configured":true},
                ]),
            ),
        );
        assert!(error.contains("No existing sensitive value for X-API-KEY"));
        assert_eq!(
            std::fs::read(&path).expect("unchanged configuration"),
            before
        );
        std::fs::remove_dir_all(home).expect("remove test home");
    }

    fn start_http_fixture(home: &Path, script: &str) -> (Child, u16) {
        let mut server = start_rendered_script(home, script);
        let port_file = home.join("http-port");
        let deadline = Instant::now() + Duration::from_secs(10);
        let failure = loop {
            if let Some(port) = std::fs::read_to_string(&port_file).ok().and_then(|text| {
                text.strip_suffix('\n')?
                    .parse::<std::num::NonZeroU16>()
                    .ok()
            }) {
                return (server, port.get());
            }
            if let Some(status) = server.try_wait().expect("poll HTTP fixture") {
                break format!("exited with {status}");
            }
            if Instant::now() >= deadline {
                let _ = server.kill();
                break "did not publish a valid port in 10 seconds".to_string();
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let output = server.wait_with_output().expect("finish HTTP fixture");
        panic!(
            "HTTP fixture {failure}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    fn wait_for_fixture(mut condition: impl FnMut() -> bool) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if condition() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        false
    }

    fn write_remote_skill(path: &Path, name: &str, body: &str) {
        std::fs::create_dir_all(path).expect("Skill directory");
        std::fs::write(
            path.join("SKILL.md"),
            format!("---\nname: {name}\n---\n{body}"),
        )
        .expect("Skill markdown");
    }

    fn run_script(home: &Path, command: &str, args: Value) -> Value {
        let script = render_script(command, args).expect("render remote script");
        run_rendered_script(home, &script)
    }

    fn run_script_with_inventory_count(home: &Path, command: &str, args: Value) -> Value {
        let script = render_script(command, args)
            .expect("render remote script")
            .replace(
                "try:\n    RESPONSE = json_safe(dispatch())",
                r#"_agentbro_original_inventory = inventory
_agentbro_inventory_calls = 0
def inventory():
    global _agentbro_inventory_calls
    _agentbro_inventory_calls += 1
    return _agentbro_original_inventory()

try:
    _agentbro_result = dispatch()
    RESPONSE = json_safe({
        "result": _agentbro_result,
        "inventoryCalls": _agentbro_inventory_calls,
    })"#,
            );
        run_rendered_script(home, &script)
    }

    fn run_script_error(home: &Path, command: &str, args: Value) -> String {
        let script = render_script(command, args).expect("render remote script");
        let output = execute_rendered_script(home, &script);
        assert!(
            !output.status.success(),
            "remote script unexpectedly succeeded: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        String::from_utf8_lossy(&output.stderr).into_owned()
    }

    fn run_script_error_with_prelude(
        home: &Path,
        command: &str,
        args: Value,
        prelude: &str,
    ) -> String {
        let script = render_script(command, args)
            .expect("render remote script")
            .replace(
                "\ntry:\n    RESPONSE = json_safe(dispatch())",
                &format!("\n{prelude}\ntry:\n    RESPONSE = json_safe(dispatch())"),
            );
        let output = execute_rendered_script(home, &script);
        assert!(
            !output.status.success(),
            "remote script unexpectedly succeeded: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        String::from_utf8_lossy(&output.stderr).into_owned()
    }

    fn recovery_artifacts(root: &Path) -> Vec<std::path::PathBuf> {
        if !root.is_dir() {
            return Vec::new();
        }
        std::fs::read_dir(root)
            .expect("recovery parent")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name().is_some_and(|name| {
                    name.to_string_lossy().starts_with(".shared-skill-replace-")
                })
            })
            .collect()
    }

    fn run_rendered_script(home: &Path, script: &str) -> Value {
        let output = execute_rendered_script(home, script);
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

    fn execute_rendered_script(home: &Path, script: &str) -> std::process::Output {
        start_rendered_script(home, script)
            .wait_with_output()
            .expect("run remote script")
    }

    fn start_rendered_script(home: &Path, script: &str) -> std::process::Child {
        let mut child = Command::new("python3")
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("HOME", home)
            .env("TMPDIR", home)
            .env("PATH", "/usr/bin:/bin")
            .spawn()
            .expect("start remote script");
        child
            .stdin
            .take()
            .expect("remote script stdin")
            .write_all(script.as_bytes())
            .expect("write remote script");
        child
    }
}
