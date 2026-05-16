use super::agent_paths;
use super::{
    GitHubSkillPreview, InstallMode, McpServerConfig, McpValidationResult, PluginInstallRequest,
    TargetConfig,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn install_skill(
    source_path: &str,
    targets: &[TargetConfig],
    mode: &InstallMode,
) -> Result<Vec<String>, String> {
    install_skill_named(source_path, targets, mode, None, None)
}

pub fn install_skill_named(
    source_path: &str,
    targets: &[TargetConfig],
    mode: &InstallMode,
    directory_name: Option<&str>,
    display_name: Option<&str>,
) -> Result<Vec<String>, String> {
    let (src, temp_root) = resolve_install_source(source_path)?;
    if !src.exists() {
        return Err(format!("Source not found: {}", source_path));
    }

    let source_skill_name = src
        .file_name()
        .ok_or("Invalid source path")?
        .to_string_lossy()
        .to_string();
    let skill_name = directory_name
        .map(sanitize_file_name)
        .filter(|name| !name.is_empty())
        .unwrap_or(source_skill_name);
    let installed_ids = display_name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .map(|name| vec![name])
        .unwrap_or_else(|| skill_id_candidates(&src, &skill_name));

    let central_path = if matches!(mode, InstallMode::Symlink) {
        let central_dir = agent_paths::agentbro_skills_dir();
        fs::create_dir_all(&central_dir).map_err(|e| e.to_string())?;
        let dest = central_dir.join(&skill_name);
        copy_recursive(&src, &dest)?;
        if let Some(name) = display_name {
            rewrite_skill_name(&dest, name)?;
        }
        Some(dest)
    } else {
        None
    };

    for target in targets {
        let paths = agent_paths::paths_for_agent(&target.agent);
        let target_dir = paths
            .skill_dirs
            .first()
            .ok_or_else(|| format!("No skill directory for agent: {}", target.agent))?;

        fs::create_dir_all(target_dir).map_err(|e| e.to_string())?;
        let dest = target_dir.join(&skill_name);
        if same_location(&src, &dest) {
            continue;
        }

        match mode {
            InstallMode::Direct => {
                copy_recursive(&src, &dest)?;
                if let Some(name) = display_name {
                    rewrite_skill_name(&dest, name)?;
                }
            }
            InstallMode::Symlink => {
                if let Some(ref central) = central_path {
                    if dest.exists() || dest.symlink_metadata().is_ok() {
                        fs::remove_file(&dest)
                            .or_else(|_| fs::remove_dir_all(&dest))
                            .map_err(|e| e.to_string())?;
                    }
                    #[cfg(unix)]
                    std::os::unix::fs::symlink(central, &dest).map_err(|e| e.to_string())?;
                    #[cfg(not(unix))]
                    copy_recursive(central, &dest)?;
                }
            }
        }
    }

    if let Some(root) = temp_root {
        let _ = fs::remove_dir_all(root);
    }

    Ok(installed_ids)
}

pub fn preview_github_skills(source: &str) -> Result<Vec<GitHubSkillPreview>, String> {
    let normalized = if source.starts_with("github:")
        || source.starts_with("https://github.com/")
        || source.starts_with("http://github.com/")
    {
        source.to_string()
    } else {
        format!("github:{source}")
    };
    let (src, temp_root) = resolve_install_source(&normalized)?;
    let mut previews = Vec::new();
    collect_skill_previews(&src, &src, &mut previews)?;
    if let Some(root) = temp_root {
        let _ = fs::remove_dir_all(root);
    }
    previews.sort_by(|a, b| a.source_path.cmp(&b.source_path));
    Ok(previews)
}

fn resolve_install_source(source: &str) -> Result<(PathBuf, Option<PathBuf>), String> {
    let local = PathBuf::from(source);
    if local.exists() {
        return Ok((local, None));
    }

    if let Some(spec) = source.strip_prefix("github:") {
        return clone_github_spec(spec);
    }

    if source.starts_with("https://github.com/") || source.starts_with("http://github.com/") {
        return clone_github_url(source);
    }

    if source.starts_with("http://") || source.starts_with("https://") {
        if source.ends_with(".zip") {
            return download_zip(source);
        }
        if source.ends_with(".md")
            || source.contains("/raw/")
            || source.contains("raw.githubusercontent.com")
        {
            return download_markdown_skill(source);
        }
        return Err("Only GitHub repository URLs, raw Markdown skills, and .zip skill archives are supported for remote installs".to_string());
    }

    Err(format!("Source not found: {}", source))
}

fn download_markdown_skill(source: &str) -> Result<(PathBuf, Option<PathBuf>), String> {
    let root = temp_install_dir()?;
    let skill_dir = root.join(skill_dir_name_from_url(source));
    fs::create_dir_all(&skill_dir).map_err(|e| e.to_string())?;
    let dest = skill_dir.join("SKILL.md");

    let output = Command::new("curl")
        .args(authenticated_curl_args())
        .arg("-L")
        .arg("--fail")
        .arg(source)
        .arg("-o")
        .arg(&dest)
        .output()
        .map_err(|e| format!("Failed to run curl: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = fs::remove_dir_all(&root);
        return Err(format!("Failed to download {source}: {stderr}"));
    }

    Ok((skill_dir, Some(root)))
}

fn skill_dir_name_from_url(source: &str) -> String {
    let trimmed = source.trim_end_matches('/');
    let mut parts = trimmed
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts
        .last()
        .is_some_and(|part| part.eq_ignore_ascii_case("SKILL.md"))
    {
        parts.pop();
    }
    parts
        .last()
        .map(|part| sanitize_file_name(part.trim_end_matches(".md")))
        .filter(|part| !part.is_empty())
        .unwrap_or_else(|| "downloaded-skill".to_string())
}

fn sanitize_file_name(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if sanitized.is_empty() {
        "downloaded-skill".to_string()
    } else {
        sanitized
    }
}

fn collect_skill_previews(
    root: &Path,
    dir: &Path,
    previews: &mut Vec<GitHubSkillPreview>,
) -> Result<(), String> {
    if previews.len() >= 500 {
        return Ok(());
    }
    let skill_file = dir.join("SKILL.md");
    if skill_file.exists() {
        let frontmatter = parse_frontmatter_from_file(&skill_file);
        let directory_name = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let source_path = dir.strip_prefix(root).unwrap_or(dir).display().to_string();
        previews.push(GitHubSkillPreview {
            source_path,
            name: frontmatter
                .get("name")
                .cloned()
                .unwrap_or_else(|| directory_name.clone()),
            description: frontmatter.get("description").cloned().unwrap_or_default(),
            directory_name,
        });
    }

    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if matches!(
            name.as_str(),
            ".git" | "node_modules" | "target" | "dist" | "build" | ".next"
        ) {
            continue;
        }
        collect_skill_previews(root, &path, previews)?;
    }
    Ok(())
}

fn parse_frontmatter_from_file(path: &Path) -> std::collections::HashMap<String, String> {
    fs::read_to_string(path)
        .ok()
        .map(|content| parse_frontmatter_text(&content))
        .unwrap_or_default()
}

fn parse_frontmatter_text(content: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    if !content.starts_with("---") {
        return map;
    }
    let Some(frontmatter) = content.split("---").nth(1) else {
        return map;
    };
    for line in frontmatter.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if !key.trim().is_empty() && !value.is_empty() {
            map.insert(key.trim().to_string(), value.to_string());
        }
    }
    map
}

pub fn install_plugin(request: &PluginInstallRequest) -> Result<String, String> {
    let (src, temp_root) = resolve_install_source(&request.source)?;
    if !src.exists() {
        return Err(format!("Plugin source not found: {}", request.source));
    }
    let manifest = read_plugin_manifest(&src).ok_or_else(|| {
        "Plugin source must contain .claude-plugin/plugin.json or .codex-plugin/plugin.json"
            .to_string()
    })?;
    let plugin_id = manifest
        .get("name")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| "Plugin manifest missing name".to_string())?;
    let version = manifest
        .get("version")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("local");
    let dest = plugin_install_root(&request.agent)?
        .join("agentbro")
        .join(&plugin_id)
        .join(version);
    copy_recursive(&src, &dest)?;
    super::registry::add_source(&format!("plugin:{plugin_id}"), &request.source)?;
    if let Some(root) = temp_root {
        let _ = fs::remove_dir_all(root);
    }
    Ok(format!("plugin:{plugin_id}"))
}

fn plugin_install_root(agent: &str) -> Result<PathBuf, String> {
    let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
    match agent {
        "claude-code" => Ok(home.join(".claude").join("plugins").join("cache")),
        "codex" => Ok(home.join(".codex").join("plugins").join("cache")),
        _ => Err(format!("Plugin install is not supported for {agent}")),
    }
}

fn read_plugin_manifest(path: &Path) -> Option<serde_json::Value> {
    [".claude-plugin/plugin.json", ".codex-plugin/plugin.json"]
        .iter()
        .map(|relative| path.join(relative))
        .find(|candidate| candidate.exists())
        .and_then(|candidate| fs::read_to_string(candidate).ok())
        .and_then(|content| serde_json::from_str(&content).ok())
}

fn temp_install_dir() -> Result<PathBuf, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let dir = dirs::home_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(".agentbro")
        .join("tmp")
        .join(format!("install-{millis}"));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn clone_github_spec(spec: &str) -> Result<(PathBuf, Option<PathBuf>), String> {
    let parsed = parse_github_spec_ref(spec)?;
    clone_repo(
        &parsed.repo_url,
        parsed.branch.as_deref(),
        parsed.subpath.as_deref(),
    )
}

fn clone_github_url(source: &str) -> Result<(PathBuf, Option<PathBuf>), String> {
    let parsed = parse_github_url_ref(source)?;
    clone_repo(
        &parsed.repo_url,
        parsed.branch.as_deref(),
        parsed.subpath.as_deref(),
    )
}

#[derive(Debug, PartialEq, Eq)]
struct GithubCloneRef {
    repo_url: String,
    branch: Option<String>,
    subpath: Option<String>,
}

fn parse_github_spec_ref(spec: &str) -> Result<GithubCloneRef, String> {
    let parts: Vec<&str> = spec
        .trim_matches('/')
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() < 2 {
        return Err("GitHub source must be owner/repo or owner/repo/path".to_string());
    }

    let repo_url = format!(
        "https://github.com/{}/{}.git",
        parts[0],
        parts[1].trim_end_matches(".git")
    );
    let (branch, subpath) = if parts.len() >= 4 && parts[2] == "tree" {
        let subpath = (parts.len() > 4).then(|| parts[4..].join("/"));
        (Some(parts[3].to_string()), subpath)
    } else if parts.len() > 2 {
        (None, Some(parts[2..].join("/")))
    } else {
        (None, None)
    };
    Ok(GithubCloneRef {
        repo_url,
        branch,
        subpath,
    })
}

fn parse_github_url_ref(source: &str) -> Result<GithubCloneRef, String> {
    let trimmed = source.trim_end_matches('/');
    let without_scheme = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
        .ok_or_else(|| format!("Invalid GitHub URL: {source}"))?;
    let parts: Vec<&str> = without_scheme
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() < 2 {
        return Err(format!("Invalid GitHub URL: {source}"));
    }

    let repo = parts[1].trim_end_matches(".git");
    let repo_url = format!("https://github.com/{}/{}.git", parts[0], repo);
    let (branch, subpath) = if parts.len() >= 4 && parts[2] == "tree" {
        let subpath = (parts.len() > 4).then(|| parts[4..].join("/"));
        (Some(parts[3].to_string()), subpath)
    } else {
        (None, None)
    };
    Ok(GithubCloneRef {
        repo_url,
        branch,
        subpath,
    })
}

fn clone_repo(
    repo_url: &str,
    branch: Option<&str>,
    subpath: Option<&str>,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    let root = temp_install_dir()?;
    let repo_dir = root.join("repo");
    let mut last_error = String::new();
    for clone_url in github_clone_urls(repo_url) {
        let mut attempt = Command::new("git");
        attempt.arg("clone").arg("--depth").arg("1");
        if let Some(branch) = branch {
            attempt.arg("--branch").arg(branch);
        }
        let output = attempt
            .arg(&clone_url)
            .arg(&repo_dir)
            .output()
            .map_err(|e| format!("Failed to run git clone: {e}"))?;
        if output.status.success() {
            last_error.clear();
            break;
        }
        last_error = redact_token(&String::from_utf8_lossy(&output.stderr));
        let _ = fs::remove_dir_all(&repo_dir);
    }
    if !last_error.is_empty() {
        let _ = fs::remove_dir_all(&root);
        return Err(format!("Failed to clone {repo_url}: {last_error}"));
    }

    let src = match subpath {
        Some(path) if !path.is_empty() => repo_dir.join(path),
        _ => repo_dir,
    };
    if !src.exists() {
        let _ = fs::remove_dir_all(&root);
        return Err(format!("GitHub path not found: {}", src.display()));
    }
    Ok((src, Some(root)))
}

fn download_zip(source: &str) -> Result<(PathBuf, Option<PathBuf>), String> {
    let root = temp_install_dir()?;
    let archive = root.join("skill.zip");
    let extract_dir = root.join("unzipped");
    fs::create_dir_all(&extract_dir).map_err(|e| e.to_string())?;

    let output = Command::new("curl")
        .args(authenticated_curl_args())
        .arg("-L")
        .arg("--fail")
        .arg(source)
        .arg("-o")
        .arg(&archive)
        .output()
        .map_err(|e| format!("Failed to run curl: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = fs::remove_dir_all(&root);
        return Err(format!("Failed to download {source}: {stderr}"));
    }

    let file = fs::File::open(&archive).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    archive.extract(&extract_dir).map_err(|e| e.to_string())?;

    let src = single_child_dir(&extract_dir).unwrap_or(extract_dir);
    Ok((src, Some(root)))
}

fn github_token() -> Option<String> {
    std::env::var("AGENTBRO_GITHUB_TOKEN")
        .or_else(|_| std::env::var("GITHUB_TOKEN"))
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            super::registry::get_sync_config()
                .and_then(|config| config.github_token)
                .filter(|value| !value.trim().is_empty())
        })
}

fn authenticated_curl_args() -> Vec<String> {
    github_token()
        .map(|token| vec!["-H".to_string(), format!("Authorization: Bearer {token}")])
        .unwrap_or_default()
}

fn authenticated_github_url(repo_url: &str) -> String {
    let Some(token) = github_token() else {
        return repo_url.to_string();
    };
    if let Some(rest) = repo_url.strip_prefix("https://github.com/") {
        return format!("https://x-access-token:{token}@github.com/{rest}");
    }
    repo_url.to_string()
}

fn github_clone_urls(repo_url: &str) -> Vec<String> {
    let mut urls = vec![authenticated_github_url(repo_url)];
    if repo_url.starts_with("https://github.com/") {
        urls.push(format!("https://ghfast.top/{repo_url}"));
        urls.push(format!("https://ghproxy.net/{repo_url}"));
        urls.push(format!("https://mirror.ghproxy.com/{repo_url}"));
    }
    urls
}

fn redact_token(value: &str) -> String {
    if let Some(token) = github_token() {
        value.replace(&token, "***")
    } else {
        value.to_string()
    }
}

fn single_child_dir(dir: &Path) -> Option<PathBuf> {
    let mut entries = fs::read_dir(dir).ok()?.flatten();
    let first = entries.next()?.path();
    if entries.next().is_some() {
        return None;
    }
    if first.is_dir() {
        Some(first)
    } else {
        None
    }
}

fn skill_id_candidates(src: &Path, fallback: &str) -> Vec<String> {
    let mut ids = Vec::new();
    if let Some(name) = frontmatter_name(src) {
        ids.push(name);
    }
    if !ids.iter().any(|id| id == fallback) {
        ids.push(fallback.to_string());
    }
    ids
}

fn frontmatter_name(src: &Path) -> Option<String> {
    let index = if src.is_file() {
        Some(src.to_path_buf())
    } else {
        ["SKILL.md", "index.md", "README.md", "main.md"]
            .iter()
            .map(|name| src.join(name))
            .find(|path| path.exists())
            .or_else(|| {
                fs::read_dir(src)
                    .ok()?
                    .flatten()
                    .find(|entry| entry.path().extension().is_some_and(|ext| ext == "md"))
                    .map(|entry| entry.path())
            })
    }?;
    let content = fs::read_to_string(index).ok()?;
    if !content.starts_with("---") {
        return None;
    }
    let frontmatter = content.split("---").nth(1)?;
    frontmatter.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        if key.trim() != "name" {
            return None;
        }
        let value = value
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        (!value.is_empty()).then_some(value)
    })
}

pub fn uninstall_skill(skill_path: &str) -> Result<(), String> {
    let path = PathBuf::from(skill_path);
    if !path.exists() && path.symlink_metadata().is_err() {
        return Ok(());
    }
    if path
        .symlink_metadata()
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false)
    {
        return fs::remove_file(&path).map_err(|e| e.to_string());
    }

    if path.is_dir() {
        fs::remove_dir_all(&path).map_err(|e| e.to_string())
    } else {
        fs::remove_file(&path).map_err(|e| e.to_string())
    }
}

fn rewrite_skill_name(skill_path: &Path, display_name: &str) -> Result<(), String> {
    let Some(index) = find_skill_index_for_rewrite(skill_path) else {
        return Ok(());
    };
    let content = fs::read_to_string(&index).map_err(|e| e.to_string())?;
    let escaped = display_name.replace('"', "\\\"");
    let updated = if content.starts_with("---") {
        let mut parts = content.split("---");
        let _ = parts.next();
        let Some(frontmatter) = parts.next() else {
            return Ok(());
        };
        let rest = parts.next().unwrap_or("");
        let mut found_name = false;
        let mut lines = Vec::new();
        for line in frontmatter.lines() {
            if line
                .split_once(':')
                .map(|(key, _)| key.trim() == "name")
                .unwrap_or(false)
            {
                lines.push(format!("name: \"{escaped}\""));
                found_name = true;
            } else {
                lines.push(line.to_string());
            }
        }
        if !found_name {
            lines.insert(0, format!("name: \"{escaped}\""));
        }
        format!("---\n{}\n---{}", lines.join("\n"), rest)
    } else {
        format!("---\nname: \"{escaped}\"\n---\n{content}")
    };
    fs::write(index, updated).map_err(|e| e.to_string())
}

fn find_skill_index_for_rewrite(skill_path: &Path) -> Option<PathBuf> {
    if skill_path.is_file() {
        return Some(skill_path.to_path_buf());
    }
    ["SKILL.md", "index.md", "README.md", "main.md"]
        .iter()
        .map(|name| skill_path.join(name))
        .find(|path| path.exists())
}

pub fn toggle_skill(skill_id: &str, agent: &str, enabled: bool) -> Result<(), String> {
    if let Some(server_name) = skill_id.strip_prefix("mcp:") {
        return toggle_mcp_server(server_name, agent, enabled);
    }
    if let Some(plugin_id) = skill_id.strip_prefix("plugin:") {
        return toggle_plugin(plugin_id, agent, enabled);
    }

    let paths = agent_paths::paths_for_agent(agent);
    let settings_path = match paths.settings_file {
        Some(p) => p,
        None => return Err(format!("Agent {} has no settings file", agent)),
    };

    if !settings_path.exists() {
        if let Some(parent) = settings_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&settings_path, "{}").map_err(|e| e.to_string())?;
    }

    let content = fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;
    let mut settings: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| e.to_string())?;

    let disabled = settings
        .as_object_mut()
        .ok_or("Settings is not an object")?
        .entry("disabledSkills")
        .or_insert_with(|| serde_json::json!([]));

    if let Some(arr) = disabled.as_array_mut() {
        let skill_val = serde_json::Value::String(skill_id.to_string());
        if enabled {
            arr.retain(|v| v != &skill_val);
        } else if !arr.contains(&skill_val) {
            arr.push(skill_val);
        }
    }

    let content = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    fs::write(&settings_path, content).map_err(|e| e.to_string())
}

fn toggle_plugin(plugin_id: &str, agent: &str, enabled: bool) -> Result<(), String> {
    let settings_path = agent_paths::paths_for_agent(agent)
        .settings_file
        .ok_or_else(|| format!("Agent {} has no settings file", agent))?;
    let mut json = read_json_object(&settings_path)?;
    let enabled_plugins = json
        .as_object_mut()
        .ok_or("Settings is not an object")?
        .entry("enabledPlugins")
        .or_insert_with(|| serde_json::json!({}));
    enabled_plugins
        .as_object_mut()
        .ok_or("enabledPlugins is not an object")?
        .insert(plugin_id.to_string(), serde_json::Value::Bool(enabled));
    write_json_object(&settings_path, &json)
}

pub fn upsert_mcp_server(agent: &str, server: &McpServerConfig) -> Result<(), String> {
    if server.name.trim().is_empty() {
        return Err("MCP server name cannot be empty".to_string());
    }
    if server.command.trim().is_empty() {
        return Err("MCP command cannot be empty".to_string());
    }

    let config_path = agent_paths::paths_for_agent(agent)
        .mcp_config
        .ok_or_else(|| format!("Agent {} has no MCP config file", agent))?;
    let mut json = read_json_object(&config_path)?;
    let servers = json
        .as_object_mut()
        .ok_or("MCP config is not an object")?
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));

    let servers = servers
        .as_object_mut()
        .ok_or("mcpServers is not an object")?;
    let mut value = serde_json::json!({
        "command": server.command,
        "args": server.args,
    });
    if !server.env.is_empty() {
        value["env"] = serde_json::to_value(&server.env).map_err(|e| e.to_string())?;
    }
    servers.insert(server.name.clone(), value);
    write_json_object(&config_path, &json)
}

pub fn validate_mcp_server(agent: &str, server_name: &str) -> Result<McpValidationResult, String> {
    let server = super::scanner::read_mcp_server_config(agent, server_name)
        .ok_or_else(|| format!("MCP server not found: {server_name}"))?;
    validate_mcp_config(&server)
}

pub fn validate_mcp_config(server: &McpServerConfig) -> Result<McpValidationResult, String> {
    let mut warnings = Vec::new();
    let command = server.command.trim();
    if command.is_empty() {
        return Ok(McpValidationResult {
            valid: false,
            message: "MCP command is empty".to_string(),
            warnings,
        });
    }
    if !command_available(command) {
        return Ok(McpValidationResult {
            valid: false,
            message: format!("Command not found: {command}"),
            warnings,
        });
    }
    if command == "docker" && !server.args.iter().any(|arg| arg == "run") {
        warnings.push("Docker MCP 配置通常需要包含 run 参数。".to_string());
    }
    if (command == "npx" || command == "npm") && server.args.is_empty() {
        warnings.push("Node MCP 配置缺少包名参数。".to_string());
    }
    for (key, value) in &server.env {
        if value.trim().is_empty() {
            warnings.push(format!("环境变量 {key} 为空。"));
        }
    }
    Ok(McpValidationResult {
        valid: true,
        message: "MCP 配置可被本机启动器解析。".to_string(),
        warnings,
    })
}

fn command_available(command: &str) -> bool {
    let path = Path::new(command);
    if path.components().count() > 1 || path.is_absolute() {
        return path.exists();
    }
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join(command))
                .any(|candidate| candidate.exists())
        })
        .unwrap_or(false)
}

pub fn remove_mcp_server(agent: &str, server_name: &str) -> Result<(), String> {
    let config_path = agent_paths::paths_for_agent(agent)
        .mcp_config
        .ok_or_else(|| format!("Agent {} has no MCP config file", agent))?;
    if !config_path.exists() {
        return Ok(());
    }
    let mut json = read_json_object(&config_path)?;
    for key in ["mcpServers", "mcp_servers"] {
        if let Some(servers) = json.get_mut(key).and_then(|value| value.as_object_mut()) {
            servers.remove(server_name);
        }
    }
    write_json_object(&config_path, &json)
}

fn toggle_mcp_server(server_name: &str, agent: &str, enabled: bool) -> Result<(), String> {
    let config_path = agent_paths::paths_for_agent(agent)
        .mcp_config
        .ok_or_else(|| format!("Agent {} has no MCP config file", agent))?;
    let mut json = read_json_object(&config_path)?;
    let disabled = json
        .as_object_mut()
        .ok_or("MCP config is not an object")?
        .entry("disabledMcpServers")
        .or_insert_with(|| serde_json::json!([]));
    let server = serde_json::Value::String(server_name.to_string());
    if let Some(list) = disabled.as_array_mut() {
        if enabled {
            list.retain(|item| item != &server);
        } else if !list.contains(&server) {
            list.push(server);
        }
    }
    write_json_object(&config_path, &json)
}

fn read_json_object(path: &Path) -> Result<serde_json::Value, String> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        return Ok(serde_json::json!({}));
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

fn write_json_object(path: &Path, json: &serde_json::Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(json).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| e.to_string())
}

fn copy_recursive(src: &Path, dest: &Path) -> Result<(), String> {
    if src.is_file() {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(src, dest).map_err(|e| e.to_string())?;
        return Ok(());
    }

    fs::create_dir_all(dest).map_err(|e| e.to_string())?;

    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if src_path.is_dir() {
            copy_recursive(&src_path, &dest_path)?;
        } else {
            fs::copy(&src_path, &dest_path).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

fn same_location(src: &Path, dest: &Path) -> bool {
    match (src.canonicalize(), dest.canonicalize()) {
        (Ok(src), Ok(dest)) => src == dest,
        _ => false,
    }
}

pub fn apply_pack(pack: &super::SkillPack) -> Result<(), String> {
    let meta = super::registry::load();
    for skill_id in &pack.skills {
        let source_entry = meta.sources.get(skill_id.as_str());
        let skill_path = source_entry
            .map(|_| super::agent_paths::agentbro_skills_dir().join(skill_id))
            .filter(|p| p.exists());

        let src = match skill_path {
            Some(p) => p.display().to_string(),
            None => match source_entry {
                Some(entry) if !entry.origin.trim().is_empty() => entry.origin.clone(),
                _ => continue,
            },
        };

        let targets: Vec<super::TargetConfig> = pack
            .target_agents
            .iter()
            .map(|a| super::TargetConfig {
                agent: a.clone(),
                install_mode: super::InstallMode::Direct,
            })
            .collect();

        install_skill(&src, &targets, &super::InstallMode::Direct)?;
    }
    Ok(())
}

pub fn copy_recursive_pub(src: &Path, dest: &Path) -> Result<(), String> {
    copy_recursive(src, dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_test_dir(name: &str) -> PathBuf {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        std::env::temp_dir().join(format!("agent-island-{name}-{millis}"))
    }

    #[test]
    fn collect_skill_previews_includes_root_and_nested_skills() {
        let root = temp_test_dir("preview-root");
        let nested = root.join("skills").join("nested-skill");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            root.join("SKILL.md"),
            "---\nname: Root Skill\ndescription: root description\n---\n",
        )
        .unwrap();
        fs::write(
            nested.join("SKILL.md"),
            "---\nname: Nested Skill\ndescription: nested description\n---\n",
        )
        .unwrap();

        let mut previews = Vec::new();
        collect_skill_previews(&root, &root, &mut previews).unwrap();

        let root_preview = previews
            .iter()
            .find(|preview| preview.name == "Root Skill")
            .expect("root skill should be included");
        assert_eq!(root_preview.source_path, "");
        assert_eq!(root_preview.description, "root description");

        let nested_preview = previews
            .iter()
            .find(|preview| preview.name == "Nested Skill")
            .expect("nested skill should be included");
        assert_eq!(nested_preview.source_path, "skills/nested-skill");
        assert_eq!(nested_preview.description, "nested description");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn parses_github_tree_branch_sources() {
        assert_eq!(
            parse_github_spec_ref("owner/repo/tree/feature/skills/foo").unwrap(),
            GithubCloneRef {
                repo_url: "https://github.com/owner/repo.git".to_string(),
                branch: Some("feature".to_string()),
                subpath: Some("skills/foo".to_string()),
            },
        );
        assert_eq!(
            parse_github_url_ref("https://github.com/owner/repo/tree/dev").unwrap(),
            GithubCloneRef {
                repo_url: "https://github.com/owner/repo.git".to_string(),
                branch: Some("dev".to_string()),
                subpath: None,
            },
        );
    }
}
