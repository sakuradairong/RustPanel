-- RP-09: audit intents (pre-exec), events (outcomes), and delivery outbox.
CREATE TABLE IF NOT EXISTS audit_intents (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    operation_id TEXT NOT NULL UNIQUE,
    actor_id INTEGER,
    request_id TEXT,
    action TEXT NOT NULL,
    resource TEXT,
    params_masked TEXT NOT NULL DEFAULT '{}',
    ip TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_intents_actor ON audit_intents(actor_id);
CREATE INDEX IF NOT EXISTS idx_audit_intents_created ON audit_intents(created_at);

CREATE TABLE IF NOT EXISTS audit_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    intent_id INTEGER,
    operation_id TEXT NOT NULL,
    outcome TEXT NOT NULL,
    exit_code INTEGER,
    detail_masked TEXT,
    duration_ms INTEGER,
    created_at TEXT NOT NULL,
    FOREIGN KEY(intent_id) REFERENCES audit_intents(id)
);

CREATE INDEX IF NOT EXISTS idx_audit_events_operation ON audit_events(operation_id);

CREATE TABLE IF NOT EXISTS audit_outbox (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    event_id INTEGER NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    next_retry_at TEXT,
    delivered_at TEXT,
    last_error TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY(event_id) REFERENCES audit_events(id)
);

CREATE INDEX IF NOT EXISTS idx_audit_outbox_pending
    ON audit_outbox(delivered_at, next_retry_at);
