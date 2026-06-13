//! Skill Manager v2 — SQLite-backed skill management subsystem.

pub mod agent_meta;
pub mod commands;
pub mod db;
pub mod diagnosis;
pub mod fsutil;
pub mod models;
pub mod service;
pub mod snapshot;
#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

static SERVICE: OnceLock<Arc<service::Service>> = OnceLock::new();

/// Initialize (or reuse) the global service bound to the settings' sqlite path.
pub fn service() -> Result<Arc<service::Service>, String> {
    if let Some(svc) = SERVICE.get() {
        return Ok(svc.clone());
    }
    let svc = build_service()?;
    // store a clone; if racing, the first wins
    let _ = SERVICE.set(svc.clone());
    Ok(SERVICE.get().unwrap().clone())
}

/// Rebuild the service (e.g. after sqlite path setting changed). Returns the
/// new service; the global slot is replaced.
pub fn rebuild_service() -> Result<Arc<service::Service>, String> {
    let svc = build_service()?;
    // OnceLock can't be reset; store into a separate Mutex-free replaceable cell.
    // We rely on the inner Db being reopenable instead — so here we just return
    // the freshly built service without replacing the global lock.
    Ok(svc)
}

fn build_service() -> Result<Arc<service::Service>, String> {
    let home = fsutil::home();
    // Temporarily open a transient connection to read the configured sqlite path.
    let sqlite_path = resolve_sqlite_path(&home)?;
    let svc = service::Service::new(&sqlite_path, home.clone())?;
    svc.init()?;
    Ok(Arc::new(svc))
}

fn resolve_sqlite_path(home: &std::path::Path) -> Result<PathBuf, String> {
    let default = fsutil::default_sqlite_path();
    let settings_file = home.join(".agentbro/skill-manager-settings.json");
    if let Ok(content) = std::fs::read_to_string(&settings_file) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(p) = v.get("sqlitePath").and_then(|p| p.as_str()) {
                if !p.is_empty() {
                    return Ok(fsutil::expand_tilde(p));
                }
            }
        }
    }
    Ok(default)
}
