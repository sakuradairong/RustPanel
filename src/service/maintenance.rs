//! Background maintenance: audit outbox delivery, intent reconciliation and
//! retention purges. Runs on fixed intervals from the panel process until a
//! proper job framework (RP-12) takes over scheduling.
use std::time::Duration;

use crate::{audit, log_info, log_warn, service::{db::DBPool, sessions, sse_tickets}};

const OUTBOX_INTERVAL: Duration = Duration::from_secs(30);
const HOUSEKEEPING_INTERVAL: Duration = Duration::from_secs(3600);
/// Intents pending longer than this have an unknown execution outcome.
const STALE_INTENT_SECS: i64 = 3600;
/// Audit events/intents/outbox rows older than this are deleted.
const AUDIT_RETENTION_DAYS: i64 = 90;

fn housekeeping(pool: &DBPool) {
    // These helpers check out their own connections; run before acquiring ours.
    match audit::reconcile_stale_intents(pool, STALE_INTENT_SECS) {
        Ok(count) if count > 0 => {
            log_info!(target: "maintenance", "marked {count} stale audit intents as unknown")
        }
        Ok(_) => {}
        Err(err) => {
            log_warn!(target: "maintenance", "intent reconciliation failed: {}", err.message())
        }
    }
    match audit::purge_audit(pool, AUDIT_RETENTION_DAYS) {
        Ok(count) if count > 0 => log_info!(target: "maintenance", "purged {count} old audit rows"),
        Ok(_) => {}
        Err(err) => log_warn!(target: "maintenance", "audit purge failed: {}", err.message()),
    }
    let mut conn = match pool.get() {
        Ok(conn) => conn,
        Err(err) => {
            log_warn!(target: "maintenance", "housekeeping skipped, no connection: {err}");
            return;
        }
    };
    match sessions::purge_expired(&mut conn) {
        Ok(count) if count > 0 => {
            log_info!(target: "maintenance", "purged {count} expired/revoked sessions")
        }
        Ok(_) => {}
        Err(err) => log_warn!(target: "maintenance", "session purge failed: {err}"),
    }
    match sse_tickets::purge_stale(&mut conn) {
        Ok(count) if count > 0 => {
            log_info!(target: "maintenance", "purged {count} consumed/expired SSE tickets")
        }
        Ok(_) => {}
        Err(err) => log_warn!(target: "maintenance", "SSE ticket purge failed: {err}"),
    }
}

/// Never returns; run inside `tokio::spawn` from the panel startup.
pub async fn run_forever(pool: DBPool) {
    let mut outbox_tick = tokio::time::interval(OUTBOX_INTERVAL);
    let mut housekeeping_tick = tokio::time::interval(HOUSEKEEPING_INTERVAL);
    loop {
        tokio::select! {
            _ = outbox_tick.tick() => {
                let delivery_pool = pool.clone();
                match tokio::task::spawn_blocking(move || audit::deliver_pending_outbox(&delivery_pool, 100)).await {
                    Ok(Ok(count)) if count > 0 => {
                        log_info!(target: "maintenance", "delivered {count} audit outbox rows")
                    }
                    Ok(Ok(_)) => {}
                    Ok(Err(err)) => log_warn!(target: "maintenance", "audit outbox delivery failed: {}", err.message()),
                    Err(err) => log_warn!(target: "maintenance", "audit outbox task failed: {err}"),
                }
            }
            _ = housekeeping_tick.tick() => {
                let pool = pool.clone();
                let _ = tokio::task::spawn_blocking(move || housekeeping(&pool)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel::{connection::SimpleConnection, r2d2::ConnectionManager, RunQueryDsl, SqliteConnection};

    #[test]
    fn housekeeping_works_with_one_pool_connection() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let pool = DBPool::builder()
            .max_size(1)
            .connection_timeout(Duration::from_millis(100))
            .build(ConnectionManager::<SqliteConnection>::new(
                file.path().to_str().unwrap(),
            ))
            .unwrap();
        crate::repository::migrations::migrate(&pool).unwrap();
        pool.get().unwrap().batch_execute(
            "INSERT INTO rp_users (id, username, password, salt, authority, error_count, status, created_at, updated_at)
             VALUES (1, 'test', 'hash', '', 'user', 0, 0, '2000-01-01', '2000-01-01');
             INSERT INTO audit_intents (operation_id, action, created_at) VALUES ('test', 'test', '2000-01-01 00:00:00');
             INSERT INTO sse_tickets (jti, user_id, stream, expires_at_unix, created_at) VALUES ('expired', 1, 'monitor', 1, '2000-01-01');"
        ).unwrap();
        housekeeping(&pool);
        #[derive(diesel::QueryableByName)]
        struct Count {
            #[diesel(sql_type = diesel::sql_types::BigInt)]
            count: i64,
        }
        let count: Count = diesel::sql_query("SELECT COUNT(*) AS count FROM sse_tickets")
            .get_result(&mut *pool.get().unwrap())
            .unwrap();
        assert_eq!(count.count, 0);
        let count: Count = diesel::sql_query("SELECT COUNT(*) AS count FROM audit_intents")
            .get_result(&mut *pool.get().unwrap())
            .unwrap();
        assert_eq!(count.count, 0, "audit purge must acquire a connection");
    }
}
