use std::collections::BTreeMap;

pub fn parse_content(content: &str) -> BTreeMap<String, String> {
    let Some(frontmatter) = frontmatter_section(content) else {
        return BTreeMap::new();
    };
    parse_section(frontmatter)
}

pub fn parse_section(frontmatter: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let mut block: Option<BlockScalar> = None;

    for line in frontmatter.lines() {
        if let Some(active) = block.as_mut() {
            if active.accept_line(line) {
                continue;
            }
            let active = block.take().expect("block was active");
            active.insert_into(&mut map);
        }

        let Some((key, value)) = split_key_value(line) else {
            continue;
        };
        if key.is_empty() {
            continue;
        }
        if let Some(style) = block_scalar_style(value) {
            block = Some(BlockScalar::new(key.to_string(), style));
            continue;
        }

        let value = strip_quotes(value);
        if !value.is_empty() {
            map.insert(key.to_string(), value.to_string());
        }
    }

    if let Some(active) = block {
        active.insert_into(&mut map);
    }

    map
}

fn frontmatter_section(content: &str) -> Option<&str> {
    if !content.starts_with("---") {
        return None;
    }
    let rest = &content[3..];
    let end = rest.find("\n---")?;
    Some(&rest[..end])
}

fn split_key_value(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    let (key, value) = trimmed.split_once(':')?;
    Some((key.trim(), value.trim()))
}

fn block_scalar_style(value: &str) -> Option<BlockStyle> {
    match value.chars().next()? {
        '|' => Some(BlockStyle::Literal),
        '>' => Some(BlockStyle::Folded),
        _ => None,
    }
}

fn strip_quotes(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

#[derive(Clone, Copy)]
enum BlockStyle {
    Literal,
    Folded,
}

struct BlockScalar {
    key: String,
    style: BlockStyle,
    indent: Option<usize>,
    lines: Vec<String>,
}

impl BlockScalar {
    fn new(key: String, style: BlockStyle) -> Self {
        Self {
            key,
            style,
            indent: None,
            lines: Vec::new(),
        }
    }

    fn accept_line(&mut self, line: &str) -> bool {
        if line.trim().is_empty() {
            if self.indent.is_some() || !self.lines.is_empty() {
                self.lines.push(String::new());
            }
            return true;
        }

        let indent = leading_whitespace(line);
        if self.indent.is_none() {
            if indent == 0 && split_key_value(line).is_some() {
                return false;
            }
            self.indent = Some(indent);
        }

        let block_indent = self.indent.unwrap_or(indent);
        if indent < block_indent && split_key_value(line).is_some() {
            return false;
        }

        let content = line
            .char_indices()
            .nth(block_indent)
            .map(|(idx, _)| &line[idx..])
            .unwrap_or("");
        self.lines.push(content.trim_end().to_string());
        true
    }

    fn insert_into(self, map: &mut BTreeMap<String, String>) {
        let value = match self.style {
            BlockStyle::Literal => self.lines.join("\n").trim().to_string(),
            BlockStyle::Folded => fold_lines(&self.lines),
        };
        if !value.is_empty() {
            map.insert(self.key, value);
        }
    }
}

fn leading_whitespace(line: &str) -> usize {
    line.chars().take_while(|ch| ch.is_whitespace()).count()
}

fn fold_lines(lines: &[String]) -> String {
    let mut out = String::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !out.ends_with('\n') && !out.is_empty() {
                out.push('\n');
            }
        } else {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push(' ');
            }
            out.push_str(trimmed);
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_literal_block_after_blank_line() {
        let parsed = parse_content(
            "---\nname: post-creator\ndescription: |\n\n  第一行\n  第二行\nversion: 1\n---\n# Body\n",
        );
        assert_eq!(parsed.get("name").map(String::as_str), Some("post-creator"));
        assert_eq!(
            parsed.get("description").map(String::as_str),
            Some("第一行\n第二行")
        );
        assert_eq!(parsed.get("version").map(String::as_str), Some("1"));
    }

    #[test]
    fn parses_folded_block() {
        let parsed = parse_content("---\ndescription: >-\n  first\n  second\n---\n");
        assert_eq!(
            parsed.get("description").map(String::as_str),
            Some("first second")
        );
    }
}
