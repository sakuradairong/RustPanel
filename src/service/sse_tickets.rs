//! One-time short-lived SSE tickets (RP-05 / C.4).
use diesel::{
    sql_query,
    sql_types::{BigInt, Integer, Text},
    QueryableByName, RunQueryDsl, SqliteConnection,
};
use rand::{distributions::Alphanumeric, Rng};

use crate::errors::common::CommonError;

pub const MONITOR_STREAM: &str = "monitor";
pub const DEFAULT_TTL_SECS: i64 = 60;

#[derive(QueryableByName)]
struct CountRow {
    #[diesel(sql_type = BigInt)]
    count: i64,
}

#[derive(QueryableByName)]
struct UserIdRow {
    #[diesel(sql_type = Integer)]
    user_id: i32,
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn now_utc() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%d %H:%M:%S%.3f")
        .to_string()
}

fn new_jti() -> String {
    let suffix: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();
    format!("sse-{suffix}")
}

/// Issue a one-time ticket bound to `user_id` + `stream`.
pub fn issue_ticket(
    conn: &mut SqliteConnection,
    user_id: i32,
    stream: &str,
    ttl_secs: i64,
) -> Result<(String, i64), CommonError> {
    if stream != MONITOR_STREAM {
        return Err(CommonError::BadRequest(format!(
            "Unsupported SSE stream: {stream}"
        )));
    }
    let ttl = ttl_secs.clamp(5, 60);
    let jti = new_jti();
    let expires_at = now_unix() + ttl;
    sql_query(
        "INSERT INTO sse_tickets (jti, user_id, stream, expires_at_unix, consumed_at_unix, created_at)
         VALUES (?1, ?2, ?3, ?4, NULL, ?5)",
    )
    .bind::<Text, _>(&jti)
    .bind::<Integer, _>(user_id)
    .bind::<Text, _>(stream)
    .bind::<BigInt, _>(expires_at)
    .bind::<Text, _>(now_utc())
    .execute(conn)
    .map_err(|_| CommonError::ServiceUnavailable("SSE ticket store unavailable".into()))?;
    Ok((jti, ttl))
}

/// Atomically consume a ticket for `expected_stream`. Rejects expired/reused/wrong-stream.
pub fn consume_ticket(
    conn: &mut SqliteConnection,
    ticket: &str,
    expected_stream: &str,
) -> Result<i32, CommonError> {
    let now = now_unix();
    let updated = sql_query(
        "UPDATE sse_tickets
         SET consumed_at_unix = ?1
         WHERE jti = ?2
           AND stream = ?3
           AND consumed_at_unix IS NULL
           AND expires_at_unix > ?1",
    )
    .bind::<BigInt, _>(now)
    .bind::<Text, _>(ticket)
    .bind::<Text, _>(expected_stream)
    .execute(conn)
    .map_err(|_| CommonError::ServiceUnavailable("SSE ticket store unavailable".into()))?;
    if updated == 0 {
        // Distinguish wrong stream vs missing/expired/reused without leaking too much.
        let wrong_stream: Vec<CountRow> = sql_query(
            "SELECT COUNT(*) AS count FROM sse_tickets
             WHERE jti = ?1 AND stream != ?2 AND consumed_at_unix IS NULL AND expires_at_unix > ?3",
        )
        .bind::<Text, _>(ticket)
        .bind::<Text, _>(expected_stream)
        .bind::<BigInt, _>(now)
        .load(conn)
        .map_err(|_| CommonError::ServiceUnavailable("SSE ticket store unavailable".into()))?;
        if wrong_stream.first().map(|r| r.count > 0).unwrap_or(false) {
            return Err(CommonError::Forbidden("SSE ticket stream mismatch".into()));
        }
        return Err(CommonError::Unauthorized(
            "SSE ticket invalid, expired, or already used".into(),
        ));
    }
    let rows: Vec<UserIdRow> = sql_query("SELECT user_id FROM sse_tickets WHERE jti = ?1 LIMIT 1")
        .bind::<Text, _>(ticket)
        .load(conn)
        .map_err(|_| CommonError::ServiceUnavailable("SSE ticket store unavailable".into()))?;
    rows.first()
        .map(|r| r.user_id)
        .ok_or_else(|| CommonError::Unauthorized("SSE ticket invalid".into()))
}

/// Active-connection recheck: user must still exist and be enabled.
pub fn user_still_active(conn: &mut SqliteConnection, user_id: i32) -> Result<bool, CommonError> {
    #[derive(QueryableByName)]
    struct StatusRow {
        #[diesel(sql_type = Integer)]
        status: i32,
    }
    let rows: Vec<StatusRow> =
        sql_query("SELECT status FROM rp_users WHERE id = ?1 LIMIT 1")
            .bind::<Integer, _>(user_id)
            .load(conn)
            .map_err(|_| {
                CommonError::ServiceUnavailable("Authentication database unavailable".into())
            })?;
    Ok(rows.first().map(|r| r.status == 0).unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::migrations;
    use crate::service::db::DBPool;
    use diesel::{connection::SimpleConnection, r2d2::ConnectionManager};
    use std::time::Duration;
    use tempfile::NamedTempFile;

    fn pool_with_user() -> (NamedTempFile, DBPool) {
        let file = NamedTempFile::new().unwrap();
        let pool = DBPool::builder()
            .max_size(2)
            .connection_timeout(Duration::from_millis(200))
            .build(ConnectionManager::<SqliteConnection>::new(
                file.path().to_str().unwrap(),
            ))
            .unwrap();
        migrations::migrate(&pool).unwrap();
        pool.get()
            .unwrap()
            .batch_execute(
                "INSERT INTO rp_users (
                username, password, salt, authority, error_count, status,
                created_at, updated_at, password_algo, must_change_password, token_version
            ) VALUES (
                'u', 'h', '', 'user', 0, 0,
                '2026-10-10 00:00:00', '2026-10-10 00:00:00', 'argon2id', 0, 0
            );",
            )
            .unwrap();
        (file, pool)
    }

    #[test]
    fn ticket_is_single_use_and_stream_bound() {
        let (_f, pool) = pool_with_user();
        let mut conn = pool.get().unwrap();
        let (ticket, ttl) = issue_ticket(&mut conn, 1, MONITOR_STREAM, 60).unwrap();
        assert_eq!(ttl, 60);
        assert_eq!(consume_ticket(&mut conn, &ticket, MONITOR_STREAM).unwrap(), 1);
        assert!(consume_ticket(&mut conn, &ticket, MONITOR_STREAM).is_err());

        let (other, _) = issue_ticket(&mut conn, 1, MONITOR_STREAM, 60).unwrap();
        assert!(matches!(
            consume_ticket(&mut conn, &other, "jobs"),
            Err(CommonError::Forbidden(_))
        ));
        // Wrong-stream attempt must not consume the ticket.
        assert_eq!(
            consume_ticket(&mut conn, &other, MONITOR_STREAM).unwrap(),
            1
        );
    }
}
