//! Revocable login sessions keyed by JWT `jti`.
use crypto::{digest::Digest, sha2::Sha256};
use diesel::{
    sql_query,
    sql_types::{BigInt, Nullable, Text},
    RunQueryDsl, SqliteConnection,
};

use crate::{common::fun::generate_random_string, errors::common::CommonError};

#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub user_id: i32,
    pub jti: String,
    pub expires_at_unix: i64,
}

pub fn hash_jti(jti: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.input_str(jti);
    hasher.result_str()
}

pub fn new_jti() -> String {
    generate_random_string(32)
}

pub fn create_session(
    conn: &mut SqliteConnection,
    user_id: i32,
    ttl_seconds: i64,
    ip: Option<&str>,
    user_agent: Option<&str>,
) -> Result<SessionRecord, String> {
    let jti = new_jti();
    let jti_hash = hash_jti(&jti);
    let now = chrono::Utc::now().timestamp();
    let expires_at_unix = now + ttl_seconds;
    let issued_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let expires_at = chrono::DateTime::from_timestamp(expires_at_unix, 0)
        .unwrap_or_else(|| chrono::Utc::now())
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let ua_hash = user_agent.map(hash_jti);

    sql_query(
        "INSERT INTO sessions (jti_hash, user_id, issued_at, expires_at, revoked_at, ua_hash, ip)
         VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6)",
    )
    .bind::<Text, _>(&jti_hash)
    .bind::<BigInt, _>(i64::from(user_id))
    .bind::<Text, _>(&issued_at)
    .bind::<Text, _>(&expires_at)
    .bind::<Nullable<Text>, _>(ua_hash.as_deref())
    .bind::<Nullable<Text>, _>(ip)
    .execute(conn)
    .map_err(|err| format!("Failed to create session: {err}"))?;

    Ok(SessionRecord {
        user_id,
        jti,
        expires_at_unix,
    })
}

#[derive(diesel::QueryableByName)]
struct SessionRow {
    #[diesel(sql_type = BigInt)]
    user_id: i64,
    #[diesel(sql_type = Text)]
    expires_at: String,
    #[diesel(sql_type = Nullable<Text>)]
    revoked_at: Option<String>,
}

/// Returns Ok(()) when the session is present, unrevoked, and not expired.
pub fn validate_session(
    conn: &mut SqliteConnection,
    jti: &str,
    expected_user_id: i32,
) -> Result<(), CommonError> {
    let jti_hash = hash_jti(jti);
    let rows: Vec<SessionRow> = sql_query(
        "SELECT user_id, expires_at, revoked_at FROM sessions WHERE jti_hash = ?1 LIMIT 1",
    )
    .bind::<Text, _>(&jti_hash)
    .load(conn)
    .map_err(|_| CommonError::ServiceUnavailable("Authentication database unavailable".into()))?;

    let row = rows
        .into_iter()
        .next()
        .ok_or_else(|| CommonError::Unauthorized("Invalid credentials".into()))?;
    if row.user_id != i64::from(expected_user_id) {
        return Err(CommonError::Unauthorized("Invalid credentials".into()));
    }
    if row.revoked_at.is_some() {
        return Err(CommonError::Unauthorized("Invalid credentials".into()));
    }
    let expires = chrono::NaiveDateTime::parse_from_str(&row.expires_at, "%Y-%m-%d %H:%M:%S")
        .map_err(|_| CommonError::Unauthorized("Invalid credentials".into()))?;
    if expires < chrono::Utc::now().naive_utc() {
        return Err(CommonError::Unauthorized("Invalid credentials".into()));
    }
    Ok(())
}

pub fn revoke_session(conn: &mut SqliteConnection, jti: &str) -> Result<(), String> {
    let jti_hash = hash_jti(jti);
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    sql_query(
        "UPDATE sessions SET revoked_at = ?1 WHERE jti_hash = ?2 AND revoked_at IS NULL",
    )
    .bind::<Text, _>(&now)
    .bind::<Text, _>(&jti_hash)
    .execute(conn)
    .map_err(|err| format!("Failed to revoke session: {err}"))?;
    Ok(())
}

pub fn revoke_all_for_user(conn: &mut SqliteConnection, user_id: i32) -> Result<(), String> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    sql_query(
        "UPDATE sessions SET revoked_at = ?1 WHERE user_id = ?2 AND revoked_at IS NULL",
    )
    .bind::<Text, _>(&now)
    .bind::<BigInt, _>(i64::from(user_id))
    .execute(conn)
    .map_err(|err| format!("Failed to revoke user sessions: {err}"))?;
    Ok(())
}

pub fn bump_token_version(conn: &mut SqliteConnection, user_id: i32) -> Result<i32, String> {
    sql_query("UPDATE rp_users SET token_version = token_version + 1 WHERE id = ?1")
        .bind::<BigInt, _>(i64::from(user_id))
        .execute(conn)
        .map_err(|err| format!("Failed to bump token_version: {err}"))?;
    #[derive(diesel::QueryableByName)]
    struct VersionRow {
        #[diesel(sql_type = BigInt)]
        token_version: i64,
    }
    let rows: Vec<VersionRow> = sql_query("SELECT token_version FROM rp_users WHERE id = ?1")
        .bind::<BigInt, _>(i64::from(user_id))
        .load(conn)
        .map_err(|err| format!("Failed to read token_version: {err}"))?;
    rows.first()
        .map(|row| row.token_version as i32)
        .ok_or_else(|| "User missing after token_version bump".into())
}

/// Delete sessions that no longer grant anything: expired rows, and rows
/// revoked more than a day ago. Revocation stays enforced after deletion
/// because `validate_session` rejects unknown jti (fail-closed), but recent
/// revocations are kept for operator visibility.
pub fn purge_expired(conn: &mut SqliteConnection) -> Result<u64, String> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let day_ago = (chrono::Utc::now() - chrono::Duration::days(1))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let changed = sql_query(
        "DELETE FROM sessions
         WHERE expires_at <= ?1
            OR (revoked_at IS NOT NULL AND revoked_at <= ?2)",
    )
    .bind::<Text, _>(&now)
    .bind::<Text, _>(&day_ago)
    .execute(conn)
    .map_err(|err| format!("Failed to purge sessions: {err}"))?;
    Ok(changed as u64)
}

#[cfg(test)]
mod purge_tests {
    use super::*;
    use crate::repository::migrations;
    use crate::service::db::DBPool;
    use diesel::{r2d2::ConnectionManager, QueryableByName};
    use std::time::Duration;
    use tempfile::NamedTempFile;

    #[test]
    fn purge_removes_expired_and_old_revoked_sessions() {
        let file = NamedTempFile::new().unwrap();
        let pool = DBPool::builder()
            .max_size(2)
            .connection_timeout(Duration::from_millis(200))
            .build(ConnectionManager::<SqliteConnection>::new(
                file.path().to_str().unwrap(),
            ))
            .unwrap();
        migrations::migrate(&pool).unwrap();
        use diesel::connection::SimpleConnection;
        pool.get()
            .unwrap()
            .batch_execute(
                "INSERT INTO rp_users (
                username, password, salt, authority, error_count, status,
                created_at, updated_at, password_algo, must_change_password, token_version
            ) VALUES ('u', 'h', '', 'user', 0, 0,
                '2026-10-10 00:00:00', '2026-10-10 00:00:00', 'argon2id', 0, 0);",
            )
            .unwrap();
        let mut conn = pool.get().unwrap();

        // Expired: ttl in the past.
        let expired = create_session(&mut conn, 1, -60, None, None).unwrap();
        // Revoked a day ago.
        let old_revoked = create_session(&mut conn, 1, 3600, None, None).unwrap();
        revoke_session(&mut conn, &old_revoked.jti).unwrap();
        sql_query("UPDATE sessions SET revoked_at = '2000-01-01 00:00:00' WHERE jti_hash = ?1")
            .bind::<Text, _>(&hash_jti(&old_revoked.jti))
            .execute(&mut conn)
            .unwrap();
        // Live, and revoked just now: both must survive.
        let live = create_session(&mut conn, 1, 3600, None, None).unwrap();
        let recent_revoked = create_session(&mut conn, 1, 3600, None, None).unwrap();
        revoke_session(&mut conn, &recent_revoked.jti).unwrap();

        assert_eq!(purge_expired(&mut conn).unwrap(), 2);

        #[derive(QueryableByName)]
        struct CountRow {
            #[diesel(sql_type = BigInt)]
            count: i64,
        }
        let rows: Vec<CountRow> = sql_query("SELECT COUNT(*) AS count FROM sessions")
            .load(&mut conn)
            .unwrap();
        assert_eq!(rows[0].count, 2);
        // Deletion keeps enforcement fail-closed: consuming a deleted jti fails.
        assert!(validate_session(&mut conn, &expired.jti, 1).is_err());
        assert!(validate_session(&mut conn, &live.jti, 1).is_ok());
    }
}
