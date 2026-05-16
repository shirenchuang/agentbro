use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshConfigHost {
    pub name: String,
    pub hostname: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
}

pub fn read_ssh_config_hosts() -> Vec<SshConfigHost> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let path: PathBuf = home.join(".ssh").join("config");
    match fs::read_to_string(path) {
        Ok(content) => parse_ssh_config(&content),
        Err(_) => Vec::new(),
    }
}

pub fn parse_ssh_config(content: &str) -> Vec<SshConfigHost> {
    let mut hosts = Vec::new();
    let mut current: Option<SshConfigHost> = None;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = split_directive(line) else {
            continue;
        };
        let key = key.to_ascii_lowercase();
        let value = value.trim();

        if key == "host" {
            if let Some(host) = current.take() {
                hosts.push(host);
            }
            current = usable_host_name(value).map(|name| SshConfigHost {
                name,
                hostname: None,
                user: None,
                port: None,
                identity_file: None,
            });
            continue;
        }

        if key == "match" {
            if let Some(host) = current.take() {
                hosts.push(host);
            }
            continue;
        }

        let Some(host) = current.as_mut() else {
            continue;
        };

        match key.as_str() {
            "hostname" => host.hostname = Some(value.to_string()),
            "user" => host.user = Some(value.to_string()),
            "port" => host.port = value.parse::<u16>().ok(),
            "identityfile" => host.identity_file = Some(value.to_string()),
            _ => {}
        }
    }

    if let Some(host) = current {
        hosts.push(host);
    }

    hosts
}

fn split_directive(line: &str) -> Option<(&str, &str)> {
    if let Some((key, value)) = line.split_once('=') {
        return Some((key.trim(), value.trim()));
    }
    let mut parts = line.splitn(2, char::is_whitespace);
    let key = parts.next()?;
    let value = parts.next()?.trim();
    if value.is_empty() {
        None
    } else {
        Some((key, value))
    }
}

fn usable_host_name(value: &str) -> Option<String> {
    let name = value.split_whitespace().next()?.trim();
    if name.contains('*') || name.contains('?') || name == "localhost" || name == "127.0.0.1" {
        None
    } else {
        Some(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::parse_ssh_config;

    #[test]
    fn parses_basic_ssh_config_hosts() {
        let hosts = parse_ssh_config(
            r#"
Host *
  ServerAliveInterval 60

Host dev-server
  HostName 192.168.1.100
  User deploy
  Port 2222
  IdentityFile ~/.ssh/id_rsa

Host web
  HostName=web.example.com
"#,
        );

        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0].name, "dev-server");
        assert_eq!(hosts[0].hostname.as_deref(), Some("192.168.1.100"));
        assert_eq!(hosts[0].user.as_deref(), Some("deploy"));
        assert_eq!(hosts[0].port, Some(2222));
        assert_eq!(hosts[0].identity_file.as_deref(), Some("~/.ssh/id_rsa"));
        assert_eq!(hosts[1].name, "web");
        assert_eq!(hosts[1].hostname.as_deref(), Some("web.example.com"));
    }

    #[test]
    fn skips_wildcard_localhost_and_match_blocks() {
        let hosts = parse_ssh_config(
            r#"
Host localhost
  User local

Host server-?
  User deploy

Host before-match
  HostName before.example.com

Match host *.internal
  ForwardAgent yes

Host after-match
  HostName after.example.com
"#,
        );

        assert_eq!(
            hosts.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(),
            vec!["before-match", "after-match",]
        );
    }
}
