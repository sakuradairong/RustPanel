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
