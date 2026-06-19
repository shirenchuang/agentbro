use std::path::{Path, PathBuf};

pub fn agentbro_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".agentbro")
}

pub fn migrate_file(old: &Path, new: &Path) {
    if old.exists() && !new.exists() {
        if let Some(parent) = new.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::rename(old, new);
    }
}

pub fn migrate_sqlite(old_db: &Path, new_db: &Path) {
    migrate_file(old_db, new_db);
    let old_wal = old_db.with_extension("db-wal");
    let new_wal = new_db.with_extension("db-wal");
    migrate_file(&old_wal, &new_wal);
    let old_shm = old_db.with_extension("db-shm");
    let new_shm = new_db.with_extension("db-shm");
    migrate_file(&old_shm, &new_shm);
}
