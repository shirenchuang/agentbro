use anyhow::Context;
use rusqlite::backup::{Backup, StepResult};
use rusqlite::{Connection, OpenFlags};
use std::fs;
use std::io;
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

pub fn migrate_sqlite(old_db: &Path, new_db: &Path) -> anyhow::Result<()> {
    if new_db.try_exists()? {
        return Ok(());
    }
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar = new_db.as_os_str().to_os_string();
        sidecar.push(suffix);
        let sidecar = Path::new(&sidecar);
        anyhow::ensure!(
            !sidecar.try_exists()?,
            "Cannot create {} while its database log {} exists without the main file",
            new_db.display(),
            sidecar.display()
        );
    }
    if !old_db.try_exists()? {
        return Ok(());
    }

    let parent = new_db
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let staging = parent.join(format!("agentbro-db-migration-{}", uuid::Uuid::new_v4()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(&staging)?;
    }
    #[cfg(not(unix))]
    fs::create_dir(&staging)?;
    let copy = staging.join("database.sqlite");
    let migration = copy_sqlite(old_db, &copy).and_then(|()| {
        fs::set_permissions(&copy, fs::metadata(old_db)?.permissions())?;
        match rename_path_if_absent(&copy, new_db) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(error.into()),
        }
    });
    if let Err(error) = fs::remove_dir_all(&staging) {
        log::warn!("Could not clean up SQLite migration directory: {error}");
    }
    migration.with_context(|| {
        format!(
            "Could not migrate SQLite database {} to {}",
            old_db.display(),
            new_db.display()
        )
    })
}

fn copy_sqlite(source: &Path, destination: &Path) -> anyhow::Result<()> {
    let source = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut destination = Connection::open(destination)?;
    let result = Backup::new(&source, &mut destination)?.step(-1)?;
    anyhow::ensure!(
        matches!(result, StepResult::Done),
        "The legacy database is busy. Try again."
    );
    Ok(())
}

#[cfg(unix)]
pub(crate) fn rename_path_if_absent(source: &Path, target: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let source = CString::new(source.as_os_str().as_bytes())?;
    let target = CString::new(target.as_os_str().as_bytes())?;
    #[cfg(target_os = "macos")]
    let result = unsafe {
        libc::renameatx_np(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            target.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            target.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let result = -1;
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(windows)]
pub(crate) fn rename_path_if_absent(source: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::MoveFileW;

    let source: Vec<_> = source.as_os_str().encode_wide().chain([0]).collect();
    let target: Vec<_> = target.as_os_str().encode_wide().chain([0]).collect();
    if unsafe { MoveFileW(source.as_ptr(), target.as_ptr()) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::installer::TempInstallDir;
    use rusqlite::config::DbConfig;

    fn write_wal_database(path: &Path) -> Connection {
        let db = Connection::open(path).unwrap();
        db.set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)
            .unwrap();
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA wal_autocheckpoint = 0;
             CREATE TABLE entries (value TEXT);
             INSERT INTO entries VALUES ('committed');",
        )
        .unwrap();
        db
    }

    #[test]
    fn leaves_legacy_wal_files_with_their_database_when_destination_exists() {
        let directory = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let old = directory.path().join("legacy.db");
        let new = directory.path().join("active.db");
        drop(write_wal_database(&old));
        let old_wal = old.with_extension("db-wal");
        let original_wal = fs::read(&old_wal).unwrap();
        assert!(!original_wal.is_empty());
        let existing = Connection::open(&new).unwrap();
        existing
            .execute_batch(
                "CREATE TABLE active_data (value TEXT); INSERT INTO active_data VALUES ('kept');",
            )
            .unwrap();
        drop(existing);
        let original_target = fs::read(&new).unwrap();

        migrate_sqlite(&old, &new).unwrap();

        assert!(old.is_file());
        assert_eq!(fs::read(&old_wal).unwrap(), original_wal);
        assert_eq!(fs::read(&new).unwrap(), original_target);
        assert!(!new.with_extension("db-wal").exists());
        assert!(!new.with_extension("db-shm").exists());
    }

    #[test]
    fn copies_committed_wal_entries_without_moving_an_open_database() {
        let directory = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let old = directory.path().join("legacy.db");
        let new = directory.path().join("当前资料").join("skills.sqlite3");
        let writer = write_wal_database(&old);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&old, fs::Permissions::from_mode(0o600)).unwrap();
        }
        writer
            .execute_batch("BEGIN IMMEDIATE; INSERT INTO entries VALUES ('uncommitted');")
            .unwrap();
        let old_wal = old.with_extension("db-wal");
        assert!(old_wal.is_file());

        migrate_sqlite(&old, &new).unwrap();

        assert!(old.is_file());
        assert!(old_wal.is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&new).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let migrated = Connection::open(&new).unwrap();
        let value: String = migrated
            .query_row("SELECT value FROM entries", [], |row| row.get(0))
            .unwrap();
        let count: i64 = migrated
            .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, "committed");
        assert_eq!(count, 1);
        writer.execute_batch("ROLLBACK").unwrap();
    }

    #[test]
    fn does_not_publish_an_unreadable_legacy_database() {
        let directory = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let old = directory.path().join("legacy.db");
        let new = directory.path().join("active.db");
        let original = b"unreadable SQLite contents";
        fs::write(&old, original).unwrap();

        let error = migrate_sqlite(&old, &new).unwrap_err();

        assert!(error
            .to_string()
            .contains("Could not migrate SQLite database"));
        assert!(!new.exists());
        assert_eq!(fs::read(&old).unwrap(), original);
    }

    #[test]
    fn preserves_orphaned_destination_logs_without_attaching_a_different_database() {
        let directory = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let old = directory.path().join("legacy.db");
        let new = directory.path().join("active.sqlite3");
        let orphan = directory.path().join("active.sqlite3-wal");
        drop(write_wal_database(&old));
        fs::write(&orphan, b"existing orphaned log").unwrap();

        let error = migrate_sqlite(&old, &new).unwrap_err();
        assert!(error.to_string().contains("without the main file"));
        assert!(!new.exists());
        assert!(old.is_file());
        assert_eq!(fs::read(orphan).unwrap(), b"existing orphaned log");
    }
}
