//! Permission checks resolved from SQLite on every request (immediate effect).
use diesel::{
    sql_query,
    sql_types::{BigInt, Text},
    QueryableByName, RunQueryDsl, SqliteConnection,
};

use crate::{
    api::auth::AuthUser,
    errors::common::CommonError,
    service::db::DBPool,
};

pub const PERM_USER_ADMIN: &str = "user:admin";
pub const PERM_SITE_WRITE: &str = "site:write";
pub const PERM_FILE_WRITE: &str = "file:write";
pub const PERM_FILE_READ: &str = "file:read";
pub const PERM_APP_INSTALL: &str = "app:install";
pub const PERM_APP_UNINSTALL: &str = "app:uninstall";
pub const PERM_APP_READ: &str = "app:read";
pub const PERM_DOCKER_WRITE: &str = "docker:write";
pub const PERM_FIREWALL_WRITE: &str = "firewall:write";
pub const PERM_FIREWALL_READ: &str = "firewall:read";
pub const PERM_SERVICE_WRITE: &str = "service:write";
pub const PERM_PROCESS_WRITE: &str = "process:write";
pub const PERM_CERT_ISSUE: &str = "cert:issue";
pub const PERM_CERT_DEPLOY: &str = "cert:deploy";

#[derive(QueryableByName)]
struct CountRow {
    #[diesel(sql_type = BigInt)]
    count: i64,
}

pub fn user_has_permission(
    conn: &mut SqliteConnection,
    user_id: i32,
    permission: &str,
) -> Result<bool, CommonError> {
    let rows: Vec<CountRow> = sql_query(
        "SELECT COUNT(*) AS count
         FROM user_roles ur
         JOIN role_permissions rp ON rp.role_id = ur.role_id
         JOIN permissions p ON p.id = rp.permission_id
         WHERE ur.user_id = ?1 AND p.code = ?2",
    )
    .bind::<BigInt, _>(i64::from(user_id))
    .bind::<Text, _>(permission)
    .load(conn)
    .map_err(|_| CommonError::ServiceUnavailable("Authorization database unavailable".into()))?;
    if rows.first().map(|row| row.count > 0).unwrap_or(false) {
        return Ok(true);
    }
    // Compatibility: users created before role assignment still honor legacy authority.
    #[derive(QueryableByName)]
    struct AuthorityRow {
        #[diesel(sql_type = Text)]
        authority: String,
    }
    let authority: Vec<AuthorityRow> =
        sql_query("SELECT authority FROM rp_users WHERE id = ?1 LIMIT 1")
            .bind::<BigInt, _>(i64::from(user_id))
            .load(conn)
            .map_err(|_| {
                CommonError::ServiceUnavailable("Authorization database unavailable".into())
            })?;
    let Some(row) = authority.first() else {
        return Ok(false);
    };
    if row.authority == "admin" {
        return Ok(true);
    }
    Ok(false)
}

pub fn require_perm(pool: &DBPool, user: &AuthUser, permission: &str) -> Result<(), CommonError> {
    let mut conn = pool.get().map_err(|_| {
        CommonError::ServiceUnavailable("Authorization database unavailable".into())
    })?;
    if user_has_permission(&mut conn, user.id, permission)? {
        Ok(())
    } else {
        Err(CommonError::Forbidden(format!(
            "Missing permission: {permission}"
        )))
    }
}

/// Assign the built-in role that matches legacy authority strings.
pub fn assign_role_for_authority(
    conn: &mut SqliteConnection,
    user_id: i32,
    authority: &str,
) -> Result<(), String> {
    let role_code = if authority == "admin" { "admin" } else { "viewer" };
    sql_query("DELETE FROM user_roles WHERE user_id = ?1")
        .bind::<BigInt, _>(i64::from(user_id))
        .execute(conn)
        .map_err(|err| format!("Failed to clear user_roles: {err}"))?;
    sql_query(
        "INSERT INTO user_roles (user_id, role_id)
         SELECT ?1, id FROM roles WHERE code = ?2",
    )
    .bind::<BigInt, _>(i64::from(user_id))
    .bind::<Text, _>(role_code)
    .execute(conn)
    .map_err(|err| format!("Failed to assign role {role_code}: {err}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::migrations;
    use diesel::{connection::SimpleConnection, r2d2::ConnectionManager};
    use std::time::Duration;
    use tempfile::NamedTempFile;

    fn pool() -> DBPool {
        let file = NamedTempFile::new().unwrap();
        let path = file.into_temp_path();
        let path_str = path.to_str().unwrap().to_owned();
        std::mem::forget(path);
        let pool = DBPool::builder()
            .max_size(1)
            .connection_timeout(Duration::from_millis(500))
            .build(ConnectionManager::<SqliteConnection>::new(path_str))
            .unwrap();
        migrations::migrate(&pool).unwrap();
        pool
    }

    #[test]
    fn viewer_cannot_install_or_write_firewall_but_admin_can() {
        let pool = pool();
        pool.get()
            .unwrap()
            .batch_execute(
                "INSERT INTO rp_users (
                    id, username, password, salt, authority, error_count, status,
                    created_at, updated_at, password_algo, must_change_password, token_version
                ) VALUES
                (10, 'admin-user', 'h', '', 'admin', 0, 0, '2026-10-10 00:00:00', '2026-10-10 00:00:00', 'argon2id', 0, 0),
                (11, 'view-user', 'h', '', 'user', 0, 0, '2026-10-10 00:00:00', '2026-10-10 00:00:00', 'argon2id', 0, 0);
                INSERT OR IGNORE INTO user_roles (user_id, role_id) VALUES (10, 1), (11, 3);",
            )
            .unwrap();
        let mut conn = pool.get().unwrap();
        assert!(user_has_permission(&mut conn, 10, PERM_APP_INSTALL).unwrap());
        assert!(user_has_permission(&mut conn, 10, PERM_FIREWALL_WRITE).unwrap());
        assert!(!user_has_permission(&mut conn, 11, PERM_APP_INSTALL).unwrap());
        assert!(!user_has_permission(&mut conn, 11, PERM_SITE_WRITE).unwrap());
        assert!(!user_has_permission(&mut conn, 11, PERM_FILE_WRITE).unwrap());
        assert!(user_has_permission(&mut conn, 11, PERM_FILE_READ).unwrap());
    }
}
