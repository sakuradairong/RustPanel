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
use rand::Rng;
use std::fs;

use crate::{
    common::fun::generate_random_string,
    repository::migrations::{self, MigrationError},
    service::password::{self, ALGO_ARGON2ID},
};

pub type DBPool = r2d2::Pool<ConnectionManager<SqliteConnection>>;

/// One-time bootstrap credentials. Printed once at first install; never stored in plaintext.
pub struct BootstrapAdmin {
    pub username: String,
    pub password: String,
}

/// Apply schema migrations, then seed the first admin when the user table is empty.
///
/// Schema authority is `schema_migrations`, not `panel.lock`. Lock file creation remains a
/// bootstrap marker for operators but never skips migrations. Errors refuse startup.
pub fn migrate_and_bootstrap(pool: &DBPool) -> Result<(i64, Option<BootstrapAdmin>), MigrationError> {
    let version = migrations::migrate(pool)?;
    let bootstrap = seed_initial_admin_if_empty(pool)?;
    Ok((version, bootstrap))
}

fn seed_initial_admin_if_empty(pool: &DBPool) -> Result<Option<BootstrapAdmin>, MigrationError> {
    if migrations::user_count(pool)? > 0 {
        // Historical installs may still have panel.lock; schema is already authoritative.
        return Ok(None);
    }

    // Preserve prior behaviour: if an operator-created lock exists and there are no users,
    // do not silently invent a second bootstrap admin. Fail closed so recovery is explicit.
    if fs::metadata("./panel.lock")
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
    let mut rng = rand::thread_rng();
    let username = generate_random_string(rng.gen_range(6..12));
    let password = generate_random_string(rng.gen_range(8..18));
    let password_hash = password::hash_argon2id(&password)
        .map_err(|_| MigrationError::new("Failed to hash initial admin password with Argon2id"))?;

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
        .execute(&mut *conn)
        .map_err(|err| MigrationError::new(format!("Failed to create initial admin user: {err}")))?;

    fs::File::create("./panel.lock").map_err(|err| {
        MigrationError::new(format!("Failed to write panel.lock after bootstrap: {err}"))
    })?;
    Ok(Some(BootstrapAdmin { username, password }))
}

/// Backward-compatible entry used by older call sites; prefer [`migrate_and_bootstrap`].
pub fn install(pool: &DBPool) {
    if let Err(err) = migrate_and_bootstrap(pool) {
        panic!("Database migrate/bootstrap failed: {}", err.message());
    }
}

pub fn init() {}
