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
    common::fun::{generate_random_string, sha1_salt},
    repository::migrations::{self, MigrationError},
};

pub type DBPool = r2d2::Pool<ConnectionManager<SqliteConnection>>;

/// Apply schema migrations, then seed the first admin when the user table is empty.
///
/// Schema authority is `schema_migrations`, not `panel.lock`. Lock file creation remains a
/// bootstrap marker for operators but never skips migrations. Errors refuse startup.
pub fn migrate_and_bootstrap(pool: &DBPool) -> Result<i64, MigrationError> {
    let version = migrations::migrate(pool)?;
    seed_initial_admin_if_empty(pool)?;
    Ok(version)
}

fn seed_initial_admin_if_empty(pool: &DBPool) -> Result<(), MigrationError> {
    if migrations::user_count(pool)? > 0 {
        // Historical installs may still have panel.lock; schema is already authoritative.
        return Ok(());
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
    let password = generate_random_string(rng.gen_range(8..18));
    let salt = generate_random_string(rng.gen_range(8..16));
    let password_clone = password.clone();
    let salt_clone = salt.clone();

    diesel::insert_into(schema::rp_users::table)
        .values((
            schema::rp_users::username.eq(generate_random_string(rng.gen_range(6..12))),
            schema::rp_users::password.eq(sha1_salt(password, salt)),
            schema::rp_users::salt.eq(salt_clone),
            schema::rp_users::initial_password.eq(password_clone),
            schema::rp_users::authority.eq("admin"),
            schema::rp_users::error_count.eq(0),
            schema::rp_users::status.eq(0),
            schema::rp_users::created_at.eq(now),
            schema::rp_users::updated_at.eq(now),
        ))
        .execute(&mut *conn)
        .map_err(|err| MigrationError::new(format!("Failed to create initial admin user: {err}")))?;

    fs::File::create("./panel.lock").map_err(|err| {
        MigrationError::new(format!("Failed to write panel.lock after bootstrap: {err}"))
    })?;
    Ok(())
}

/// Backward-compatible entry used by older call sites; prefer [`migrate_and_bootstrap`].
pub fn install(pool: &DBPool) {
    if let Err(err) = migrate_and_bootstrap(pool) {
        panic!("Database migrate/bootstrap failed: {}", err.message());
    }
}

pub fn init() {}
