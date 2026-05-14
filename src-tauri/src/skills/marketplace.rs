use super::{registry, MarketplaceItem, MarketplaceMcpConfig};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

pub fn list_items() -> Result<Vec<MarketplaceItem>, String> {
    let mut items = default_items();
    for source in registry::list_marketplace_sources()
        .into_iter()
        .filter(|source| source.enabled)
    {
        let content = read_manifest(&source.url)?;
        let mut remote_items = parse_manifest(&content)
            .map_err(|error| format!("Failed to parse marketplace {}: {error}", source.name))?;
        items.append(&mut remote_items);
    }
    dedupe_items(items)
}

fn read_manifest(url: &str) -> Result<String, String> {
    let expanded = expand_user_path(url);
    if expanded.exists() {
        return fs::read_to_string(expanded).map_err(|e| e.to_string());
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        let output = Command::new("curl")
            .args(["-L", "--fail", url])
            .output()
            .map_err(|e| format!("Failed to run curl: {e}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }
        return String::from_utf8(output.stdout).map_err(|e| e.to_string());
    }
    Err(format!("Marketplace manifest not found: {url}"))
}

fn parse_manifest(content: &str) -> Result<Vec<MarketplaceItem>, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(content)?;
    if value.is_array() {
        return serde_json::from_value(value);
    }
    if let Some(items) = value.get("items") {
        return serde_json::from_value(items.clone());
    }
    serde_json::from_value(value)
}

fn dedupe_items(items: Vec<MarketplaceItem>) -> Result<Vec<MarketplaceItem>, String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for item in items {
        if item.id.trim().is_empty() {
            return Err("Marketplace item id cannot be empty".to_string());
        }
        if item.name.trim().is_empty() {
            return Err(format!("Marketplace item {} name cannot be empty", item.id));
        }
        if seen.insert(item.id.clone()) {
            result.push(item);
        }
    }
    Ok(result)
}

fn default_items() -> Vec<MarketplaceItem> {
    vec![
        market_item(
            "anthropic-skills",
            "Anthropic Skills",
            "官方示例 Skills 集合，适合作为 Claude Code / Codex 的基础能力包。",
            "skill",
            "github",
            "anthropics/skills",
            "Anthropic",
            "#5856d6",
            None,
        ),
        market_item(
            "openai-skills",
            "OpenAI Skills",
            "面向 Codex 工作流的 Skills 集合，可按子目录导入。",
            "skill",
            "github",
            "openai/skills",
            "OpenAI",
            "#34c759",
            None,
        ),
        market_item(
            "github-mcp",
            "GitHub MCP",
            "Issues、PR、Actions 和仓库 API 集成。",
            "mcp",
            "github",
            "github/github-mcp-server",
            "GitHub",
            "#1d1d1f",
            Some((
                "docker",
                vec!["run", "-i", "--rm", "ghcr.io/github/github-mcp-server"],
            )),
        ),
        market_item(
            "playwright-mcp",
            "Playwright MCP",
            "浏览器自动化、截图、页面检查和端到端验证。",
            "mcp",
            "github",
            "microsoft/playwright-mcp",
            "Microsoft",
            "#007aff",
            Some(("npx", vec!["-y", "@playwright/mcp@latest"])),
        ),
        market_item(
            "context7-mcp",
            "Context7 MCP",
            "面向代码助手的最新文档检索 MCP 能力。",
            "mcp",
            "github",
            "upstash/context7",
            "Upstash",
            "#ff9500",
            Some(("npx", vec!["-y", "@upstash/context7-mcp"])),
        ),
        market_item(
            "notion-mcp",
            "Notion MCP",
            "Notion 页面、数据库和知识库读写能力。",
            "mcp",
            "github",
            "makenotion/notion-mcp-server",
            "Notion",
            "#8e8e93",
            Some(("npx", vec!["-y", "@notionhq/notion-mcp-server"])),
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn market_item(
    id: &str,
    name: &str,
    description: &str,
    category: &str,
    source_type: &str,
    source: &str,
    author: &str,
    accent: &str,
    mcp: Option<(&str, Vec<&str>)>,
) -> MarketplaceItem {
    MarketplaceItem {
        id: id.to_string(),
        name: name.to_string(),
        description: description.to_string(),
        category: category.to_string(),
        source_type: source_type.to_string(),
        source: source.to_string(),
        sub_path: None,
        author: author.to_string(),
        accent: accent.to_string(),
        mcp: mcp.map(|(command, args)| MarketplaceMcpConfig {
            command: command.to_string(),
            args: args.into_iter().map(ToString::to_string).collect(),
            env: std::collections::HashMap::new(),
        }),
        plugin: None,
    }
}

fn expand_user_path(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}
