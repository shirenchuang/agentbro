use super::manager::RemoteHost;

pub fn launch_at_path(
    host: &RemoteHost,
    directory: &str,
    target_name: Option<&str>,
) -> Result<(), String> {
    let shell_command = terminal_shell_command(host, directory, target_name);

    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("osascript")
            .args([
                "-e",
                "on run argv",
                "-e",
                "tell application \"Terminal\"",
                "-e",
                "activate",
                "-e",
                "do script (item 1 of argv)",
                "-e",
                "end tell",
                "-e",
                "end run",
            ])
            .arg(shell_command)
            .output()
            .map_err(|error| format!("Launch remote terminal: {error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = shell_command;
        Err("Opening a remote terminal is only supported on macOS".to_string())
    }
}

fn terminal_shell_command(host: &RemoteHost, directory: &str, target_name: Option<&str>) -> String {
    let remote_command = remote_shell_command(directory, target_name);
    let mut parts = Vec::new();
    if let Some(auth_socket) = host
        .auth_socket
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        parts.push(format!(
            "SSH_AUTH_SOCK={}",
            shell_quote(&super::path::expand_tilde(auth_socket))
        ));
    }
    parts.push(shell_quote(
        &crate::agents::executable::command_path("ssh").to_string_lossy(),
    ));
    if let Some(port) = host.port {
        parts.push("-p".to_string());
        parts.push(port.to_string());
    }
    if let Some(identity) = host
        .identity_file
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        parts.push("-i".to_string());
        parts.push(shell_quote(&super::path::expand_tilde(identity)));
    }
    parts.push("-t".to_string());
    parts.push(shell_quote(&host.ssh_target));
    parts.push(shell_quote(&remote_command));
    parts.join(" ")
}

fn remote_shell_command(directory: &str, target_name: Option<&str>) -> String {
    let directory = shell_quote(directory);
    let interactive_shell = r#"exec "${SHELL:-/bin/bash}" -l"#;
    match target_name.filter(|value| !value.is_empty()) {
        Some(name) => {
            let name = shell_quote(name);
            format!(
                "cd {directory} && {{ printf '\\nAgentBro target: %s\\n' {name}; ls -ld -- {name} 2>/dev/null || true; {interactive_shell}; }}"
            )
        }
        None => format!("cd {directory} && {interactive_shell}"),
    }
}

fn shell_quote(value: &str) -> String {
    crate::agents::hook_manager::shell_quote(value)
}

#[cfg(test)]
mod tests {
    use super::{remote_shell_command, terminal_shell_command};
    use crate::remote::manager::RemoteHost;

    fn host() -> RemoteHost {
        RemoteHost {
            id: "server".to_string(),
            name: "Server".to_string(),
            ssh_target: "dev@example.com".to_string(),
            port: Some(2222),
            identity_file: Some("/Users/me/Keys/team key".to_string()),
            auth_socket: Some("/tmp/agent socket".to_string()),
            remote_socket_path: "/tmp/agentbro.sock".to_string(),
            auto_connect: false,
        }
    }

    #[test]
    fn directory_command_enters_remote_path_before_starting_shell() {
        let command = terminal_shell_command(&host(), "/home/dev/project files", None);
        assert!(command.contains("SSH_AUTH_SOCK='/tmp/agent socket'"));
        assert!(command.contains("-p 2222"));
        assert!(command.contains("-i '/Users/me/Keys/team key'"));
        assert!(command.contains("-t 'dev@example.com'"));
        assert!(command.contains("cd '\\''/home/dev/project files'\\''"));
        assert!(command.contains(r#"exec "${SHELL:-/bin/bash}" -l"#));
    }

    #[test]
    fn file_command_enters_parent_and_identifies_target() {
        let command = remote_shell_command("/home/dev/config dir", Some("config's file.toml"));
        assert!(command.starts_with("cd '/home/dev/config dir'"));
        assert!(command.contains(r#"'config'\''s file.toml'"#));
        assert!(command.contains("ls -ld --"));
        assert!(command.ends_with(r#"exec "${SHELL:-/bin/bash}" -l; }"#));
    }
}
