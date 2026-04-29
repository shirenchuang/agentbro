// Remote SSH support — tunnel management, auto-reconnect, hook installation
//
// External dependency required: tokio (already a Tauri dependency)
// No additional crates needed beyond std + tokio.

pub mod manager;
pub mod ssh_tunnel;
pub mod installer;

pub use manager::{RemoteHost, RemoteManager, ConnectionStatus};
