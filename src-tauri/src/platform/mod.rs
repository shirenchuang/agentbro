// Platform-specific utilities (display detection, etc.)

pub mod display;
pub mod display_controller;
pub mod idle;
#[cfg(target_os = "macos")]
pub(crate) mod macos_applications;
pub mod monitor_tracker;
pub mod notifications;
pub mod process;
#[cfg(target_os = "windows")]
pub(crate) mod windows_display;
#[cfg(target_os = "windows")]
pub(crate) mod windows_environment;
#[cfg(target_os = "windows")]
pub(crate) mod windows_shell;
#[cfg(target_os = "windows")]
pub(crate) mod windows_startup;
pub mod wsl;
