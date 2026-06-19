use super::registry::{self, SkillExplanationEntry};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillExplanation {
    pub skill_id: String,
    pub lang: String,
    pub model: String,
    pub text: String,
    pub cached_at: String,
    pub from_cache: bool,
}

pub fn get_cached(skill_id: &str, lang: &str) -> Option<SkillExplanation> {
    registry::get_skill_explanation(skill_id, lang).map(|entry| SkillExplanation {
        skill_id: entry.skill_id,
        lang: entry.lang,
        model: entry.model,
        text: entry.text,
        cached_at: entry.cached_at,
        from_cache: true,
    })
}

pub fn generate(
    skill_id: &str,
    skill_path: &str,
    lang: &str,
    refresh: bool,
) -> Result<SkillExplanation, String> {
    if !refresh {
        if let Some(cached) = get_cached(skill_id, lang) {
            return Ok(cached);
        }
    }

    let content = read_skill_content(skill_path)?;
    let (text, model) = match generate_with_api(skill_id, &content, lang) {
        Ok(result) => result,
        Err(_) => (
            local_explanation(skill_id, &content, lang),
            "local-summary".to_string(),
        ),
    };
    let entry = SkillExplanationEntry {
        skill_id: skill_id.to_string(),
        lang: lang.to_string(),
        model,
        text,
        cached_at: Utc::now().to_rfc3339(),
    };
    registry::cache_skill_explanation(entry.clone())?;
    Ok(SkillExplanation {
        skill_id: entry.skill_id,
        lang: entry.lang,
        model: entry.model,
        text: entry.text,
        cached_at: entry.cached_at,
        from_cache: false,
    })
}

fn read_skill_content(skill_path: &str) -> Result<String, String> {
    let path = expand_user_path(skill_path);
    let file = if path.is_file() {
        path
    } else {
        ["SKILL.md", "index.md", "README.md", "main.md"]
            .iter()
            .map(|name| path.join(name))
            .find(|candidate| candidate.exists())
            .ok_or_else(|| format!("No readable skill markdown found in {}", path.display()))?
    };
    fs::read_to_string(&file).map_err(|e| e.to_string())
}

fn generate_with_api(
    skill_id: &str,
    content: &str,
    lang: &str,
) -> Result<(String, String), String> {
    let api_key = std::env::var("AGENTBRO_AI_API_KEY")
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .map_err(|_| "No AI API key configured".to_string())?;
    let api_url = std::env::var("AGENTBRO_AI_API_URL")
        .unwrap_or_else(|_| "https://api.openai.com/v1/chat/completions".to_string());
    let model = std::env::var("AGENTBRO_AI_MODEL").unwrap_or_else(|_| "gpt-4.1-mini".to_string());
    let prompt = explanation_prompt(skill_id, content, lang);
    let body = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content": "You explain local AI agent skills for a developer. Be concise, concrete, and avoid marketing language." },
            { "role": "user", "content": prompt }
        ],
        "temperature": 0.2
    });
    let auth_header = format!("Authorization: Bearer {api_key}");
    let body_text = body.to_string();
    let output = Command::new("curl")
        .arg("-sS")
        .arg("-L")
        .arg("--fail")
        .arg("-X")
        .arg("POST")
        .arg("-H")
        .arg("Content-Type: application/json")
        .arg("-H")
        .arg(auth_header)
        .arg("-d")
        .arg(body_text)
        .arg(api_url)
        .output()
        .map_err(|e| format!("Failed to run curl: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    let text = json
        .pointer("/choices/0/message/content")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "AI response did not contain text".to_string())?;
    Ok((text.to_string(), model))
}

fn explanation_prompt(skill_id: &str, content: &str, lang: &str) -> String {
    let clipped = if content.len() > 24_000 {
        &content[..24_000]
    } else {
        content
    };
    format!(
        "Language: {lang}\nSkill id: {skill_id}\n\nExplain this skill in 4 sections: purpose, when to use it, important requirements, and operational risks. Skill markdown:\n\n{clipped}"
    )
}

fn local_explanation(skill_id: &str, content: &str, lang: &str) -> String {
    let frontmatter = parse_frontmatter(content);
    let name = frontmatter
        .get("name")
        .cloned()
        .unwrap_or_else(|| skill_id.to_string());
    let description = frontmatter.get("description").cloned().unwrap_or_else(|| {
        first_non_empty_body_line(content).unwrap_or_else(|| "No description found.".to_string())
    });
    let requirements = frontmatter
        .iter()
        .filter(|(key, _)| {
            let key = key.to_lowercase();
            key.contains("require")
                || key.contains("bin")
                || key.contains("tool")
                || key.contains("mcp")
        })
        .map(|(key, value)| format!("- {key}: {value}"))
        .collect::<Vec<_>>();
    let headings = content
        .lines()
        .filter_map(|line| line.trim().strip_prefix('#').map(str::trim))
        .filter(|line| !line.is_empty())
        .take(6)
        .map(|line| format!("- {line}"))
        .collect::<Vec<_>>();

    if lang.starts_with("zh") {
        format!(
            "### 用途\n{name}：{description}\n\n### 适用场景\n这个 Skill 适合在任务需要调用其说明中的流程、工具或领域知识时启用。它的主要内容结构包括：\n{}\n\n### 重要要求\n{}\n\n### 风险提示\n这是本地结构化解释。设置 AGENTBRO_AI_API_KEY 或 OPENAI_API_KEY 后可生成更完整的 AI 解释。",
            if headings.is_empty() { "- 未发现标题结构".to_string() } else { headings.join("\n") },
            if requirements.is_empty() { "- 未在 frontmatter 中发现显式依赖".to_string() } else { requirements.join("\n") },
        )
    } else {
        format!(
            "### Purpose\n{name}: {description}\n\n### When To Use\nUse this skill when the task needs the workflow, tools, or domain knowledge described in the skill. Main sections found:\n{}\n\n### Requirements\n{}\n\n### Risks\nThis is a local structured explanation. Set AGENTBRO_AI_API_KEY or OPENAI_API_KEY to generate a fuller AI explanation.",
            if headings.is_empty() { "- No heading structure found".to_string() } else { headings.join("\n") },
            if requirements.is_empty() { "- No explicit frontmatter requirements found".to_string() } else { requirements.join("\n") },
        )
    }
}

fn parse_frontmatter(content: &str) -> std::collections::HashMap<String, String> {
    crate::skills::frontmatter::parse_content(content)
        .into_iter()
        .collect()
}

fn first_non_empty_body_line(content: &str) -> Option<String> {
    content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("---") && !line.starts_with('#'))
        .map(ToString::to_string)
}

fn expand_user_path(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    Path::new(path).to_path_buf()
}
