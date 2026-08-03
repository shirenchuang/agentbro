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
