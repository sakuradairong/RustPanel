//! Versioned SQLite schema migrations. Fail closed before the panel accepts requests.
use crypto::{digest::Digest, sha2::Sha256};
use diesel::{
    connection::SimpleConnection,
    sql_query,
    sql_types::{BigInt, Text},
    QueryableByName,
    RunQueryDsl, SqliteConnection,
};
use std::path::Path;

use crate::service::db::DBPool;

/// Product binary expects this schema version after a successful migrate.
pub const EXPECTED_SCHEMA_VERSION: i64 = 7;

/// Bookkeeping table for applied migrations. Created outside numbered SQL so the
/// runner can always inspect history even on a blank database.
const ENSURE_MIGRATIONS_TABLE: &str = "
CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    checksum TEXT NOT NULL,
    applied_at TEXT NOT NULL
);
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationError {
    message: String,
}

impl MigrationError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for MigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for MigrationError {}

struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "001_initial",
        sql: include_str!("sql/001_initial.sql"),
    },
    Migration {
        version: 2,
        name: "002_argon2_password",
        sql: include_str!("sql/002_argon2_password.sql"),
    },
    Migration {
        version: 3,
        name: "003_sessions",
        sql: include_str!("sql/003_sessions.sql"),
    },
    Migration {
        version: 4,
        name: "004_rbac",
        sql: include_str!("sql/004_rbac.sql"),
    },
    Migration {
        version: 5,
        name: "005_audit",
        sql: include_str!("sql/005_audit.sql"),
    },
    Migration {
        version: 6,
        name: "006_sse_tickets",
        sql: include_str!("sql/006_sse_tickets.sql"),
    },
    Migration {
        version: 7,
        name: "007_secrets",
        sql: include_str!("sql/007_secrets.sql"),
    },
];

#[derive(QueryableByName)]
struct VersionRow {
    #[diesel(sql_type = BigInt)]
    version: i64,
}

#[derive(QueryableByName)]
struct ChecksumRow {
    #[diesel(sql_type = Text)]
    checksum: String,
}

#[derive(QueryableByName)]
struct CountRow {
    #[diesel(sql_type = BigInt)]
    count: i64,
}

fn checksum(sql: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.input_str(sql.trim());
    hasher.result_str()
}

fn now_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

fn table_exists(conn: &mut SqliteConnection, name: &str) -> Result<bool, MigrationError> {
    #[derive(QueryableByName)]
    struct PresentRow {
        #[diesel(sql_type = BigInt)]
        present: i64,
    }
    // Bind the table name; never interpolate identifiers from callers.
    let rows: Vec<PresentRow> = sql_query(
        "SELECT COUNT(*) AS present FROM sqlite_master WHERE type = 'table' AND name = ?1",
    )
    .bind::<Text, _>(name)
    .load(conn)
    .map_err(|err| MigrationError::new(format!("Failed to inspect table {name}: {err}")))?;
    Ok(rows.first().map(|row| row.present > 0).unwrap_or(false))
}

fn applied_versions(conn: &mut SqliteConnection) -> Result<Vec<(i64, String)>, MigrationError> {
    #[derive(QueryableByName)]
    struct AppliedRow {
        #[diesel(sql_type = BigInt)]
        version: i64,
        #[diesel(sql_type = Text)]
        checksum: String,
    }
    let rows: Vec<AppliedRow> =
        sql_query("SELECT version, checksum FROM schema_migrations ORDER BY version ASC")
            .load(conn)
            .map_err(|err| MigrationError::new(format!("Failed to read schema_migrations: {err}")))?;
    Ok(rows
        .into_iter()
        .map(|row| (row.version, row.checksum))
        .collect())
}

fn record_migration(
    conn: &mut SqliteConnection,
    migration: &Migration,
    digest: &str,
) -> Result<(), MigrationError> {
    sql_query(
        "INSERT INTO schema_migrations (version, name, checksum, applied_at) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind::<BigInt, _>(migration.version)
    .bind::<Text, _>(migration.name)
    .bind::<Text, _>(digest)
    .bind::<Text, _>(now_utc())
    .execute(conn)
    .map_err(|err| {
        MigrationError::new(format!(
            "Failed to record migration {}: {err}",
            migration.version
        ))
    })?;
    Ok(())
}

/// Adopt a pre-migration database that already has `rp_users` but no history rows.
fn adopt_legacy_baseline(conn: &mut SqliteConnection) -> Result<(), MigrationError> {
    let applied = applied_versions(conn)?;
    if !applied.is_empty() {
        return Ok(());
    }
    if !table_exists(conn, "rp_users")? {
        return Ok(());
    }
    let baseline = &MIGRATIONS[0];
    let digest = checksum(baseline.sql);
    record_migration(conn, baseline, &digest)?;
    Ok(())
}

fn apply_pending(conn: &mut SqliteConnection) -> Result<i64, MigrationError> {
    conn.batch_execute(ENSURE_MIGRATIONS_TABLE)
        .map_err(|err| MigrationError::new(format!("Failed to create schema_migrations: {err}")))?;

    adopt_legacy_baseline(conn)?;

    let applied = applied_versions(conn)?;
    let mut current = applied.last().map(|(version, _)| *version).unwrap_or(0);

    for migration in MIGRATIONS {
        let digest = checksum(migration.sql);
        if let Some((_, recorded)) = applied.iter().find(|(version, _)| *version == migration.version)
        {
            if recorded != &digest {
                return Err(MigrationError::new(format!(
                    "Migration {} checksum mismatch: database has a different SQL history than this binary",
                    migration.version
                )));
            }
            current = migration.version;
            continue;
        }

        if migration.version != current + 1 {
            return Err(MigrationError::new(format!(
                "Migration gap: expected version {}, found pending {}",
                current + 1,
                migration.version
            )));
        }

        // Keep DDL and bookkeeping in one immediate transaction so a crash mid-apply
        // does not leave an unrecorded schema change.
        conn.batch_execute("BEGIN IMMEDIATE;")
            .map_err(|err| MigrationError::new(format!("Failed to begin migration transaction: {err}")))?;

        if let Err(err) = conn.batch_execute(migration.sql) {
            let _ = conn.batch_execute("ROLLBACK;");
            return Err(MigrationError::new(format!(
                "Migration {} ({}) failed: {err}",
                migration.version, migration.name
            )));
        }
        if let Err(err) = record_migration(conn, migration, &digest) {
            let _ = conn.batch_execute("ROLLBACK;");
            return Err(err);
        }
        conn.batch_execute("COMMIT;")
            .map_err(|err| MigrationError::new(format!("Failed to commit migration {}: {err}", migration.version)))?;
        current = migration.version;
    }

    if current != EXPECTED_SCHEMA_VERSION {
        return Err(MigrationError::new(format!(
            "Schema version mismatch: database is at {current}, binary expects {EXPECTED_SCHEMA_VERSION}"
        )));
    }
    Ok(current)
}

/// Run all pending migrations. Refuses to start when schema and binary disagree.
pub fn migrate(pool: &DBPool) -> Result<i64, MigrationError> {
    let mut conn = pool
        .get()
        .map_err(|err| MigrationError::new(format!("Database pool unavailable for migrations: {err}")))?;
    apply_pending(&mut conn)
}

/// Current applied schema version, or an error if history is unreadable.
pub fn current_version(pool: &DBPool) -> Result<i64, MigrationError> {
    let mut conn = pool
        .get()
        .map_err(|err| MigrationError::new(format!("Database pool unavailable: {err}")))?;
    if !table_exists(&mut conn, "schema_migrations")? {
        return Ok(0);
    }
    let rows: Vec<VersionRow> =
        sql_query("SELECT COALESCE(MAX(version), 0) AS version FROM schema_migrations")
            .load(&mut conn)
            .map_err(|err| MigrationError::new(format!("Failed to read schema version: {err}")))?;
    Ok(rows.first().map(|row| row.version).unwrap_or(0))
}

/// Count rows in `rp_users` after migrations (used by bootstrap).
pub fn user_count(pool: &DBPool) -> Result<i64, MigrationError> {
    let mut conn = pool
        .get()
        .map_err(|err| MigrationError::new(format!("Database pool unavailable: {err}")))?;
    let rows: Vec<CountRow> = sql_query("SELECT COUNT(*) AS count FROM rp_users")
        .load(&mut conn)
        .map_err(|err| MigrationError::new(format!("Failed to count users: {err}")))?;
    Ok(rows.first().map(|row| row.count).unwrap_or(0))
}

/// Open a SQLite file read-only and confirm integrity plus expected schema version.
pub fn verify_database_file(path: &Path) -> Result<i64, MigrationError> {
    use diesel::Connection;
    let uri = format!("file:{}?mode=ro", path.display());
    let mut conn = SqliteConnection::establish(&uri)
        .map_err(|err| MigrationError::new(format!("Failed to open database for verification: {err}")))?;

    #[derive(QueryableByName)]
    struct IntegrityRow {
        #[diesel(sql_type = Text)]
        integrity_check: String,
    }
    let integrity: Vec<IntegrityRow> = sql_query("PRAGMA integrity_check")
        .load(&mut conn)
        .map_err(|err| MigrationError::new(format!("integrity_check failed: {err}")))?;
    let ok = integrity
        .first()
        .map(|row| row.integrity_check.eq_ignore_ascii_case("ok"))
        .unwrap_or(false);
    if !ok {
        return Err(MigrationError::new(format!(
            "SQLite integrity_check failed: {:?}",
            integrity
                .first()
                .map(|row| row.integrity_check.as_str())
                .unwrap_or("empty")
        )));
    }

    if !table_exists(&mut conn, "schema_migrations")? {
        return Err(MigrationError::new(
            "Verified database is missing schema_migrations",
        ));
    }
    let expected_migration = MIGRATIONS
        .iter()
        .find(|migration| migration.version == EXPECTED_SCHEMA_VERSION)
        .ok_or_else(|| {
            MigrationError::new("Binary is missing the expected schema migration definition")
        })?;
    let rows: Vec<ChecksumRow> = sql_query(
        "SELECT checksum FROM schema_migrations WHERE version = ?1 LIMIT 1",
    )
    .bind::<BigInt, _>(EXPECTED_SCHEMA_VERSION)
    .load(&mut conn)
    .map_err(|err| MigrationError::new(format!("Failed to read migration checksum: {err}")))?;
    let expected = checksum(expected_migration.sql);
    let recorded = rows
        .first()
        .ok_or_else(|| MigrationError::new("Verified database missing expected schema version"))?;
    if recorded.checksum != expected {
        return Err(MigrationError::new(
            "Verified database migration checksum does not match this binary",
        ));
    }
    Ok(EXPECTED_SCHEMA_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel::r2d2::ConnectionManager;
    use std::time::Duration;
    use tempfile::NamedTempFile;

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
    fn empty_database_reaches_expected_version() {
        let file = NamedTempFile::new().unwrap();
        let pool = pool_for(file.path());
        assert_eq!(migrate(&pool).unwrap(), EXPECTED_SCHEMA_VERSION);
        assert_eq!(current_version(&pool).unwrap(), EXPECTED_SCHEMA_VERSION);
        assert!(table_exists(&mut pool.get().unwrap(), "rp_users").unwrap());
        // Idempotent.
        assert_eq!(migrate(&pool).unwrap(), EXPECTED_SCHEMA_VERSION);
    }

    #[test]
    fn legacy_database_with_users_is_adopted_without_data_loss() {
        let file = NamedTempFile::new().unwrap();
        let pool = pool_for(file.path());
        pool.get()
            .unwrap()
            .batch_execute(
                "CREATE TABLE rp_users (
            id INTEGER PRIMARY KEY, username TEXT NOT NULL, password TEXT NOT NULL,
            salt TEXT NOT NULL, initial_password TEXT NOT NULL, authority TEXT NOT NULL,
            error_count INTEGER NOT NULL, status INTEGER NOT NULL,
            created_at TIMESTAMP NOT NULL, updated_at TIMESTAMP NOT NULL
        ); INSERT INTO rp_users VALUES (
            1, 'legacy-admin', 'hash', 'salt', 'plain', 'admin', 0, 0,
            '2026-10-10 00:00:00', '2026-10-10 00:00:00'
        );",
            )
            .unwrap();
        assert_eq!(migrate(&pool).unwrap(), EXPECTED_SCHEMA_VERSION);
        assert_eq!(user_count(&pool).unwrap(), 1);
        let rows: Vec<CountRow> = sql_query(
            "SELECT COUNT(*) AS count FROM rp_users WHERE username = 'legacy-admin'",
        )
        .load(&mut *pool.get().unwrap())
        .unwrap();
        assert_eq!(rows[0].count, 1);
        // RP-01 clears historical plaintext and flags a mandatory password change.
        let flagged: Vec<CountRow> = sql_query(
            "SELECT COUNT(*) AS count FROM rp_users WHERE must_change_password = 1",
        )
        .load(&mut *pool.get().unwrap())
        .unwrap();
        assert_eq!(flagged[0].count, 1);
        let has_column: Vec<CountRow> = sql_query(
            "SELECT COUNT(*) AS count FROM pragma_table_info('rp_users') WHERE name = 'initial_password'",
        )
        .load(&mut *pool.get().unwrap())
        .unwrap();
        assert_eq!(has_column[0].count, 0);
    }

    #[test]
    fn checksum_mismatch_refuses_to_continue() {
        let file = NamedTempFile::new().unwrap();
        let pool = pool_for(file.path());
        migrate(&pool).unwrap();
        pool.get()
            .unwrap()
            .batch_execute(
                "UPDATE schema_migrations SET checksum = 'tampered-checksum' WHERE version = 1",
            )
            .unwrap();
        let error = migrate(&pool).unwrap_err();
        assert!(error.message().contains("checksum mismatch"));
    }
}
