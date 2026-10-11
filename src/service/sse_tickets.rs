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

#[derive(Debug, Clone, QueryableByName)]
pub struct StreamIdentity {
    #[diesel(sql_type = Integer)]
    pub user_id: i32,
    #[diesel(sql_type = Text)]
    pub session_hash: String,
    #[diesel(sql_type = Integer)]
    pub token_version: i32,
    #[diesel(sql_type = BigInt)]
    pub access_expires_at_unix: i64,
}

impl From<&crate::api::auth::AuthUser> for StreamIdentity {
    fn from(user: &crate::api::auth::AuthUser) -> Self {
        Self {
            user_id: user.id,
            session_hash: crate::service::sessions::hash_jti(&user.jti),
            token_version: user.token_version,
            access_expires_at_unix: user.expires_at_unix,
        }
    }
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

/// Maximum unconsumed tickets a single user may hold; reissues beyond this
/// are rejected so authenticated minting cannot grow the table unthrottled.
const MAX_LIVE_TICKETS_PER_USER: i64 = 10;

/// Delete consumed and expired tickets.
pub fn purge_stale(conn: &mut SqliteConnection) -> Result<u64, CommonError> {
    let changed = sql_query(
        "DELETE FROM sse_tickets
         WHERE consumed_at_unix IS NOT NULL
            OR expires_at_unix <= ?1",
    )
    .bind::<BigInt, _>(now_unix())
    .execute(conn)
    .map_err(|_| CommonError::ServiceUnavailable("SSE ticket store unavailable".into()))?;
    Ok(changed as u64)
}

/// Issue a one-time ticket bound to a validated session and stream.
pub fn issue_ticket(
    conn: &mut SqliteConnection,
    identity: &StreamIdentity,
    stream: &str,
    ttl_secs: i64,
) -> Result<(String, i64), CommonError> {
    conn.immediate_transaction(|conn| {
        validate_identity(conn, identity)?;
        let user_id = identity.user_id;
        if stream != MONITOR_STREAM {
            return Err(CommonError::BadRequest(format!(
                "Unsupported SSE stream: {stream}"
            )));
        }
        let ttl = ttl_secs.clamp(5, 60);
        let jti = new_jti();
        let expires_at = now_unix() + ttl;
        // Drop this user's dead tickets, then enforce a live-ticket cap.
        purge_stale(conn)?;
        let live: Vec<CountRow> = sql_query(
            "SELECT COUNT(*) AS count FROM sse_tickets
             WHERE user_id = ?1 AND consumed_at_unix IS NULL AND expires_at_unix > ?2",
        )
        .bind::<Integer, _>(user_id)
        .bind::<BigInt, _>(now_unix())
        .load(conn)
        .map_err(|_| CommonError::ServiceUnavailable("SSE ticket store unavailable".into()))?;
        if live.first().map(|r| r.count).unwrap_or(0) >= MAX_LIVE_TICKETS_PER_USER {
            return Err(CommonError::TooManyRequests(
                "Too many live SSE tickets; consume or wait for expiry".into(),
            ));
        }
        sql_query(
            "INSERT INTO sse_tickets (jti, user_id, stream, expires_at_unix, consumed_at_unix, created_at,
             session_hash, token_version, access_expires_at_unix)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7, ?8)",
        )
        .bind::<Text, _>(&jti)
        .bind::<Integer, _>(user_id)
        .bind::<Text, _>(stream)
        .bind::<BigInt, _>(expires_at)
        .bind::<Text, _>(now_utc())
        .bind::<Text, _>(&identity.session_hash)
        .bind::<Integer, _>(identity.token_version)
        .bind::<BigInt, _>(identity.access_expires_at_unix)
        .execute(conn)
        .map_err(|_| CommonError::ServiceUnavailable("SSE ticket store unavailable".into()))?;
        Ok((jti, ttl))
    })
}

/// Atomically consume a ticket for `expected_stream`. Rejects expired/reused/wrong-stream.
pub fn consume_ticket(
    conn: &mut SqliteConnection,
    ticket: &str,
    expected_stream: &str,
) -> Result<StreamIdentity, CommonError> {
    conn.immediate_transaction(|conn| {
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
        let rows: Vec<StreamIdentity> = sql_query("SELECT user_id, session_hash, token_version, access_expires_at_unix FROM sse_tickets WHERE jti = ?1 LIMIT 1")
            .bind::<Text, _>(ticket)
            .load(conn)
            .map_err(|_| CommonError::ServiceUnavailable("SSE ticket store unavailable".into()))?;
        let identity = rows.into_iter().next()
            .ok_or_else(|| CommonError::Unauthorized("SSE ticket invalid".into()))?;
        validate_identity(conn, &identity)?;
        Ok(identity)
    })
}

/// Rechecked at connection and during streaming; all errors end the stream.
pub fn validate_identity(
    conn: &mut SqliteConnection,
    identity: &StreamIdentity,
) -> Result<(), CommonError> {
    let rows: Vec<CountRow> = sql_query(
        "SELECT COUNT(*) AS count FROM rp_users u JOIN sessions s ON s.user_id = u.id
            WHERE u.id = ?1 AND u.status = 0 AND u.must_change_password = 0
            AND u.token_version = ?2 AND s.jti_hash = ?3 AND s.revoked_at IS NULL
            AND CAST(strftime('%s', s.expires_at) AS INTEGER) > ?4 AND ?5 > ?4",
    )
    .bind::<Integer, _>(identity.user_id)
    .bind::<Integer, _>(identity.token_version)
    .bind::<Text, _>(&identity.session_hash)
    .bind::<BigInt, _>(now_unix())
    .bind::<BigInt, _>(identity.access_expires_at_unix)
    .load(conn)
    .map_err(|_| CommonError::ServiceUnavailable("Authentication database unavailable".into()))?;
    if !rows.first().map(|r| r.count > 0).unwrap_or(false) {
        return Err(CommonError::Unauthorized(
            "SSE session revoked or expired".into(),
        ));
    }
    if !crate::service::rbac::user_has_permission(conn, identity.user_id, "monitor:read")? {
        return Err(CommonError::Forbidden(
            "Missing permission: monitor:read".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::migrations;
    use crate::service::db::DBPool;
    use diesel::{connection::SimpleConnection, r2d2::ConnectionManager};
    use std::time::Duration;
    use tempfile::NamedTempFile;

    fn pool_with_user() -> (NamedTempFile, DBPool, StreamIdentity) {
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
        let mut conn = pool.get().unwrap();
        crate::service::rbac::assign_role_for_authority(&mut conn, 1, "user").unwrap();
        let session =
            crate::service::sessions::create_session(&mut conn, 1, 3600, None, None).unwrap();
        let identity = StreamIdentity {
            user_id: 1,
            session_hash: crate::service::sessions::hash_jti(&session.jti),
            token_version: 0,
            access_expires_at_unix: session.expires_at_unix,
        };
        drop(conn);
        (file, pool, identity)
    }

    #[test]
    fn ticket_is_single_use_and_stream_bound() {
        let (_f, pool, identity) = pool_with_user();
        let mut conn = pool.get().unwrap();
        let (ticket, ttl) = issue_ticket(&mut conn, &identity, MONITOR_STREAM, 60).unwrap();
        assert_eq!(ttl, 60);
        assert_eq!(consume_ticket(&mut conn, &ticket, MONITOR_STREAM).unwrap().user_id, 1);
        assert!(consume_ticket(&mut conn, &ticket, MONITOR_STREAM).is_err());

        let (other, _) = issue_ticket(&mut conn, &identity, MONITOR_STREAM, 60).unwrap();
        assert!(matches!(
            consume_ticket(&mut conn, &other, "jobs"),
            Err(CommonError::Forbidden(_))
        ));
        // Wrong-stream attempt must not consume the ticket.
        assert_eq!(
            consume_ticket(&mut conn, &other, MONITOR_STREAM).unwrap().user_id,
            1
        );
    }

    #[test]
    fn session_revocation_expiry_version_and_permissions_invalidate_tickets() {
        for mutation in [
            "UPDATE sessions SET revoked_at = '2026-01-01 00:00:00'",
            "UPDATE sessions SET expires_at = '2026-01-01 00:00:00'",
            "UPDATE rp_users SET token_version = 1",
            "UPDATE rp_users SET must_change_password = 1",
            "DELETE FROM user_roles",
        ] {
            let (_f, pool, identity) = pool_with_user();
            let mut conn = pool.get().unwrap();
            let (ticket, _) = issue_ticket(&mut conn, &identity, MONITOR_STREAM, 60).unwrap();
            conn.batch_execute(mutation).unwrap();
            assert!(
                validate_identity(&mut conn, &identity).is_err(),
                "{mutation}"
            );
            assert!(
                consume_ticket(&mut conn, &ticket, MONITOR_STREAM).is_err(),
                "{mutation}"
            );
        }
        let (_f, pool, mut identity) = pool_with_user();
        identity.access_expires_at_unix = 1;
        assert!(validate_identity(&mut pool.get().unwrap(), &identity).is_err());
    }

    #[test]
    fn ticket_issuance_is_capped_and_stale_rows_purged() {
        let (_f, pool, identity) = pool_with_user();
        let mut conn = pool.get().unwrap();
        for _ in 0..MAX_LIVE_TICKETS_PER_USER {
            issue_ticket(&mut conn, &identity, MONITOR_STREAM, 60).unwrap();
        }
        // Live-ticket cap reached: further issuance is rejected, not queued.
        assert!(matches!(
            issue_ticket(&mut conn, &identity, MONITOR_STREAM, 60),
            Err(CommonError::TooManyRequests(_))
        ));
        // Consuming one ticket frees capacity (purge removes the dead row).
        #[derive(QueryableByName)]
        struct JtiRow {
            #[diesel(sql_type = Text)]
            jti: String,
        }
        let rows: Vec<CountRow> = sql_query(
            "SELECT COUNT(*) AS count FROM sse_tickets WHERE consumed_at_unix IS NULL",
        )
        .load(&mut conn)
        .unwrap();
        assert_eq!(rows[0].count, MAX_LIVE_TICKETS_PER_USER);
        let any: Vec<JtiRow> = sql_query("SELECT jti FROM sse_tickets LIMIT 1")
            .load(&mut conn)
            .unwrap();
        consume_ticket(&mut conn, &any[0].jti.as_str(), MONITOR_STREAM).unwrap();
        issue_ticket(&mut conn, &identity, MONITOR_STREAM, 60).unwrap();

        // Consumed rows are removed by the next purge/issue cycle.
        let rows: Vec<CountRow> = sql_query(
            "SELECT COUNT(*) AS count FROM sse_tickets WHERE consumed_at_unix IS NOT NULL",
        )
        .load(&mut conn)
        .unwrap();
        assert_eq!(rows[0].count, 0, "consumed tickets must be purged on issue");
    }

    #[test]
    fn purge_stale_removes_consumed_and_expired() {
        let (_f, pool, identity) = pool_with_user();
        let mut conn = pool.get().unwrap();
        let (a, _) = issue_ticket(&mut conn, &identity, MONITOR_STREAM, 60).unwrap();
        let (b, _) = issue_ticket(&mut conn, &identity, MONITOR_STREAM, 5).unwrap();
        consume_ticket(&mut conn, &a, MONITOR_STREAM).unwrap();
        // Force-expire the second ticket.
        sql_query("UPDATE sse_tickets SET expires_at_unix = 1 WHERE jti = ?1")
            .bind::<Text, _>(&b)
            .execute(&mut conn)
            .unwrap();
        assert_eq!(purge_stale(&mut conn).unwrap(), 2);
        let rows: Vec<CountRow> = sql_query("SELECT COUNT(*) AS count FROM sse_tickets")
            .load(&mut conn)
            .unwrap();
        assert_eq!(rows[0].count, 0);
    }
}
