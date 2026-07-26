// Remote SSH support — tunnel management, auto-reconnect, hook installation
//
// External dependency required: tokio (already a Tauri dependency)
// No additional crates needed beyond std + tokio.

mod attach;
pub mod installer;
pub mod manager;
pub mod path;
pub mod skill_manager;
pub mod ssh_config;
pub mod ssh_tunnel;
pub mod terminal;

pub use manager::{ConnectionStatus, RemoteHost, RemoteManager};
pub use ssh_config::SshConfigHost;
