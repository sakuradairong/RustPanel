/*
 * @Descripttion:
 * @version:
 * @Author: Wynters
 * @Date: 2024-05-08 18:11:45
 * @LastEditTime: 2024-05-20 18:57:58
 * @FilePath: \RustPanel\src\server\db.rs
 */
use chrono::{NaiveDateTime, Utc};
use diesel::{r2d2::ConnectionManager, ExpressionMethods, RunQueryDsl, SqliteConnection};
use std::{fs, io::Write, path::{Path, PathBuf}};

use crate::{
    common::fun::generate_random_string,
    repository::migrations::{self, MigrationError},
    service::password::{self, ALGO_ARGON2ID},
};

pub type DBPool = r2d2::Pool<ConnectionManager<SqliteConnection>>;

/// Protected local delivery location; credentials never enter the application logger.
pub struct BootstrapAdmin {
    pub username: String,
    pub credentials_path: PathBuf,
}

/// Apply schema migrations, then seed the first admin when the user table is empty.
///
/// Schema authority is `schema_migrations`, not `panel.lock`. Lock file creation remains a
/// bootstrap marker for operators but never skips migrations. Errors refuse startup.
pub fn migrate_and_bootstrap(pool: &DBPool) -> Result<(i64, Option<BootstrapAdmin>), MigrationError> {
    let version = migrations::migrate(pool)?;
    let bootstrap = seed_initial_admin_if_empty(pool, &bootstrap_credentials_path(), Path::new("./panel.lock"))?;
    Ok((version, bootstrap))
}

pub fn bootstrap_credentials_path() -> PathBuf {
    std::env::var_os("RP_BOOTSTRAP_CREDENTIAL_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("./runtime/bootstrap-admin.txt"))
}

fn seed_initial_admin_if_empty(
    pool: &DBPool,
    credentials_path: &Path,
    marker_path: &Path,
) -> Result<Option<BootstrapAdmin>, MigrationError> {
    if migrations::user_count(pool)? > 0 {
        // Historical installs may still have panel.lock; schema is already authoritative.
        return Ok(None);
    }

    // Preserve prior behaviour: if an operator-created lock exists and there are no users,
    // do not silently invent a second bootstrap admin. Fail closed so recovery is explicit.
    if fs::metadata(marker_path)
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
    {
        return Err(MigrationError::new(
            "panel.lock exists but rp_users is empty; refuse automatic re-bootstrap",
        ));
    }

    let mut conn = pool.get().map_err(|err| {
        MigrationError::new(format!("Database pool unavailable during bootstrap: {err}"))
    })?;

    use crate::models::structure::schema;
    let now: NaiveDateTime = Utc::now().naive_utc();
    let username = generate_random_string(12);
    let password = generate_random_string(32);
    let password_hash = password::hash_argon2id(&password)
        .map_err(|_| MigrationError::new("Failed to hash initial admin password with Argon2id"))?;

    let result = conn
        .immediate_transaction::<_, crate::errors::common::CommonError, _>(|conn| {
            // Recheck under the write lock; concurrent startup must not seed two admins.
            #[derive(diesel::QueryableByName)]
            struct Count {
                #[diesel(sql_type = diesel::sql_types::BigInt)]
                count: i64,
            }
            let count: Count =
                diesel::sql_query("SELECT COUNT(*) AS count FROM rp_users").get_result(conn)?;
            if count.count != 0 {
                return Ok(None);
            }
            diesel::insert_into(schema::rp_users::table)
                .values((
                    schema::rp_users::username.eq(&username),
                    schema::rp_users::password.eq(&password_hash),
                    schema::rp_users::salt.eq(""),
                    schema::rp_users::authority.eq("admin"),
                    schema::rp_users::error_count.eq(0),
                    schema::rp_users::status.eq(0),
                    schema::rp_users::created_at.eq(now),
                    schema::rp_users::updated_at.eq(now),
                    schema::rp_users::password_algo.eq(ALGO_ARGON2ID),
                    schema::rp_users::must_change_password.eq(1),
                    schema::rp_users::token_version.eq(0),
                ))
                .execute(conn)
                .map_err(|err| {
                    crate::errors::common::CommonError::InternalServerError(format!(
                        "Failed to create initial admin user: {err}"
                    ))
                })?;

            // Fetch the new id and attach the admin role (migration seed only covers pre-existing rows).
            #[derive(diesel::QueryableByName)]
            struct IdRow {
                #[diesel(sql_type = diesel::sql_types::BigInt)]
                id: i64,
            }
            let rows: Vec<IdRow> =
                diesel::sql_query("SELECT id FROM rp_users WHERE username = ?1 LIMIT 1")
                    .bind::<diesel::sql_types::Text, _>(&username)
                    .load(conn)
                    .map_err(|err| {
                        crate::errors::common::CommonError::InternalServerError(format!(
                            "Failed to load bootstrap admin id: {err}"
                        ))
                    })?;
            let user_id = rows.first().map(|row| row.id as i32).ok_or_else(|| {
                crate::errors::common::CommonError::InternalServerError(
                    "Bootstrap admin row missing after insert".into(),
                )
            })?;
            crate::service::rbac::assign_role_for_authority(conn, user_id, "admin")
                .map_err(crate::errors::common::CommonError::InternalServerError)?;

            write_bootstrap_credentials(credentials_path, &username, &password).map_err(|e| {
                crate::errors::common::CommonError::InternalServerError(format!(
                    "Bootstrap credential delivery failed: {e}"
                ))
            })?;
            Ok(Some(BootstrapAdmin {
                username: username.clone(),
                credentials_path: credentials_path.to_path_buf(),
            }))
        })
        .map_err(|e| MigrationError::new(format!("Bootstrap transaction failed: {e}")))?;
    // This legacy marker is advisory: failure must not strand an already committed admin.
    if result.is_some() {
        if let Err(err) = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(marker_path)
        {
            crate::log_warn!("Unable to create legacy bootstrap marker: {err}");
        }
    }
    Ok(result)
}

/// Backward-compatible entry used by older call sites; prefer [`migrate_and_bootstrap`].
pub fn install(pool: &DBPool) {
    if let Err(err) = migrate_and_bootstrap(pool) {
        panic!("Database migrate/bootstrap failed: {}", err.message());
    }
}

pub fn init() {}

fn write_bootstrap_credentials(path: &Path, username: &str, password: &str) -> std::io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(parent)?;
    let metadata = fs::symlink_metadata(parent)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Credential directory must be a real directory",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o022 != 0 || metadata.uid() != unsafe { libc::geteuid() } {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Credential directory must be owned by the process user and not writable by others",
            ));
        }
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(path)?;
    let result = (|| {
        writeln!(file, "username:{username}\npassword:{password}")?;
        file.sync_all()?;
        fs::File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}

#[cfg(test)]
mod bootstrap_tests {
    use super::*;
    use diesel::{connection::SimpleConnection, r2d2::ConnectionManager};
    use std::os::unix::fs::{symlink, MetadataExt};

    fn database(path: &Path) -> DBPool {
        let pool = DBPool::builder()
            .max_size(1)
            .build(ConnectionManager::<SqliteConnection>::new(
                path.to_str().unwrap(),
            ))
            .unwrap();
        migrations::migrate(&pool).unwrap();
        pool
    }

    #[test]
    fn bootstrap_credentials_are_private_durable_and_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let pool = database(&dir.path().join("db"));
        let credentials = dir.path().join("credentials");
        let marker = dir.path().join("marker");
        let admin = seed_initial_admin_if_empty(&pool, &credentials, &marker)
            .unwrap()
            .unwrap();
        assert_eq!(fs::metadata(&credentials).unwrap().mode() & 0o7777, 0o600);
        let content = fs::read_to_string(&credentials).unwrap();
        assert!(content.contains(&admin.username));
        let password = content
            .lines()
            .find_map(|l| l.strip_prefix("password:"))
            .unwrap();
        let mut conn = pool.get().unwrap();
        use crate::models::structure::{rp_users::Users, schema::rp_users};
        let user: Users = rp_users::table.first(&mut *conn).unwrap();
        assert!(
            password::verify_stored(&user.password, &user.salt, &user.password_algo, password)
                .unwrap()
                .0
        );
        assert_eq!(user.must_change_password, 1);
        assert!(
            crate::service::rbac::user_has_permission(&mut conn, user.id, "user:admin").unwrap()
        );
        drop(conn);
        assert!(seed_initial_admin_if_empty(&pool, &credentials, &marker)
            .unwrap()
            .is_none());
        assert_eq!(fs::read_to_string(credentials).unwrap(), content);
    }

    #[test]
    fn failed_delivery_or_role_assignment_does_not_create_an_unreachable_admin() {
        for failure in ["existing", "symlink", "role"] {
            let dir = tempfile::tempdir().unwrap();
            let pool = database(&dir.path().join("db"));
            let credentials = dir.path().join("credentials");
            let marker = dir.path().join("marker");
            match failure {
                "existing" => fs::write(&credentials, "keep").unwrap(),
                "symlink" => symlink(dir.path().join("outside"), &credentials).unwrap(),
                _ => pool.get().unwrap().batch_execute("CREATE TRIGGER fail_role BEFORE INSERT ON user_roles BEGIN SELECT RAISE(ABORT, 'role failed'); END;").unwrap(),
            }
            assert!(seed_initial_admin_if_empty(&pool, &credentials, &marker).is_err());
            assert_eq!(migrations::user_count(&pool).unwrap(), 0);
            assert!(!marker.exists());
            if failure == "existing" {
                assert_eq!(fs::read_to_string(&credentials).unwrap(), "keep");
            }
            if failure == "role" {
                assert!(!credentials.exists());
            }
        }
    }
}
