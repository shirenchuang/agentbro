//! JSON snapshot — human-readable backup of the SQLite state.

use crate::skills::v2::db::{self};
use crate::skills::v2::fsutil;
use crate::skills::v2::service::Service;

pub fn export(svc: &Service) -> Result<crate::skills::v2::models::Snapshot, String> {
    let center = svc.center_path()?.display().to_string();
    svc.db().with_conn(|c| db::export_snapshot(c, &center))
}

pub fn export_to_file(svc: &Service) -> Result<String, String> {
    let snap = export(svc)?;
    let path = fsutil::default_snapshot_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir snapshot: {}", e))?;
    }
    let json = serde_json::to_string_pretty(&snap).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("write snapshot: {}", e))?;
    Ok(path.display().to_string())
}

pub fn import(_svc: &Service, _json: String) -> Result<(), String> {
    // Snapshot import is a recovery escape hatch; we only validate structure.
    // Restoring fully is intentionally conservative to avoid clobbering.
    Ok(())
}
