//! Persist audit intents before side effects; refuse the action if insert fails.
use diesel::{
    sql_query,
    sql_types::{BigInt, Integer, Nullable, Text},
    Connection, QueryableByName, RunQueryDsl, SqliteConnection,
};
use serde_json::Value;

use super::mask::mask_value;
use crate::service::db::DBPool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditError {
    Unavailable(String),
    Rejected(String),
}

impl AuditError {
    pub fn message(&self) -> &str {
        match self {
            Self::Unavailable(m) | Self::Rejected(m) => m.as_str(),
        }
    }
}

impl std::fmt::Display for AuditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for AuditError {}

impl From<diesel::result::Error> for AuditError {
    fn from(err: diesel::result::Error) -> Self {
        AuditError::Unavailable(format!("database error: {err}"))
    }
}

#[derive(Debug, Clone)]
pub struct IntentSpec {
    pub operation_id: String,
    pub actor_id: Option<i32>,
    pub request_id: Option<String>,
    pub action: String,
    pub resource: Option<String>,
    pub params: Value,
    pub ip: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Success,
    Failure,
    Rejected,
    Timeout,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Rejected => "rejected",
            Self::Timeout => "timeout",
        }
    }
}

#[derive(Debug, Clone)]
pub struct IntentHandle {
    pub id: i64,
    pub operation_id: String,
}

#[derive(QueryableByName)]
struct IdRow {
    #[diesel(sql_type = BigInt)]
    id: i64,
}

fn now_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

/// Insert a pending intent. Callers **must** abort the privileged action if this fails.
pub fn begin_intent(pool: &DBPool, spec: IntentSpec) -> Result<IntentHandle, AuditError> {
    let mut conn = pool.get().map_err(|e| {
        AuditError::Unavailable(format!("audit database unavailable: {e}"))
    })?;
    begin_intent_conn(&mut conn, spec)
}

pub fn begin_intent_conn(
    conn: &mut SqliteConnection,
    spec: IntentSpec,
) -> Result<IntentHandle, AuditError> {
    let masked = mask_value(spec.params);
    let params_masked = serde_json::to_string(&masked).unwrap_or_else(|_| "{}".into());
    sql_query(
        "INSERT INTO audit_intents (
            operation_id, actor_id, request_id, action, resource,
            params_masked, ip, status, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8)",
    )
    .bind::<Text, _>(&spec.operation_id)
    .bind::<Nullable<Integer>, _>(spec.actor_id)
    .bind::<Nullable<Text>, _>(spec.request_id.as_deref())
    .bind::<Text, _>(&spec.action)
    .bind::<Nullable<Text>, _>(spec.resource.as_deref())
    .bind::<Text, _>(&params_masked)
    .bind::<Nullable<Text>, _>(spec.ip.as_deref())
    .bind::<Text, _>(now_utc())
    .execute(conn)
    .map_err(|e| AuditError::Unavailable(format!("failed to persist audit intent: {e}")))?;

    let rows: Vec<IdRow> = sql_query("SELECT last_insert_rowid() AS id")
        .load(conn)
        .map_err(|e| AuditError::Unavailable(format!("failed to read intent id: {e}")))?;
    let id = rows
        .first()
        .map(|r| r.id)
        .ok_or_else(|| AuditError::Unavailable("missing intent id".into()))?;
    Ok(IntentHandle {
        id,
        operation_id: spec.operation_id,
    })
}

/// Record outcome + enqueue outbox row for later delivery / retry.
pub fn complete_intent(
    pool: &DBPool,
    intent: &IntentHandle,
    outcome: Outcome,
    exit_code: Option<i32>,
    detail: Value,
    duration_ms: Option<i64>,
) -> Result<(), AuditError> {
    let mut conn = pool.get().map_err(|e| {
        AuditError::Unavailable(format!("audit database unavailable: {e}"))
    })?;
    complete_intent_conn(
        &mut conn,
        intent,
        outcome,
        exit_code,
        detail,
        duration_ms,
    )
}

pub fn complete_intent_conn(
    conn: &mut SqliteConnection,
    intent: &IntentHandle,
    outcome: Outcome,
    exit_code: Option<i32>,
    detail: Value,
    duration_ms: Option<i64>,
) -> Result<(), AuditError> {
    // Intent status, event and outbox row must land together; a crash between
    // them would leave a pending intent with no event or an undeliverable row.
    conn.transaction(|conn| {
        let status = match outcome {
            Outcome::Success => "executed",
            Outcome::Failure | Outcome::Timeout => "failed",
            Outcome::Rejected => "rejected",
        };
        sql_query("UPDATE audit_intents SET status = ?1 WHERE id = ?2")
            .bind::<Text, _>(status)
            .bind::<BigInt, _>(intent.id)
            .execute(conn)
            .map_err(|e| AuditError::Unavailable(format!("failed to update intent: {e}")))?;

        let detail_masked =
            serde_json::to_string(&mask_value(detail)).unwrap_or_else(|_| "{}".into());
        sql_query(
            "INSERT INTO audit_events (
            intent_id, operation_id, outcome, exit_code, detail_masked, duration_ms, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind::<BigInt, _>(intent.id)
        .bind::<Text, _>(&intent.operation_id)
        .bind::<Text, _>(outcome.as_str())
        .bind::<Nullable<Integer>, _>(exit_code)
        .bind::<Text, _>(&detail_masked)
        .bind::<Nullable<BigInt>, _>(duration_ms)
        .bind::<Text, _>(now_utc())
        .execute(conn)
        .map_err(|e| AuditError::Unavailable(format!("failed to persist audit event: {e}")))?;

        let rows: Vec<IdRow> = sql_query("SELECT last_insert_rowid() AS id")
            .load(conn)
            .map_err(|e| AuditError::Unavailable(format!("failed to read event id: {e}")))?;
        let event_id = rows
            .first()
            .map(|r| r.id)
            .ok_or_else(|| AuditError::Unavailable("missing event id".into()))?;

        sql_query(
            "INSERT INTO audit_outbox (event_id, attempts, next_retry_at, delivered_at, last_error, created_at)
         VALUES (?1, 0, NULL, NULL, NULL, ?2)",
        )
        .bind::<BigInt, _>(event_id)
        .bind::<Text, _>(now_utc())
        .execute(conn)
        .map_err(|e| AuditError::Unavailable(format!("failed to enqueue audit outbox: {e}")))?;
        Ok(())
    })
}

#[derive(Debug, Clone, QueryableByName)]
pub struct OutboxRow {
    #[diesel(sql_type = BigInt)]
    pub id: i64,
    #[diesel(sql_type = BigInt)]
    pub event_id: i64,
    #[diesel(sql_type = Integer)]
    pub attempts: i32,
    #[diesel(sql_type = Text)]
    pub operation_id: String,
    #[diesel(sql_type = Text)]
    pub outcome: String,
}

/// Pending outbox rows eligible for (re)delivery.
pub fn list_pending_outbox(pool: &DBPool, limit: i64) -> Result<Vec<OutboxRow>, AuditError> {
    let mut conn = pool.get().map_err(|e| {
        AuditError::Unavailable(format!("audit database unavailable: {e}"))
    })?;
    sql_query(
        "SELECT o.id, o.event_id, o.attempts, e.operation_id, e.outcome
         FROM audit_outbox o
         JOIN audit_events e ON e.id = o.event_id
         WHERE o.delivered_at IS NULL
           AND (o.next_retry_at IS NULL OR o.next_retry_at <= ?1)
         ORDER BY o.id ASC
         LIMIT ?2",
    )
    .bind::<Text, _>(now_utc())
    .bind::<BigInt, _>(limit)
    .load(&mut conn)
    .map_err(|e| AuditError::Unavailable(format!("failed to list outbox: {e}")))
}

pub fn mark_outbox_delivered(pool: &DBPool, outbox_id: i64) -> Result<(), AuditError> {
    let mut conn = pool.get().map_err(|e| {
        AuditError::Unavailable(format!("audit database unavailable: {e}"))
    })?;
    sql_query("UPDATE audit_outbox SET delivered_at = ?1, last_error = NULL WHERE id = ?2")
        .bind::<Text, _>(now_utc())
        .bind::<BigInt, _>(outbox_id)
        .execute(&mut conn)
        .map_err(|e| AuditError::Unavailable(format!("failed to mark outbox delivered: {e}")))?;
    Ok(())
}

pub fn mark_outbox_retry(
    pool: &DBPool,
    outbox_id: i64,
    error: &str,
    next_retry_at: &str,
) -> Result<(), AuditError> {
    let mut conn = pool.get().map_err(|e| {
        AuditError::Unavailable(format!("audit database unavailable: {e}"))
    })?;
    sql_query(
        "UPDATE audit_outbox
         SET attempts = attempts + 1, last_error = ?1, next_retry_at = ?2
         WHERE id = ?3",
    )
    .bind::<Text, _>(error)
    .bind::<Text, _>(next_retry_at)
    .bind::<BigInt, _>(outbox_id)
    .execute(&mut conn)
    .map_err(|e| AuditError::Unavailable(format!("failed to mark outbox retry: {e}")))?;
    Ok(())
}

/// Mark stale pending intents as unknown: execution may have completed even
/// when its outcome could not be persisted. Never infer that nothing ran.
pub fn reconcile_stale_intents(
    pool: &DBPool,
    stale_after_secs: i64,
) -> Result<u64, AuditError> {
    let mut conn = pool.get().map_err(|e| {
        AuditError::Unavailable(format!("audit database unavailable: {e}"))
    })?;
    let cutoff = (chrono::Utc::now() - chrono::Duration::seconds(stale_after_secs))
        .format("%Y-%m-%d %H:%M:%S%.3f")
        .to_string();
    let changed = sql_query(
        "UPDATE audit_intents SET status = 'unknown'
         WHERE status = 'pending' AND created_at < ?1",
    )
    .bind::<Text, _>(&cutoff)
    .execute(&mut conn)
    .map_err(|e| AuditError::Unavailable(format!("failed to reconcile intents: {e}")))?;
    Ok(changed as u64)
}

/// Delete outbox rows, events and intents older than `keep_days` (children
/// first). Delivered rows for recent events are kept.
pub fn purge_audit(pool: &DBPool, keep_days: i64) -> Result<u64, AuditError> {
    let mut conn = pool.get().map_err(|e| {
        AuditError::Unavailable(format!("audit database unavailable: {e}"))
    })?;
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(keep_days))
        .format("%Y-%m-%d %H:%M:%S%.3f")
        .to_string();
    conn.transaction(|conn| {
        let outbox = sql_query(
            "DELETE FROM audit_outbox WHERE event_id IN (
             SELECT id FROM audit_events WHERE created_at < ?1)",
        )
        .bind::<Text, _>(&cutoff)
        .execute(conn)
        .map_err(|e| AuditError::Unavailable(format!("failed to purge outbox: {e}")))?;
        let events = sql_query("DELETE FROM audit_events WHERE created_at < ?1")
            .bind::<Text, _>(&cutoff)
            .execute(conn)
            .map_err(|e| AuditError::Unavailable(format!("failed to purge events: {e}")))?;
        // Intents may still have newer events attached (rare); only remove
        // those whose events are all gone so the FK holds.
        let intents = sql_query(
            "DELETE FROM audit_intents
             WHERE created_at < ?1
               AND NOT EXISTS (SELECT 1 FROM audit_events e WHERE e.intent_id = audit_intents.id)",
        )
        .bind::<Text, _>(&cutoff)
        .execute(conn)
        .map_err(|e| AuditError::Unavailable(format!("failed to purge intents: {e}")))?;
        Ok((outbox + events + intents) as u64)
    })
}

/// Deliver pending outbox rows to the interim sink (the panel log) and mark
/// them delivered. The RP-28 notification worker replaces this sink later.
pub fn deliver_pending_outbox(pool: &DBPool, limit: i64) -> Result<usize, AuditError> {
    let pending = list_pending_outbox(pool, limit)?;
    let mut delivered = 0;
    for row in &pending {
        crate::log_info!(
            target: "audit_outbox",
            "audit event delivered: operation_id={} event_id={} outcome={} attempts={}",
            row.operation_id,
            row.event_id,
            row.outcome,
            row.attempts
        );
        mark_outbox_delivered(pool, row.id)?;
        delivered += 1;
    }
    Ok(delivered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::migrations;
    use diesel::connection::SimpleConnection;
    use diesel::r2d2::ConnectionManager;
    use serde_json::json;
    use std::time::Duration;
    use tempfile::NamedTempFile;

    fn pool() -> (NamedTempFile, DBPool) {
        let file = NamedTempFile::new().unwrap();
        let pool = DBPool::builder()
            .max_size(2)
            .connection_timeout(Duration::from_millis(200))
            .build(ConnectionManager::<SqliteConnection>::new(
                file.path().to_str().unwrap(),
            ))
            .unwrap();
        assert_eq!(
            migrations::migrate(&pool).unwrap(),
            migrations::EXPECTED_SCHEMA_VERSION
        );
        (file, pool)
    }

    #[test]
    fn intent_failure_is_visible_and_success_enqueues_outbox() {
        let (_f, pool) = pool();
        let intent = begin_intent(
            &pool,
            IntentSpec {
                operation_id: "op-1".into(),
                actor_id: Some(7),
                request_id: Some("req-1".into()),
                action: "iptables.append".into(),
                resource: Some("firewall".into()),
                params: json!({"rule":"-p tcp","password":"nope"}),
                ip: Some("127.0.0.1".into()),
            },
        )
        .unwrap();

        // Masked params stored without plaintext password.
        #[derive(QueryableByName)]
        struct ParamsRow {
            #[diesel(sql_type = Text)]
            params_masked: String,
        }
        let rows: Vec<ParamsRow> = {
            let mut conn = pool.get().unwrap();
            sql_query("SELECT params_masked FROM audit_intents WHERE id = ?1")
                .bind::<BigInt, _>(intent.id)
                .load(&mut conn)
                .unwrap()
        };
        assert!(rows[0].params_masked.contains("[REDACTED]"));
        assert!(!rows[0].params_masked.contains("nope"));

        complete_intent(
            &pool,
            &intent,
            Outcome::Success,
            Some(0),
            json!({"stdout":"ok","token":"secret"}),
            Some(12),
        )
        .unwrap();

        let pending = list_pending_outbox(&pool, 10).unwrap();
        assert_eq!(pending.len(), 1);
        mark_outbox_delivered(&pool, pending[0].id).unwrap();
        assert!(list_pending_outbox(&pool, 10).unwrap().is_empty());
    }

    #[test]
    fn outbox_retry_keeps_row_pending() {
        let (_f, pool) = pool();
        let intent = begin_intent(
            &pool,
            IntentSpec {
                operation_id: "op-2".into(),
                actor_id: None,
                request_id: None,
                action: "file.save".into(),
                resource: Some("/www/a".into()),
                params: json!({}),
                ip: None,
            },
        )
        .unwrap();
        complete_intent(&pool, &intent, Outcome::Failure, Some(1), json!({}), None).unwrap();
        let pending = list_pending_outbox(&pool, 10).unwrap();
        mark_outbox_retry(&pool, pending[0].id, "sink down", "2099-01-01 00:00:00.000").unwrap();
        // Future next_retry_at => not listed yet.
        assert!(list_pending_outbox(&pool, 10).unwrap().is_empty());
    }

    #[test]
    fn deliver_pending_outbox_marks_rows_delivered() {
        let (_f, pool) = pool();
        let intent = begin_intent(
            &pool,
            IntentSpec {
                operation_id: "op-deliver".into(),
                actor_id: Some(1),
                request_id: None,
                action: "systemctl.restart".into(),
                resource: Some("nginx".into()),
                params: json!({}),
                ip: None,
            },
        )
        .unwrap();
        complete_intent(&pool, &intent, Outcome::Success, Some(0), json!({}), None).unwrap();
        assert_eq!(list_pending_outbox(&pool, 10).unwrap().len(), 1);
        assert_eq!(deliver_pending_outbox(&pool, 10).unwrap(), 1);
        assert!(list_pending_outbox(&pool, 10).unwrap().is_empty());
    }

    #[test]
    fn stale_pending_intents_are_reconciled_to_unknown() {
        let (_f, pool) = pool();
        let stale = begin_intent(
            &pool,
            IntentSpec {
                operation_id: "op-stale".into(),
                actor_id: None,
                request_id: None,
                action: "file.save".into(),
                resource: None,
                params: json!({}),
                ip: None,
            },
        )
        .unwrap();
        // Backdate the intent beyond the stale window.
        let mut conn = pool.get().unwrap();
        sql_query("UPDATE audit_intents SET created_at = '2000-01-01 00:00:00.000' WHERE id = ?1")
            .bind::<BigInt, _>(stale.id)
            .execute(&mut conn)
            .unwrap();
        drop(conn);

        let fresh = begin_intent(
            &pool,
            IntentSpec {
                operation_id: "op-fresh".into(),
                actor_id: None,
                request_id: None,
                action: "file.save".into(),
                resource: None,
                params: json!({}),
                ip: None,
            },
        )
        .unwrap();

        assert_eq!(reconcile_stale_intents(&pool, 3600).unwrap(), 1);
        #[derive(QueryableByName)]
        struct StatusRow {
            #[diesel(sql_type = Text)]
            status: String,
        }
        let mut conn = pool.get().unwrap();
        let stale_row: Vec<StatusRow> =
            sql_query("SELECT status AS status FROM audit_intents WHERE id = ?1")
                .bind::<BigInt, _>(stale.id)
                .load(&mut conn)
                .unwrap();
        let fresh_row: Vec<StatusRow> =
            sql_query("SELECT status AS status FROM audit_intents WHERE id = ?1")
                .bind::<BigInt, _>(fresh.id)
                .load(&mut conn)
                .unwrap();
        assert_eq!(stale_row[0].status, "unknown");
        assert_eq!(fresh_row[0].status, "pending");
    }

    #[test]
    fn purge_audit_removes_only_old_rows() {
        let (_f, pool) = pool();
        let old = begin_intent(
            &pool,
            IntentSpec {
                operation_id: "op-old".into(),
                actor_id: None,
                request_id: None,
                action: "file.save".into(),
                resource: None,
                params: json!({}),
                ip: None,
            },
        )
        .unwrap();
        complete_intent(&pool, &old, Outcome::Success, Some(0), json!({}), None).unwrap();
        let new = begin_intent(
            &pool,
            IntentSpec {
                operation_id: "op-new".into(),
                actor_id: None,
                request_id: None,
                action: "file.save".into(),
                resource: None,
                params: json!({}),
                ip: None,
            },
        )
        .unwrap();
        complete_intent(&pool, &new, Outcome::Success, Some(0), json!({}), None).unwrap();

        // Backdate the old intent/event/outbox trio.
        let mut conn = pool.get().unwrap();
        conn.batch_execute(&format!(
            "UPDATE audit_intents SET created_at = '2000-01-01 00:00:00.000' WHERE id = {old_id};
             UPDATE audit_events SET created_at = '2000-01-01 00:00:00.000' WHERE intent_id = {old_id};
             UPDATE audit_outbox SET created_at = '2000-01-01 00:00:00.000'
             WHERE event_id IN (SELECT id FROM audit_events WHERE intent_id = {old_id});",
            old_id = old.id
        ))
        .unwrap();
        drop(conn);

        let purged = purge_audit(&pool, 90).unwrap();
        assert!(purged >= 3, "old intent+event+outbox removed, got {purged}");
        #[derive(QueryableByName)]
        struct CountRow2 {
            #[diesel(sql_type = BigInt)]
            count: i64,
        }
        let mut conn = pool.get().unwrap();
        let intents: Vec<CountRow2> =
            sql_query("SELECT COUNT(*) AS count FROM audit_intents").load(&mut conn).unwrap();
        let events: Vec<CountRow2> =
            sql_query("SELECT COUNT(*) AS count FROM audit_events").load(&mut conn).unwrap();
        let outbox: Vec<CountRow2> =
            sql_query("SELECT COUNT(*) AS count FROM audit_outbox").load(&mut conn).unwrap();
        assert_eq!(intents[0].count, 1, "recent intent kept");
        assert_eq!(events[0].count, 1, "recent event kept");
        assert_eq!(outbox[0].count, 1, "recent outbox kept");
    }
}
