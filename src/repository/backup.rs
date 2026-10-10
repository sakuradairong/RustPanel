//! Consistent SQLite snapshots for panel database recovery drills.
use diesel::connection::SimpleConnection;
use std::{
    fs,
    path::{Path, PathBuf},
};

use super::migrations::{self, MigrationError, EXPECTED_SCHEMA_VERSION};
use crate::service::db::DBPool;

/// Create a consistent on-disk copy with `VACUUM INTO`, then verify integrity and schema.
///
/// Do not copy `data.db3` while ignoring the WAL; this path asks SQLite for a self-contained
/// snapshot and refuses to return success unless the destination passes `integrity_check`
/// and carries the schema version expected by this binary.
pub fn backup_database(pool: &DBPool, destination: &Path) -> Result<PathBuf, MigrationError> {
    if destination.exists() {
        return Err(MigrationError::new(format!(
            "Backup destination already exists: {}",
            destination.display()
        )));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            MigrationError::new(format!(
                "Failed to create backup directory {}: {err}",
                parent.display()
            ))
        })?;
    }

    let absolute = resolve_backup_path(destination).map_err(|err| {
        MigrationError::new(format!(
            "Failed to resolve backup path {}: {err}",
            destination.display()
        ))
    })?;

    validate_backup_path(&absolute)?;

    let mut conn = pool.get().map_err(|err| {
        MigrationError::new(format!("Database pool unavailable for backup: {err}"))
    })?;

    let path_text = absolute
        .to_str()
        .ok_or_else(|| MigrationError::new("Backup path is not valid UTF-8"))?;
    // Quote the path for SQLite string literal after rejecting characters that break quoting.
    let sql = format!("VACUUM INTO '{}';", escape_sqlite_string(path_text));
    conn.batch_execute(&sql).map_err(|err| {
        MigrationError::new(format!(
            "VACUUM INTO failed for {}: {err}",
            absolute.display()
        ))
    })?;

    let version = migrations::verify_database_file(&absolute)?;
    if version != EXPECTED_SCHEMA_VERSION {
        let _ = fs::remove_file(&absolute);
        return Err(MigrationError::new(format!(
            "Backup schema version {version} does not match binary expectation {EXPECTED_SCHEMA_VERSION}"
        )));
    }
    Ok(absolute)
}

fn resolve_backup_path(destination: &Path) -> std::io::Result<PathBuf> {
    if let Ok(existing) = destination.canonicalize() {
        return Ok(existing);
    }
    // Destination does not exist yet; canonicalize the parent and join the file name.
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let file_name = destination.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing file name")
    })?;
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    fs::create_dir_all(parent)?;
    Ok(parent.canonicalize()?.join(file_name))
}

fn validate_backup_path(path: &Path) -> Result<(), MigrationError> {
    let text = path
        .to_str()
        .ok_or_else(|| MigrationError::new("Backup path is not valid UTF-8"))?;
    if text.contains('\0') || text.contains('\'') {
        return Err(MigrationError::new(
            "Backup path must not contain NUL or single-quote characters",
        ));
    }
    Ok(())
}

fn escape_sqlite_string(value: &str) -> String {
    value.replace('\'', "''")
}

/// Read-only verification entry used by tests and future CLI doctor/backup commands.
pub fn verify_backup_file(path: &Path) -> Result<i64, MigrationError> {
    migrations::verify_database_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::migrations::migrate;
    use diesel::{r2d2::ConnectionManager, SqliteConnection};
    use std::time::Duration;
    use tempfile::{tempdir, NamedTempFile};

    fn pool_for(path: &Path) -> DBPool {
        DBPool::builder()
            .max_size(1)
            .connection_timeout(Duration::from_millis(500))
            .build(ConnectionManager::<SqliteConnection>::new(
                path.to_str().unwrap(),
            ))
            .unwrap()
    }

    #[test]
    fn vacuum_backup_passes_integrity_and_schema_checks() {
        let source = NamedTempFile::new().unwrap();
        let pool = pool_for(source.path());
        migrate(&pool).unwrap();
        pool.get()
            .unwrap()
            .batch_execute(
                "INSERT INTO rp_users (
                    username, password, salt, authority,
                    error_count, status, created_at, updated_at,
                    password_algo, must_change_password, token_version
                ) VALUES (
                    'backup-user', 'hash', 'salt', 'admin', 0, 0,
                    '2026-10-10 00:00:00', '2026-10-10 00:00:00',
                    'sha1', 0, 0
                );",
            )
            .unwrap();

        let dir = tempdir().unwrap();
        let dest = dir.path().join("panel-backup.db3");
        let written = backup_database(&pool, &dest).unwrap();
        assert!(written.exists());
        assert_eq!(
            verify_backup_file(&written).unwrap(),
            EXPECTED_SCHEMA_VERSION
        );

        // Destination must remain a usable independent database.
        let restored = pool_for(&written);
        assert_eq!(migrations::user_count(&restored).unwrap(), 1);
    }

    #[test]
    fn refuses_to_overwrite_existing_backup_target() {
        let source = NamedTempFile::new().unwrap();
        let pool = pool_for(source.path());
        migrate(&pool).unwrap();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("exists.db3");
        fs::write(&dest, b"occupied").unwrap();
        let error = backup_database(&pool, &dest).unwrap_err();
        assert!(error.message().contains("already exists"));
    }
}
