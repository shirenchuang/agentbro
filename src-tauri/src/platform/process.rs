use std::ffi::OsStr;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn background_command<S: AsRef<OsStr>>(program: S) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    configure_background_command(&mut command);
    command
}

pub fn background_tokio_command<S: AsRef<OsStr>>(program: S) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(program);
    configure_background_tokio_command(&mut command);
    command
}

#[cfg(target_os = "windows")]
pub async fn windows_output_pipe() -> std::io::Result<(
    tokio::net::windows::named_pipe::NamedPipeServer,
    std::process::Stdio,
)> {
    use std::os::windows::fs::OpenOptionsExt;
    use tokio::net::windows::named_pipe::ServerOptions;

    // Tokio's standard child pipes use blocking reads on Windows that can outlive the timeout.
    let name = format!(r"\\.\pipe\AgentBro-output-{}", uuid::Uuid::new_v4());
    let reader = ServerOptions::new()
        .access_outbound(false)
        .first_pipe_instance(true)
        .reject_remote_clients(true)
        .max_instances(1)
        .create(&name)?;
    let writer = std::fs::OpenOptions::new()
        .write(true)
        .share_mode(0)
        .open(name)?;
    reader.connect().await?;
    Ok((reader, std::process::Stdio::from(writer)))
}

fn configure_background_command(command: &mut std::process::Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    #[cfg(not(target_os = "windows"))]
    let _ = command;
}

fn configure_background_tokio_command(command: &mut tokio::process::Command) {
    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }

    #[cfg(not(target_os = "windows"))]
    let _ = command;
}
