use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepLinkPayload {
    pub resource: String,
    pub params: HashMap<String, String>,
}

pub fn parse_deep_link(url: &str) -> Option<DeepLinkPayload> {
    // agentbro://v1/import?resource=provider&app=claude&name=My+Provider&apiKey=sk-xxx&baseUrl=https://...
    // ccswitch://v1/import?resource=provider&...
    let url = url.trim();
    let stripped = if let Some(rest) = url.strip_prefix("agentbro://") {
        rest
    } else if let Some(rest) = url.strip_prefix("ccswitch://") {
        rest
    } else {
        return None;
    };

    let path_and_query: Vec<&str> = stripped.splitn(2, '?').collect();
    let path = path_and_query.first()?;

    if !path.starts_with("v1/import") {
        return None;
    }

    let query = path_and_query.get(1).unwrap_or(&"");
    let params: HashMap<String, String> = query
        .split('&')
        .filter(|s| !s.is_empty())
        .filter_map(|pair| {
            let mut kv = pair.splitn(2, '=');
            let k = kv.next()?;
            let v = kv.next().unwrap_or("");
            Some((url_decode(k), url_decode(v)))
        })
        .collect();

    let resource = params.get("resource")?.clone();

    Some(DeepLinkPayload { resource, params })
}

fn url_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.bytes();
    while let Some(b) = chars.next() {
        match b {
            b'+' => result.push(' '),
            b'%' => {
                let h = chars.next().and_then(|c| (c as char).to_digit(16));
                let l = chars.next().and_then(|c| (c as char).to_digit(16));
                if let (Some(h), Some(l)) = (h, l) {
                    result.push((h * 16 + l) as u8 as char);
                }
            }
            _ => result.push(b as char),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_provider() {
        let url = "agentbro://v1/import?resource=provider&app=claude&name=My+Provider&baseUrl=https%3A%2F%2Fapi.anthropic.com";
        let payload = parse_deep_link(url).unwrap();
        assert_eq!(payload.resource, "provider");
        assert_eq!(payload.params.get("app").unwrap(), "claude");
        assert_eq!(payload.params.get("name").unwrap(), "My Provider");
        assert_eq!(
            payload.params.get("baseUrl").unwrap(),
            "https://api.anthropic.com"
        );
    }

    #[test]
    fn test_parse_ccswitch_compat() {
        let url = "ccswitch://v1/import?resource=mcp&name=my-server";
        let payload = parse_deep_link(url).unwrap();
        assert_eq!(payload.resource, "mcp");
        assert_eq!(payload.params.get("name").unwrap(), "my-server");
    }

    #[test]
    fn test_invalid_scheme() {
        assert!(parse_deep_link("https://example.com").is_none());
    }
}
