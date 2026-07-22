use super::{agent_paths, zcode_config};
use super::{
    GitHubSkillPreview, InstallMode, McpServerConfig, McpValidationResult, PluginInstallRequest,
    TargetConfig,
};
use base64::{engine::general_purpose, Engine as _};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const GITHUB_API_CONNECT_TIMEOUT_SECS: u64 = 6;
const GITHUB_API_MAX_TIME_SECS: u64 = 18;
const GITHUB_BLOB_MAX_TIME_SECS: u64 = 4;
const GITHUB_CLONE_ATTEMPT_TIMEOUT_SECS: u64 = 45;
const GITHUB_CLONE_TOTAL_TIMEOUT_SECS: u64 = 120;
const GITHUB_SPARSE_CHECKOUT_TIMEOUT_SECS: u64 = 180;
const GITHUB_TREE_TIMEOUT_SECS: u64 = 20;
const MAX_GITHUB_SKILL_PREVIEWS: usize = 500;

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
    let result = (|| {
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

        Ok(installed_ids)
    })();

    if let Some(root) = temp_root {
        let _ = fs::remove_dir_all(root);
    }

    result
}

pub fn preview_github_skills(source: &str) -> Result<Vec<GitHubSkillPreview>, String> {
    preview_github_skills_with_token(source, None)
}

pub(crate) fn preview_github_skills_with_token(
    source: &str,
    github_token: Option<&str>,
) -> Result<Vec<GitHubSkillPreview>, String> {
    let started_at = Instant::now();
    let normalized = if source.starts_with("github:")
        || source.starts_with("https://github.com/")
        || source.starts_with("http://github.com/")
    {
        source.to_string()
    } else {
        format!("github:{source}")
    };

    match preview_github_skills_via_api(&normalized, github_token) {
        Ok(Some(previews)) => {
            log::info!(
                "GitHub Skill preview completed via API: skills={}, elapsed_ms={}",
                previews.len(),
                started_at.elapsed().as_millis()
            );
            return Ok(previews);
        }
        Ok(None) => {}
        Err(error) => log::warn!("GitHub API preview failed, falling back to clone: {error}"),
    }

    let (src, temp_root) = resolve_install_source_with_token(&normalized, github_token)?;
    let result = (|| {
        let mut previews = Vec::new();
        collect_skill_previews(&src, &src, &mut previews)?;
        previews.sort_by(|a, b| a.source_path.cmp(&b.source_path));
        Ok(previews)
    })();
    if let Some(root) = temp_root {
        let _ = fs::remove_dir_all(root);
    }
    if let Ok(previews) = &result {
        log::info!(
            "GitHub Skill preview completed via clone: skills={}, elapsed_ms={}",
            previews.len(),
            started_at.elapsed().as_millis()
        );
    }
    result
}

fn preview_github_skills_via_api(
    source: &str,
    github_token: Option<&str>,
) -> Result<Option<Vec<GitHubSkillPreview>>, String> {
    let parsed = if let Some(spec) = source.strip_prefix("github:") {
        parse_github_spec_ref(spec)?
    } else if source.starts_with("https://github.com/") || source.starts_with("http://github.com/")
    {
        parse_github_url_ref(source)?
    } else {
        return Ok(None);
    };

    let Some((owner, repo)) = github_owner_repo(&parsed.repo_url) else {
        return Ok(None);
    };
    let branch = match parsed.branch {
        Some(branch) => branch,
        None => github_default_branch(&owner, &repo, github_token)?,
    };
    let tree_url = format!(
        "https://api.github.com/repos/{owner}/{repo}/git/trees/{}?recursive=1",
        url_path_encode(&branch)
    );
    let value = curl_json(&tree_url, github_token)?;
    if value
        .get("truncated")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return Ok(None);
    }

    let root_prefix = parsed
        .subpath
        .as_deref()
        .map(normalize_repo_path)
        .filter(|path| !path.is_empty());
    let Some(entries) = value.get("tree").and_then(|value| value.as_array()) else {
        return Ok(None);
    };

    let matching_entries = entries
        .iter()
        .filter(|entry| {
            if entry.get("type").and_then(|value| value.as_str()) != Some("blob") {
                return false;
            }
            let Some(path) = entry.get("path").and_then(|value| value.as_str()) else {
                return false;
            };
            if !path.ends_with("/SKILL.md") && path != "SKILL.md" {
                return false;
            }
            root_prefix.as_deref().is_none_or(|prefix| {
                path == format!("{prefix}/SKILL.md") || path.starts_with(&format!("{prefix}/"))
            })
        })
        .take(MAX_GITHUB_SKILL_PREVIEWS)
        .collect::<Vec<_>>();
    let should_load_frontmatter = matching_entries.len() == 1;

    let mut previews = Vec::with_capacity(matching_entries.len());
    for entry in matching_entries {
        let Some(path) = entry.get("path").and_then(|value| value.as_str()) else {
            continue;
        };

        let source_path = path.strip_suffix("/SKILL.md").unwrap_or("").to_string();
        let directory_name = if source_path.is_empty() {
            repo.clone()
        } else {
            source_path
                .rsplit('/')
                .next()
                .unwrap_or(&source_path)
                .to_string()
        };
        let frontmatter = if should_load_frontmatter {
            entry
                .get("url")
                .and_then(|value| value.as_str())
                .and_then(|url| read_github_blob_text(url, github_token).ok())
                .map(|content| parse_frontmatter_text(&content))
                .unwrap_or_default()
        } else {
            Default::default()
        };
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

    previews.sort_by(|a, b| a.source_path.cmp(&b.source_path));
    Ok(Some(previews))
}

fn resolve_install_source(source: &str) -> Result<(PathBuf, Option<PathBuf>), String> {
    resolve_install_source_with_token(source, None)
}

fn resolve_install_source_with_token(
    source: &str,
    github_token: Option<&str>,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    let local = PathBuf::from(source);
    if local.exists() {
        return Ok((local, None));
    }

    if let Some(spec) = source.strip_prefix("github:") {
        return clone_github_spec(spec, github_token);
    }

    if let Some(spec) = source.strip_prefix("skillssh:") {
        return clone_skillssh_spec(spec, github_token);
    }

    if source.starts_with("https://github.com/") || source.starts_with("http://github.com/") {
        return clone_github_url(source, github_token);
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

pub(crate) fn resolve_external_skill_source(
    source: &str,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    resolve_install_source(source)
}

pub(crate) fn resolve_external_skill_source_with_token(
    source: &str,
    github_token: Option<&str>,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    resolve_install_source_with_token(source, github_token)
}

pub(crate) fn resolve_github_repo_skills_with_cancel(
    source: &str,
    skill_ids: &[String],
    github_token: Option<&str>,
    cancel: &AtomicBool,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    let parsed = if let Some(spec) = source.strip_prefix("github:") {
        parse_github_spec_ref(spec)?
    } else if source.starts_with("https://github.com/") || source.starts_with("http://github.com/")
    {
        parse_github_url_ref(source)?
    } else {
        return Err(format!("Invalid GitHub repository source: {source}"));
    };
    clone_repo_skills_with_cancel(
        &parsed.repo_url,
        parsed.branch.as_deref(),
        parsed.subpath.as_deref(),
        skill_ids,
        github_token,
        cancel,
    )
}

fn download_markdown_skill(source: &str) -> Result<(PathBuf, Option<PathBuf>), String> {
    let root = temp_install_dir()?;
    let skill_dir = root.join(skill_dir_name_from_url(source));
    fs::create_dir_all(&skill_dir).map_err(|e| e.to_string())?;
    let dest = skill_dir.join("SKILL.md");

    let output = Command::new(crate::agents::executable::command_path("curl"))
        .args(authenticated_curl_args(None))
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
    if previews.len() >= MAX_GITHUB_SKILL_PREVIEWS {
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
    crate::skills::frontmatter::parse_content(content)
        .into_iter()
        .collect()
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

fn clone_github_spec(
    spec: &str,
    github_token: Option<&str>,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    let parsed = parse_github_spec_ref(spec)?;
    clone_repo(
        &parsed.repo_url,
        parsed.branch.as_deref(),
        parsed.subpath.as_deref(),
        github_token,
    )
}

fn clone_github_url(
    source: &str,
    github_token: Option<&str>,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    let parsed = parse_github_url_ref(source)?;
    clone_repo(
        &parsed.repo_url,
        parsed.branch.as_deref(),
        parsed.subpath.as_deref(),
        github_token,
    )
}

fn clone_skillssh_spec(
    spec: &str,
    github_token: Option<&str>,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    let normalized = spec.trim().replace('@', "/");
    let parts: Vec<&str> = normalized
        .trim_matches('/')
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() < 3 {
        return Err(
            "skills.sh source must be formatted as skillssh:owner/repo/skill-id".to_string(),
        );
    }

    let repo_spec = format!("{}/{}", parts[0], parts[1]);
    let skill_id = parts[2..].join("/");
    let cancel = AtomicBool::new(false);
    let (repo_dir, temp_root) = clone_repo_skills_with_cancel(
        &format!("https://github.com/{repo_spec}.git"),
        None,
        None,
        std::slice::from_ref(&skill_id),
        github_token,
        &cancel,
    )?;
    match locate_skillssh_skill_dir(&repo_dir, &skill_id) {
        Ok(skill_dir) => Ok((skill_dir, temp_root)),
        Err(err) => {
            if let Some(root) = temp_root {
                let _ = fs::remove_dir_all(root);
            }
            Err(err)
        }
    }
}

pub(crate) fn locate_skillssh_skill_dir(
    repo_dir: &Path,
    skill_id: &str,
) -> Result<PathBuf, String> {
    let skill_id = skill_id.trim().trim_matches('/');
    if skill_id.is_empty() {
        return Err("skills.sh skill id is empty".to_string());
    }

    for candidate in [
        repo_dir.join(skill_id),
        repo_dir.join("skills").join(skill_id),
        repo_dir.join(".agents").join("skills").join(skill_id),
    ] {
        if is_skill_source_dir(&candidate) {
            return Ok(candidate);
        }
    }

    let mut previews = Vec::new();
    collect_skill_previews(repo_dir, repo_dir, &mut previews)?;
    if previews.is_empty() {
        return Err(format!(
            "No valid Skill directories found in cloned skills.sh repository for '{skill_id}'"
        ));
    }

    let mut ranked = previews
        .into_iter()
        .filter_map(|preview| {
            let score = skillssh_match_score(&preview, skill_id)?;
            let source_len = preview.source_path.len();
            let path = if preview.source_path.is_empty() {
                repo_dir.to_path_buf()
            } else {
                repo_dir.join(&preview.source_path)
            };
            Some((score, source_len, path))
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));

    ranked
        .into_iter()
        .map(|(_, _, path)| path)
        .next()
        .ok_or_else(|| format!("Could not find skills.sh skill '{skill_id}' in cloned repository"))
}

fn is_skill_source_dir(dir: &Path) -> bool {
    dir.join("SKILL.md").is_file()
}

fn skillssh_match_score(preview: &GitHubSkillPreview, skill_id: &str) -> Option<u8> {
    let target = normalize_skillssh_match(skill_id);
    if target.is_empty() {
        return None;
    }
    if normalize_skillssh_match(&preview.source_path) == target {
        return Some(0);
    }
    if normalize_skillssh_match(&preview.directory_name) == target {
        return Some(1);
    }
    if normalize_skillssh_match(&preview.name) == target {
        return Some(2);
    }
    let source_basename = preview
        .source_path
        .rsplit('/')
        .next()
        .unwrap_or(&preview.source_path);
    if normalize_skillssh_match(source_basename) == target {
        return Some(3);
    }
    None
}

fn normalize_skillssh_match(value: &str) -> String {
    let mut out = String::new();
    for ch in value.trim().trim_matches('/').chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if matches!(ch, '/' | '\\') {
            out.push('/');
        } else if matches!(ch, '-' | '_' | '.') || ch.is_whitespace() {
            out.push('-');
        }
    }
    out
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

fn github_owner_repo(repo_url: &str) -> Option<(String, String)> {
    let rest = repo_url
        .strip_prefix("https://github.com/")
        .or_else(|| repo_url.strip_prefix("http://github.com/"))?;
    let mut parts = rest.split('/').filter(|part| !part.is_empty());
    let owner = parts.next()?.to_string();
    let repo = parts.next()?.trim_end_matches(".git").to_string();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner, repo))
}

fn github_default_branch(
    owner: &str,
    repo: &str,
    github_token: Option<&str>,
) -> Result<String, String> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}");
    let value = curl_json(&url, github_token)?;
    value
        .get("default_branch")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .ok_or_else(|| format!("GitHub API response missing default branch for {owner}/{repo}"))
}

fn curl_json(url: &str, github_token: Option<&str>) -> Result<serde_json::Value, String> {
    let content = curl_text(url, github_token)?;
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse GitHub API response: {e}"))
}

fn curl_text(url: &str, github_token: Option<&str>) -> Result<String, String> {
    curl_text_with_timeout(url, GITHUB_API_MAX_TIME_SECS, github_token)
}

fn curl_text_with_timeout(
    url: &str,
    max_time_secs: u64,
    github_token: Option<&str>,
) -> Result<String, String> {
    let output = Command::new(crate::agents::executable::command_path("curl"))
        .args(authenticated_curl_args(github_token))
        .arg("-L")
        .arg("--fail")
        .arg("--connect-timeout")
        .arg(GITHUB_API_CONNECT_TIMEOUT_SECS.to_string())
        .arg("--max-time")
        .arg(max_time_secs.to_string())
        .arg("-H")
        .arg("Accept: application/vnd.github+json")
        .arg("-H")
        .arg("User-Agent: AgentBro")
        .arg(url)
        .output()
        .map_err(|e| format!("Failed to run curl: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "Failed to fetch {url}: {}",
            redact_token(&String::from_utf8_lossy(&output.stderr), github_token)
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("GitHub response was not UTF-8: {e}"))
}

fn read_github_blob_text(url: &str, github_token: Option<&str>) -> Result<String, String> {
    let content = curl_text_with_timeout(url, GITHUB_BLOB_MAX_TIME_SECS, github_token)?;
    let value = serde_json::from_str::<serde_json::Value>(&content)
        .map_err(|e| format!("Failed to parse GitHub blob response: {e}"))?;
    let encoded = value
        .get("content")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "GitHub blob response missing content".to_string())?
        .replace(['\n', '\r'], "");
    let bytes = general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| format!("Failed to decode GitHub blob content: {e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("GitHub blob content was not UTF-8: {e}"))
}

fn normalize_repo_path(path: &str) -> String {
    path.split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

fn url_path_encode(path: &str) -> String {
    path.split('/')
        .map(url_segment_encode)
        .collect::<Vec<_>>()
        .join("/")
}

fn url_segment_encode(segment: &str) -> String {
    let mut out = String::new();
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn clone_repo(
    repo_url: &str,
    branch: Option<&str>,
    subpath: Option<&str>,
    github_token: Option<&str>,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    clone_repo_with_cancel(repo_url, branch, subpath, github_token, None)
}

fn clone_repo_with_cancel(
    repo_url: &str,
    branch: Option<&str>,
    subpath: Option<&str>,
    github_token: Option<&str>,
    cancel: Option<&AtomicBool>,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    let root = temp_install_dir()?;
    let repo_dir = root.join("repo");
    let started_at = Instant::now();
    let mut cloned = false;
    let mut last_error = "No GitHub clone attempts were available".to_string();
    let clone_urls = github_clone_urls(repo_url, github_token);
    let network_env = git_network_env();
    let single_attempt = clone_urls.len() == 1;
    for (attempt_index, clone_url) in clone_urls.into_iter().enumerate() {
        if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            let _ = fs::remove_dir_all(&root);
            return Err("Installation cancelled".to_string());
        }
        let remaining = Duration::from_secs(GITHUB_CLONE_TOTAL_TIMEOUT_SECS)
            .saturating_sub(started_at.elapsed());
        if remaining.is_zero() {
            last_error = format!(
                "GitHub clone exceeded the {} second total timeout",
                GITHUB_CLONE_TOTAL_TIMEOUT_SECS
            );
            break;
        }
        let attempt_timeout = if single_attempt {
            remaining
        } else {
            remaining.min(Duration::from_secs(GITHUB_CLONE_ATTEMPT_TIMEOUT_SECS))
        };
        log::info!(
            "GitHub clone attempt {} started with a {} second timeout",
            attempt_index + 1,
            attempt_timeout.as_secs()
        );
        let mut attempt = Command::new(crate::agents::executable::command_path("git"));
        attempt
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "Never")
            .arg("clone")
            .arg("--quiet")
            .arg("--depth")
            .arg("1")
            .arg("--single-branch")
            .arg("--no-tags");
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            attempt.process_group(0);
        }
        for (name, value) in &network_env {
            attempt.env(name, value);
        }
        if let Some(branch) = branch {
            attempt.arg("--branch").arg(branch);
        }
        attempt
            .arg(&clone_url)
            .arg(&repo_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let output = match command_output_with_timeout(&mut attempt, attempt_timeout, cancel) {
            Ok(output) => output,
            Err(error) => {
                last_error = error;
                let _ = fs::remove_dir_all(&repo_dir);
                continue;
            }
        };
        if output.status.success() {
            cloned = true;
            break;
        }
        last_error = redact_token(&String::from_utf8_lossy(&output.stderr), github_token);
        let _ = fs::remove_dir_all(&repo_dir);
    }
    if !cloned {
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

fn clone_repo_skills_with_cancel(
    repo_url: &str,
    branch: Option<&str>,
    subpath: Option<&str>,
    skill_ids: &[String],
    github_token: Option<&str>,
    cancel: &AtomicBool,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    let root = temp_install_dir()?;
    let repo_dir = root.join("repo");
    let started_at = Instant::now();
    let mut cloned = false;
    let mut last_error = "No GitHub clone attempts were available".to_string();
    let clone_urls = github_clone_urls(repo_url, github_token);
    let network_env = git_network_env();
    let single_attempt = clone_urls.len() == 1;

    for (attempt_index, clone_url) in clone_urls.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            let _ = fs::remove_dir_all(&root);
            return Err("Installation cancelled".to_string());
        }
        let remaining = Duration::from_secs(GITHUB_CLONE_TOTAL_TIMEOUT_SECS)
            .saturating_sub(started_at.elapsed());
        if remaining.is_zero() {
            last_error = format!(
                "GitHub repository preparation exceeded the {} second total timeout",
                GITHUB_CLONE_TOTAL_TIMEOUT_SECS
            );
            break;
        }
        let attempt_timeout = if single_attempt {
            remaining
        } else {
            remaining.min(Duration::from_secs(GITHUB_CLONE_ATTEMPT_TIMEOUT_SECS))
        };
        log::info!(
            "GitHub sparse clone attempt {} started with a {} second timeout",
            attempt_index + 1,
            attempt_timeout.as_secs()
        );
        let mut attempt = Command::new(crate::agents::executable::command_path("git"));
        attempt
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "Never")
            .arg("clone")
            .arg("--quiet")
            .arg("--filter=blob:none")
            .arg("--no-checkout")
            .arg("--depth")
            .arg("1")
            .arg("--single-branch")
            .arg("--no-tags");
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            attempt.process_group(0);
        }
        for (name, value) in &network_env {
            attempt.env(name, value);
        }
        if let Some(branch) = branch {
            attempt.arg("--branch").arg(branch);
        }
        attempt
            .arg(&clone_url)
            .arg(&repo_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let output = match command_output_with_timeout_named(
            &mut attempt,
            attempt_timeout,
            Some(cancel),
            "GitHub repository preparation",
        ) {
            Ok(output) => output,
            Err(error) => {
                last_error = error;
                let _ = fs::remove_dir_all(&repo_dir);
                continue;
            }
        };
        if output.status.success() {
            cloned = true;
            break;
        }
        last_error = redact_token(&String::from_utf8_lossy(&output.stderr), github_token);
        let _ = fs::remove_dir_all(&repo_dir);
    }

    if !cloned {
        let _ = fs::remove_dir_all(&root);
        return Err(format!("Failed to prepare {repo_url}: {last_error}"));
    }

    let tree_file = root.join("skill-tree.txt");
    let tree_output = match run_git_with_cancel(
        &repo_dir,
        &["ls-tree", "-r", "--name-only", "HEAD"],
        Duration::from_secs(GITHUB_TREE_TIMEOUT_SECS),
        cancel,
        &network_env,
        Some(&tree_file),
        "Reading the GitHub repository directory",
    ) {
        Ok(output) => output,
        Err(error) => {
            let _ = fs::remove_dir_all(&root);
            return Err(error);
        }
    };
    if !tree_output.status.success() {
        let error = redact_token(&String::from_utf8_lossy(&tree_output.stderr), github_token);
        let _ = fs::remove_dir_all(&root);
        return Err(format!("Failed to read {repo_url}: {error}"));
    }
    let tree = match fs::read_to_string(&tree_file) {
        Ok(tree) => tree,
        Err(error) => {
            let _ = fs::remove_dir_all(&root);
            return Err(format!(
                "Failed to read the GitHub repository directory: {error}"
            ));
        }
    };
    let prefix = subpath
        .map(normalize_repo_path)
        .filter(|path| !path.is_empty());
    let manifest_paths = tree
        .lines()
        .filter(|path| *path == "SKILL.md" || path.ends_with("/SKILL.md"))
        .filter(|path| {
            prefix.as_deref().is_none_or(|prefix| {
                *path == format!("{prefix}/SKILL.md") || path.starts_with(&format!("{prefix}/"))
            })
        })
        .take(MAX_GITHUB_SKILL_PREVIEWS)
        .collect::<Vec<_>>();

    if !manifest_paths.is_empty() {
        let init_output = match run_git_with_cancel(
            &repo_dir,
            &["sparse-checkout", "init", "--no-cone"],
            Duration::from_secs(GITHUB_TREE_TIMEOUT_SECS),
            cancel,
            &network_env,
            None,
            "Preparing Skill manifests",
        ) {
            Ok(output) => output,
            Err(error) => {
                let _ = fs::remove_dir_all(&root);
                return Err(error);
            }
        };
        if !init_output.status.success() {
            let error = redact_token(&String::from_utf8_lossy(&init_output.stderr), github_token);
            let _ = fs::remove_dir_all(&root);
            return Err(format!("Failed to prepare Skill manifests: {error}"));
        }

        let manifest_patterns = manifest_paths
            .iter()
            .map(|path| format!("/{path}"))
            .collect::<Vec<_>>();
        let mut sparse_args = vec!["sparse-checkout", "set", "--no-cone", "--"];
        sparse_args.extend(manifest_patterns.iter().map(String::as_str));
        let set_output = match run_git_with_cancel(
            &repo_dir,
            &sparse_args,
            Duration::from_secs(GITHUB_TREE_TIMEOUT_SECS),
            cancel,
            &network_env,
            None,
            "Selecting Skill manifests",
        ) {
            Ok(output) => output,
            Err(error) => {
                let _ = fs::remove_dir_all(&root);
                return Err(error);
            }
        };
        if !set_output.status.success() {
            let error = redact_token(&String::from_utf8_lossy(&set_output.stderr), github_token);
            let _ = fs::remove_dir_all(&root);
            return Err(format!("Failed to select Skill manifests: {error}"));
        }

        let checkout_output = match run_git_with_cancel(
            &repo_dir,
            &["checkout", "--quiet", "HEAD"],
            Duration::from_secs(GITHUB_CLONE_ATTEMPT_TIMEOUT_SECS),
            cancel,
            &network_env,
            None,
            "Downloading Skill manifests",
        ) {
            Ok(output) => output,
            Err(error) => {
                let _ = fs::remove_dir_all(&root);
                return Err(error);
            }
        };
        if !checkout_output.status.success() {
            let error = redact_token(
                &String::from_utf8_lossy(&checkout_output.stderr),
                github_token,
            );
            let _ = fs::remove_dir_all(&root);
            return Err(format!("Failed to download Skill manifests: {error}"));
        }
    }

    let selected_dirs = selected_skill_directories(&tree, skill_ids, subpath, Some(&repo_dir));
    if !selected_dirs.is_empty() {
        let mut sparse_args = vec!["sparse-checkout", "set", "--cone", "--"];
        sparse_args.extend(selected_dirs.iter().map(String::as_str));
        let set_output = match run_git_with_cancel(
            &repo_dir,
            &sparse_args,
            Duration::from_secs(GITHUB_SPARSE_CHECKOUT_TIMEOUT_SECS),
            cancel,
            &network_env,
            None,
            "Downloading selected Skills",
        ) {
            Ok(output) => output,
            Err(error) => {
                let _ = fs::remove_dir_all(&root);
                return Err(error);
            }
        };
        if !set_output.status.success() {
            let error = redact_token(&String::from_utf8_lossy(&set_output.stderr), github_token);
            let _ = fs::remove_dir_all(&root);
            return Err(format!("Failed to download selected Skills: {error}"));
        }
    }

    log::info!(
        "GitHub sparse Skill checkout completed: selected_dirs={}, elapsed_ms={}",
        selected_dirs.len(),
        started_at.elapsed().as_millis()
    );
    let src = match subpath {
        Some(path) if !path.is_empty() => repo_dir.join(path),
        _ => repo_dir,
    };
    Ok((src, Some(root)))
}

fn selected_skill_directories(
    tree: &str,
    skill_ids: &[String],
    subpath: Option<&str>,
    repo_dir: Option<&Path>,
) -> Vec<String> {
    let prefix = subpath
        .map(normalize_repo_path)
        .filter(|path| !path.is_empty());
    let manifests = tree
        .lines()
        .filter_map(|path| {
            if path == "SKILL.md" {
                Some(".")
            } else {
                path.strip_suffix("/SKILL.md")
            }
        })
        .filter(|dir| {
            prefix
                .as_deref()
                .is_none_or(|prefix| *dir == prefix || dir.starts_with(&format!("{prefix}/")))
        })
        .take(MAX_GITHUB_SKILL_PREVIEWS)
        .collect::<Vec<_>>();
    let mut selected = Vec::new();
    let mut seen = HashSet::new();

    for skill_id in skill_ids {
        let target = normalize_skillssh_match(skill_id);
        let best = manifests
            .iter()
            .filter_map(|dir| {
                let normalized = normalize_skillssh_match(dir);
                let basename = dir.rsplit('/').next().unwrap_or(dir);
                let basename = normalize_skillssh_match(basename);
                let score = if normalized == target {
                    0
                } else if dir
                    .strip_prefix("skills/")
                    .is_some_and(|path| normalize_skillssh_match(path) == target)
                {
                    1
                } else if dir
                    .strip_prefix(".agents/skills/")
                    .is_some_and(|path| normalize_skillssh_match(path) == target)
                {
                    2
                } else if basename == target {
                    3
                } else if repo_dir.is_some_and(|repo_dir| {
                    parse_frontmatter_from_file(&repo_dir.join(dir).join("SKILL.md"))
                        .get("name")
                        .is_some_and(|name| normalize_skillssh_match(name) == target)
                }) {
                    4
                } else {
                    return None;
                };
                Some((score, dir.len(), *dir))
            })
            .min_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(b.2)));
        if let Some((_, _, dir)) = best {
            if seen.insert(dir.to_string()) {
                selected.push(dir.to_string());
            }
        }
    }
    selected
}

fn run_git_with_cancel(
    repo_dir: &Path,
    args: &[&str],
    timeout: Duration,
    cancel: &AtomicBool,
    network_env: &[(String, String)],
    stdout_file: Option<&Path>,
    operation: &str,
) -> Result<Output, String> {
    let mut command = Command::new(crate::agents::executable::command_path("git"));
    command
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never")
        .arg("-C")
        .arg(repo_dir)
        .args(args)
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    for (name, value) in network_env {
        command.env(name, value);
    }
    if let Some(path) = stdout_file {
        let file = fs::File::create(path)
            .map_err(|error| format!("Failed to create Git output file: {error}"))?;
        command.stdout(Stdio::from(file));
    } else {
        command.stdout(Stdio::null());
    }
    command_output_with_timeout_named(&mut command, timeout, Some(cancel), operation)
}

fn command_output_with_timeout(
    command: &mut Command,
    timeout: Duration,
    cancel: Option<&AtomicBool>,
) -> Result<Output, String> {
    command_output_with_timeout_named(command, timeout, cancel, "GitHub clone attempt")
}

fn command_output_with_timeout_named(
    command: &mut Command,
    timeout: Duration,
    cancel: Option<&AtomicBool>,
    operation: &str,
) -> Result<Output, String> {
    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to start {operation}: {e}"))?;
    let started_at = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child
                    .wait_with_output()
                    .map_err(|e| format!("Failed to read git clone result: {e}"));
            }
            Ok(None) if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) => {
                terminate_child_process_group(&mut child);
                return Err("Installation cancelled".to_string());
            }
            Ok(None) if started_at.elapsed() < timeout => thread::sleep(Duration::from_millis(100)),
            Ok(None) => {
                terminate_child_process_group(&mut child);
                return Err(format!(
                    "{operation} timed out after {} seconds",
                    timeout.as_secs()
                ));
            }
            Err(error) => {
                terminate_child_process_group(&mut child);
                return Err(format!("Failed to monitor {operation}: {error}"));
            }
        }
    }
}

fn terminate_child_process_group(child: &mut Child) {
    #[cfg(unix)]
    {
        // `git clone` launches git-remote-https; killing the process group prevents it
        // from continuing to hold the network connection after cancellation.
        unsafe {
            libc::killpg(child.id() as libc::pid_t, libc::SIGKILL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn download_zip(source: &str) -> Result<(PathBuf, Option<PathBuf>), String> {
    let root = temp_install_dir()?;
    let archive = root.join("skill.zip");
    let extract_dir = root.join("unzipped");
    fs::create_dir_all(&extract_dir).map_err(|e| e.to_string())?;

    let output = Command::new(crate::agents::executable::command_path("curl"))
        .args(authenticated_curl_args(None))
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

fn effective_github_token(explicit_token: Option<&str>) -> Option<String> {
    explicit_token
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(github_token)
}

fn authenticated_curl_args(github_token: Option<&str>) -> Vec<String> {
    effective_github_token(github_token)
        .map(|token| vec!["-H".to_string(), format!("Authorization: Bearer {token}")])
        .unwrap_or_default()
}

fn authenticated_github_url(repo_url: &str, github_token: Option<&str>) -> String {
    let Some(token) = effective_github_token(github_token) else {
        return repo_url.to_string();
    };
    if let Some(rest) = repo_url.strip_prefix("https://github.com/") {
        return format!("https://x-access-token:{token}@github.com/{rest}");
    }
    repo_url.to_string()
}

fn github_clone_urls(repo_url: &str, github_token: Option<&str>) -> Vec<String> {
    let mut urls = vec![authenticated_github_url(repo_url, github_token)];
    if effective_github_token(github_token).is_some() {
        return urls;
    }
    if repo_url.starts_with("https://github.com/") {
        urls.push(format!("https://ghfast.top/{repo_url}"));
        urls.push(format!("https://ghproxy.net/{repo_url}"));
    }
    urls
}

fn git_network_env() -> Vec<(String, String)> {
    let names = [
        "HTTPS_PROXY",
        "HTTP_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "https_proxy",
        "http_proxy",
        "all_proxy",
        "no_proxy",
    ];
    let missing = names
        .into_iter()
        .filter(|name| std::env::var_os(name).is_none())
        .collect::<Vec<_>>();
    crate::agents::executable::interactive_login_shell_vars(&missing)
}

fn redact_token(value: &str, github_token: Option<&str>) -> String {
    if let Some(token) = effective_github_token(github_token) {
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
    if agent == "zcode" {
        return toggle_zcode_skill(skill_id, enabled);
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

fn toggle_zcode_skill(skill_id: &str, enabled: bool) -> Result<(), String> {
    let paths = agent_paths::paths_for_agent("zcode");
    let settings_path = paths
        .settings_file
        .ok_or_else(|| "ZCode has no settings file".to_string())?;
    let skill_path = find_zcode_skill_path(&paths.skill_dirs, skill_id)
        .ok_or_else(|| format!("ZCode skill not found: {skill_id}"))?;
    let mut json = read_json_object(&settings_path)?;
    zcode_config::skill_overrides_mut(&mut json)?.insert(
        skill_path.display().to_string(),
        serde_json::json!({ "enable": enabled }),
    );
    write_json_object(&settings_path, &json)
}

fn find_zcode_skill_path(skill_dirs: &[PathBuf], skill_id: &str) -> Option<PathBuf> {
    for root in skill_dirs {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = path.file_name().and_then(|name| name.to_str());
            if file_name == Some(skill_id) || frontmatter_name(&path).as_deref() == Some(skill_id) {
                return Some(path);
            }
        }
    }
    None
}

fn toggle_plugin(plugin_id: &str, agent: &str, enabled: bool) -> Result<(), String> {
    let settings_path = agent_paths::paths_for_agent(agent)
        .settings_file
        .ok_or_else(|| format!("Agent {} has no settings file", agent))?;
    let mut json = read_json_object(&settings_path)?;
    if agent == "zcode" {
        zcode_config::enabled_plugins_mut(&mut json)?
            .insert(plugin_id.to_string(), serde_json::Value::Bool(enabled));
    } else {
        let enabled_plugins = json
            .as_object_mut()
            .ok_or("Settings is not an object")?
            .entry("enabledPlugins")
            .or_insert_with(|| serde_json::json!({}));
        enabled_plugins
            .as_object_mut()
            .ok_or("enabledPlugins is not an object")?
            .insert(plugin_id.to_string(), serde_json::Value::Bool(enabled));
    }
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
    let mut value = serde_json::json!({
        "command": server.command,
        "args": server.args,
    });
    if !server.env.is_empty() {
        value["env"] = serde_json::to_value(&server.env).map_err(|e| e.to_string())?;
    }
    let servers = if agent == "zcode" {
        value["type"] = serde_json::json!("stdio");
        value["enabled"] = serde_json::json!(true);
        zcode_config::mcp_servers_mut(&mut json)?
    } else {
        json.as_object_mut()
            .ok_or("MCP config is not an object")?
            .entry("mcpServers")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or("mcpServers is not an object")?
    };
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
    if agent == "zcode" {
        if let Some(servers) = json
            .pointer_mut("/mcp/servers")
            .and_then(serde_json::Value::as_object_mut)
        {
            servers.remove(server_name);
        }
    } else {
        for key in ["mcpServers", "mcp_servers"] {
            if let Some(servers) = json.get_mut(key).and_then(|value| value.as_object_mut()) {
                servers.remove(server_name);
            }
        }
    }
    write_json_object(&config_path, &json)
}

fn toggle_mcp_server(server_name: &str, agent: &str, enabled: bool) -> Result<(), String> {
    let config_path = agent_paths::paths_for_agent(agent)
        .mcp_config
        .ok_or_else(|| format!("Agent {} has no MCP config file", agent))?;
    let mut json = read_json_object(&config_path)?;
    if agent == "zcode" {
        let server = zcode_config::mcp_servers_mut(&mut json)?
            .get_mut(server_name)
            .ok_or_else(|| format!("MCP server not found: {server_name}"))?;
        server
            .as_object_mut()
            .ok_or_else(|| format!("Invalid ZCode MCP server: {server_name}"))?
            .insert("enabled".to_string(), serde_json::Value::Bool(enabled));
        return write_json_object(&config_path, &json);
    }
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
        std::env::temp_dir().join(format!("agentbro-{name}-{millis}"))
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

    #[test]
    fn builds_github_api_paths_without_clone_paths() {
        assert_eq!(
            github_owner_repo("https://github.com/anthropics/skills.git"),
            Some(("anthropics".to_string(), "skills".to_string())),
        );
        assert_eq!(normalize_repo_path("/skills//pdf/"), "skills/pdf");
        assert_eq!(
            url_path_encode("feature/skill name"),
            "feature/skill%20name"
        );
        assert_eq!(url_segment_encode("skill name+v1"), "skill%20name%2Bv1");
    }

    #[test]
    fn sparse_checkout_selects_only_requested_skill_directories() {
        let tree = [
            ".agents/skills/shared/SKILL.md",
            ".claude/skills/shared/SKILL.md",
            "skills/first/SKILL.md",
            "skills/first/references/guide.md",
            "skills/second/SKILL.md",
            "packages/unrelated/package.json",
        ]
        .join("\n");
        let selected = selected_skill_directories(
            &tree,
            &[
                "first".to_string(),
                "shared".to_string(),
                "missing".to_string(),
            ],
            None,
            None,
        );

        assert_eq!(
            selected,
            vec![
                "skills/first".to_string(),
                ".agents/skills/shared".to_string()
            ]
        );
    }

    #[test]
    fn sparse_checkout_honors_repository_subpath() {
        let tree = ["catalog/skills/demo/SKILL.md", "other/skills/demo/SKILL.md"].join("\n");

        assert_eq!(
            selected_skill_directories(&tree, &["demo".to_string()], Some("catalog/skills"), None),
            vec!["catalog/skills/demo".to_string()]
        );
    }

    #[test]
    fn sparse_checkout_matches_frontmatter_names() {
        let root = temp_test_dir("sparse-frontmatter");
        let skill = root.join("catalog").join("renamed-folder");
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: Frontend Design\ndescription: design helper\n---\n",
        )
        .unwrap();

        assert_eq!(
            selected_skill_directories(
                "catalog/renamed-folder/SKILL.md",
                &["frontend-design".to_string()],
                None,
                Some(&root)
            ),
            vec!["catalog/renamed-folder".to_string()]
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn private_github_clone_does_not_fall_back_to_public_proxies() {
        let urls = github_clone_urls(
            "https://github.com/owner/private-repo.git",
            Some("test-token"),
        );
        assert_eq!(urls.len(), 1);
        assert!(urls[0].contains("x-access-token:test-token@github.com"));
        assert_eq!(
            redact_token("clone failed for test-token", Some("test-token")),
            "clone failed for ***",
        );
    }

    #[cfg(unix)]
    #[test]
    fn command_output_stops_a_cancelled_process_group() {
        use std::os::unix::process::CommandExt;
        use std::sync::Arc;

        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_from_thread = cancel.clone();
        let setter = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            cancel_from_thread.store(true, Ordering::Relaxed);
        });
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "sleep 10"])
            .process_group(0)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let started_at = Instant::now();

        let error = command_output_with_timeout(
            &mut command,
            Duration::from_secs(5),
            Some(cancel.as_ref()),
        )
        .unwrap_err();

        setter.join().unwrap();
        assert_eq!(error, "Installation cancelled");
        assert!(started_at.elapsed() < Duration::from_secs(2));
    }

    #[test]
    #[ignore]
    fn live_preview_github_skills_vercel_labs() {
        let previews = preview_github_skills("https://github.com/vercel-labs/skills").unwrap();
        assert!(
            previews
                .iter()
                .any(|preview| preview.source_path.contains("find-skills")),
            "expected find-skills in previews, got {previews:?}"
        );
    }

    #[test]
    fn locate_skillssh_skill_dir_finds_nested_directory_match() {
        let root = temp_test_dir("skillssh-nested");
        let bogus = root.join("find-skills");
        let real = root.join("providers").join("vercel").join("find-skills");
        fs::create_dir_all(&bogus).unwrap();
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("SKILL.md"), "---\nname: find-skills\n---\n").unwrap();

        let found = locate_skillssh_skill_dir(&root, "find-skills").unwrap();
        assert_eq!(found, real);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn locate_skillssh_skill_dir_prefers_agents_skills_variant() {
        let root = temp_test_dir("skillssh-agents");
        let agents = root.join(".agents").join("skills").join("my-skill");
        let cursor = root.join(".cursor").join("skills").join("my-skill");
        for dir in [&agents, &cursor] {
            fs::create_dir_all(dir).unwrap();
            fs::write(dir.join("SKILL.md"), "content").unwrap();
        }

        let found = locate_skillssh_skill_dir(&root, "my-skill").unwrap();
        assert_eq!(found, agents);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn locate_skillssh_skill_dir_finds_frontmatter_name_match() {
        let root = temp_test_dir("skillssh-frontmatter");
        let real = root.join("catalog").join("renamed-folder");
        fs::create_dir_all(&real).unwrap();
        fs::write(
            real.join("SKILL.md"),
            "---\nname: Frontend Design\ndescription: design helper\n---\n",
        )
        .unwrap();

        let found = locate_skillssh_skill_dir(&root, "frontend-design").unwrap();
        assert_eq!(found, real);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    #[ignore]
    fn live_resolves_skills_sh_find_skills() {
        let (dir, temp_root) =
            resolve_install_source("skillssh:vercel-labs/skills/find-skills").unwrap();
        assert!(dir.join("SKILL.md").is_file());
        if let Some(root) = temp_root {
            let _ = fs::remove_dir_all(root);
        }
    }
}
