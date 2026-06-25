use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexMcpServer {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexPluginConfig {
    pub id: String,
    pub enabled: bool,
}

pub fn read_mcp_servers_path(path: &Path) -> Vec<CodexMcpServer> {
    std::fs::read_to_string(path)
        .map(|content| parse_mcp_servers(&content))
        .unwrap_or_default()
}

pub fn read_project_plugins_path(path: &Path) -> Vec<CodexPluginConfig> {
    std::fs::read_to_string(path)
        .map(|content| parse_project_plugins(&content))
        .unwrap_or_default()
}

pub fn read_plugin_enabled_config(path: &Path) -> HashMap<String, bool> {
    std::fs::read_to_string(path)
        .map(|content| parse_plugin_enabled_config(&content))
        .unwrap_or_default()
}

pub fn parse_mcp_servers(content: &str) -> Vec<CodexMcpServer> {
    #[derive(Default)]
    struct Pending {
        name: String,
        command: String,
        args: Vec<String>,
    }

    fn push(out: &mut Vec<CodexMcpServer>, pending: Option<Pending>) {
        if let Some(server) = pending {
            out.push(CodexMcpServer {
                name: server.name,
                command: server.command,
                args: server.args,
            });
        }
    }

    let mut out = Vec::new();
    let mut pending: Option<Pending> = None;
    for line in logical_lines(content) {
        if let Some(parts) = table_path(&line) {
            push(&mut out, pending.take());
            pending = mcp_server_name(&parts).map(|name| Pending {
                name,
                ..Pending::default()
            });
            continue;
        }

        let Some(server) = pending.as_mut() else {
            continue;
        };
        let Some((key, value)) = split_assignment(&line) else {
            continue;
        };
        match key.trim() {
            "command" => {
                server.command =
                    toml_string(value.trim()).unwrap_or_else(|| value.trim().to_string())
            }
            "args" => server.args = toml_string_array(value.trim()),
            _ => {}
        }
    }
    push(&mut out, pending);
    out
}

pub fn parse_project_plugins(content: &str) -> Vec<CodexPluginConfig> {
    let mut plugins = parse_plugin_enabled_config(content)
        .into_iter()
        .map(|(id, enabled)| CodexPluginConfig { id, enabled })
        .collect::<Vec<_>>();
    plugins.sort_by_key(|plugin| plugin.id.to_lowercase());
    plugins
}

pub fn parse_plugin_enabled_config(content: &str) -> HashMap<String, bool> {
    let mut out = HashMap::new();
    let mut current_plugin: Option<String> = None;
    for line in logical_lines(content) {
        if let Some(parts) = table_path(&line) {
            current_plugin = plugin_name(&parts);
            if let Some(plugin) = current_plugin.as_ref() {
                out.entry(plugin.clone()).or_insert(true);
            }
            continue;
        }

        let Some(plugin) = current_plugin.as_ref() else {
            continue;
        };
        let Some((key, value)) = split_assignment(&line) else {
            continue;
        };
        if key.trim() == "enabled" {
            if let Some(enabled) = toml_bool(value.trim()) {
                out.insert(plugin.clone(), enabled);
            }
        }
    }
    out
}

fn mcp_server_name(parts: &[String]) -> Option<String> {
    (parts.len() == 2 && parts[0] == "mcp_servers").then(|| parts[1].clone())
}

fn plugin_name(parts: &[String]) -> Option<String> {
    (parts.len() == 2 && parts[0] == "plugins").then(|| parts[1].clone())
}

fn logical_lines(content: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut bracket_depth = 0isize;

    for raw in content.lines() {
        let line = strip_comment(raw).trim().to_string();
        if line.is_empty() {
            continue;
        }
        if current.is_empty() {
            current.push_str(&line);
        } else {
            current.push(' ');
            current.push_str(&line);
        }

        bracket_depth += bracket_delta(&line);
        if bracket_depth <= 0 {
            lines.push(current.trim().to_string());
            current.clear();
            bracket_depth = 0;
        }
    }
    if !current.trim().is_empty() {
        lines.push(current.trim().to_string());
    }
    lines
}

fn table_path(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') || trimmed.starts_with("[[") {
        return None;
    }
    let inner = &trimmed[1..trimmed.len().saturating_sub(1)];
    split_dotted_path(inner)
}

fn split_dotted_path(value: &str) -> Option<Vec<String>> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut escaped = false;
    for ch in value.chars() {
        if in_string {
            if escaped {
                current.push(match ch {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    '"' => '"',
                    '\\' => '\\',
                    other => other,
                });
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '"' => in_string = false,
                other => current.push(other),
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '.' => {
                let part = current.trim();
                if part.is_empty() {
                    return None;
                }
                parts.push(part.to_string());
                current.clear();
            }
            other => current.push(other),
        }
    }
    if in_string || escaped {
        return None;
    }
    let part = current.trim();
    if part.is_empty() {
        return None;
    }
    parts.push(part.to_string());
    Some(parts)
}

fn split_assignment(line: &str) -> Option<(&str, &str)> {
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
        } else if ch == '=' {
            return Some((&line[..index], &line[index + 1..]));
        }
    }
    None
}

fn toml_bool(value: &str) -> Option<bool> {
    match value.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn toml_string(value: &str) -> Option<String> {
    let value = value.trim();
    let inner = value.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::new();
    let mut escaped = false;
    for ch in inner.chars() {
        if escaped {
            out.push(match ch {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '"' => '"',
                '\\' => '\\',
                other => other,
            });
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else {
            out.push(ch);
        }
    }
    if escaped {
        out.push('\\');
    }
    Some(out)
}

fn toml_string_array(value: &str) -> Vec<String> {
    let value = value.trim();
    let Some(inner) = value
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    else {
        return Vec::new();
    };
    split_array_items(inner)
        .into_iter()
        .filter_map(|item| toml_string(item.trim()))
        .collect()
}

fn split_array_items(value: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut start = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in value.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
        } else if ch == ',' {
            items.push(&value[start..index]);
            start = index + 1;
        }
    }
    items.push(&value[start..]);
    items
}

fn strip_comment(line: &str) -> String {
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
        } else if ch == '#' {
            return line[..index].to_string();
        }
    }
    line.to_string()
}

fn bracket_delta(line: &str) -> isize {
    let mut delta = 0;
    let mut in_string = false;
    let mut escaped = false;
    for ch in line.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '[' => delta += 1,
            ']' => delta -= 1,
            _ => {}
        }
    }
    delta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_codex_mcp_servers_with_quotes_comments_and_multiline_args() {
        let servers = parse_mcp_servers(
            r##"
[mcp_servers."context.7"] # keep the comment out
command = "npx"
args = [
  "-y",
  "@upstash/context7-mcp",
  "literal # not comment",
]

[mcp_servers.filesystem]
command = "node"
args = ["server.js"]
"##,
        );

        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].name, "context.7");
        assert_eq!(servers[0].command, "npx");
        assert_eq!(
            servers[0].args,
            vec!["-y", "@upstash/context7-mcp", "literal # not comment"]
        );
        assert_eq!(servers[1].name, "filesystem");
    }

    #[test]
    fn parses_only_top_level_codex_plugin_enablement() {
        let plugins = parse_plugin_enabled_config(
            r#"
[plugins."documents@openai-primary-runtime"]
enabled = true

[plugins."archived@openai-curated"]
enabled = false

[plugins."documents@openai-primary-runtime".mcp_servers.context7]
enabled = false
"#,
        );

        assert_eq!(plugins.get("documents@openai-primary-runtime"), Some(&true));
        assert_eq!(plugins.get("archived@openai-curated"), Some(&false));
        assert_eq!(plugins.len(), 2);
    }
}
