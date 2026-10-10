//! Background maintenance: audit outbox delivery, intent reconciliation and
//! retention purges. Runs on fixed intervals from the panel process until a
//! proper job framework (RP-12) takes over scheduling.
use std::time::Duration;

use crate::{audit, log_info, log_warn, service::{db::DBPool, sessions, sse_tickets}};

const OUTBOX_INTERVAL: Duration = Duration::from_secs(30);
const HOUSEKEEPING_INTERVAL: Duration = Duration::from_secs(3600);
/// Intents pending longer than this are assumed crashed and marked aborted.
const STALE_INTENT_SECS: i64 = 3600;
/// Audit events/intents/outbox rows older than this are deleted.
const AUDIT_RETENTION_DAYS: i64 = 90;

fn housekeeping(pool: &DBPool) {
    let mut conn = match pool.get() {
        Ok(conn) => conn,
        Err(err) => {
            log_warn!(target: "maintenance", "housekeeping skipped, no connection: {err}");
            return;
        }
    };
    match audit::reconcile_stale_intents(pool, STALE_INTENT_SECS) {
        Ok(count) if count > 0 => {
            log_info!(target: "maintenance", "marked {count} stale audit intents as aborted")
        }
        Ok(_) => {}
        Err(err) => log_warn!(target: "maintenance", "intent reconciliation failed: {}", err.message()),
    }
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
    match audit::purge_audit(pool, AUDIT_RETENTION_DAYS) {
        Ok(count) if count > 0 => {
            log_info!(target: "maintenance", "purged {count} audit rows older than {AUDIT_RETENTION_DAYS} days")
        }
        Ok(_) => {}
        Err(err) => log_warn!(target: "maintenance", "audit purge failed: {}", err.message()),
    }
}

/// Never returns; run inside `tokio::spawn` from the panel startup.
pub async fn run_forever(pool: DBPool) {
    let mut outbox_tick = tokio::time::interval(OUTBOX_INTERVAL);
    let mut housekeeping_tick = tokio::time::interval(HOUSEKEEPING_INTERVAL);
    loop {
        tokio::select! {
            _ = outbox_tick.tick() => {
                match audit::deliver_pending_outbox(&pool, 100) {
                    Ok(count) if count > 0 => {
                        log_info!(target: "maintenance", "delivered {count} audit outbox rows")
                    }
                    Ok(_) => {}
                    Err(err) => log_warn!(target: "maintenance", "audit outbox delivery failed: {}", err.message()),
                }
            }
            _ = housekeeping_tick.tick() => {
                let pool = pool.clone();
                let _ = tokio::task::spawn_blocking(move || housekeeping(&pool)).await;
            }
        }
    }
}
