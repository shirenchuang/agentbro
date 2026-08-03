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
        child.wait_with_output().expect("run remote script")
    }
}
