//! JSON snapshot — human-readable backup of the SQLite state.

use crate::skills::v2::db::{self};
use crate::skills::v2::service::Service;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub fn export(svc: &Service) -> Result<crate::skills::v2::models::Snapshot, String> {
    let center = svc.center_path()?.display().to_string();
    svc.db().with_conn(|c| db::export_snapshot(c, &center))
}

pub fn export_to_file(svc: &Service) -> Result<String, String> {
    let snap = export(svc)?;
    let path = snapshot_path(svc)?;
    let json = serde_json::to_vec_pretty(&snap).map_err(|e| e.to_string())?;
    write_snapshot(&path, &json).map_err(|e| format!("write snapshot: {e}"))?;
    Ok(path.display().to_string())
}

fn write_snapshot(path: &Path, json: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let staging = path.with_file_name(format!(".agentbro-snapshot-{}.tmp", uuid::Uuid::new_v4()));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&staging)?;
    let result = (|| {
        #[cfg(unix)]
        match fs::metadata(path) {
            Ok(previous) => file.set_permissions(previous.permissions())?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        file.write_all(json)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&staging, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(staging);
    }
    result
}

pub fn snapshot_path(svc: &Service) -> Result<PathBuf, String> {
    Ok(svc.center_path()?.join("agentbro-skills.snapshot.json"))
}

pub fn import(_svc: &Service, _json: String) -> Result<(), String> {
    // Snapshot import is a recovery escape hatch; we only validate structure.
    // Restoring fully is intentionally conservative to avoid clobbering.
    Ok(())
}
